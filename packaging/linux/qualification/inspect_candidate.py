#!/usr/bin/env python3
"""Read-only candidate inspection; no extraction, install or payload execution."""
import argparse
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import re
import subprocess
import tarfile

BINARIES = ("sniper", "sniper-cli", "sniper-desktop")
DOC = "usr/share/doc/sniper/"
EXPECTED = {
    *("usr/bin/" + name for name in BINARIES),
    "usr/share/applications/sniper.desktop",
    "usr/share/icons/hicolor/256x256/apps/sniper.png",
    *(DOC + name for name in ("LICENSE", "README.md", "build-manifest.json", "SHA256SUMS")),
}
ENV = {**os.environ, "LC_ALL": "C", "DPKG_COLORS": "never", "DPKG_NLS": "0"}
for key in ("LD_LIBRARY_PATH", "LD_PRELOAD", "DPKG_ROOT", "DPKG_ADMINDIR", "DPKG_DATADIR"):
    ENV.pop(key, None)


def require(condition, message):
    if not condition:
        raise ValueError(message)


def sha(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def plain_path(value):
    path = Path(value).absolute()
    require(".." not in path.parts, "parent traversal is not accepted")
    for ancestor in (path, *path.parents):
        require(not ancestor.is_symlink(), "symlink input is not accepted: " + str(ancestor))
    return path


def archive_files(stream, expected, prefix=""):
    """Hash regular files, retaining only bounded metadata; no extraction API."""
    directories = {""}
    for name in expected:
        directories.update(str(p) for p in PurePosixPath(name).parents if str(p) != ".")
    files, metadata, seen = {}, {}, set()
    total = 0
    with tarfile.open(fileobj=stream, mode="r|*") as archive:
        for member in archive:
            raw = member.name.removeprefix("./").rstrip("/")
            if raw == ".":
                raw = ""
            require(not raw.startswith("/") and ".." not in PurePosixPath(raw).parts, "unsafe member path")
            if prefix:
                require(raw == prefix or raw.startswith(prefix + "/"), "unexpected portable root")
                raw = raw[len(prefix):].lstrip("/")
            require(raw not in seen and len(seen) < 100, "duplicate or excess archive member")
            seen.add(raw)
            require((member.uid, member.gid) == (0, 0), "non-root archive ownership")
            require(not member.pax_headers, "unexpected extended member attributes")
            if member.isdir():
                require(raw in directories and member.mode == 0o755, "unexpected directory or mode")
                continue
            require(member.isfile() and raw in expected, "unexpected file, symlink or special member")
            mode = 0o755 if str(PurePosixPath(raw).parent) in ("bin", "usr/bin") else 0o644
            require(member.mode == mode, "unexpected mode for " + raw)
            total += member.size
            require(0 <= member.size <= 128 * 1024 * 1024 and total <= 256 * 1024 * 1024, "oversized payload")
            h, md5 = hashlib.sha256(), hashlib.md5(usedforsecurity=False)
            keep = PurePosixPath(raw).name in ("build-manifest.json", "SHA256SUMS", "control", "md5sums")
            require(not keep or member.size < 1024 * 1024, "oversized metadata")
            captured = bytearray()
            with archive.extractfile(member) as source:
                while chunk := source.read(1024 * 1024):
                    h.update(chunk)
                    md5.update(chunk)
                    if keep:
                        captured.extend(chunk)
            files[raw] = {"sha256": h.hexdigest(), "md5": md5.hexdigest(), "size": member.size, "mode": mode, "uid": 0, "gid": 0}
            if keep:
                metadata[raw] = bytes(captured)
    require(set(files) == set(expected), "missing payload files")
    return files, metadata


def deb_stream(deb, option, expected):
    process = subprocess.Popen(["dpkg-deb", option, str(deb)], stdout=subprocess.PIPE, env=ENV)
    try:
        result = archive_files(process.stdout, expected)
        require(process.wait() == 0, "dpkg-deb failed")
        return result
    finally:
        if process.poll() is None:
            process.terminate()
        process.wait()
        process.stdout.close()


def checksum_entries(data, length):
    entries = {}
    for line in data.decode("utf-8").splitlines():
        checksum, name = line.split("  ", 1)
        require(re.fullmatch(r"[0-9a-f]{" + str(length) + "}", checksum) and name not in entries, "invalid checksum entry")
        entries[name] = checksum
    return entries


def verify_checksums(files, metadata, path):
    entries = checksum_entries(metadata[path], 64)
    require(set(entries) == set(files) - {path}, "checksum file does not enumerate complete payload")
    require(all(files[name]["sha256"] == value for name, value in entries.items()), "internal checksum mismatch")


def inspect(directory, expected_manifest_sha256):
    directory = plain_path(directory)
    manifest_path = plain_path(directory / "build-manifest.json")
    require(re.fullmatch(r"[0-9a-f]{64}", expected_manifest_sha256), "expected manifest SHA-256 is required")
    manifest_bytes = manifest_path.read_bytes()
    require(hashlib.sha256(manifest_bytes).hexdigest() == expected_manifest_sha256, "external manifest SHA-256 mismatch")
    manifest = json.loads(manifest_bytes)
    build = manifest["build"]
    require(build["test_fixture"] is False and build["target"] == "debian13-x86_64", "fixture or incorrect target")
    require(re.fullmatch(r"[0-9]+\.[0-9]+\.[0-9]+", build["version"]), "invalid version")
    require(manifest["system_libraries_bundled"] is False, "unexpected bundled libraries")
    require(manifest["qualification"] == "not-qualified", "candidate labeling changed; review again")
    stem = "Sniper-" + build["version"] + "-debian13-x86_64"
    require(set(manifest["artifacts"]) == {stem + ".deb", stem + ".tar.gz"}, "unexpected artifact inventory")
    pinned_hashes = {**manifest["artifacts"], "build-manifest.json": expected_manifest_sha256}
    for filename, checksum in pinned_hashes.items():
        artifact = plain_path(directory / filename)
        require(artifact.is_file() and sha(artifact) == checksum, "artifact SHA-256 mismatch: " + filename)
        sidecar = plain_path(directory / (filename + ".sha256"))
        require(sidecar.read_text() == checksum + "  " + filename + "\n", "sidecar mismatch: " + filename)
    deb = directory / (stem + ".deb")
    _, controls = deb_stream(deb, "--ctrl-tarfile", {"control", "md5sums"})
    control = {}
    for line in controls["control"].decode().splitlines():
        require(":" in line and not line.startswith(" "), "unexpected multiline control field")
        key, value = line.split(":", 1)
        require(key not in control, "duplicate control field")
        control[key] = value.strip()
    require(control.get("Package") == "sniper" and control.get("Architecture") == "amd64", "wrong package identity")
    require(control.get("Version") == build["version"] + "-1", "wrong package version")
    require(control.get("Depends") == manifest["debian_depends"], "dependency metadata differs")
    require(set(control) == {"Package", "Version", "Architecture", "Maintainer", "Section", "Priority", "Homepage", "Installed-Size", "Depends", "Description"}, "new control policy requires review")
    files, metadata = deb_stream(deb, "--fsys-tarfile", EXPECTED)
    verify_checksums(files, metadata, DOC + "SHA256SUMS")
    md5s = checksum_entries(controls["md5sums"], 32)
    require(md5s == {name: value["md5"] for name, value in files.items()}, "dpkg MD5 manifest mismatch")
    internal = json.loads(metadata[DOC + "build-manifest.json"])
    base = {k: v for k, v in manifest.items() if k != "artifacts"}
    require({k: v for k, v in internal.items() if k != "payload_files"} == base, "internal build manifest differs")
    expected_payload = {name: item["sha256"] for name, item in files.items() if name not in {DOC + "build-manifest.json", DOC + "SHA256SUMS"}}
    require(internal["payload_files"] == expected_payload, "payload manifest differs")
    portable_map = {name: ("bin/" + name[8:] if name.startswith("usr/bin/") else name.removeprefix(DOC) if name.startswith(DOC) else name.removeprefix("usr/")) for name in EXPECTED}
    with (directory / (stem + ".tar.gz")).open("rb") as stream:
        portable_files, portable_metadata = archive_files(stream, set(portable_map.values()), stem)
    verify_checksums(portable_files, portable_metadata, "SHA256SUMS")
    for path, portable_path in portable_map.items():
        if PurePosixPath(path).name not in ("build-manifest.json", "SHA256SUMS"):
            require(files[path] == portable_files[portable_path], "portable and Debian bytes differ: " + path)
    portable_manifest = json.loads(portable_metadata["build-manifest.json"])
    require({k: v for k, v in portable_manifest.items() if k != "payload_files"} == base, "portable build identity differs")
    require(portable_manifest["payload_files"] == {name: item["sha256"] for name, item in portable_files.items() if name not in {"build-manifest.json", "SHA256SUMS"}}, "portable payload manifest differs")
    for binary in BINARIES:
        require(files["usr/bin/" + binary]["sha256"] == build["binaries"][binary]["sha256"], "binary identity mismatch")
        require(files["usr/bin/" + binary]["size"] == build["binaries"][binary]["size"], "binary size mismatch")
    # Recheck all independently pinned inputs after streaming both archives. A
    # later observed digest must never replace the digest that authorized them.
    for filename, checksum in pinned_hashes.items():
        require(sha(plain_path(directory / filename)) == checksum, "input changed during inspection: " + filename)
        sidecar = plain_path(directory / (filename + ".sha256"))
        require(sidecar.read_text() == checksum + "  " + filename + "\n", "sidecar changed during inspection: " + filename)
    return {"schema": 1, "scope": "archive inspection only; no install, extraction or application execution", "candidate_directory": str(directory), "deb": str(deb), "deb_sha256": manifest["artifacts"][deb.name], "manifest_sha256": expected_manifest_sha256, "package": "sniper", "version": control["Version"], "architecture": "amd64", "source": build["source"], "dependency_resolution": manifest["dependency_resolution"], "files": files, "elf": manifest["elf"]}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("candidate_directory")
    parser.add_argument("expected_manifest_sha256")
    args = parser.parse_args()
    print(json.dumps(inspect(args.candidate_directory, args.expected_manifest_sha256), indent=2, sort_keys=True))
