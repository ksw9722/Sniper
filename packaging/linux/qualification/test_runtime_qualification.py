"""Pure QA mocks plus an inert private-root dpkg filter test; no app execution."""
from pathlib import Path
import io
import json
import tarfile
import subprocess
import tempfile
import sys
import unittest
from unittest.mock import Mock, patch

sys.path.insert(0, str(Path(__file__).parent))
import qualify_ci_lifecycle as qa

MOUNTS = """1 0 0:1 / / rw - overlay overlay rw
2 1 8:1 /input /qa/input ro - ext4 /dev/sda rw
3 1 8:1 /scripts /qa/scripts ro - ext4 /dev/sda rw
4 1 8:1 /results /qa/results rw - ext4 /dev/sda rw
5 1 0:2 / /proc rw - proc proc rw
6 1 8:1 /hosts /etc/hosts rw - ext4 /dev/sda rw
"""
ENV = {"SNIPER_PACKAGE_QA_MODE": "explicit-docker-runtime", "RUNNER_ENVIRONMENT": "github-hosted", "GITHUB_RUN_ID": "123", "GITHUB_SHA": "a" * 40, "SNIPER_QA_IMAGE": "debian@sha256:" + "b" * 64}


class BoundaryTests(unittest.TestCase):
    def check(self, **changes):
        arguments = {"environment": ENV, "distro": {"ID": "debian", "VERSION_ID": "13"}, "machine": "x86_64", "uid": 0, "docker_marker": True, "socket_present": False, "mountinfo": MOUNTS}
        arguments.update(changes)
        qa.validate_boundary(**arguments)

    def test_exact_boundary_passes(self):
        self.check()

    def test_reject_environment_identity_failures(self):
        for key in ENV:
            with self.subTest(key=key), self.assertRaises(ValueError):
                self.check(environment={**ENV, key: ""})
        for key in ("DPKG_ROOT", "DPKG_FORCE", "LD_PRELOAD", "LD_AUDIT", "DOCKER_HOST"):
            with self.subTest(key=key), self.assertRaises(ValueError):
                self.check(environment={**ENV, key: "unexpected"})

    def test_reject_wrong_system_user_marker_and_socket(self):
        for change in ({"socket_present": True}, {"docker_marker": False}, {"uid": 1000}, {"machine": "aarch64"}, {"distro": {"ID": "ubuntu", "VERSION_ID": "24.04"}}):
            with self.subTest(change=change), self.assertRaises(ValueError):
                self.check(**change)

    def test_reject_writable_input_script_readonly_results_and_wrong_root(self):
        variants = (MOUNTS.replace("/qa/input ro", "/qa/input rw"), MOUNTS.replace("/qa/scripts ro", "/qa/scripts rw"), MOUNTS.replace("/qa/results rw", "/qa/results ro"), MOUNTS.replace("- overlay overlay", "- ext4 /dev/sda"), "\n".join(MOUNTS.splitlines()[1:]))
        for mountinfo in variants:
            with self.subTest(mountinfo=mountinfo), self.assertRaises(ValueError):
                self.check(mountinfo=mountinfo)

    def test_reject_unexpected_host_or_socket_mounts(self):
        for point in ("/host", "/usr", "/home", "/var/run/docker.sock", "/run/docker.sock"):
            with self.subTest(point=point), self.assertRaises(ValueError):
                self.check(mountinfo=MOUNTS + f"7 1 8:1 /host {point} rw - ext4 /dev/sda rw\n")


