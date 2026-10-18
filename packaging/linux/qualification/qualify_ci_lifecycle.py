#!/usr/bin/env python3
"""REVIEW DRAFT: APT runtime + dpkg lifecycle in an explicit socket-free container.

This script DOES mutate the container's /usr and /var/lib/dpkg. Guards prevent
ordinary accidental use; the reviewed container job is the isolation boundary.
It never starts Sniper, accesses the GUI sysroot, or cleans
up with shell deletion. A failed run is preserved; discard the whole CI job.
"""
import argparse
import json
import os
from pathlib import Path
import platform
import re
import stat
import subprocess
import sys
from datetime import datetime, timezone

sys.dont_write_bytecode = True
from inspect_candidate import EXPECTED, ENV, inspect, plain_path, require, sha

ACK = "I approve exact-candidate APT runtime installation and package lifecycle in this explicit disposable Debian 13 container"
# Debian slim excludes /usr/share/doc/*. Re-include only our four hashed
# payload files, for these install commands, without editing dpkg config.
DOC_PATHS = tuple("/usr/share/doc/sniper/" + name for name in
                  ("LICENSE", "README.md", "SHA256SUMS", "build-manifest.json"))
DPKG_DOC_OPTIONS = tuple("--path-include=" + path for path in DOC_PATHS)
APT_DOC_OPTIONS = tuple(part for option in DPKG_DOC_OPTIONS for part in ("-o", "DPkg::Options::=" + option))




def validate_boundary(environment, distro, machine, uid, docker_marker, socket_present, mountinfo):
    require(environment.get("SNIPER_PACKAGE_QA_MODE") == "explicit-docker-runtime", "explicit Docker runtime mode required")
    require(environment.get("RUNNER_ENVIRONMENT") == "github-hosted", "GitHub-hosted parent runner required")
    require(re.fullmatch(r"[0-9]+", environment.get("GITHUB_RUN_ID", "")), "missing workflow run identity")
    require(re.fullmatch(r"[0-9a-f]{40}", environment.get("GITHUB_SHA", "")), "missing source revision identity")
    require(re.fullmatch(r"debian@sha256:[0-9a-f]{64}", environment.get("SNIPER_QA_IMAGE", "")), "official Debian image digest required")
    require(docker_marker and uid == 0 and not socket_present, "requires socket-free Docker container root")
    require(distro.get("ID") == "debian" and distro.get("VERSION_ID") == "13" and machine == "x86_64", "Debian 13 x86_64 only")
    mounts = {}
    for row in mountinfo.splitlines():
        left, right = row.split(" - ", 1)
        parts, fs = left.split(), right.split()[0]
        point, options = parts[4], set(parts[5].split(","))
        require("\\" not in point, "escaped mount path requires review")
        require(point not in mounts, "duplicate mountpoint requires review")
        mounts[point] = options
        if point == "/":
            require(fs == "overlay", "root must be the explicit container overlay")
        elif point in ("/qa/input", "/qa/scripts", "/qa/results", "/etc/hosts", "/etc/hostname", "/etc/resolv.conf"):
            pass
        else:
            require(fs in {"proc", "sysfs", "cgroup", "cgroup2", "devpts", "tmpfs", "mqueue"} and (point in {"/proc", "/sys", "/dev"} or point.startswith(("/proc/", "/sys/", "/dev/"))), "unexpected data mount: " + point)
    require("/" in mounts, "missing container root mount")
    for point, flag in (("/qa/input", "ro"), ("/qa/scripts", "ro"), ("/qa/results", "rw")):
        require(flag in mounts.get(point, set()), "missing required mount boundary: " + point)
    for key in ("DPKG_ROOT", "DPKG_ADMINDIR", "DPKG_DATADIR", "DPKG_FORCE", "LD_PRELOAD", "LD_LIBRARY_PATH", "LD_AUDIT", "DOCKER_HOST", "DOCKER_CONTEXT"):
        require(not environment.get(key), "ambient override requires review: " + key)


