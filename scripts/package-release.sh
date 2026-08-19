#!/usr/bin/env bash

set -euo pipefail

if [[ $# -ne 2 ]]; then
    echo "usage: $0 <target-triple> <release-binary>" >&2
    exit 2
fi

target="$1"
binary="$2"
repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
dist_dir="${PRAXIS_DIST_DIR:-$repo_root/dist}"

if [[ ! "$target" =~ ^[A-Za-z0-9_.-]+$ ]]; then
    echo "invalid target triple: $target" >&2
    exit 2
fi

if [[ ! -f "$binary" || ! -x "$binary" ]]; then
    echo "release binary is missing or not executable: $binary" >&2
    exit 2
fi

package_id="$(cargo pkgid --manifest-path "$repo_root/Cargo.toml" -p praxis)"
version="${package_id##*@}"
if [[ "$version" == "$package_id" ]]; then
    echo "could not determine the Praxis package version" >&2
    exit 1
fi
if [[ ! "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+[-+0-9A-Za-z.]*$ ]]; then
    echo "invalid Praxis package version: $version" >&2
    exit 1
fi

package_name="praxis-v${version}-${target}"
archive_name="${package_name}.tar.gz"
staging_root="$(mktemp -d)"

cleanup() {
    rm -rf -- "$staging_root"
}
trap cleanup EXIT

package_dir="$staging_root/$package_name"
mkdir -p -- "$package_dir" "$dist_dir"
install -m 0755 -- "$binary" "$package_dir/praxis"
install -m 0644 -- "$repo_root/README.md" "$repo_root/LICENSE" "$package_dir/"

source_date_epoch="${SOURCE_DATE_EPOCH:-$(git -C "$repo_root" show -s --format=%ct HEAD)}"
tar \
    --sort=name \
    --mtime="@$source_date_epoch" \
    --owner=0 \
    --group=0 \
    --numeric-owner \
    -C "$staging_root" \
    -cf - \
    "$package_name" \
    | gzip -n > "$dist_dir/$archive_name"

(
    cd -- "$dist_dir"
    sha256sum "$archive_name" > SHA256SUMS
)

printf '%s\n' "$dist_dir/$archive_name"
