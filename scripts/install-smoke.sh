#!/bin/sh
# Exercise install.sh end to end against locally built release assets.
set -eu

script_dir=$(CDPATH='' cd "$(dirname "$0")" && pwd)
repo_dir=$(dirname "$script_dir")
cd "$repo_dir"

work_dir=$(mktemp -d "${TMPDIR:-/tmp}/tcamp-install-smoke.XXXXXX")
trap 'rm -rf "$work_dir"' 0 1 2 15

fail() {
    printf 'install-smoke: %s\n' "$1" >&2
    exit 1
}

if [ -n "${TCAMP_BINARY:-}" ]; then
    binary=$TCAMP_BINARY
else
    cargo build --locked
    binary="$repo_dir/target/debug/tcamp"
fi
[ -x "$binary" ] || fail "binary not found: $binary"

version_output=$("$binary" --version)
case "$version_output" in
    'tcamp '*) version=${version_output#tcamp } ;;
    *) fail "unexpected --version output: $version_output" ;;
esac

system=$(uname -s)
machine=$(uname -m)
case "$system" in
    Darwin) target_os=apple-darwin ;;
    Linux) target_os=unknown-linux-musl ;;
    *) fail "unsupported operating system: $system" ;;
esac
case "$machine" in
    arm64 | aarch64) target_arch=aarch64 ;;
    x86_64 | amd64) target_arch=x86_64 ;;
    *) fail "unsupported architecture: $machine" ;;
esac
target="$target_arch-$target_os"

releases="$work_dir/releases"
latest="$releases/latest/download"
pinned="$releases/download/v$version"
mkdir -p "$latest" "$pinned"
sh "$script_dir/package.sh" "$version" "$target" "$binary" "$latest" >/dev/null
asset="tcamp-$version-$target.tar.gz"
cp "$latest/$asset" "$latest/$asset.sha256" "$latest/SHA256SUMS" "$pinned/"

installer_tmp="$work_dir/installer-tmp"
mkdir "$installer_tmp"

run_installer() {
    TCAMP_RELEASES_URL=$1 \
        TCAMP_INSTALL_DIR=$2 \
        TCAMP_VERSION=$3 \
        TMPDIR="$installer_tmp" \
        sh "$repo_dir/install.sh"
}

assert_version() {
    installed=$("$1/tcamp" --version)
    [ "$installed" = "tcamp $version" ] ||
        fail "installed version is '$installed', expected 'tcamp $version'"
}

positive_dir="$work_dir/positive"
run_installer "file://$releases" "$positive_dir" ''
assert_version "$positive_dir"

pinned_dir="$work_dir/pinned"
run_installer "file://$releases" "$pinned_dir" "v$version"
assert_version "$pinned_dir"

tampered_root="$work_dir/tampered-releases"
tampered="$tampered_root/latest/download"
mkdir -p "$tampered"
cp "$latest/$asset" "$tampered/$asset"
printf '%064d  %s\n' 0 "$asset" >"$tampered/SHA256SUMS"

negative_dir="$work_dir/negative"
if run_installer "file://$tampered_root" "$negative_dir" '' \
    >"$work_dir/negative.out" 2>"$work_dir/negative.err"; then
    fail 'tampered checksum was accepted'
else
    negative_status=$?
fi
[ "$negative_status" -eq 1 ] ||
    fail "tampered checksum exited $negative_status, expected 1"
grep -F 'refusing to install' "$work_dir/negative.out" "$work_dir/negative.err" >/dev/null ||
    fail 'tampered checksum did not print a refusal'
[ ! -e "$negative_dir/tcamp" ] ||
    fail 'tampered checksum installed a binary'

preserved_dir="$work_dir/preserved"
mkdir "$preserved_dir"
printf '#!/bin/sh\nprintf "%%s\\n" "tcamp 0.0.0"\n' >"$preserved_dir/tcamp"
chmod 0755 "$preserved_dir/tcamp"
cp "$preserved_dir/tcamp" "$work_dir/preserved.expected"
if run_installer "file://$tampered_root" "$preserved_dir" '' \
    >"$work_dir/preserved.out" 2>"$work_dir/preserved.err"; then
    fail 'tampered checksum replaced an existing install'
else
    preserved_status=$?
fi
[ "$preserved_status" -eq 1 ] ||
    fail "preserved install run exited $preserved_status, expected 1"
cmp "$preserved_dir/tcamp" "$work_dir/preserved.expected" ||
    fail 'existing binary was modified'

[ -z "$(ls -A "$installer_tmp")" ] ||
    fail 'installer left temporary files behind'

sh -n "$repo_dir/install.sh"
sh -n "$script_dir/package.sh"
sh -n "$script_dir/install-smoke.sh"
sh -n "$script_dir/check-release-gates.sh"
if command -v shellcheck >/dev/null 2>&1; then
    shellcheck "$repo_dir/install.sh" "$script_dir/package.sh" "$script_dir/install-smoke.sh" "$script_dir/check-release-gates.sh"
fi

printf 'install-smoke: ok\n'
