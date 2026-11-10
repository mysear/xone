#!/usr/bin/env bash
set -Eeuo pipefail
phase=mount_output
mkdir -p /output /inputs
mount -t virtiofs r01out /output
exec > >(tee -a /output/bootstrap.log /dev/hvc0) 2>&1
finish() {
  status=$?
  set +e
  printf '{"phase":"%s","exit_code":%s,"shutdown_requested":true}\n' "$phase" "$status" > /output/guest-exit.json
  sync
  systemctl poweroff --no-block
}
trap finish EXIT
echo R01_UBUNTU_BUILD_GUEST_BEGIN
phase=mount_readonly_inputs
mountpoint -q /inputs || mount -t iso9660 -o ro /dev/disk/by-label/R01INPUT /inputs
findmnt /inputs /output || true
if touch /inputs/r01-should-not-write 2>/output/readonly-medium-error.log; then exit 61; fi
sha256sum /inputs/source.tar.gz /inputs/platform.tar.bz2 /inputs/vendor.tar /inputs/sdk.tar /inputs/cargo-build-sbf /inputs/criterion.tar.bz2 > /output/guest-input-hashes.txt
phase=install_prerequisites
export DEBIAN_FRONTEND=noninteractive
if ! dpkg-query -W -f='${Status}' build-essential 2>/dev/null | grep -q 'install ok installed'; then
apt-get -o Acquire::Retries=0 -o Acquire::http::Timeout=25 -o Acquire::https::Timeout=25 update
apt-get -o Acquire::Retries=0 -o Acquire::http::Timeout=25 -o Acquire::https::Timeout=25 -o APT::Keep-Downloaded-Packages=true install -y --no-install-recommends build-essential pkg-config libssl-dev libudev-dev ca-certificates
fi
mkdir -p /opt/r01/src /opt/r01/vendor /opt/r01/platform-tools /opt/r01/sdk/sbf /build /vendor /platform-tools /sdk
phase=extract_pinned_inputs
if test -x /opt/r01/platform-tools/rust/bin/rustc && test -f /opt/r01/src/Cargo.lock && test -d /opt/r01/vendor/crates && test -f /opt/r01/sdk/sbf/env.sh; then
  echo R01_REUSE_PREVIOUSLY_VERIFIED_INPUT_EXTRACTION
else
tar -xzf /inputs/source.tar.gz --strip-components=1 -C /opt/r01/src
tar -xf /inputs/vendor.tar -C /opt/r01/vendor
tar -xjf /inputs/platform.tar.bz2 -C /opt/r01/platform-tools
tar -xf /inputs/sdk.tar -C /opt/r01/sdk/sbf
mkdir -p /opt/r01/sdk/sbf/dependencies/criterion
tar -xjf /inputs/criterion.tar.bz2 --strip-components=1 -C /opt/r01/sdk/sbf/dependencies/criterion
fi
ln -sfn /platform-tools /opt/r01/sdk/sbf/dependencies/platform-tools
touch /opt/r01/sdk/sbf/dependencies/platform-tools-v1.52.md /opt/r01/sdk/sbf/dependencies/criterion-v2.3.3.md
for pair in '/opt/r01/src /build' '/opt/r01/vendor /vendor' '/opt/r01/platform-tools /platform-tools' '/opt/r01/sdk /sdk'; do
  read -r source target <<< "$pair"
  mount --bind "$source" "$target"
  mount -o remount,bind,ro "$target"
done
cp /inputs/cargo-build-sbf /usr/local/bin/cargo-build-sbf
chmod 755 /usr/local/bin/cargo-build-sbf
mkdir -p /opt/r01/cargo-home /opt/r01/rustup-home /build-target /output/deploy
cat > /opt/r01/cargo-home/config.toml <<'EOF'
[source.crates-io]
replace-with = "r01-vendor"
[source.r01-vendor]
directory = "/vendor/crates"
[source.parity-git]
git = "https://github.com/raydium-io/parity-common"
replace-with = "r01-parity"
[source.r01-parity]
directory = "/vendor/parity"
EOF
export CARGO_HOME=/opt/r01/cargo-home
export RUSTUP_HOME=/opt/r01/rustup-home
export CARGO_TARGET_DIR=/build-target
export RUSTC=/platform-tools/rust/bin/rustc
export PATH=/platform-tools/rust/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin
export RUSTFLAGS=''
export CARGO_BUILD_JOBS=2
export GIT_CONFIG_NOSYSTEM=1
export GIT_CONFIG_GLOBAL=/dev/null
export GIT_TERMINAL_PROMPT=0
phase=linux_compiler_probe
python3 /inputs/run-build.py
phase=build_and_evidence_finished
echo R01_UBUNTU_BUILD_GUEST_END
