#!/usr/bin/env python3
"""Stage Debian 13 candidate artifacts; never build, install, or launch Sniper."""
import argparse
import gzip
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import shutil
import stat
import struct
import subprocess
import sys
import tarfile
import tempfile
import tomllib

ROOT = Path(__file__).resolve().parents[2]
BINARIES = ("sniper", "sniper-cli", "sniper-desktop")
TARGET = "debian13-x86_64"
FIXTURE = b"SNIPER-PACKAGING-FIXTURE\n"
ENV = {**os.environ, "LC_ALL": "C", "DPKG_COLORS": "never", "DPKG_NLS": "0", "DEB_HOST_ARCH": "amd64"}
# Dependency resolution must use explicit distro paths, never ambient overrides.
for variable in ("LD_LIBRARY_PATH", "LD_PRELOAD", "DPKG_ROOT", "DPKG_ADMINDIR", "DPKG_DATADIR"):
    ENV.pop(variable, None)


def require(condition, message):
    if not condition:
        raise ValueError(message)


def run(*args, cwd=ROOT, clean_stderr=False):
    result = subprocess.run(args, cwd=cwd, env=ENV, text=True, capture_output=True, check=False)
    if result.stderr:
        print(result.stderr.rstrip(), file=sys.stderr)
    result.check_returncode()
    require(not clean_stderr or not result.stderr.strip(),
            "dependency tooling emitted diagnostics; refusing to publish unresolved or ambiguous dependencies")
    return result.stdout.strip()


def safe_path(value):
    path = Path(value)
    require(not any(c in str(path) for c in "\n\r\0"), "control character in path")
    require(".." not in path.parts, "parent traversal is not allowed")
    path = path.absolute()
    for part in (path, *path.parents):
        require(not part.is_symlink(), f"symlink path is not allowed: {part}")
    return path


def output_path(value):
    path = safe_path(value)
    require(any(base in path.parents for base in (ROOT / "dist", Path("/tmp"))),
            "output must be a new path below repository dist/ or /tmp/")
    require(not path.exists(), f"refusing to replace existing output: {path}")
    return path


