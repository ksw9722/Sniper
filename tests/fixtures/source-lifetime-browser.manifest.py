#!/usr/bin/env python3
"""Sync this synthetic fixture's production map DOM and refresh input hashes."""
import hashlib
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
HERE = Path(__file__).resolve().parent
html_path = HERE / "source-lifetime-browser.html"
source = (ROOT / "web/index.html").read_text()
start = source.index('          <section class="service-map-workspace"')
end = source.index('          <section class="panel-card hidden" id="targetPathsPanel"', start)
html = html_path.read_text()
fixture_start = html.index('          <section class="service-map-workspace"')
fixture_end = html.index('    <details class="fixture-log"', fixture_start)
html_path.write_text(html[:fixture_start] + source[start:end] + html[fixture_end:])
paths = ["web/service-map.js", "web/styles.css", "web/index.html"] + [
    "tests/fixtures/source-lifetime-browser." + suffix
    for suffix in ["html", "js", "css", "manifest.py", "README.md", "check.cjs"]
]
manifest = {
    "fixture": "Exact source lifetime · synthetic actual-code browser QA",
    "production_controller": "web/service-map.js",
    "production_styles": "web/styles.css",
    "production_dom": "targetServiceMapPanel extracted from web/index.html",
    "app_bootstrap": False,
    "networking": "CSP connect-src none; closed in-memory request adapter; connection guards",
    "sha256": {path: hashlib.sha256((ROOT / path).read_bytes()).hexdigest() for path in paths},
}
(HERE / "source-lifetime-browser.manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
print("Synced production DOM and refreshed source-lifetime-browser.manifest.json")
