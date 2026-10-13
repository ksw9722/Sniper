"""Linux packaging contracts. No package installation or Sniper execution."""
import argparse
import contextlib
import hashlib
import importlib.util
import io
import json
import os
import shutil
from pathlib import Path
import struct
import subprocess
import sys
import tarfile
import tempfile
import unittest
from unittest.mock import patch

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("linux_package", ROOT / "packaging/linux/package.py")
pkg = importlib.util.module_from_spec(spec)
spec.loader.exec_module(pkg)
SOURCE = {"revision": "a" * 40, "dirty": True, "tree_sha256": "b" * 64, "cargo_lock_sha256": "c" * 64}


class PackagingTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="sniper-package-contract-", dir="/tmp")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.binaries = self.root / "binaries"
        self.binaries.mkdir()
        for name in pkg.BINARIES:
            path = self.binaries / name
            path.write_bytes(pkg.FIXTURE + name.encode() + b"\n")
            path.chmod(0o755)
        self.source_patch = patch.object(pkg, "source_state", return_value=SOURCE)
        self.source_patch.start()
        self.addCleanup(self.source_patch.stop)
        self.record_path = self.root / "record.json"
        self.record_args = argparse.Namespace(
            binaries_dir=str(self.binaries), test_fixtures=True, output=str(self.record_path),
            source_date_epoch=1760000000, toolchain_file=None, dependency_manifest=None, build_command="fixture")
        with contextlib.redirect_stdout(io.StringIO()):
            pkg.record(self.record_args)
        self.args = argparse.Namespace(
            binaries_dir=str(self.binaries), test_fixtures=True, output_dir=str(self.root / "output"),
            build_record=str(self.record_path), archive_only=False, sysroot=None, dpkg_admindir=None)

    def package(self):
        with contextlib.redirect_stdout(io.StringIO()):
            pkg.package(self.args)
        return Path(self.args.output_dir)

    def rewrite_record(self, change):
        value = json.loads(self.record_path.read_text())
        change(value)
        self.record_path.write_text(json.dumps(value))

    def test_complete_archive_deb_layout_integrity_and_root_ownership(self):
        output = self.package()
        archive = next(output.glob("*.tar.gz"))
        self.assertIn("debian13-x86_64-TEST-ONLY", archive.name)
        with tarfile.open(archive) as tar:
            files = {member.name.split("/", 1)[1]: tar.extractfile(member).read()
                     for member in tar.getmembers() if member.isfile()}
            for member in tar.getmembers():
                self.assertFalse(member.issym() or member.islnk())
                self.assertEqual((member.uid, member.gid), (0, 0))
                self.assertFalse(member.mode & 0o7000)
            for binary in pkg.BINARIES:
                self.assertEqual(files[f"bin/{binary}"], pkg.FIXTURE + binary.encode() + b"\n")
                self.assertEqual(tar.getmember(archive.name.removesuffix(".tar.gz") + "/bin/" + binary).mode, 0o755)
            for name in ("LICENSE", "README.md", "build-manifest.json", "SHA256SUMS", "share/applications/sniper.desktop",
                         "share/icons/hicolor/256x256/apps/sniper.png"):
                self.assertIn(name, files)
            self.verify_checksums(files, "SHA256SUMS")
            manifest = json.loads(files["build-manifest.json"])
            self.assertEqual(manifest["qualification"], "not-qualified")
            self.assertTrue(manifest["build"]["test_fixture"])
            self.assertFalse(manifest["system_libraries_bundled"])
            self.assertEqual(set(manifest["payload_files"]), set(files) - {"build-manifest.json", "SHA256SUMS"})
        deb = next(output.glob("*.deb"))
        control = subprocess.check_output(["dpkg-deb", "--ctrl-tarfile", str(deb)])
        with tarfile.open(fileobj=io.BytesIO(control)) as tar:
            controls = {member.name.removeprefix("./"): tar.extractfile(member).read()
                        for member in tar.getmembers() if member.isfile()}
        self.assertEqual(set(controls), {"control", "md5sums"})
        self.assertIn(b"Package: sniper-packaging-fixture\n", controls["control"])
        self.assertIn(b"Architecture: amd64\n", controls["control"])
        self.assertIn(b"Maintainer: sm1ee <sm1ee@users.noreply.github.com>\n", controls["control"])
        self.assertIn(b"TEST FIXTURE ONLY", controls["control"])
        data = subprocess.check_output(["dpkg-deb", "--fsys-tarfile", str(deb)])
        with tarfile.open(fileobj=io.BytesIO(data)) as tar:
            files = {member.name.removeprefix("./"): tar.extractfile(member).read()
                     for member in tar.getmembers() if member.isfile()}
            for member in tar.getmembers():
                self.assertEqual((member.uid, member.gid), (0, 0))
                self.assertFalse(member.issym() or member.islnk())
            for binary in pkg.BINARIES:
                self.assertIn(f"usr/bin/{binary}", files)
                self.assertEqual(tar.getmember(f"./usr/bin/{binary}").mode, 0o755)
        self.assertEqual(set(files), {
            *(f"usr/bin/{binary}" for binary in pkg.BINARIES),
            "usr/share/applications/sniper.desktop", "usr/share/icons/hicolor/256x256/apps/sniper.png",
            *(f"usr/share/doc/sniper/{name}" for name in ("LICENSE", "README.md", "build-manifest.json", "SHA256SUMS")),
        })
        self.verify_checksums(files, "usr/share/doc/sniper/SHA256SUMS")
        for line in controls["md5sums"].decode().splitlines():
            checksum, name = line.split("  ", 1)
            self.assertEqual(hashlib.md5(files[name], usedforsecurity=False).hexdigest(), checksum)
        for sidecar in output.glob("*.sha256"):
            checksum, name = sidecar.read_text().strip().split("  ", 1)
            self.assertEqual(pkg.digest(output / name), checksum)
        external = json.loads((output / "build-manifest.json").read_text())
        self.assertEqual(set(external["artifacts"]), {archive.name, deb.name})

    def verify_checksums(self, files, manifest_name):
        checked = set()
        for line in files[manifest_name].decode().splitlines():
            checksum, name = line.split("  ", 1)
            self.assertEqual(hashlib.sha256(files[name]).hexdigest(), checksum)
            checked.add(name)
        self.assertEqual(checked, set(files) - {manifest_name})

    def test_repack_is_deterministic_and_never_overwrites(self):
        first = self.package()
        with self.assertRaisesRegex(ValueError, "existing output"):
            self.package()
        self.args.output_dir = str(self.root / "second")
        second = self.package()
        self.assertEqual(pkg.file_manifest(first), pkg.file_manifest(second))
        with self.assertRaisesRegex(ValueError, "existing output"):
            pkg.record(self.record_args)

    def test_archive_only_does_not_call_dpkg_dependency_tools(self):
        self.args.archive_only = True
        with patch.object(pkg, "deb_dependencies", side_effect=AssertionError("must not resolve")):
            output = self.package()
        self.assertFalse(list(output.glob("*.deb")))
        self.assertEqual(json.loads((output / "build-manifest.json").read_text())["dependency_resolution"], "not-run")

    def test_input_change_during_copy_is_rejected_before_publishing(self):
        original_copy = pkg.copy_file

        def mutate_then_copy(source, target, executable=False):
            if source.name == "sniper":
                source.write_bytes(b"changed after validation")
            original_copy(source, target, executable)

        with patch.object(pkg, "copy_file", side_effect=mutate_then_copy):
            with self.assertRaisesRegex(ValueError, "changed during staging"):
                self.package()
        self.assertFalse(Path(self.args.output_dir).exists())

    def test_source_change_during_packaging_cannot_publish_mixed_inputs(self):
        with patch.object(pkg, "source_state", side_effect=[SOURCE, {}]):
            with self.assertRaisesRegex(ValueError, "source changed during packaging"):
                self.package()
        self.assertFalse(Path(self.args.output_dir).exists())

    def test_real_record_requires_correct_toolchain_and_debian_inventory(self):
        self.record_args.output = str(self.root / "real-record.json")
        self.record_args.test_fixtures = False
        with self.assertRaisesRegex(ValueError, "real inputs require"):
            pkg.record(self.record_args)
        toolchain = self.root / "toolchain.txt"
        dependencies = self.root / "packages.tsv"
        toolchain.write_text("rustc 1.99.0\nhost: aarch64-unknown-linux-gnu\n")
        dependencies.write_text("libc6\t2.41-12\tamd64\n")
        self.record_args.toolchain_file = str(toolchain)
        self.record_args.dependency_manifest = str(dependencies)
        self.record_args.build_command = "cargo build --locked --release --bins"
        with self.assertRaisesRegex(ValueError, "Linux x86_64"):
            pkg.record(self.record_args)
        toolchain.write_text("rustc 1.99.0\nhost: x86_64-unknown-linux-gnu\n")
        with self.assertRaisesRegex(ValueError, "missing libgtk-3-0t64"):
            pkg.record(self.record_args)
        self.assertFalse(Path(self.record_args.output).exists())

    def test_fixture_mode_cannot_disguise_arbitrary_executables(self):
        (self.binaries / "sniper").write_text("#!/bin/sh\nexit 0\n")
        with self.assertRaisesRegex(ValueError, "only inert"):
            self.package()

    def test_real_mode_rejects_fixture_record(self):
        self.args.test_fixtures = False
        with self.assertRaisesRegex(ValueError, "fixture mode"):
            self.package()

    def test_missing_nonexecutable_and_symlink_inputs_are_rejected(self):
        path = self.binaries / "sniper"
        path.chmod(0o644)
        with self.assertRaisesRegex(ValueError, "not executable"):
            self.package()
        path.unlink()
        with self.assertRaisesRegex(ValueError, "missing regular"):
            self.package()
        path.symlink_to(self.binaries / "sniper-cli")
        with self.assertRaisesRegex(ValueError, "symlink"):
            self.package()

    def test_unsafe_output_paths_do_not_create_files(self):
        for path in ("/usr/sniper-package-output", str(self.root / ".." / "bad"), str(self.root) + "\nunsafe"):
            self.args.output_dir = path
            with self.assertRaises(ValueError):
                self.package()
        link = self.root / "link"
        link.symlink_to(self.root / "real", target_is_directory=True)
        self.args.output_dir = str(link / "out")
        with self.assertRaisesRegex(ValueError, "symlink"):
            self.package()
        self.assertFalse((self.root / "real").exists())

    def test_source_binary_target_version_and_epoch_mismatches_are_rejected(self):
        original = self.record_path.read_text()
        changes = [
            (lambda r: r.update(source={}), "source changed"),
            (lambda r: r["binaries"]["sniper"].update(sha256="0" * 64), "binary changed"),
            (lambda r: r.update(target="ubuntu-x86_64"), "wrong build record target"),
            (lambda r: r.update(version="0.3.1"), "invalid package version"),
            (lambda r: r.update(source_date_epoch=-1), "invalid source date epoch"),
        ]
        for change, error in changes:
            self.record_path.write_text(original)
            self.rewrite_record(change)
            with self.assertRaisesRegex(ValueError, error):
                self.package()
        self.assertFalse(Path(self.args.output_dir).exists())

    def test_wrong_os_architecture_and_distro_are_rejected(self):
        for system, machine, distro in [
            ("Darwin", "x86_64", {"ID": "debian", "VERSION_ID": "13"}),
            ("Linux", "aarch64", {"ID": "debian", "VERSION_ID": "13"}),
            ("Linux", "x86_64", {"ID": "ubuntu", "VERSION_ID": "24.04"}),
            ("Linux", "x86_64", {"ID": "debian", "VERSION_ID": "12"}),
        ]:
            with self.assertRaises(ValueError):
                pkg.host_target(system, machine, distro)

    def test_elf_rejects_nonlinux_wrong_arch_static_and_rpath(self):
        path = self.root / "elf"
        path.write_bytes(b"MZ" + bytes(62))
        with self.assertRaisesRegex(ValueError, "not ELF64"):
            pkg.inspect_elf(path)
        header = bytearray(64)
        header[:7] = b"\x7fELF\x02\x01\x01"
        struct.pack_into("<HH", header, 16, 3, 183)
        path.write_bytes(header)
        with self.assertRaisesRegex(ValueError, "not an x86_64"):
            pkg.inspect_elf(path)
        struct.pack_into("<HH", header, 16, 3, 62)
        header[7] = 9
        path.write_bytes(header)
        with self.assertRaisesRegex(ValueError, "not a Linux"):
            pkg.inspect_elf(path)
        header[7] = 0
        path.write_bytes(header)
        with patch.object(pkg, "run", return_value="no interpreter"):
            with self.assertRaisesRegex(ValueError, "unexpected Linux interpreter"):
                pkg.inspect_elf(path)
        interpreter = "[Requesting program interpreter: /lib64/ld-linux-x86-64.so.2]"
        for tag in ("RPATH", "RUNPATH"):
            with patch.object(pkg, "run", side_effect=[interpreter, f"({tag}) Library path: [/tmp/sysroot]"]):
                with self.assertRaisesRegex(ValueError, "RPATH/RUNPATH"):
                    pkg.inspect_elf(path)
        with patch.object(pkg, "run", side_effect=[interpreter, "(NEEDED) Shared library: [libc.so.6]", "GLIBC_2.41"]):
            elf = pkg.inspect_elf(path)
        self.assertEqual(elf["needed"], ["libc.so.6"])
        self.assertEqual(elf["glibc_versions"], ["GLIBC_2.41"])

    def test_dependency_tool_uses_official_metadata_and_missing_info_is_fatal(self):
        work = self.root / "deps"
        work.mkdir()
        dependency = "shlibs:Depends=libc6 (>= 2.41), libwebkit2gtk-4.1-0 (>= 2.38)"
        with patch.object(pkg, "run", return_value=dependency) as run:
            result = pkg.deb_dependencies(work, pkg.inputs(self.binaries, True))
        self.assertEqual(result, dependency.split("=", 1)[1])
        command = run.call_args.args
        self.assertEqual(command[0:2], ("dpkg-shlibdeps", "-O"))
        self.assertNotIn("--ignore-missing-info", command)
        self.assertEqual(sum(arg.startswith("-e") for arg in command), 3)
        self.assertTrue(run.call_args.kwargs["clean_stderr"])
        second = self.root / "bad-deps"
        second.mkdir()
        with patch.object(pkg, "run", return_value="shlibs:Depends=libc6"):
            with self.assertRaisesRegex(ValueError, "complete native dependency"):
                pkg.deb_dependencies(second, pkg.inputs(self.binaries, True))

    def test_extracted_dependency_metadata_is_explicit_and_read_only(self):
        sysroot = self.root / "sysroot"
        (sysroot / "usr/lib/x86_64-linux-gnu").mkdir(parents=True)
        database = self.root / "dpkg"
        database.mkdir()
        (database / "status").write_text("fixture package metadata")
        (database / "info").mkdir()
        (database / "info/format").write_text("1\n")
        work = self.root / "deps-extracted"
        work.mkdir()
        with patch.object(pkg, "run", return_value="shlibs:Depends=libc6, libwebkit2gtk-4.1-0") as run, \
                patch.object(pkg, "check_sysroot_libraries") as containment:
            pkg.deb_dependencies(work, pkg.inputs(self.binaries, True), sysroot, database)
        containment.assert_called_once()
        self.assertIn("--admindir=" + str(database), run.call_args.args)
        self.assertIn("-l" + str(sysroot / "usr/lib/x86_64-linux-gnu"), run.call_args.args)
        self.assertEqual((database / "status").read_text(), "fixture package metadata")
        self.assertEqual(list(sysroot.rglob("*")), [sysroot / "usr", sysroot / "usr/lib", sysroot / "usr/lib/x86_64-linux-gnu"])
        self.args.sysroot = str(sysroot)
        with self.assertRaisesRegex(ValueError, "supplied together"):
            self.package()

    def test_success_exit_with_dependency_diagnostics_is_fatal(self):
        warnings = [
            "dpkg-shlibdeps: warning: symbol __libc_start_main@GLIBC_2.34 used by sniper found in none of the libraries\n",
            "dpkg-shlibdeps: warning: binary sniper contains an unresolvable reference to symbol __libc_start_main: it's probably a plugin\n",
            "dpkg-shlibdeps: warning: unexpected dependency metadata\n",
        ]
        for warning in warnings:
            completed = subprocess.CompletedProcess(["dpkg-shlibdeps"], 0, "shlibs:Depends=libc6", warning)
            with patch.object(pkg.subprocess, "run", return_value=completed), contextlib.redirect_stderr(io.StringIO()):
                with self.assertRaisesRegex(ValueError, "dependency tooling emitted diagnostics"):
                    pkg.run("dpkg-shlibdeps", "-O", clean_stderr=True)
        completed = subprocess.CompletedProcess(["dpkg-shlibdeps"], 0, "shlibs:Depends=libc6", "")
        with patch.object(pkg.subprocess, "run", return_value=completed):
            self.assertEqual(pkg.run("dpkg-shlibdeps", "-O", clean_stderr=True), "shlibs:Depends=libc6")

    def test_sysroot_libraries_cannot_fall_back_or_escape_through_symlinks(self):
        root = self.root / "isolated"
        libraries = root / "usr/lib"
        libraries.mkdir(parents=True)
        elf = {"sniper": {"needed": ["libc.so.6"]}}
        binaries = {"sniper": self.binaries / "sniper"}
        with self.assertRaisesRegex(ValueError, "host fallback is not allowed"):
            pkg.check_sysroot_libraries(root, [libraries], binaries, elf)
        outside = self.root / "outside-libc.so.6"
        outside.write_bytes(b"not a system library")
        link = libraries / "libc.so.6"
        link.symlink_to(outside)
        with self.assertRaisesRegex(ValueError, "escapes its root"):
            pkg.check_sysroot_libraries(root, [libraries], binaries, elf)
        link.unlink()
        header = bytearray(64)
        header[:7] = b"\x7fELF\x02\x01\x01"
        struct.pack_into("<HH", header, 16, 3, 183)
        link.write_bytes(header)
        with self.assertRaisesRegex(ValueError, "not an x86_64"):
            pkg.check_sysroot_libraries(root, [libraries], binaries, elf)
        struct.pack_into("<HH", header, 16, 3, 62)
        link.write_bytes(header)
        pkg.check_sysroot_libraries(root, [libraries], binaries, elf)

    def test_extracted_mode_rejects_active_host_dependency_overrides(self):
        config = self.root / "host-config"
        config.mkdir()
        override = config / "shlibs.override"
        override.write_text("# default comment\n\n")
        pkg.check_host_overrides(config)
        override.write_text("libc 6 unrelated-host-package\n")
        with self.assertRaisesRegex(ValueError, "active host dependency overrides"):
            pkg.check_host_overrides(config)
        override.write_text("# no active override\n")
        (config / "symbols").mkdir()
        (config / "symbols/libc6:amd64.symbols").write_text("host override")
        with self.assertRaisesRegex(ValueError, "host symbol override"):
            pkg.check_host_overrides(config)

    def test_tar_verifier_rejects_corruption_symlinks_and_duplicate_files(self):
        expected = {"bin/sniper": hashlib.sha256(b"fixture").hexdigest()}

        def payload(kind):
            result = io.BytesIO()
            with tarfile.open(fileobj=result, mode="w") as tar:
                entry = tarfile.TarInfo("bin/sniper")
                entry.uid = entry.gid = 0
                entry.mode = 0o755
                value = b"corrupt" if kind == "corrupt" else b"fixture"
                entry.size = len(value)
                if kind == "symlink":
                    entry.type = tarfile.SYMTYPE
                    entry.linkname = "/usr/bin/unrelated"
                    tar.addfile(entry)
                else:
                    tar.addfile(entry, io.BytesIO(value))
                    if kind == "duplicate":
                        tar.addfile(entry, io.BytesIO(value))
            result.seek(0)
            return result

        for kind, error in (("corrupt", "checksum mismatch"), ("symlink", "symlink"), ("duplicate", "duplicate")):
            with self.assertRaisesRegex(ValueError, error):
                pkg.verify_tar(payload(kind), expected)

    def test_failure_keeps_prior_files_and_home_untouched(self):
        home = self.root / "home"
        (home / ".sniper").mkdir(parents=True)
        (home / ".sniper/sentinel").write_text("synthetic session")
        (home / ".profile").write_text("unchanged shell profile")
        before = pkg.file_manifest(home)
        self.args.test_fixtures = False
        self.rewrite_record(lambda record: record.update(test_fixture=False))
        with patch.dict(os.environ, {"HOME": str(home)}):
            with self.assertRaises(ValueError):
                self.package()
        self.assertEqual(pkg.file_manifest(home), before)
        self.assertFalse(Path(self.args.output_dir).exists())

    def test_desktop_and_ci_contracts_do_not_claim_native_gui_qualification(self):
        desktop = (ROOT / "packaging/linux/sniper.desktop").read_text()
        for line in ("Exec=sniper-desktop", "TryExec=sniper-desktop", "Icon=sniper", "Terminal=false", "Type=Application"):
            self.assertIn(line + "\n", desktop)
        for forbidden in ("sudo", "pkexec", "env ", "--no-sandbox"):
            self.assertNotIn(forbidden, desktop)
        workflow = (ROOT / ".github/workflows/linux.yml").read_text()
        self.assertIn("runs-on: ubuntu-24.04", workflow)
        self.assertIn("image: rust:1.99.0-trixie", workflow)
        self.assertIn("contents: read", workflow)
        self.assertIn("actions/setup-node@v6", workflow)
        self.assertIn("node-version: '24'", workflow)
        self.assertIn("cargo test --locked --release", workflow)
        self.assertIn("cargo build --locked --release --bins", workflow)
        runtime = workflow.split("  runtime-debian13-x86_64:", 1)[1]
        self.assertNotIn("\n    container:", runtime)
        self.assertIn("docker run --rm --pull=never --platform linux/amd64", runtime)
        self.assertEqual(runtime.count("--mount "), 3)
        for mount in ("dst=/qa/input,readonly", "dst=/qa/scripts,readonly", "dst=/qa/results"):
            self.assertIn(mount, runtime)
        for forbidden in ("docker.sock", "--privileged", "--security-opt", "--cap-add", "--env-file"):
            self.assertNotIn(forbidden, runtime)
        harness = (ROOT / "packaging/linux/qualification/qualify_ci_lifecycle.py").read_text()
        self.assertIn('("purged", "--purge")', harness)
        self.assertIn('command("apt-get", "install", "--no-install-recommends", "--no-remove"', harness)
        self.assertIn("verify_loader_closures(inventory, command, home, custom_data)", harness)
        for forbidden in ("gh release", "contents: write", "git push", "--no-sandbox", "WEBKIT_DISABLE_SANDBOX"):
            self.assertNotIn(forbidden, workflow)