class LifecycleSequenceTests(unittest.TestCase):
    def callbacks(self):
        return {"deb": Path("/qa/input/inert.deb"), "pinned_hash": "a" * 64, "current_hash": Mock(return_value="a" * 64), "command": Mock(), "installed": Mock(), "absent": Mock(), "clean_audit": Mock(), "package_snapshot": Mock(return_value=["libc6 installed", "new-runtime-dependency installed"]), "record_stage": Mock()}

    def test_exact_sequence_accepts_post_apt_dependency_baseline(self):
        cb = self.callbacks()
        qa.lifecycle_sequence(**cb)
        commands = [call.args for call in cb["command"].call_args_list]
        self.assertEqual(commands, [
            ("apt-get", "install", "--no-install-recommends", "--no-remove", "-y", *qa.APT_DOC_OPTIONS, str(cb["deb"])),
            ("dpkg", "--remove", "sniper"),
            ("dpkg", *qa.DPKG_DOC_OPTIONS, "--install", str(cb["deb"])),
            ("dpkg", "--purge", "sniper"),
        ])
        self.assertEqual([call.args[0] for call in cb["record_stage"].call_args_list], ["apt-installed", "removed", "reinstalled", "purged"])
        self.assertEqual(cb["installed"].call_count, 2)
        self.assertEqual(cb["absent"].call_count, 3)

    def test_initial_absence_guard_blocks_before_apt(self):
        cb = self.callbacks()
        cb["absent"].side_effect = ValueError("pre-existing package or payload")
        with self.assertRaises(ValueError):
            qa.lifecycle_sequence(**cb)
        cb["command"].assert_not_called()

    def test_changed_input_blocks_before_and_between_mutations(self):
        for hashes, expected_calls in ((["b" * 64], 0), (["a" * 64, "b" * 64], 1)):
            cb = self.callbacks()
            cb["current_hash"].side_effect = hashes
            with self.subTest(hashes=hashes), self.assertRaises(ValueError):
                qa.lifecycle_sequence(**cb)
            self.assertEqual(cb["command"].call_count, expected_calls)

    def test_apt_failure_never_repairs_or_removes(self):
        cb = self.callbacks()
        cb["command"].side_effect = ValueError("APT dependency failure")
        with self.assertRaises(ValueError):
            qa.lifecycle_sequence(**cb)
        self.assertEqual(cb["command"].call_count, 1)
        cb["record_stage"].assert_not_called()

    def test_installed_validation_failures_stop_without_cleanup(self):
        for problem in ("hash", "ownership", "status", "synthetic data", "loader closure"):
            cb = self.callbacks()
            cb["installed"].side_effect = ValueError(problem)
            with self.subTest(problem=problem), self.assertRaises(ValueError):
                qa.lifecycle_sequence(**cb)
            self.assertEqual(cb["command"].call_count, 1)
            cb["record_stage"].assert_not_called()

    def test_removal_or_sentinel_failure_stops_before_reinstall(self):
        cb = self.callbacks()
        cb["absent"].side_effect = [None, ValueError("payload remains or synthetic data changed")]
        with self.assertRaises(ValueError):
            qa.lifecycle_sequence(**cb)
        self.assertEqual(cb["command"].call_count, 2)
        self.assertEqual(cb["record_stage"].call_count, 1)

    def test_unrelated_package_delta_stops_after_remove(self):
        cb = self.callbacks()
        cb["package_snapshot"].side_effect = [["baseline"], ["changed"]]
        with self.assertRaisesRegex(ValueError, "unrelated"):
            qa.lifecycle_sequence(**cb)
        self.assertEqual(cb["command"].call_count, 2)

    def test_audit_or_recorder_failure_stops(self):
        for key, count in (("clean_audit", 2), ("record_stage", 1)):
            cb = self.callbacks()
            cb[key].side_effect = ValueError(key)
            with self.subTest(key=key), self.assertRaises(ValueError):
                qa.lifecycle_sequence(**cb)
            self.assertEqual(cb["command"].call_count, count)




# Verbatim stdout of CI 38349602193 evidence/011-command.json.
CAPTURED_LISTFILES = '/.\n/usr\n/usr/bin\n/usr/bin/sniper\n/usr/bin/sniper-cli\n/usr/bin/sniper-desktop\n/usr/share\n/usr/share/applications\n/usr/share/applications/sniper.desktop\n/usr/share/doc\n/usr/share/doc/sniper\n/usr/share/doc/sniper/LICENSE\n/usr/share/doc/sniper/README.md\n/usr/share/doc/sniper/SHA256SUMS\n/usr/share/doc/sniper/build-manifest.json\n/usr/share/icons\n/usr/share/icons/hicolor\n/usr/share/icons/hicolor/256x256\n/usr/share/icons/hicolor/256x256/apps\n/usr/share/icons/hicolor/256x256/apps/sniper.png\n'


class InstalledInventoryTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="sniper-installed-paths-", dir="/tmp")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.listed = CAPTURED_LISTFILES.splitlines()
        for row in self.listed:
            path = self.root / row.lstrip("/")
            if row.lstrip("/") in qa.EXPECTED:
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text("inert owned payload fixture\n")
            else:
                path.mkdir(parents=True, exist_ok=True)

    def inspect(self, listed=None):
        return qa.installed_inventory_evidence(self.listed if listed is None else listed,
            lambda path: qa.installed_path_type(self.root / str(path).lstrip("/")))

    def test_exact_ci_list_with_all_nine_regular_files_passes(self):
        observed = self.inspect()
        self.assertEqual(set(observed["actual_files"]), qa.EXPECTED)
        self.assertEqual((observed["missing"], observed["unexpected"]), ([], []))
        self.assertEqual(observed["path_types"]["/."], "directory")
        self.assertEqual(len(qa.EXPECTED), 9)

    def test_full_metadata_list_with_slim_missing_docs_still_fails(self):
        for name in qa.DOC_PATHS:
            (self.root / name.lstrip("/")).unlink()
        observed = self.inspect()
        self.assertEqual(observed["listed"], self.listed)
        self.assertEqual(observed["missing"], sorted(path.lstrip("/") for path in qa.DOC_PATHS))
        self.assertEqual(observed["unexpected"], [])
        self.assertNotEqual(set(observed["actual_files"]), qa.EXPECTED)

    def test_missing_binary_or_directory_in_place_of_file_remains_fatal(self):
        path = self.root / "usr/bin/sniper"
        path.unlink()
        for replacement in ("missing", "directory"):
            if replacement == "directory":
                path.mkdir()
            observed = self.inspect()
            self.assertIn("usr/bin/sniper", observed["missing"])
            self.assertEqual(observed["path_types"]["/usr/bin/sniper"], replacement)

    def test_extra_file_and_special_paths_are_not_ignored(self):
        extra = self.root / "usr/bin/unexpected"
        extra.write_text("inert extra\n")
        observed = self.inspect(self.listed + ["/usr/bin/unexpected"])
        self.assertEqual(observed["unexpected"], ["usr/bin/unexpected"])
        special = qa.installed_inventory_evidence(["/usr/bin/unexpected"], lambda path: "special")
        self.assertEqual(special["unexpected"], ["usr/bin/unexpected"])

    def test_payload_symlink_remains_rejected_by_existing_plain_path_guard(self):
        path = self.root / "usr/bin/sniper"
        path.unlink()
        path.symlink_to(self.root / "usr/bin/sniper-cli")
        self.assertEqual(self.inspect()["path_types"]["/usr/bin/sniper"], "symlink")
        with self.assertRaisesRegex(ValueError, "symlink"):
            qa.plain_path(path)

    def test_real_dpkg_doc_filter_order_and_parent_directories_in_private_roots(self):
        # Text-only package, no scripts/triggers/conffiles or executable payload.
        # Keep the current UID/GID; root-owned installation is the real CI gate.
        # This never uses the host package database or installation directory.
        stage = self.root / "fixture-stage"
        (stage / "DEBIAN").mkdir(parents=True)
        (stage / "DEBIAN/control").write_text(
            "Package: sniper-docfilter-fixture\nVersion: 1\nArchitecture: amd64\n"
            "Maintainer: Test <test@example.com>\nDescription: inert doc filter fixture\n")
        for relative in (*sorted(qa.EXPECTED), "usr/share/doc/unrelated/README"):
            path = stage / relative
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text("INERT TEXT ONLY; never execute\n")
            path.chmod(0o644)
        deb = self.root / "inert.deb"
        env = {key: value for key, value in qa.ENV.items() if not key.startswith("DPKG_")}
        home = self.root / "synthetic-home"
        home.mkdir()
        env.update(HOME=str(home), LC_ALL="C", DPKG_COLORS="never", DPKG_NLS="0",
                   PATH="/usr/sbin:/usr/bin:/sbin:/bin")
        # Refuse an unexpected ambient command hook rather than execute it.
        configs = [Path("/etc/dpkg/dpkg.cfg"), *Path("/etc/dpkg/dpkg.cfg.d").glob("*")]
        for config in configs:
            if config.is_file():
                self.assertFalse(any(line.lstrip().startswith(("pre-invoke", "post-invoke"))
                                     for line in config.read_text().splitlines()))
        subprocess.run(("dpkg-deb", "--build", str(stage), str(deb)),
                       env=env, capture_output=True, text=True, check=True)
        controls = subprocess.check_output(("dpkg-deb", "--ctrl-tarfile", str(deb)), env=env)
        with tarfile.open(fileobj=io.BytesIO(controls)) as archive:
            self.assertEqual({item.name.removeprefix("./") for item in archive if item.isfile()}, {"control"})
        # Exact relevant filter pair from the pinned official slim image.
        slim = ("--path-exclude=/usr/share/doc/*", "--path-include=/usr/share/doc/*/copyright")
        evidence = []
        for phase, options, missing in (
            ("slim", slim, sorted(path.lstrip("/") for path in qa.DOC_PATHS)),
            ("included", (*slim, *qa.DPKG_DOC_OPTIONS), []),
            ("wrong-order", (*qa.DPKG_DOC_OPTIONS, *slim), sorted(path.lstrip("/") for path in qa.DOC_PATHS)),
        ):
            owned = self.root / phase
            install, database, log = owned / "root", owned / "db", owned / "dpkg.log"
            install.mkdir(parents=True)
            for name in ("info", "updates", "triggers"):
                (database / name).mkdir(parents=True, exist_ok=True)
            (database / "status").write_text("")
            argv = ("dpkg", "--force-not-root", "--instdir=" + str(install),
                    "--admindir=" + str(database), "--log=" + str(log), *options, "--unpack", str(deb))
            self.assertTrue(all(path.is_relative_to(self.root) for path in (install, database, log, deb)))
            self.assertIn("--instdir=" + str(install), argv)
            self.assertIn("--admindir=" + str(database), argv)
            result = subprocess.run(argv, env=env, capture_output=True, text=True, check=False)
            self.assertEqual(result.returncode, 0, result.stderr)
            listed = subprocess.run(("dpkg-query", "--admindir=" + str(database), "--listfiles", "sniper-docfilter-fixture"),
                                    env=env, capture_output=True, text=True, check=True).stdout.splitlines()
            observed = qa.installed_inventory_evidence(listed,
                lambda path: qa.installed_path_type(install / str(path).lstrip("/")))
            self.assertEqual(observed["missing"], missing)
            self.assertEqual(observed["unexpected"], [])
            self.assertTrue(all("/" + name in listed for name in qa.EXPECTED))
            self.assertFalse((install / "usr/share/doc/unrelated/README").exists())
            if phase == "included":
                self.assertTrue((install / "usr/share/doc/sniper").is_dir())
                self.assertEqual(set(observed["actual_files"]), qa.EXPECTED)
            evidence.append({"phase": phase, "argv": argv, "exit_code": result.returncode,
                             "missing": observed["missing"], "unexpected": observed["unexpected"]})
        print("Private inert dpkg filter evidence: " + json.dumps(evidence, sort_keys=True))

    def test_per_operation_options_are_four_exact_doc_files_only(self):
        self.assertEqual(set(qa.DOC_PATHS), {"/usr/share/doc/sniper/" + name for name in
                        ("LICENSE", "README.md", "SHA256SUMS", "build-manifest.json")})
        self.assertEqual(qa.DPKG_DOC_OPTIONS, tuple("--path-include=" + path for path in qa.DOC_PATHS))
        self.assertEqual(qa.APT_DOC_OPTIONS, tuple(part for option in qa.DPKG_DOC_OPTIONS
                         for part in ("-o", "DPkg::Options::=" + option)))
        self.assertFalse(any("*" in option or "force" in option for option in qa.DPKG_DOC_OPTIONS))

    def test_apt_config_preserves_all_four_forwarded_dpkg_options(self):
        # apt-config only parses/prints options; no package action is invoked.
        result = subprocess.run(("apt-config", *qa.APT_DOC_OPTIONS, "dump"),
                                env=qa.ENV, capture_output=True, text=True, check=True)
        for option in qa.DPKG_DOC_OPTIONS:
            self.assertEqual(result.stdout.count('DPkg::Options:: "' + option + '";'), 1)

    def test_filter_evidence_preserves_config_and_records_narrow_overrides(self):
        config = self.root / "etc/dpkg"
        (config / "dpkg.cfg.d").mkdir(parents=True)
        path = config / "dpkg.cfg.d/docker"
        original = "# Debian slim\npath-exclude /usr/share/doc/*\npath-include /usr/share/doc/*/copyright\n"
        path.write_text(original)
        observed = qa.path_filter_evidence(config)
        self.assertEqual(observed["per_install_options"], list(qa.DPKG_DOC_OPTIONS))
        self.assertEqual(observed["observed_configs"][str(path)]["path_filters"], original.splitlines()[1:])
        self.assertEqual(path.read_text(), original)
        self.assertEqual(list((config / "dpkg.cfg.d").iterdir()), [path])