def lifecycle_sequence(deb, pinned_hash, current_hash, command, installed, absent,
                       clean_audit, package_snapshot, record_stage):
    """Callbacks permit pure mocked sequence/failure tests, with no installer run."""
    absent()
    require(current_hash() == pinned_hash, "candidate changed before initial APT install")
    command("apt-get", "install", "--no-install-recommends", "--no-remove", "-y", *APT_DOC_OPTIONS, str(deb))
    installed()
    baseline = package_snapshot()
    record_stage("apt-installed")
    for stage, action in (("removed", "--remove"), ("reinstalled", "--install"), ("purged", "--purge")):
        require(current_hash() == pinned_hash, "candidate changed during qualification")
        command("dpkg", *(DPKG_DOC_OPTIONS if action == "--install" else ()),
                action, str(deb) if action == "--install" else "sniper")
        installed() if action == "--install" else absent()
        clean_audit()
        require(package_snapshot() == baseline, "unrelated package states changed")
        record_stage(stage)





def path_filter_evidence(config=Path("/etc/dpkg")):
    files = {}
    for path in [config / "dpkg.cfg", *sorted((config / "dpkg.cfg.d").glob("*"))]:
        if path.is_file():
            files[str(path)] = {"sha256": sha(path), "path_filters": [line for line in path.read_text().splitlines()
                                if re.match(r"^\s*path-(?:include|exclude)(?:\s|=)", line)]}
    return {"observed_configs": files, "per_install_options": list(DPKG_DOC_OPTIONS),
            "scope": "include only four Sniper documentation payload files; inherited configuration is unchanged"}


def installed_path_type(path):
    try:
        mode = path.lstat().st_mode
    except FileNotFoundError:
        return "missing"
    return ("regular" if stat.S_ISREG(mode) else "directory" if stat.S_ISDIR(mode) else
            "symlink" if stat.S_ISLNK(mode) else "special")


def installed_inventory_evidence(listed, path_type=installed_path_type):
    types = {path: path_type(Path(path)) for path in listed}
    actual = {path.lstrip("/") for path, kind in types.items() if kind not in {"directory", "missing"}}
    return {"listed": listed, "path_types": types, "actual_files": sorted(actual),
            "missing": sorted(EXPECTED - actual), "unexpected": sorted(actual - EXPECTED)}


def check_candidate_overrides(command):
    for tool in ("dpkg-divert", "dpkg-statoverride"):
        if tool == "dpkg-statoverride":
            result = command(tool, "--list", allowed=(0, 1))
            # dpkg-statoverride documents 1 for an empty --list result only.
            require(result.stderr == "" and (result.returncode == 0 or
                    (result.returncode == 1 and result.stdout == "")),
                    "unexpected statoverride listing result")
        else:
            result = command(tool, "--list")
        rows = result.stdout.splitlines()
        require(not any(any("/" + path in row for path in EXPECTED) for row in rows),
                "candidate path diversion/override requires review")


