# Sniper on Debian 13: x86_64, X11

**Debian 13 (trixie), x86_64, X11 is a supported target from v0.3.1.**
Its `.deb` and portable `.tar.gz` are published with each release from v0.3.1;
the `v0.3.1` tag is the source for builds of your own.
Ubuntu, other Debian versions, other distributions, ARM, musl, native Wayland
and XWayland are outside this qualification. These Linux checks make no new
macOS qualification claim.

The qualification baseline is source revision
[`7720f15b43fe23d2808d5bc8aeffd02394b5c031`](https://github.com/sm1ee/Sniper/commit/7720f15b43fe23d2808d5bc8aeffd02394b5c031).
Native checks used frozen binaries whose recorded executable inputs match both
[`0ae4f9f722195a0cdd0ad083e23e8901eca03c9f`](https://github.com/sm1ee/Sniper/commit/0ae4f9f722195a0cdd0ad083e23e8901eca03c9f)
and the qualification baseline above. Actual native GTK 3/WebKitGTK 4.1 checks
covered window rendering and
resize/restore, Service Map filters and exact retained-row evidence, native
popup stability, pagination, session switching, persisted restart and CLI/API
parity. Separate [exact-revision Linux CI](https://github.com/sm1ee/Sniper/actions/runs/38351782558)
verified the package lifecycle in a disposable Debian 13 runtime. CI artifacts
were built independently from the clean qualification baseline and are tied to
their own package and manifest hashes.
Native checks retained the default WebKit sandbox configuration without disabling it; renderer kernel restrictions were not independently enumerated. This is target qualification, not general sandbox
certification.

The binaries use system GTK 3, WebKitGTK **4.1 ABI**, JavaScriptCoreGTK 4.1,
libsoup 3 and their transitive dependencies. No system libraries, WebKit helpers,
CA certificates or browser profiles are bundled. An archive is portable between
matching, provisioned Debian 13 machines; it is not a self-contained Linux app.
The build manifest records actual ELF dependencies and GLIBC symbol versions.
A glibc version match alone does not establish another distro's compatibility.

## Get the release files, source or CI artifacts

From the [latest release](https://github.com/sm1ee/Sniper/releases/latest),
download `Sniper-<version>-debian13-x86_64.deb` or the portable
`Sniper-<version>-debian13-x86_64.tar.gz` with its `.sha256`, and check it
before following the instructions below:

```sh
sha256sum -c Sniper-<version>-debian13-x86_64.deb.sha256
```

The release also carries `-build-manifest.json` (source revision, compiler,
dependencies and hashes, with its own `.sha256`) and `-runtime-PASS.json`, the
CI check that installed, removed, reinstalled and purged that exact package in a
fresh Debian 13. Both name the package by its SHA-256.

For a new source checkout:

```sh
git clone --single-branch --branch v0.3.1 https://github.com/sm1ee/Sniper.git
cd Sniper
git rev-parse HEAD
```

Follow [Build and record exact native inputs](#build-and-record-exact-native-inputs)
for prerequisites, tests, binaries and packages. Record the exact commit you use;
a branch can advance beyond the qualification baseline.

For prebuilt candidates, open the repository's **Actions → Linux candidate**
workflow and choose a successful run for the `v0.3.1` tag.
Both `debian13-x86_64` and `runtime-debian13-x86_64` jobs must pass. Download
`sniper-debian13-x86_64-candidate` and
`sniper-debian13-x86_64-runtime-evidence` from that same run. Inspect the build
manifest's source revision, compiler/dependency versions and hashes; check that
the runtime evidence's `PASS.json` identifies the same package and manifest
SHA-256. Then verify the sidecar checksums before following the instructions
below. CI artifacts may expire and are not published Linux releases.

The packager deliberately records `qualification: "not-qualified"` in every
artifact manifest: packaging alone cannot certify a particular build. Keep the
separate exact-artifact lifecycle evidence with the download. Headless CI does
not replace native desktop qualification or extend support to another target.

## Candidate artifacts and ownership

- `Sniper-0.3.1-debian13-x86_64.tar.gz`: one versioned directory with `bin/sniper`,
  `bin/sniper-cli`, `bin/sniper-desktop`, `LICENSE`, this README, icon,
  `share/applications/sniper.desktop`, `build-manifest.json`, and `SHA256SUMS`.
- `Sniper-0.3.1-debian13-x86_64.deb`: package-owned `/usr/bin/sniper`,
  `/usr/bin/sniper-cli`, `/usr/bin/sniper-desktop`, desktop entry under
  `/usr/share/applications`, PNG under the standard hicolor 256x256 apps directory,
  and documentation, license, build manifest and checksums under
  `/usr/share/doc/sniper`.
- Each artifact and the external build manifest have a `.sha256` sidecar in
  `<hash>  <filename>` format. SHA-256 provides integrity, not publisher identity;
  these candidates are unsigned. Obtain both artifact and checksum from a trusted
  source. Internal checksum paths are relative to the portable directory or,
  for the Debian payload, its filesystem root.

The `.deb` has no pre/post-install, removal or configuration scripts, service,
conffiles, PATH registration or launcher wrapper. Its package manager owns the
installed files and handles upgrades/removal. Installation does not edit shell
profiles, proxy configuration, trust stores, browser settings or Sniper settings.
Removing or purging the package preserves user sessions, certificates and settings
in `~/.sniper` (or the explicitly configured `SNIPER_DATA_DIR`). Package-manager
standard icon/desktop-cache triggers may run; Sniper adds no custom triggers.

For a qualified candidate, inspect and verify the files before installation:

```sh
sha256sum -c Sniper-0.3.1-debian13-x86_64.deb.sha256
dpkg-deb --info Sniper-0.3.1-debian13-x86_64.deb
dpkg-deb --contents Sniper-0.3.1-debian13-x86_64.deb
# Changes system package state; use only on the intended Debian 13 machine:
sudo apt install ./Sniper-0.3.1-debian13-x86_64.deb
# After closing Sniper, to remove its package-owned files:
sudo apt remove sniper
```

`apt` resolves the exact generated Depends from configured Debian repositories.
A normal desktop session, display connection and working WebKit sandbox are
required. Do not disable the sandbox to make a container launch pass. Missing
WebKit subprocesses, GIO/GStreamer modules or sandbox facilities are runtime
prerequisite failures, not a reason to copy host libraries into the package.

## Portable use and PATH

Verify the archive, then extract it into a new user-owned directory. Do not
extract over an existing installation or into a system directory:

```sh
sha256sum -c Sniper-0.3.1-debian13-x86_64.tar.gz.sha256
tar -xzf Sniper-0.3.1-debian13-x86_64.tar.gz
cd Sniper-0.3.1-debian13-x86_64
sha256sum -c SHA256SUMS
# Optional for this shell only; no shell profile is changed:
export PATH="$PWD/bin:$PATH"
./bin/sniper-desktop
```

Use a provisioned Debian 13 runtime with the dependencies listed in the manifest;
an extracted development sysroot alone is not an installed WebKit runtime. The
portable desktop entry is a template using `Exec=sniper-desktop` and `Icon=sniper`.
Extraction does not register it. Run the binary by its full path, or manually
configure your desktop launcher and PATH if wanted. The `.deb` uses `/usr/bin`
and needs no Sniper-managed PATH entry.

All three binaries retain the existing `~/.sniper` default. Set
`SNIPER_DATA_DIR` explicitly for an isolated instance; never run a scratch binary
against real sessions. Only one server/desktop may own a data directory. Launch
tests only in a real X11 session using a fresh directory:

```sh
SNIPER_DATA_DIR="$(mktemp -d /tmp/sniper-native-test.XXXXXX)" \
SNIPER_UI_ADDR=127.0.0.1:0 SNIPER_PROXY_ADDR=127.0.0.1:0 \
SNIPER_INSTALL_AGENT_SKILLS=0 SNIPER_INSTALL_CLI_PATH=0 \
  ./bin/sniper-desktop
```

Use the resulting scratch runtime's API address explicitly with `sniper-cli
--api <URL>`, or use the same `SNIPER_DATA_DIR` for discovery. Do not assume the
server and a CLI in a different container share a loopback network namespace.
Sniper's startup version check may read official GitHub release metadata.
Neither packaging nor this example sets a system proxy or installs a CA.

Linux self-install/update is not supported. Close the app and use the package
manager, or extract a newer archive into a new directory and adjust your own PATH
or launcher. To remove a portable copy, close it and delete only the extracted
versioned directory after checking it contains no files you added. There is no
recursive uninstall script; user data stays separate. Do not delete `~/.sniper`.

## Build and record exact native inputs

Use a coherent Debian 13 x86_64 build environment. CI uses the official
`rust:1.99.0-trixie` image on `ubuntu-24.04`; the Ubuntu runner hosts the container
and is not the binary ABI target. The image tag and Debian updates are mutable,
so the manifest records actual compiler and installed dependency versions. This
is traceability, not a bit-reproducible source-build claim.

Required official Debian build packages include `build-essential`, `pkg-config`,
`libgtk-3-dev`, `libwebkit2gtk-4.1-dev`, `libssl-dev`, `dpkg-dev`, `binutils`,
`python3` and `desktop-file-utils`. Use Node.js 24 for frontend tests, matching
the Windows workflow; CI provisions it with `actions/setup-node@v6`. Let Debian resolve
transitive packages. Provision a disposable container through its owner; do not
run ad hoc host installs to fill gaps. Keep `Cargo.lock` and all native GUI
dependencies intact.

From the repository root, after provisioning:

```sh
cargo fmt --all -- --check
cargo test --locked --release
node --check web/app.js
node --test tests/*.test.cjs
python3 -B -m unittest discover -s tests -p 'linux_packaging_test.py' -v
cargo build --locked --release --bins
mkdir -p dist
rustc --version --verbose > dist/linux-toolchain.txt
dpkg-query -W -f='${binary:Package}\t${Version}\t${Architecture}\n' > dist/linux-debian-packages.tsv
python3 -B packaging/linux/package.py record \
  --binaries-dir target/release --output dist/linux-build-record.json \
  --source-date-epoch "$(git show -s --format=%ct HEAD)" \
  --toolchain-file dist/linux-toolchain.txt \
  --dependency-manifest dist/linux-debian-packages.tsv \
  --build-command 'cargo build --locked --release --bins'
python3 -B packaging/linux/package.py package \
  --binaries-dir target/release --build-record dist/linux-build-record.json \
  --output-dir dist/linux-candidate
```

Use `--offline` for Cargo when the complete verified cache is available. A cache
miss is a prerequisite failure, not permission to remove or replace dependencies.
The package script runs neither Cargo nor the supplied binaries. It accepts exact
explicit inputs, hashes all three, validates ELF64/Linux/x86_64 plus the glibc
interpreter, rejects RPATH/RUNPATH, and rechecks the source content snapshot and
binary hashes before copying. Keep source and binaries unchanged from the build
through recording/packaging. The recorded build command is an operator attestation;
the packager cannot retroactively prove which command produced a binary.

Output must be a **new** path below repository `dist/` or `/tmp/`. Existing output,
parent traversal and symlinked paths are rejected. No package install, system
write, dependency download or native launch occurs. Build records include source
revision, dirty status and a digest of tracked/nonignored source content, not an
assumption that every local build came from a clean commit. The payload manifests
and checksums enumerate files; both archives are read back without extraction to
verify the exact file set, hashes, permissions and root ownership before output.
No arbitrary files from the binary directory are copied. Reusing an old build record after an edit or binary change fails closed.

`dpkg-shlibdeps` derives Depends from the real linked ELF symbols and Debian
package metadata. Missing dependency information and stderr diagnostics are
fatal even if the tool exits successfully, with one installed-metadata exception:
the exact Debian 13 libc6 DEP17 M8 loader-diversion diagnostic is accepted only
after read-only diversion, package status/ownership, canonical ELF and package
checksum checks. Its original text and validation evidence are recorded in
`dependency_diagnostics` in each build manifest. Unknown, combined and
missing-symbol diagnostics remain fatal; extracted-metadata mode accepts no
stderr diagnostics. Unresolved strong symbols cannot be hidden by a superficially
complete Depends line. No guessed fallback list or `--ignore-missing-info` is
used. For an approved isolated extraction,
`package` also accepts paired `--sysroot <root> --dpkg-admindir <private-db>`.
The private database must contain official package status, symbols/shlibs and
`.list` records mapping the actual absolute extracted library paths. Its
`info/format` must contain `1` so dpkg recognizes the official multiarch-qualified
control filenames; absence selects the legacy unqualified format. It is input
to dependency tooling only, not evidence that packages were installed on the host.
The manifest labels this mode `extracted-official-dpkg-metadata`. Preserve the
signed repository metadata and `.deb` hashes separately in the provisioning
record. In extracted mode, each directly linked library must resolve inside the sysroot
with the native ELF architecture; missing libraries, escaping library symlinks,
and active host dpkg symbol/shlibs override files are rejected. Ambient library
and dpkg-data overrides are removed from the dependency-tool environment. These
flags do not make an extracted WebKit runtime relocatable.

If only an archive can be checked, use `package --archive-only` and record the
missing `.deb` dependency-resolution gate explicitly. The archive retains ELF
requirements and the full build package manifest; no complete `.deb` claim follows.

## Checks and per-artifact qualification

`tests/linux_packaging_test.py` uses inert, explicitly marked fixture bytes. The
fixture flag requires those exact bytes; outputs contain `TEST-ONLY` in their
names and the test `.deb` has a distinct package name. Never distribute or install
these fixtures. Tests inspect archives, ownership, checksums, desktop paths and
rejections without installing packages or running the fake binaries.

With real frozen Debian 13 binaries and an approved extracted sysroot available,
the optional native resolver regression uses those files only as read-only input:

```sh
SNIPER_PACKAGING_NATIVE_BINARIES=/absolute/path/to/frozen/bin \
SNIPER_PACKAGING_SYSROOT=/absolute/path/to/extracted/root \
SNIPER_PACKAGING_DPKG_ADMINDIR=/absolute/path/to/private-dpkg-metadata \
  python3 -B -m unittest discover -s tests -p 'linux_packaging_test.py' -v
```

It stages copies of the binaries, verifies clean positive dependency resolution,
then removes one required libc symbol from a temporary **copy** of the private
database and verifies rejection. It checks that original binary and symbol-file
hashes are unchanged. No application is launched and no package is installed.
Without the three paths, this native-input regression is explicitly skipped;
the deterministic diagnostic and containment tests still run.

CI builds all three real binaries, runs Rust and frontend tests, creates both
candidate artifacts and validates the desktop entry. A separate disposable
Debian 13 runtime checks APT installation with declared dependencies, ELF loader
diagnostics, exact payload ownership/hashes, removal, reinstallation and purge,
while preserving synthetic user data.

The runtime starts from official `debian:13-slim` with validation tools. Because
that image excludes documentation by default, the qualification's initial APT
install and dpkg reinstall each supply per-operation `--path-include` options
for exactly four files: `/usr/share/doc/sniper/LICENSE`,
`/usr/share/doc/sniper/README.md`, `/usr/share/doc/sniper/SHA256SUMS` and
`/usr/share/doc/sniper/build-manifest.json`. This permits full-payload
qualification while leaving inherited dpkg configuration unchanged; it does not
create a persistent or general documentation-inclusion setting.

Loader diagnostics do not test dynamically loaded plugins or WebKit
subprocesses. Artifacts remain CI candidates only; the workflow creates no tag,
release upload, deployment or self-update. Each run's evidence applies to its
exact artifact hashes, not every future build.

Primary references:
- [Debian dpkg-shlibdeps](https://manpages.debian.org/trixie/dpkg-dev/dpkg-shlibdeps.1.en.html)
- [Debian WebKitGTK 4.1 development package](https://packages.debian.org/trixie/libwebkit2gtk-4.1-dev)
- [Official Rust container variants](https://hub.docker.com/_/rust)
- [Docker Library Rust tag metadata](https://github.com/docker-library/official-images/blob/master/library/rust)
- [GitHub container jobs](https://docs.github.com/en/actions/how-tos/write-workflows/choose-where-workflows-run/run-jobs-in-a-container)