class OverrideListingTests(unittest.TestCase):
    def exercise(self, stat_result=(1, "", ""), divert_result=(0, "", "")):
        calls = []
        def command(*argv, allowed=(0,)):
            calls.append((argv, allowed))
            code, output, error = stat_result if argv[0] == "dpkg-statoverride" else divert_result
            qa.require(code in allowed, "command failed")
            return subprocess.CompletedProcess(argv, code, output, error)
        qa.check_candidate_overrides(command)
        return calls

    def test_exact_ci_empty_result_is_accepted_only_for_statoverride(self):
        # CI run 38347689220, evidence/006-command.json: 1, "", "".
        self.assertEqual(self.exercise(), [(("dpkg-divert", "--list"), (0,)),
                                          (("dpkg-statoverride", "--list"), (0, 1))])
        with self.assertRaisesRegex(ValueError, "command failed"):
            self.exercise(divert_result=(1, "", ""))

    def test_nonempty_successful_lists_keep_candidate_path_checks(self):
        self.exercise(stat_result=(0, "root root 755 /usr/bin/unrelated\n", ""),
                      divert_result=(0, "diversion of /usr/bin/unrelated to /usr/bin/unrelated.old by example\n", ""))
        for path in qa.EXPECTED:
            with self.subTest(path=path, tool="statoverride"), self.assertRaisesRegex(ValueError, "candidate path"):
                self.exercise(stat_result=(0, "root root 755 /" + path + "\n", ""))
            with self.subTest(path=path, tool="divert"), self.assertRaisesRegex(ValueError, "candidate path"):
                self.exercise(divert_result=(0, "diversion of /" + path + " to /old by example\n", ""))

    def test_error_diagnostic_and_contradictory_empty_status_are_rejected(self):
        for result in ((2, "", ""), (2, "", "fatal\n"), (1, "unexpected\n", ""),
                       (1, "\n", ""), (1, "", "diagnostic\n"), (1, "", " "),
                       (0, "root root 755 /usr/bin/unrelated\n", "diagnostic\n")):
            with self.subTest(result=result), self.assertRaises(ValueError):
                self.exercise(stat_result=result)

    def test_empty_private_database_real_readonly_contract(self):
        with tempfile.TemporaryDirectory(prefix="sniper-statoverride-readonly-", dir="/tmp") as directory:
            database = Path(directory)
            before = database.stat()
            env = {key: value for key, value in qa.ENV.items() if not key.startswith("DPKG_")}
            env.update(LC_ALL="C", DPKG_COLORS="never", DPKG_NLS="0")
            result = subprocess.run(("dpkg-statoverride", "--admindir", str(database), "--list"),
                                    text=True, capture_output=True, env=env, check=False)
            self.assertEqual((result.returncode, result.stdout, result.stderr), (1, "", ""))
            after = database.stat()
            self.assertEqual(list(database.iterdir()), [])
            self.assertEqual((before.st_mode, before.st_uid, before.st_gid, before.st_mtime_ns),
                             (after.st_mode, after.st_uid, after.st_gid, after.st_mtime_ns))
            self.exercise(stat_result=(result.returncode, result.stdout, result.stderr))


