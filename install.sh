#!/bin/sh
# Install tcamp from a GitHub releases page.
set -eu

usage() {
    cat >&2 <<'EOF'
usage: install.sh [--version <version>]

environment:
  TCAMP_INSTALL_DIR   destination directory (default: $HOME/.local/bin)
  TCAMP_VERSION       release tag or version to install (default: latest)
  TCAMP_RELEASES_URL  releases base URL
EOF
}

fail() {
    printf 'install.sh: %s\n' "$1" >&2
    exit "${2:-1}"
}

install_dir=${TCAMP_INSTALL_DIR:-}
version=${TCAMP_VERSION:-}

while [ "$#" -gt 0 ]; do
    case "$1" in
        --version)
            [ "$#" -ge 2 ] && [ -n "$2" ] || {
                usage
                exit 2
            }
            version=$2
            shift 2
            ;;
        --version=*)
            version=${1#--version=}
            [ -n "$version" ] || {
                usage
                exit 2
            }
            shift
            ;;
        --help | -h)
            usage
            exit 0
            ;;
        *)
            printf 'install.sh: unknown argument: %s\n' "$1" >&2
            usage
            exit 2
            ;;
    esac
done

if [ -z "$install_dir" ]; then
    if [ -n "${HOME:-}" ]; then
        install_dir=$HOME/.local/bin
    else
        fail 'HOME is not set and TCAMP_INSTALL_DIR is empty' 2
    fi
fi

releases_url=${TCAMP_RELEASES_URL:-https://github.com/0xnicholas/tokencamp-tools/releases}
while [ "${releases_url%/}" != "$releases_url" ]; do
    releases_url=${releases_url%/}
done

system=$(uname -s 2>/dev/null || true)
machine=$(uname -m 2>/dev/null || true)
case "$system" in
    Darwin) target_os=apple-darwin ;;
    Linux) target_os=unknown-linux-musl ;;
    *)
        fail "unsupported operating system: ${system:-unknown}; supported: macOS and Linux" 2
        ;;
esac
case "$machine" in
    arm64 | aarch64) target_arch=aarch64 ;;
    x86_64 | amd64) target_arch=x86_64 ;;
    *)
        fail "unsupported architecture: ${machine:-unknown}; supported: arm64 and x86_64" 2
        ;;
esac
target="$target_arch-$target_os"

command -v curl >/dev/null 2>&1 || fail 'curl is required' 1
command -v tar >/dev/null 2>&1 || fail 'tar is required' 1
command -v install >/dev/null 2>&1 || fail 'install is required' 1

case "$version" in
    '')
        download_base="$releases_url/latest/download"
        ;;
    v*)
        asset_version=${version#v}
        download_base="$releases_url/download/$version"
        ;;
    *)
        asset_version=$version
        download_base="$releases_url/download/v$version"
        ;;
esac

tmp_dir=$(mktemp -d "${TMPDIR:-/tmp}/tcamp-install.XXXXXX") ||
    fail 'could not create a temporary directory' 1
tmp_bin=
previous_bin=
had_previous=0
cleanup() {
    if [ -n "$tmp_bin" ]; then
        rm -f "$tmp_bin" || true
    fi
    rm -rf "$tmp_dir" || true
}
trap cleanup 0 1 2 15

sums="$tmp_dir/SHA256SUMS"
curl -fsSL "$download_base/SHA256SUMS" -o "$sums" ||
    fail "could not download SHA256SUMS from $download_base" 1

if [ -n "$version" ]; then
    asset="tcamp-$asset_version-$target.tar.gz"
    expected_hash=$(awk -v name="$asset" '$2 == name { print $1 }' "$sums")
    [ -n "$expected_hash" ] ||
        fail "refusing to install: $asset is not listed in SHA256SUMS" 1
else
    suffix="-$target.tar.gz"
    selection=$(awk -v suffix="$suffix" '
        index($2, "tcamp-") == 1 &&
            length($2) > length(suffix) &&
            substr($2, length($2) - length(suffix) + 1) == suffix {
                count++
                name = $2
            }
        END { print count + 0, name }
    ' "$sums")
    match_count=${selection%% *}
    asset=${selection#* }
    [ "$match_count" -eq 1 ] ||
        fail "refusing to install: expected one archive for $target in SHA256SUMS" 1
    expected_hash=$(awk -v name="$asset" '$2 == name { print $1 }' "$sums")
    asset_version=${asset#tcamp-}
    asset_version=${asset_version%"-$target.tar.gz"}
fi

case "${asset_version:-}" in
    '' | *[!0-9A-Za-z.-]*) fail "invalid version: $version" 2 ;;
esac

case "$expected_hash" in
    '' | *[!0-9a-f]*) fail "refusing to install: invalid checksum for $asset" 1 ;;
esac
[ "${#expected_hash}" -eq 64 ] ||
    fail "refusing to install: invalid checksum for $asset" 1

tarball="$tmp_dir/$asset"
curl -fsSL "$download_base/$asset" -o "$tarball" ||
    fail "could not download $asset" 1

if command -v sha256sum >/dev/null 2>&1; then
    actual_hash=$(sha256sum "$tarball" | awk '{ print $1 }')
elif command -v shasum >/dev/null 2>&1; then
    actual_hash=$(shasum -a 256 "$tarball" | awk '{ print $1 }')
else
    fail 'refusing to install: neither sha256sum nor shasum is available' 1
fi
[ "$actual_hash" = "$expected_hash" ] ||
    fail "refusing to install: checksum mismatch for $asset" 1

extract_dir="$tmp_dir/extract"
mkdir "$extract_dir"
tar -xzf "$tarball" -C "$extract_dir" ||
    fail "could not extract $asset" 1
extracted="$extract_dir/tcamp"
[ -f "$extracted" ] ||
    fail "refusing to install: $asset does not contain tcamp" 1

if [ ! -d "$install_dir" ]; then
    (umask 022 && mkdir -p "$install_dir") ||
        fail "could not create $install_dir" 1
fi
[ -d "$install_dir" ] || fail "not a directory: $install_dir" 1

tmp_bin="$install_dir/.tcamp.tmp.$$"
if [ -f "$install_dir/tcamp" ]; then
    previous_bin="$tmp_dir/tcamp.previous"
    cp -p "$install_dir/tcamp" "$previous_bin" ||
        fail "could not back up $install_dir/tcamp" 1
    had_previous=1
fi

install -m 0755 "$extracted" "$tmp_bin" ||
    fail "could not stage tcamp in $install_dir" 1
mv -f "$tmp_bin" "$install_dir/tcamp" ||
    fail "could not replace $install_dir/tcamp" 1
tmp_bin=

installed_output=$("$install_dir/tcamp" --version 2>/dev/null) ||
    installed_output=
if [ "$installed_output" != "tcamp $asset_version" ]; then
    if [ "$had_previous" -eq 1 ]; then
        mv -f "$previous_bin" "$install_dir/tcamp" || true
    else
        rm -f "$install_dir/tcamp" || true
    fi
    fail "refusing to install: installed tcamp reported '${installed_output:-no output}' instead of 'tcamp $asset_version'" 1
fi

printf 'Installed tcamp %s to %s\n' "$asset_version" "$install_dir/tcamp"
case ":${PATH:-}:" in
    *":$install_dir:"*) ;;
    *) printf '%s\n' "export PATH=\"$install_dir:\$PATH\"" ;;
esac