def digest(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def host_target(system=None, machine=None, distro=None):
    system = platform.system() if system is None else system
    machine = platform.machine() if machine is None else machine
    if distro is None:
        distro = platform.freedesktop_os_release()
    require(system == "Linux" and machine == "x86_64", "requires Linux x86_64; cross-packaging is unsupported")
    require(distro.get("ID") == "debian" and distro.get("VERSION_ID") == "13",
            "requires Debian 13; no other distro compatibility is claimed")
    return {"id": "debian", "version_id": "13", "architecture": "x86_64"}


def source_state():
    names = run("git", "ls-files", "-z", "--cached", "--others", "--exclude-standard").split("\0")
    inventory = {}
    for name in sorted(set(filter(None, names))):
        path = safe_path(ROOT / name)
        # Deleted files are represented explicitly in a dirty source snapshot.
        inventory[name] = digest(path) if path.is_file() else None
    return {
        "revision": run("git", "rev-parse", "HEAD"),
        "dirty": bool(run("git", "status", "--porcelain", "--untracked-files=normal")),
        "tree_sha256": hashlib.sha256(json.dumps(inventory, sort_keys=True).encode()).hexdigest(),
        "cargo_lock_sha256": digest(ROOT / "Cargo.lock"),
    }


def inputs(directory, fixtures=False):
    directory = safe_path(directory)
    require(directory.is_dir(), "binary directory does not exist")
    result = {}
    for name in BINARIES:
        path = safe_path(directory / name)
        require(path.is_file() and stat.S_ISREG(path.stat().st_mode), f"missing regular binary: {name}")
        require(path.stat().st_mode & 0o111, f"binary is not executable: {name}")
        if fixtures:
            require(path.read_bytes() == FIXTURE + name.encode() + b"\n", "fixture mode accepts only inert test fixtures")
        result[name] = path
    return result


def validate_elf_header(path):
    with path.open("rb") as stream:
        header = stream.read(64)
    require(len(header) == 64 and header[:7] == b"\x7fELF\x02\x01\x01", f"not ELF64 little-endian: {path.name}")
    require(header[7] in (0, 3), f"not a Linux/System V ELF: {path.name}")
    kind, machine = struct.unpack_from("<HH", header, 16)
    require(kind in (2, 3) and machine == 62, f"not an x86_64 executable: {path.name}")

def inspect_elf(path):
    validate_elf_header(path)
    program = run("readelf", "--wide", "--program-headers", str(path))
    require("[Requesting program interpreter: /lib64/ld-linux-x86-64.so.2]" in program,
            f"unexpected Linux interpreter: {path.name}")
    dynamic = run("readelf", "--wide", "--dynamic", str(path))
    require(not re.search(r"\((?:RPATH|RUNPATH)\)", dynamic), f"RPATH/RUNPATH is not distributable: {path.name}")
    needed = sorted(set(re.findall(r"\(NEEDED\).*\[([^\]]+)\]", dynamic)))
    require(bool(needed), f"missing shared-library metadata: {path.name}")
    versions = run("readelf", "--wide", "--version-info", str(path))
    return {"interpreter": "/lib64/ld-linux-x86-64.so.2", "needed": needed,
            "glibc_versions": sorted(set(re.findall(r"\bGLIBC_[0-9.]+", versions)))}


def write_json(path, value):
    path.write_text(json.dumps(value, sort_keys=True, indent=2) + "\n", encoding="utf-8")
    path.chmod(0o644)


def record(args):
    target = host_target()
    binaries = inputs(args.binaries_dir, args.test_fixtures)
    destination = output_path(args.output)
    require(args.source_date_epoch >= 0, "source date epoch must be nonnegative")
    if args.test_fixtures:
        toolchain, dependencies = "TEST FIXTURE ONLY", "TEST FIXTURE ONLY"
    else:
        require(args.toolchain_file and args.dependency_manifest and args.build_command,
                "real inputs require toolchain file, Debian package manifest, and build command")
        toolchain = safe_path(args.toolchain_file).read_text(encoding="utf-8").strip()
        dependencies = safe_path(args.dependency_manifest).read_text(encoding="utf-8").strip()
        require(toolchain.startswith("rustc ") and "host: x86_64-unknown-linux-gnu" in toolchain,
                "toolchain file must contain rustc --version --verbose for Linux x86_64")
        for package in ("libc6", "libgtk-3-0t64", "libwebkit2gtk-4.1-0"):
            require(re.search(r"(?m)^" + re.escape(package) + r"(?::amd64)?\t\S+\tamd64$", dependencies),
                    f"Debian package version manifest is missing {package}")
    value = {
        "schema": 1, "target": TARGET, "host": target, "test_fixture": args.test_fixtures,
        "version": tomllib.loads((ROOT / "Cargo.toml").read_text())["package"]["version"],
        "source": source_state(), "source_date_epoch": args.source_date_epoch,
        "build_command": args.build_command, "toolchain": toolchain, "debian_packages": dependencies,
        "binaries": {name: {"sha256": digest(path), "size": path.stat().st_size}
                     for name, path in binaries.items()},
    }
    destination.parent.mkdir(parents=True, exist_ok=True)
    # Exclusive creation prevents a previous run's evidence being overwritten.
    with destination.open("x", encoding="utf-8") as stream:
        stream.write(json.dumps(value, sort_keys=True, indent=2) + "\n")
    print(destination)


def copy_file(source, target, executable=False):
    target.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(source, target)
    target.chmod(0o755 if executable else 0o644)


def file_manifest(root):
    return {str(path.relative_to(root)): digest(path) for path in sorted(root.rglob("*")) if path.is_file()}


def seal_payload(root, docdir, manifest):
    value = {**manifest, "payload_files": file_manifest(root)}
    write_json(docdir / "build-manifest.json", value)
    (docdir / "SHA256SUMS").write_text("".join(f"{checksum}  {name}\n" for name, checksum in file_manifest(root).items()))
    return value


def check_host_overrides(config=Path("/etc/dpkg")):
    # dpkg's configuration directory is independent of --admindir. An extracted
    # database must not silently inherit a host administrator's dependency rules.
    for name in ("shlibs.override", "shlibs.default"):
        path = config / name
        if path.exists():
            active = [line for line in path.read_text().splitlines() if line.strip() and not line.lstrip().startswith("#")]
            require(not active, f"active host dependency overrides are unsupported with an extracted sysroot: {path}")
    require(not any((config / "symbols").glob("*.symbols*")),
            "host symbol override files are unsupported with an extracted sysroot")


def check_sysroot_libraries(root, libraries, binaries, elf=None):
    # The custom lookup directories precede host paths in dpkg-shlibdeps. Ensure
    # every direct dependency is actually present there with the native ABI;
    # otherwise dpkg may continue searching unrelated host library directories.
    for binary, path in binaries.items():
        metadata = elf[binary] if elf is not None else inspect_elf(path)
        for soname in metadata["needed"]:
            require(Path(soname).name == soname and soname not in (".", ".."), "unsafe ELF library name")
            matches = [directory / soname for directory in libraries if (directory / soname).exists() or (directory / soname).is_symlink()]
            require(matches, f"sysroot is missing direct library {soname}; host fallback is not allowed")
            for library in matches:
                require(library.resolve().is_relative_to(root), f"sysroot library escapes its root: {soname}")
                require(library.is_file(), f"sysroot library is not a regular file: {soname}")
                validate_elf_header(library)


def deb_dependencies(work, binaries, sysroot=None, admindir=None, elf=None):
    (work / "debian").mkdir()
    (work / "debian/control").write_text(
        "Source: sniper\nSection: devel\nPriority: optional\nMaintainer: sm1ee <sm1ee@users.noreply.github.com>\n"
        "\nPackage: sniper\nArchitecture: amd64\nDescription: Sniper candidate package\n")
    # No --ignore-missing-info: unidentified libraries must block a .deb.
    options = []
    if sysroot is not None:
        root = safe_path(sysroot)
        database = safe_path(admindir)
        require(root.is_dir() and root != Path("/"), "invalid extracted sysroot")
        require(safe_path(database / "status").is_file(), "private dpkg database needs a status file")
        format_file = safe_path(database / "info/format")
        require(format_file.is_file() and format_file.read_text().strip() == "1",
                "private dpkg database needs info/format=1 for official multiarch metadata")
        check_host_overrides()
        libraries = []
        options.append("--admindir=" + str(database))
        for relative in ("usr/lib/x86_64-linux-gnu", "lib/x86_64-linux-gnu", "usr/lib"):
            library = root / relative
            if library.is_dir():
                require(library.resolve().is_relative_to(root), "sysroot library path escapes its root")
                libraries.append(library)
                options.append("-l" + str(library))
        require(libraries, "sysroot has no native library directory")
        check_sysroot_libraries(root, libraries, binaries, elf)
    result = run("dpkg-shlibdeps", "-O", "--warnings=symbol-not-found", *options,
                 *("-e" + str(path) for path in binaries.values()), cwd=work, clean_stderr=True)
    matches = re.findall(r"^shlibs:Depends=(.+)$", result, re.MULTILINE)
    require(len(matches) == 1 and "libc6" in matches[0] and "libwebkit2gtk-4.1-0" in matches[0],
            "dpkg-shlibdeps did not identify the complete native dependency set")
    return matches[0]


def archive(root, destination, epoch):
    with destination.open("xb") as raw:
        with gzip.GzipFile(filename="", mode="wb", fileobj=raw, mtime=epoch) as compressed:
            with tarfile.open(fileobj=compressed, mode="w") as tar:
                for path in [root, *sorted(root.rglob("*"))]:
                    info = tar.gettarinfo(str(path), arcname=str(path.relative_to(root.parent)))
                    info.uid = info.gid = 0
                    info.uname = info.gname = "root"
                    info.mtime = epoch
                    info.mode = 0o755 if path.is_dir() or path.parent.name == "bin" else 0o644
                    if path.is_file():
                        with path.open("rb") as stream:
                            tar.addfile(info, stream)
                    else:
                        tar.addfile(info)


def verify_tar(stream, expected, prefix=""):
    """Read back package bytes without extracting or executing any member."""
    seen = set()
    checked = set()
    directories = {""}
    for name in expected:
        directories.update(str(parent) for parent in Path(name).parents if str(parent) != ".")
    with tarfile.open(fileobj=stream, mode="r|*") as tar:
        for member in tar:
            name = member.name.removeprefix("./").rstrip("/")
            if name == ".":
                name = ""
            if prefix:
                require(name == prefix or name.startswith(prefix + "/"), "unexpected archive root")
                name = name[len(prefix):].lstrip("/")
            require(name not in seen, "duplicate archive member")
            seen.add(name)
            require((member.uid, member.gid) == (0, 0), "incorrect package ownership")
            if member.isdir():
                require(name in directories and member.mode == 0o755, "unexpected archive directory or mode")
                continue
            require(member.isfile() and name in expected, "unexpected archive member or symlink")
            executable = str(Path(name).parent) in ("bin", "usr/bin")
            require(member.mode == (0o755 if executable else 0o644), "incorrect package file mode")
            with tar.extractfile(member) as content:
                require(hashlib.file_digest(content, "sha256").hexdigest() == expected[name], "packaged file checksum mismatch")
            checked.add(name)
    require(checked == set(expected), "archive is missing payload files")


def verify_deb(path, expected):
    process = subprocess.Popen(["dpkg-deb", "--fsys-tarfile", str(path)], stdout=subprocess.PIPE, env=ENV)
    try:
        verify_tar(process.stdout, expected)
        require(process.wait() == 0, "could not read Debian payload")
    finally:
        process.stdout.close()
        if process.poll() is None:
            process.terminate()
        process.wait()


def package(args):
    host_target()
    require(bool(args.sysroot) == bool(args.dpkg_admindir), "--sysroot and --dpkg-admindir must be supplied together")
    require(not (args.archive_only and args.sysroot), "archive-only does not resolve Debian dependencies")
    require(not (args.test_fixtures and args.sysroot), "fixtures do not consume real dependency metadata")
    binaries = inputs(args.binaries_dir, args.test_fixtures)
    out = output_path(args.output_dir)
    build = json.loads(safe_path(args.build_record).read_text(encoding="utf-8"))
    require(build.get("schema") == 1 and build.get("target") == TARGET, "wrong build record target or schema")
    require(build.get("test_fixture") is args.test_fixtures, "fixture mode does not match build record")
    require(build.get("source") == source_state(), "source changed since the build record; rebuild and record again")
    version = tomllib.loads((ROOT / "Cargo.toml").read_text())["package"]["version"]
    require(build.get("version") == version and re.fullmatch(r"[0-9]+\.[0-9]+\.[0-9]+", version), "invalid package version")
    for name, path in binaries.items():
        require(build["binaries"].get(name) == {"sha256": digest(path), "size": path.stat().st_size}, f"binary changed: {name}")
    suffix = "-TEST-ONLY" if args.test_fixtures else ""
    name = f"Sniper-{version}-{TARGET}{suffix}"
    epoch = build["source_date_epoch"]
    require(isinstance(epoch, int) and 0 <= epoch <= 4294967295, "invalid source date epoch")
    out.parent.mkdir(parents=True, exist_ok=True)
    # The caller's destination never replaces an existing tree, including on failure.
    with tempfile.TemporaryDirectory(prefix=".sniper-package-", dir=out.parent) as temporary:
        work = Path(temporary)
        artifacts = work / "artifacts"
        artifacts.mkdir()
        portable = work / name
        portable.mkdir()
        staged_binaries = {}
        for binary, path in binaries.items():
            staged = portable / "bin" / binary
            copy_file(path, staged, True)
            require(digest(staged) == build["binaries"][binary]["sha256"], f"binary changed during staging: {binary}")
            staged_binaries[binary] = staged
        # Inspect and derive dependencies from the same owned bytes we package.
        elf = {} if args.test_fixtures else {binary: inspect_elf(path) for binary, path in staged_binaries.items()}
        if elf:
            require("libwebkit2gtk-4.1.so.0" in elf["sniper-desktop"]["needed"], "desktop binary is not linked to WebKitGTK 4.1")
        depends = None
        if not args.archive_only and not args.test_fixtures:
            # dpkg requires a package staging root, even before control is written.
            # This temporary marker never enters the portable archive.
            marker = portable / "DEBIAN"
            marker.mkdir()
            try:
                depends = deb_dependencies(work, staged_binaries, args.sysroot, args.dpkg_admindir, elf)
            finally:
                marker.rmdir()
        manifest = {"build": build, "elf": elf, "debian_depends": depends,
                    "qualification": "not-qualified", "display_candidate": "X11",
                    "system_libraries_bundled": False,
                    "dependency_resolution": "not-run" if depends is None else
                        ("extracted-official-dpkg-metadata" if args.sysroot else "installed-dpkg-metadata")}
        copy_file(ROOT / "LICENSE", portable / "LICENSE")
        copy_file(ROOT / "packaging/linux/README.md", portable / "README.md")
        copy_file(ROOT / "packaging/linux/sniper.desktop", portable / "share/applications/sniper.desktop")
        copy_file(ROOT / "packaging/macos/AppIcon.iconset/icon_256x256.png", portable / "share/icons/hicolor/256x256/apps/sniper.png")
        seal_payload(portable, portable, manifest)
        archive(portable, artifacts / (name + ".tar.gz"), epoch)
        with (artifacts / (name + ".tar.gz")).open("rb") as stream:
            verify_tar(stream, file_manifest(portable), name)
        if not args.archive_only:
            debroot = work / "debroot"
            docdir = debroot / "usr/share/doc/sniper"
            for binary, path in staged_binaries.items():
                copy_file(path, debroot / "usr/bin" / binary, True)
            for relative in ("share/applications/sniper.desktop", "share/icons/hicolor/256x256/apps/sniper.png"):
                copy_file(portable / relative, debroot / "usr" / relative)
            for filename in ("LICENSE", "README.md"):
                copy_file(portable / filename, docdir / filename)
            seal_payload(debroot, docdir, manifest)
            expected_deb = file_manifest(debroot)
            control = debroot / "DEBIAN"
            control.mkdir()
            package_name = "sniper-packaging-fixture" if args.test_fixtures else "sniper"
            control_text = (f"Package: {package_name}\nVersion: {version}-1\nArchitecture: amd64\n"
                            "Maintainer: sm1ee <sm1ee@users.noreply.github.com>\nSection: devel\nPriority: optional\n"
                            "Homepage: https://github.com/sm1ee/Sniper\n"
                            f"Installed-Size: {sum(p.stat().st_size for p in debroot.rglob('*') if p.is_file()) // 1024 + 1}\n")
            if depends:
                control_text += f"Depends: {depends}\n"
            control_text += "Description: Sniper Debian 13 native qualification candidate\n"
            if args.test_fixtures:
                control_text += " TEST FIXTURE ONLY. Contains no usable application. Do not install.\n"
            (control / "control").write_text(control_text)
            payload = sorted(p for p in debroot.rglob("*") if p.is_file() and control not in p.parents)
            (control / "md5sums").write_text("".join(
                f"{hashlib.md5(p.read_bytes(), usedforsecurity=False).hexdigest()}  {p.relative_to(debroot)}\n" for p in payload))
            for path in debroot.rglob("*"):
                path.chmod(0o755 if path.is_dir() or path.parent == debroot / "usr/bin" else 0o644)
                os.utime(path, (epoch, epoch))
            debroot.chmod(0o755)
            # Root ownership in the archive needs no root privileges or host chown.
            previous = ENV.get("SOURCE_DATE_EPOCH")
            ENV["SOURCE_DATE_EPOCH"] = str(epoch)
            try:
                run("dpkg-deb", "--root-owner-group", "--build", str(debroot), str(artifacts / (name + ".deb")))
                verify_deb(artifacts / (name + ".deb"), expected_deb)
            finally:
                if previous is None:
                    ENV.pop("SOURCE_DATE_EPOCH", None)
                else:
                    ENV["SOURCE_DATE_EPOCH"] = previous
        require(build["source"] == source_state(), "source changed during packaging; rebuild and record again")
        write_json(artifacts / "build-manifest.json", {**manifest, "artifacts": file_manifest(artifacts)})
        for path in sorted(artifacts.iterdir()):
            path.with_name(path.name + ".sha256").write_text(f"{digest(path)}  {path.name}\n")
        require(not out.exists() and not out.is_symlink(), "output appeared during packaging; refusing replacement")
        artifacts.rename(out)
    print(out)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    subs = parser.add_subparsers(dest="command", required=True)
    record_parser = subs.add_parser("record", help="record exact already-built inputs; does not compile")
    record_parser.add_argument("--output", required=True)
    record_parser.add_argument("--toolchain-file")
    record_parser.add_argument("--dependency-manifest")
    record_parser.add_argument("--build-command", default="")
    record_parser.add_argument("--source-date-epoch", type=int, required=True)
    package_parser = subs.add_parser("package", help="package the exact recorded binaries; does not execute them")
    package_parser.add_argument("--build-record", required=True)
    package_parser.add_argument("--output-dir", required=True)
    package_parser.add_argument("--sysroot", help="isolated official Debian 13 library root (requires private dpkg metadata)")
    package_parser.add_argument("--dpkg-admindir", help="read-only private dpkg database matching --sysroot")
    package_parser.add_argument("--archive-only", action="store_true", help="omit .deb when no coherent dpkg database is available")
    for sub in (record_parser, package_parser):
        sub.add_argument("--binaries-dir", required=True)
        sub.add_argument("--test-fixtures", action="store_true", help="only accept inert fixtures; label every artifact TEST-ONLY")
    args = parser.parse_args()
    try:
        (record if args.command == "record" else package)(args)
    except (ValueError, OSError, KeyError, TypeError, subprocess.CalledProcessError) as error:
        parser.exit(1, f"Packaging failed: {error}\n")


if __name__ == "__main__":
    main()
