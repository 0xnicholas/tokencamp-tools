#!/bin/sh
# Build one tcamp release archive and its checksum files.
set -eu

usage() {
    printf 'usage: %s <version> <target> <tcamp-binary> <output-dir>\n' "$0" >&2
}

if [ "$#" -ne 4 ]; then
    usage
    exit 2
fi

version=$1
target=$2
binary=$3
out_dir=$4

case "$version" in
    v*) version=${version#v} ;;
esac
case "$version" in
    '' | *[!0-9A-Za-z.-]*)
        printf 'package.sh: invalid version: %s\n' "$1" >&2
        exit 2
        ;;
esac
case "$target" in
    '' | *[!0-9A-Za-z._-]*)
        printf 'package.sh: invalid target: %s\n' "$target" >&2
        exit 2
        ;;
esac

[ -f "$binary" ] || {
    printf 'package.sh: binary not found: %s\n' "$binary" >&2
    exit 1
}

repo_dir=$(CDPATH='' cd "$(dirname "$0")/.." && pwd)
license="$repo_dir/LICENSE"
[ -f "$license" ] || {
    printf 'package.sh: LICENSE not found: %s\n' "$license" >&2
    exit 1
}

asset="tcamp-$version-$target.tar.gz"
tmp_dir=$(mktemp -d "${TMPDIR:-/tmp}/tcamp-package.XXXXXX")
trap 'rm -rf "$tmp_dir"' 0 1 2 15
stage="$tmp_dir/tcamp-$version-$target"
mkdir "$stage"
install -m 0755 "$binary" "$stage/tcamp"
install -m 0644 "$license" "$stage/LICENSE"

mkdir -p "$out_dir"
tar -czf "$out_dir/$asset" -C "$stage" tcamp LICENSE

sha256_for() {
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$1"
    elif command -v shasum >/dev/null 2>&1; then
        shasum -a 256 "$1"
    else
        printf 'package.sh: neither sha256sum nor shasum is available\n' >&2
        exit 1
    fi
}

(
    cd "$out_dir"
    sha256_for "$asset" >"$asset.sha256"
)

(
    cd "$out_dir"
    : >SHA256SUMS
    for archive in *.tar.gz; do
        [ -f "$archive" ] || continue
        sha256_for "$archive" >>SHA256SUMS
    done
)

printf '%s\n' "$out_dir/$asset"
