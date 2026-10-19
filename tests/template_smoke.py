"""Verify passive Nuclei templates against an isolated Sniper and a loopback
upstream: import a synthetic release, enable templates, capture in scope,
restart, and rescan after disabling. Nothing leaves this machine.

Run: python tests/template_smoke.py [directory containing sniper]
"""

import gzip
import http.client
import http.server
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import tarfile
import tempfile
import threading
import time
import urllib.request

DETECT = """id: example-detect
info:
  name: Example Detect
  severity: low
  tags: tech
http:
  - method: GET
    path: ['{{BaseURL}}']
    matchers:
      - type: word
        words: ['Example Server']
    extractors:
      - type: regex
        group: 1
        regex: ['Example Server ([0-9.]+)']
"""

GLOBAL = """id: example-global
info:
  name: Example Global
  severity: info
http:
  - global-matchers: true
    matchers:
      - type: regex
        name: example-header
        part: header
        regex: ['(?i)x-example: yes']
"""

ACTIVE = """id: example-active
info:
  name: Example Active
  severity: medium
http:
  - method: GET
    path: ['{{BaseURL}}/.env']
    matchers:
      - type: word
        words: ['APP_KEY=']
"""

DNS = """id: example-dns
info:
  name: Example DNS
  severity: info
dns:
  - name: '{{FQDN}}'
"""


def release():
    raw = io.BytesIO()
    with tarfile.open(fileobj=raw, mode="w") as archive:
        for path, text in [("http/detect.yaml", DETECT), ("http/global.yaml", GLOBAL),
                           ("http/active.yaml", ACTIVE), ("dns/dns.yaml", DNS)]:
            body = text.encode()
            info = tarfile.TarInfo("example-templates-1.0/" + path)
            info.size = len(body)
            archive.addfile(info, io.BytesIO(body))
    return gzip.compress(raw.getvalue())


def eventually(operation, timeout=30):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        result = operation()
        if result:
            return result
        time.sleep(0.1)
    raise AssertionError("Timed out waiting for the isolated runtime")


def main():
    binaries = Path(sys.argv[1] if len(sys.argv) > 1 else "target/release").resolve()

    class Upstream(http.server.BaseHTTPRequestHandler):
        def log_message(self, *_):
            pass

        def do_GET(self):
            body = b"<html><title>Example Server 1.2</title></html>"
            self.send_response(200)
            self.send_header("Content-Type", "text/html")
            self.send_header("X-Example", "yes")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)

    upstream = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Upstream)
    threading.Thread(target=upstream.serve_forever, daemon=True).start()
    target = f"http://127.0.0.1:{upstream.server_port}/"
    # Loopback API calls must not go through any proxy the environment sets.
    opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
    temporary = tempfile.TemporaryDirectory(prefix="sniper-template-smoke-")
    data = Path(temporary.name)
    env = dict(os.environ, SNIPER_DATA_DIR=str(data), SNIPER_UI_ADDR="127.0.0.1:0",
               SNIPER_PROXY_ADDR="127.0.0.1:0", SNIPER_INSTALL_AGENT_SKILLS="0",
               SNIPER_INSTALL_CLI_PATH="0", RUST_LOG="off")
    env.pop("SNIPER_API_ADDR", None)
    for key in ["HTTP_PROXY", "HTTPS_PROXY", "ALL_PROXY", "http_proxy", "https_proxy", "all_proxy"]:
        env.pop(key, None)
    suffix = ".exe" if os.name == "nt" else ""
    flags = subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0
    process = None

    def start():
        nonlocal process
        process = subprocess.Popen([str(binaries / ("sniper" + suffix))], env=env,
                                   stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, creationflags=flags)

        def ready():
            assert process.poll() is None, "Runtime exited before becoming ready"
            try:
                state = json.loads((data / "runtime-state.json").read_text())
                return state if state["pid"] == process.pid else None
            except (OSError, ValueError):
                return None

        return eventually(ready)

    def stop():
        process.terminate()
        try:
            process.wait(timeout=10)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait(timeout=5)

    try:
        runtime = start()
        ui = "http://" + runtime["ui_addr"]

        def call(path, payload=None, raw=None, content_type="application/json", expect=200):
            body = raw if raw is not None else (None if payload is None else json.dumps(payload).encode())
            request = urllib.request.Request(ui + path, data=body, headers={"Content-Type": content_type,
                                                                            "Origin": ui})
            try:
                with opener.open(request, timeout=30) as response:
                    assert response.status == expect, (path, response.status)
                    text = response.read()
                    return json.loads(text) if text else None
            except urllib.error.HTTPError as error:
                raise AssertionError(f"{path}: {error.code} {error.read()[:200]!r}") from error

        status = eventually(lambda: (s := call("/api/templates/library"))["state"] == "idle" and s)
        assert status["templates"] == 0, status
        call("/api/templates/library/import", raw=release(), content_type="application/gzip", expect=202)
        status = eventually(lambda: (s := call("/api/templates/library"))["state"] == "idle"
                            and s["templates"] == 4 and s)
        assert status["passive_supported"] == 2, status
        assert status["library"]["source"] == "import" and not status["library"]["verified"], status
        listed = call("/api/templates?support=passive")
        assert sorted(item["id"] for item in listed["items"]) == ["example-detect", "example-global"], listed
        detail = call("/api/templates/example-dns")
        assert detail["protocol"] == "dns" and "protocol:dns" in detail["reasons"], detail
        enabled = call("/api/templates/passive", {"filter": {"support": "passive"}, "enabled": True})
        assert enabled["passive_enabled"] == 2, enabled

        call("/api/runtime", {"scope_patterns": ["127.0.0.1"]})
        proxy_host, proxy_port = runtime["proxy_addr"].rsplit(":", 1)
        connection = http.client.HTTPConnection(proxy_host, int(proxy_port), timeout=10)
        connection.request("GET", target)
        response = connection.getresponse()
        response.read()
        assert response.status == 200, response.status

        def template_findings():
            findings = call("/api/findings?limit=1000")
            found = {item["rule_id"]: item for item in findings if item.get("rule_id", "").startswith("nuclei-passive:")}
            return found if len(found) == 2 else None

        found = eventually(template_findings)
        assert set(found) == {"nuclei-passive:example-detect", "nuclei-passive:example-global:example-header"}, found
        detect = call("/api/findings/" + found["nuclei-passive:example-detect"]["id"])
        assert detect["evidence"] == "1.2" and detect["category"] == "nuclei", detect
        print(json.dumps({"captured": sorted(found)}), flush=True)

        stop()
        runtime = start()
        ui = "http://" + runtime["ui_addr"]
        status = eventually(lambda: (s := call("/api/templates/library"))["state"] == "idle" and s)
        assert status["templates"] == 4 and status["passive_enabled"] == 2, status
        assert template_findings(), "template findings did not survive a restart"

        call("/api/templates/passive", {"ids": ["example-global"], "enabled": False})
        rescanned = call("/api/findings/rescan", {})
        assert rescanned["scanned"] == 1, rescanned
        remaining = [item["rule_id"] for item in call("/api/findings?limit=1000")
                     if item.get("rule_id", "").startswith("nuclei-passive:")]
        assert remaining == ["nuclei-passive:example-detect"], remaining
        print("Import, passive capture, restart and rescan after disabling passed.", flush=True)
    finally:
        if process is not None and process.poll() is None:
            stop()
        upstream.shutdown()
        upstream.server_close()
        temporary.cleanup()


if __name__ == "__main__":
    main()