def verify_loader_closures(inventory, command, home, custom_data):
    """Inspect linked-library resolution with glibc --verify/--list; no app entry."""
    loader = Path("/lib64/ld-linux-x86-64.so.2")
    require(not Path("/etc/ld.so.preload").exists(), "unexpected system preload requires review")

    def owner(path):
        real = path.resolve(strict=True)
        require(real.is_file() and (str(real).startswith("/usr/lib/") or str(real).startswith("/lib/")), "library resolves outside distro paths")
        # Debian 13 ships libraries at canonical /usr locations. Query the
        # resolved file only: alias searches can also report DEP17 diversions.
        result = command("dpkg-query", "--search", "--", str(real))
        require(not result.stderr.strip(), "library ownership diagnostic")
        rows = result.stdout.splitlines()
        require(len(rows) == 1 and ": " in rows[0], "library must have exactly one ownership row: " + str(real))
        package, owned = rows[0].rsplit(": ", 1)
        require(owned == str(real) and re.fullmatch(r"[a-z0-9][a-z0-9+.-]*(?::amd64)?", package),
                "ambiguous library ownership")
        status = command("dpkg-query", "--show", "--showformat=${Status}", package)
        require(not status.stderr.strip() and status.stdout == "install ok installed", "library owner is not fully installed")
        return package.split(":", 1)[0]

    require(owner(loader) == "libc6", "interpreter must belong to installed libc6")
    for binary in ("sniper", "sniper-cli", "sniper-desktop"):
        path = "/usr/bin/" + binary
        programs = command("readelf", "--wide", "--program-headers", path).stdout
        expected = inventory["elf"][binary]
        require(expected["interpreter"] == str(loader) and programs.count("[Requesting program interpreter: " + str(loader) + "]") == 1, "unexpected ELF interpreter")
        dynamic = command("readelf", "--wide", "--dynamic", path).stdout
        require(not re.search(r"\((?:RPATH|RUNPATH)\)", dynamic), "unexpected dynamic search override")
        needed = sorted(set(re.findall(r"\(NEEDED\).*\[([^\]]+)\]", dynamic)))
        require(needed == expected["needed"], "ELF needed list differs from manifest")
        prefix = ("env", "-i", "PATH=/usr/sbin:/usr/bin:/sbin:/bin", "LC_ALL=C", "HOME=" + str(home), "SNIPER_DATA_DIR=" + str(custom_data), str(loader))
        verified = command(*prefix, "--verify", path)
        require(not verified.stdout.strip() and not verified.stderr.strip(), "loader verify diagnostic")
        listed = command(*prefix, "--list", path)
        require(not listed.stderr.strip() and "not found" not in listed.stdout, "loader dependency diagnostic")
        seen = set()
        for row in listed.stdout.splitlines():
            row = row.strip()
            if row.startswith("linux-vdso.so.1 ("):
                continue
            match = re.fullmatch(r"(?:(\S+) => )?(/\S+) \(0x[0-9a-f]+\)", row)
            require(match is not None, "unrecognized loader dependency output")
            soname, filename = match.groups()
            owner(Path(filename))
            seen.add(soname or Path(filename).name)
        require(set(needed).issubset(seen), "loader output is missing direct dependencies")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("candidate_directory")
    parser.add_argument("expected_manifest_sha256")
    parser.add_argument("new_results_directory")
    parser.add_argument("--approve-disposable-ci-lifecycle", required=True)
    args = parser.parse_args()
    require(args.approve_disposable_ci_lifecycle == ACK, "explicit lifecycle approval text is required")
    distro = platform.freedesktop_os_release()
    socket_present = any(Path(path).exists() or Path(path).is_symlink() for path in ("/var/run/docker.sock", "/run/docker.sock"))
    validate_boundary(os.environ, distro, platform.machine(), os.geteuid(), Path("/.dockerenv").is_file(), socket_present, Path("/proc/self/mountinfo").read_text())
    candidate = plain_path(args.candidate_directory)
    results = plain_path(args.new_results_directory)
    require(candidate == Path("/qa/input"), "candidate must be the read-only input mount")
    require(Path("/qa/results") in results.parents and not results.exists(), "new result path inside writable evidence mount required")
    require(Path(__file__).resolve().parent == Path("/qa/scripts"), "scripts must run from their read-only mount")
    inventory = inspect(candidate, args.expected_manifest_sha256)
    require(inventory["source"].get("revision") == os.environ["GITHUB_SHA"], "candidate source revision differs from this workflow")
    results.mkdir(mode=0o755)
    (results / "inventory.json").write_text(json.dumps(inventory, indent=2, sort_keys=True) + "\n")
    (results / "environment.json").write_text(json.dumps({"os_release": distro, "machine": platform.machine(), "kernel": platform.release(), "run_id": os.environ["GITHUB_RUN_ID"], "run_attempt": os.environ.get("GITHUB_RUN_ATTEMPT"), "repository": os.environ.get("GITHUB_REPOSITORY"), "git_sha": os.environ["GITHUB_SHA"], "started_utc": datetime.now(timezone.utc).isoformat(), "image_digest": os.environ["SNIPER_QA_IMAGE"], "scope": "fresh Debian slim plus validation tools; APT runtime dependencies, ELF loader diagnostics and lifecycle; no GUI launch"}, indent=2) + "\n")
    home = results / "synthetic-home"
    custom_data = results / "synthetic-custom-data"
    (home / ".sniper" / "sessions").mkdir(parents=True)
    custom_data.mkdir()
    for path in (home / ".sniper/sentinel", home / ".sniper/sessions/synthetic.json", home / ".profile", home / ".bashrc", custom_data / "sentinel"):
        path.write_text("synthetic data must remain byte-identical\n")
        path.chmod(0o600)
    env = {**ENV, "HOME": str(home), "SNIPER_DATA_DIR": str(custom_data), "SNIPER_INSTALL_AGENT_SKILLS": "0", "SNIPER_INSTALL_CLI_PATH": "0", "DEBIAN_FRONTEND": "noninteractive", "PATH": "/usr/sbin:/usr/bin:/sbin:/bin"}
    count = 0

    def command(*argv, allowed=(0,)):
        nonlocal count
        count += 1
        result = subprocess.run(argv, env=env, text=True, capture_output=True, check=False)
        record = {"argv": argv, "exit_code": result.returncode, "stdout": result.stdout, "stderr": result.stderr}
        (results / f"{count:03d}-command.json").write_text(json.dumps(record, indent=2) + "\n")
        print(" ".join(argv), "=>", result.returncode, flush=True)
        require(result.returncode in allowed, "command failed; retain evidence and discard the CI container")
        return result

    def synthetic_snapshot():
        return {str(path.relative_to(results)): {"sha256": sha(path), "mode": stat.S_IMODE(path.stat().st_mode)} for root in (home, custom_data) for path in sorted(root.rglob("*")) if path.is_file()}

    sentinels = synthetic_snapshot()
    (results / "synthetic-data-before.json").write_text(json.dumps(sentinels, indent=2) + "\n")

    def packages_except_sniper():
        value = command("dpkg-query", "--show", "--showformat=${binary:Package}\t${Version}\t${Architecture}\t${Status}\n").stdout
        return sorted(line for line in value.splitlines() if line.split("\t", 1)[0] not in {"sniper", "sniper:amd64"})

    def clean_audit():
        result = command("dpkg", "--audit")
        require(not result.stdout.strip() and not result.stderr.strip(), "dpkg audit is not clean")

    def absent():
        state = command("dpkg-query", "--show", "--showformat=${db:Status-Status}", "sniper", allowed=(0, 1))
        require(state.returncode == 1 or state.stdout == "not-installed", "Sniper still has package state")
        for name in EXPECTED:
            path = plain_path(Path("/") / name)
            require(not path.exists(), "payload remains: " + name)
        docdir = plain_path("/usr/share/doc/sniper")
        require(not docdir.exists(), "package documentation directory remains")
        require(synthetic_snapshot() == sentinels, "synthetic user data or profile changed")

    installed_checks = 0

    def installed():
        nonlocal installed_checks
        installed_checks += 1
        status = command("dpkg-query", "--show", "--showformat=${Status}\t${Version}\t${Architecture}\t${Conffiles}", "sniper").stdout
        require(status == "install ok installed\t" + inventory["version"] + "\tamd64\t", "incorrect installed state, identity or conffiles")
        listed = command("dpkg-query", "--listfiles", "sniper").stdout.splitlines()
        observed = installed_inventory_evidence(listed)
        (results / f"installed-files-{installed_checks}.json").write_text(json.dumps(observed, indent=2, sort_keys=True) + "\n")
        require(set(observed["actual_files"]) == EXPECTED,
                "dpkg file inventory differs: missing=" + repr(observed["missing"]) + "; unexpected=" + repr(observed["unexpected"]))
        for name, expected in inventory["files"].items():
            path = plain_path(Path("/") / name)
            info = path.lstat()
            require(stat.S_ISREG(info.st_mode), "installed path is not a regular file: " + name)
            require((info.st_uid, info.st_gid, stat.S_IMODE(info.st_mode), info.st_size) == (0, 0, expected["mode"], expected["size"]), "installed ownership/mode/size differs: " + name)
            require(sha(path) == expected["sha256"], "installed hash differs: " + name)
            owners = command("dpkg-query", "--search", "/" + name).stdout.splitlines()
            require(owners in [["sniper: /" + name], ["sniper:amd64: /" + name]], "unexpected package ownership: " + name)
        result = command("dpkg", "--verify", "--verify-format=rpm", "sniper")
        require(not result.stdout.strip() and not result.stderr.strip(), "dpkg verification differs")
        result = command("desktop-file-validate", "/usr/share/applications/sniper.desktop")
        require(not result.stdout.strip() and not result.stderr.strip(), "desktop entry validation diagnostic")
        require(synthetic_snapshot() == sentinels, "synthetic user data or profile changed")
        verify_loader_closures(inventory, command, home, custom_data)
        clean_audit()

    require(command("dpkg", "--print-architecture").stdout.strip() == "amd64", "wrong dpkg architecture")
    absent()
    clean_audit()
    tool_baseline = packages_except_sniper()
    (results / "tool-baseline.json").write_text(json.dumps(tool_baseline, indent=2) + "\n")
    check_candidate_overrides(command)
    (results / "dpkg-path-filters.json").write_text(json.dumps(path_filter_evidence(), indent=2, sort_keys=True) + "\n")
    deb = Path(inventory["deb"])
    plan = command("apt-get", "--simulate", "install", "--no-install-recommends", "--no-remove", *APT_DOC_OPTIONS, str(deb))
    require(not any(row.startswith("Remv ") for row in plan.stdout.splitlines()), "APT would remove an unrelated package")
    planned = []
    for row in plan.stdout.splitlines():
        if row.startswith("Inst "):
            name = row.split()[1]
            require(not name.split(":", 1)[0].endswith("-dev") and name not in {"rustc", "cargo", "build-essential"}, "APT unexpectedly plans development tools")
            planned.append(name)
    require("sniper" in planned, "APT simulation did not select Sniper")
    (results / "apt-planned-packages.json").write_text(json.dumps(planned, indent=2) + "\n")

    def record_stage(stage):
        if stage == "apt-installed":
            after = packages_except_sniper()
            before_by_name = {row.split("\t", 1)[0]: row for row in tool_baseline}
            after_by_name = {row.split("\t", 1)[0]: row for row in after}
            require(set(before_by_name).issubset(after_by_name), "APT removed an unrelated package")
            changed_names = {name.split(":", 1)[0] for name, row in after_by_name.items() if before_by_name.get(name) != row}
            require(changed_names.issubset({name.split(":", 1)[0] for name in planned}), "APT package delta differs from simulation")
            (results / "apt-package-delta.json").write_text(json.dumps({"before": tool_baseline, "after": after, "added_or_changed": sorted(set(after) - set(tool_baseline)), "removed_or_changed": sorted(set(tool_baseline) - set(after))}, indent=2) + "\n")

        (results / (stage + "-synthetic-data.json")).write_text(json.dumps(synthetic_snapshot(), indent=2) + "\n")
        (results / (stage + ".json")).write_text(json.dumps({"stage": stage, "passed": True, "candidate_sha256": inventory["deb_sha256"], "utc": datetime.now(timezone.utc).isoformat()}, indent=2) + "\n")

    lifecycle_sequence(deb, inventory["deb_sha256"], lambda: sha(deb), command, installed, absent, clean_audit, packages_except_sniper, record_stage)
    (results / "PASS.json").write_text(json.dumps({"result": "passed", "scope": "APT declared-runtime installation; three ELF loader dependency diagnostics; dpkg remove/reinstall/purge; exact payload ownership/hashes; synthetic user data preserved", "candidate_sha256": inventory["deb_sha256"], "manifest_sha256": inventory["manifest_sha256"], "image_digest": os.environ["SNIPER_QA_IMAGE"], "native_gui": "not launched or tested by this script", "dlopen_plugins_or_webkit_subprocesses": "not tested by loader diagnostics"}, indent=2) + "\n")
    print("PASS: fresh-runtime package and loader diagnostics only. Evidence:", results)


if __name__ == "__main__":
    main()
