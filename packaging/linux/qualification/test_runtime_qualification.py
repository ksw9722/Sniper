"""Pure boundary/sequence/loader mocks. No APT, dpkg lifecycle or app execution."""
from pathlib import Path
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
        self.assertEqual([call.args[:2] for call in cb["command"].call_args_list], [("apt-get", "install"), ("dpkg", "--remove"), ("dpkg", "--install"), ("dpkg", "--purge")])
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