class NativeDependencyRegressionTests(unittest.TestCase):
    @unittest.skipUnless(all(os.environ.get(name) for name in (
        "SNIPER_PACKAGING_NATIVE_BINARIES", "SNIPER_PACKAGING_SYSROOT", "SNIPER_PACKAGING_DPKG_ADMINDIR")),
        "set the three SNIPER_PACKAGING_* paths to run the real ELF/private-database regression")
    def test_missing_required_native_libc_symbol_fails_closed(self):
        original = pkg.inputs(os.environ["SNIPER_PACKAGING_NATIVE_BINARIES"])
        sysroot = pkg.safe_path(os.environ["SNIPER_PACKAGING_SYSROOT"])
        database = pkg.safe_path(os.environ["SNIPER_PACKAGING_DPKG_ADMINDIR"])
        symbols = database / "info/libc6:amd64.symbols"
        before_symbols = pkg.digest(symbols)
        before_binaries = {name: pkg.digest(path) for name, path in original.items()}
        with tempfile.TemporaryDirectory(prefix="sniper-native-dependency-regression-", dir="/tmp") as temporary:
            root = Path(temporary)
            stage = root / "stage"
            (stage / "DEBIAN").mkdir(parents=True)
            staged = {}
            for name, path in original.items():
                staged[name] = stage / "usr/bin" / name
                pkg.copy_file(path, staged[name], True)
            baseline = root / "baseline"
            baseline.mkdir()
            diagnostics = io.StringIO()
            with contextlib.redirect_stderr(diagnostics):
                depends = pkg.deb_dependencies(baseline, staged, sysroot, database)
            self.assertEqual(diagnostics.getvalue(), "")
            self.assertIn("libc6", depends)
            self.assertIn("libwebkit2gtk-4.1-0", depends)
            private = root / "private-db"
            shutil.copytree(database, private)
            damaged = private / "info/libc6:amd64.symbols"
            lines = damaged.read_text().splitlines(keepends=True)
            removed = [line for line in lines if line.startswith(" __libc_start_main@GLIBC_2.34 ")]
            self.assertEqual(len(removed), 1, "expected exactly one required libc symbol in official metadata")
            damaged.write_text("".join(line for line in lines if line not in removed))
            negative = root / "negative"
            negative.mkdir()
            diagnostics = io.StringIO()
            with contextlib.redirect_stderr(diagnostics):
                with self.assertRaisesRegex(ValueError, "dependency tooling emitted diagnostics"):
                    pkg.deb_dependencies(negative, staged, sysroot, private)
            self.assertIn("__libc_start_main", diagnostics.getvalue())
        self.assertEqual(pkg.digest(symbols), before_symbols)
        self.assertEqual({name: pkg.digest(path) for name, path in original.items()}, before_binaries)


if __name__ == "__main__":
    unittest.main()
