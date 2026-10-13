"""Pure unit tests. Inert tar bytes and syntax parsing only; no installs."""
import ast
import importlib.util
import io
import json
import tempfile
from pathlib import Path
import tarfile
import unittest
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location("candidate_probe", Path(__file__).with_name("inspect_candidate.py"))
probe = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(probe)


def tar_bytes(entries):
    output = io.BytesIO()
    with tarfile.open(fileobj=output, mode="w") as archive:
        for name, kind, mode, uid, gid, data, pax in entries:
            member = tarfile.TarInfo(name)
            member.type, member.mode = kind, mode
            member.uid, member.gid = uid, gid
            member.pax_headers = pax
            member.size = len(data) if kind == tarfile.REGTYPE else 0
            archive.addfile(member, io.BytesIO(data) if kind == tarfile.REGTYPE else None)
    return io.BytesIO(output.getvalue())


def regular(name="usr/bin/sniper", mode=0o755, uid=0, gid=0):
    return (name, tarfile.REGTYPE, mode, uid, gid, b"INERT TEST BYTES\n", {})


class ArchiveInspectionTests(unittest.TestCase):
    def test_lifecycle_syntax_without_execution(self):
        source = Path(__file__).with_name("qualify_ci_lifecycle.py").read_text()
        ast.parse(source, filename="qualify_ci_lifecycle.py")

    def test_regular_payload_hash_and_mode(self):
        files, metadata = probe.archive_files(tar_bytes([regular()]), {"usr/bin/sniper"})
        self.assertEqual(set(files), {"usr/bin/sniper"})
        self.assertEqual(files["usr/bin/sniper"]["mode"], 0o755)
        self.assertEqual(files["usr/bin/sniper"]["sha256"], probe.hashlib.sha256(b"INERT TEST BYTES\n").hexdigest())
        self.assertFalse(metadata)

    def test_portable_prefix(self):
        files, _ = probe.archive_files(tar_bytes([regular("candidate/bin/sniper")]), {"bin/sniper"}, "candidate")
        self.assertEqual(set(files), {"bin/sniper"})

    def test_reject_unsafe_member_paths(self):
        for name in ("/usr/bin/sniper", "../usr/bin/sniper", "usr/../bin/sniper"):
            with self.subTest(name=name), self.assertRaises(ValueError):
                probe.archive_files(tar_bytes([regular(name)]), {"usr/bin/sniper"})

    def test_reject_links_and_special_members(self):
        for kind in (tarfile.SYMTYPE, tarfile.LNKTYPE, tarfile.CHRTYPE, tarfile.FIFOTYPE):
            member = ("usr/bin/sniper", kind, 0o755, 0, 0, b"", {})
            with self.subTest(kind=kind), self.assertRaises(ValueError):
                probe.archive_files(tar_bytes([member]), {"usr/bin/sniper"})

    def test_reject_duplicate_or_missing_payload(self):
        for entries in ([regular(), regular()], []):
            with self.subTest(entries=len(entries)), self.assertRaises(ValueError):
                probe.archive_files(tar_bytes(entries), {"usr/bin/sniper"})

    def test_reject_wrong_modes_and_ownership(self):
        for entry in (regular(mode=0o4755), regular(mode=0o644), regular(uid=1000), regular(gid=1000)):
            with self.subTest(entry=entry), self.assertRaises(ValueError):
                probe.archive_files(tar_bytes([entry]), {"usr/bin/sniper"})

    def test_reject_unexpected_payload_directory(self):
        entry = ("etc", tarfile.DIRTYPE, 0o755, 0, 0, b"", {})
        with self.assertRaises(ValueError):
            probe.archive_files(tar_bytes([entry, regular()]), {"usr/bin/sniper"})

    def test_reject_extended_attributes(self):
        entry = list(regular())
        entry[-1] = {"comment": "requires explicit review"}
        with self.assertRaises(ValueError):
            probe.archive_files(tar_bytes([tuple(entry)]), {"usr/bin/sniper"})

    def test_checksum_duplicate_and_invalid_digest(self):
        digest = "a" * 64
        for value in (f"{digest}  path\n{digest}  path\n", "xyz  path\n"):
            with self.subTest(value=value), self.assertRaises(ValueError):
                probe.checksum_entries(value.encode(), 64)

    def test_wrong_expected_manifest_hash_stops_before_archive_tool(self):
        with patch.object(probe, "plain_path", return_value=Path("/inert")), patch.object(Path, "read_bytes", return_value=b"wrong manifest bytes"), patch.object(probe.subprocess, "Popen", side_effect=AssertionError("no external process allowed")):
            with self.assertRaisesRegex(ValueError, "SHA-256 mismatch"):
                probe.inspect("/inert", "b" * 64)


    def fixture_candidate(self, root):
        """Tiny inert archives and a mocked .deb reader, never a runnable package."""
        content = {name: b"inert fixture bytes\n" for name in probe.EXPECTED}
        build = {"version": "0.3.0", "test_fixture": False, "target": "debian13-x86_64", "source": {}, "binaries": {name: {"sha256": probe.hashlib.sha256(content["usr/bin/" + name]).hexdigest(), "size": len(content["usr/bin/" + name])} for name in probe.BINARIES}}
        base = {"build": build, "system_libraries_bundled": False, "qualification": "not-qualified", "debian_depends": "libc6", "dependency_resolution": "test-mocked-reader", "elf": {name: {"interpreter": "/lib64/ld-linux-x86-64.so.2", "needed": []} for name in probe.BINARIES}}
        portable_map = {name: ("bin/" + name[8:] if name.startswith("usr/bin/") else name.removeprefix(probe.DOC) if name.startswith(probe.DOC) else name.removeprefix("usr/")) for name in content}
        def sealed(values, doc):
            values = {name: data for name, data in values.items() if name not in {doc + "build-manifest.json", doc + "SHA256SUMS"}}
            payload = {name: probe.hashlib.sha256(data).hexdigest() for name, data in values.items()}
            values[doc + "build-manifest.json"] = json.dumps({**base, "payload_files": payload}).encode()
            values[doc + "SHA256SUMS"] = "".join(probe.hashlib.sha256(data).hexdigest() + "  " + name + "\n" for name, data in sorted(values.items())).encode()
            return values
        deb_content = sealed(content, probe.DOC)
        portable = sealed({portable_map[name]: data for name, data in content.items()}, "")
        stem = "Sniper-0.3.0-debian13-x86_64"
        deb = root / (stem + ".deb")
        deb.write_bytes(b"INERT MOCKED DEB; NOT INSTALLABLE\n")
        archive = root / (stem + ".tar.gz")
        archive.write_bytes(tar_bytes([(stem + "/" + name, tarfile.REGTYPE, 0o755 if name.startswith("bin/") else 0o644, 0, 0, data, {}) for name, data in portable.items()]).getvalue())
        manifest = {**base, "artifacts": {deb.name: probe.sha(deb), archive.name: probe.sha(archive)}}
        manifest_path = root / "build-manifest.json"
        manifest_path.write_text(json.dumps(manifest))
        expected = probe.sha(manifest_path)
        for name, digest in {**manifest["artifacts"], "build-manifest.json": expected}.items():
            (root / (name + ".sha256")).write_text(digest + "  " + name + "\n")
        files = {name: {"sha256": probe.hashlib.sha256(data).hexdigest(), "md5": probe.hashlib.md5(data, usedforsecurity=False).hexdigest(), "size": len(data), "mode": 0o755 if name.startswith("usr/bin/") else 0o644, "uid": 0, "gid": 0} for name, data in deb_content.items()}
        fields = {"Package": "sniper", "Version": "0.3.0-1", "Architecture": "amd64", "Maintainer": "fixture", "Section": "devel", "Priority": "optional", "Homepage": "https://example.com", "Installed-Size": "1", "Depends": "libc6", "Description": "inert mocked reader"}
        controls = {"control": "".join(key + ": " + value + "\n" for key, value in fields.items()).encode(), "md5sums": "".join(item["md5"] + "  " + name + "\n" for name, item in sorted(files.items())).encode()}
        def reader(path, option, expected_files):
            return ({}, controls) if option == "--ctrl-tarfile" else (files, deb_content)
        return expected, reader, deb

    def test_reject_inputs_changed_during_inspection(self):
        for target in ("deb", "manifest", "sidecar"):
            with self.subTest(target=target), tempfile.TemporaryDirectory(prefix="sniper-inert-inspection-") as temporary:
                root = Path(temporary)
                expected, reader, deb = self.fixture_candidate(root)
                changed = False
                def mutate(path, option, expected_files):
                    nonlocal changed
                    if not changed:
                        changed = True
                        destination = deb if target == "deb" else root / ("build-manifest.json" if target == "manifest" else deb.name + ".sha256")
                        destination.write_bytes(destination.read_bytes() + b"changed")
                    return reader(path, option, expected_files)
                with patch.object(probe, "deb_stream", side_effect=mutate), self.assertRaisesRegex(ValueError, "changed during inspection"):
                    probe.inspect(root, expected)

    def test_keeps_pinned_artifact_digest(self):
        with tempfile.TemporaryDirectory(prefix="sniper-inert-inspection-") as temporary:
            root = Path(temporary)
            expected, reader, deb = self.fixture_candidate(root)
            pinned = probe.sha(deb)
            with patch.object(probe, "deb_stream", side_effect=reader):
                self.assertEqual(probe.inspect(root, expected)["deb_sha256"], pinned)


if __name__ == "__main__":
    unittest.main()
