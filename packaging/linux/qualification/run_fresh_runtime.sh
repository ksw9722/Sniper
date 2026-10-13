#!/usr/bin/env bash
# Runs only as the entrypoint of the reviewed explicit Docker invocation.
# It bootstraps validation tools inside that disposable image, never the host.
set -euo pipefail
umask 022
export LC_ALL=C DEBIAN_FRONTEND=noninteractive
[[ ${SNIPER_PACKAGE_QA_MODE:-} == explicit-docker-runtime ]]
[[ ${RUNNER_ENVIRONMENT:-} == github-hosted ]]
[[ ${GITHUB_RUN_ID:-} =~ ^[0-9]+$ ]]
[[ ${GITHUB_SHA:-} =~ ^[0-9a-f]{40}$ ]]
[[ ${EXPECTED_MANIFEST_SHA256:-} =~ ^[0-9a-f]{64}$ ]]
[[ ${SNIPER_QA_IMAGE:-} =~ ^debian@sha256:[0-9a-f]{64}$ ]]
[[ $(id -u) == 0 && -f /.dockerenv ]]
[[ ! -e /var/run/docker.sock && ! -L /var/run/docker.sock ]]
[[ ! -e /run/docker.sock && ! -L /run/docker.sock ]]
[[ $(. /etc/os-release && printf '%s:%s' "$ID" "$VERSION_ID") == debian:13 ]]
[[ $(uname -m) == x86_64 ]]
[[ $(realpath -- "$0") == /qa/scripts/run_fresh_runtime.sh ]]
for variable in DPKG_ROOT DPKG_ADMINDIR DPKG_DATADIR DPKG_FORCE LD_PRELOAD LD_LIBRARY_PATH LD_AUDIT DOCKER_HOST DOCKER_CONTEXT; do
  [[ -z ${!variable:-} ]]
done
# Check the mount boundary before the first APT command, while only base tools exist.
awk '
  {
    p=$5; o="," $6 ","; f="";
    for(i=7;i<=NF;i++) if($i=="-") {f=$(i+1); break}
    if(p ~ /\\/ || seen[p]++) exit 1;
    if(p=="/") {if(f!="overlay") exit 1; root=1}
    else if(p=="/qa/input" || p=="/qa/scripts") {if(o !~ /,ro,/) exit 1; qa[p]=1}
    else if(p=="/qa/results") {if(o !~ /,rw,/) exit 1; qa[p]=1}
    else if(p=="/etc/hosts" || p=="/etc/hostname" || p=="/etc/resolv.conf") {}
    else if(f ~ /^(proc|sysfs|cgroup|cgroup2|devpts|tmpfs|mqueue)$/ && p ~ /^\/(proc|sys|dev)(\/|$)/) {}
    else exit 1;
  }
  END {if(!root || !qa["/qa/input"] || !qa["/qa/scripts"] || !qa["/qa/results"]) exit 1}
' /proc/self/mountinfo
[[ ! -e /qa/results/bootstrap.log && ! -e /qa/results/evidence ]]
exec > >(tee /qa/results/bootstrap.log) 2>&1
cat /proc/self/mountinfo > /qa/results/mountinfo.txt
cat /etc/os-release > /qa/results/os-release.txt
printf '%s\n' "$SNIPER_QA_IMAGE" > /qa/results/image-digest.txt
dpkg-query -W -f='${binary:Package}\t${Version}\t${Architecture}\t${Status}\n' > /qa/results/pristine-packages.tsv
# Official slim's two Debian Deb822 entries are a deliberately narrow contract.
# New source layouts fail closed for review instead of being rewritten here.
[[ -f /etc/apt/sources.list.d/debian.sources && ! -L /etc/apt/sources.list.d/debian.sources ]]
if [[ -e /etc/apt/sources.list ]]; then
  ! grep -Eq '^[[:space:]]*[^#[:space:]]' /etc/apt/sources.list
fi
while IFS= read -r source; do
  [[ $source == /etc/apt/sources.list.d/debian.sources ]]
done < <(find /etc/apt/sources.list.d -maxdepth 1 \( -name '*.list' -o -name '*.sources' \) -print)
cat /etc/apt/sources.list.d/debian.sources > /qa/results/apt-sources.txt
awk -f /qa/scripts/check_debian_sources.awk /etc/apt/sources.list.d/debian.sources > /qa/results/apt-keyrings.txt
while IFS= read -r keyring; do
  [[ $keyring =~ ^/usr/share/keyrings/debian-archive-keyring[.](pgp|gpg)$ ]]
  [[ -f $keyring ]]
done < /qa/results/apt-keyrings.txt
apt-config dump > /qa/results/apt-config.txt
! grep -Ei '(AllowUnauthenticated|AllowInsecureRepositories|AllowDowngradeToInsecureRepositories)[[:space:]]+"(true|1)"' /qa/results/apt-config.txt
apt-get update
apt-get install --no-install-recommends --no-remove -y python3 desktop-file-utils binutils
dpkg-query -W -f='${binary:Package}\t${Version}\t${Architecture}\t${Status}\n' > /qa/results/validation-tool-packages.tsv
# No development toolchain or GTK/WebKit development headers are pre-provisioned.
! awk -F '\t' '$1 ~ /-dev(:amd64)?$/ || $1 ~ /^(rustc|cargo|build-essential)(:amd64)?$/ {bad=1} END {exit !bad}' /qa/results/validation-tool-packages.tsv
python3 -B /qa/scripts/qualify_ci_lifecycle.py \
  /qa/input "$EXPECTED_MANIFEST_SHA256" /qa/results/evidence \
  --approve-disposable-ci-lifecycle \
  'I approve exact-candidate APT runtime installation and package lifecycle in this explicit disposable Debian 13 container'