class LoaderDiagnosticTests(unittest.TestCase):
    def exercise(self, bad_output=False, ownership_problem=None):
        calls = []
        loader = "/lib64/ld-linux-x86-64.so.2"
        inventory = {"elf": {name: {"interpreter": loader, "needed": ["libc.so.6"]} for name in ("sniper", "sniper-cli", "sniper-desktop")}}
        def resolve(path, *args, **kwargs):
            return Path("/usr/lib/x86_64-linux-gnu") / path.name
        def command(*argv, allowed=(0,)):
            calls.append(argv)
            value = ""
            if argv[:2] == ("dpkg-query", "--search"):
                value = "libc6:amd64: " + argv[-1] + "\n"
                if argv[-1] == loader or ownership_problem == "diversion":
                    value = ("diversion by libc6 from: /lib64/ld-linux-x86-64.so.2\n"
                             "diversion by libc6 to: /lib64/ld-linux-x86-64.so.2.usr-is-merged\n") + value
                elif ownership_problem == "wrong-file":
                    value = "libc6:amd64: /usr/lib/unrelated.so\n"
                elif ownership_problem == "multiple-owners":
                    value += "other: " + argv[-1] + "\n"
                elif ownership_problem == "duplicate":
                    value += value
                elif ownership_problem == "unknown":
                    value += "unknown output\n"
            elif argv[:2] == ("dpkg-query", "--show"):
                value = "install ok unpacked" if ownership_problem == "unpacked" else "install ok installed"
            elif "--program-headers" in argv:
                value = "[Requesting program interpreter: " + loader + "]\n"
            elif "--dynamic" in argv:
                value = "(NEEDED) Shared library: [libc.so.6]\n"
            elif "--list" in argv:
                value = "libc.so.6 => /lib/x86_64-linux-gnu/libc.so.6 (0xabc)\n" + loader + " (0xdef)\nlinux-vdso.so.1 (0x123)\n"
                if bad_output:
                    value += "missing.so => not found\n"
            return subprocess.CompletedProcess(argv, 0, value, "diagnostic\n" if ownership_problem == "stderr" else "")
        with patch.object(Path, "resolve", resolve), patch.object(Path, "is_file", return_value=True), patch.object(Path, "exists", return_value=False), patch.object(qa.subprocess, "run", side_effect=AssertionError("no process execution allowed")):
            qa.verify_loader_closures(inventory, command, Path("/synthetic-home"), Path("/synthetic-data"))
        return calls

    def test_all_three_binaries_use_only_loader_diagnostics(self):
        calls = self.exercise()
        self.assertEqual({row[-1] for row in calls if "--list" in row}, {"/usr/bin/sniper", "/usr/bin/sniper-cli", "/usr/bin/sniper-desktop"})
        self.assertFalse(any(row[0].startswith("/usr/bin/sniper") for row in calls))
        self.assertTrue(all(row[0:2] == ("env", "-i") for row in calls if "--list" in row or "--verify" in row))

    def test_official_diverted_alias_uses_only_canonical_ownership(self):
        calls = self.exercise()
        searches = [row for row in calls if row[:2] == ("dpkg-query", "--search")]
        self.assertTrue(searches)
        self.assertTrue(all(row[2] == "--" and row[-1].startswith("/usr/lib/") for row in searches))
        self.assertFalse(any(row[-1] == "/lib64/ld-linux-x86-64.so.2" for row in searches))

    def test_canonical_ownership_diagnostics_or_ambiguity_are_fatal(self):
        for problem in ("diversion", "wrong-file", "multiple-owners", "duplicate", "unknown", "stderr", "unpacked"):
            with self.subTest(problem=problem), self.assertRaises(ValueError):
                self.exercise(ownership_problem=problem)

    def test_missing_library_is_rejected(self):
        with self.assertRaisesRegex(ValueError, "loader dependency"):
            self.exercise(bad_output=True)


if __name__ == "__main__":
    unittest.main()
