#!/usr/bin/env bash

set -euo pipefail

if [[ $# -ne 2 ]]; then
    echo "usage: $0 <platform-id> <release-binary>" >&2
    exit 2
fi

platform="$1"
binary="$2"
repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
dist_dir="${REGURGITATE_DIST_DIR:-$repo_root/dist}"

if [[ ! "$platform" =~ ^(linux|macos)-(x86_64|aarch64)$ ]]; then
    echo "invalid release platform: $platform" >&2
    exit 2
fi

if [[ ! -f "$binary" || ! -x "$binary" ]]; then
    echo "release binary is missing or not executable: $binary" >&2
    exit 2
fi

package_id="$(cargo pkgid --manifest-path "$repo_root/Cargo.toml" -p regurgitate)"
version="${package_id##*@}"
if [[ "$version" == "$package_id" ]]; then
    echo "could not determine the Regurgitate package version" >&2
    exit 1
fi
if [[ ! "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+[-+0-9A-Za-z.]*$ ]]; then
    echo "invalid Regurgitate package version: $version" >&2
    exit 1
fi

archive_name="regurgitate-v${version}-${platform}.tar.gz"
staging_root="$(mktemp -d)"

cleanup() {
    rm -rf -- "$staging_root"
}
trap cleanup EXIT

mkdir -p -- "$dist_dir"
install -m 0755 -- "$binary" "$staging_root/regurgitate"
install -m 0644 -- "$repo_root/README.md" "$repo_root/LICENSE" "$staging_root/"

source_date_epoch="${SOURCE_DATE_EPOCH:-$(git -C "$repo_root" show -s --format=%ct HEAD)}"
write_archive() {
    if tar --version 2>/dev/null | grep -Fq "GNU tar"; then
        tar \
            --sort=name \
            --mtime="@$source_date_epoch" \
            --owner=0 \
            --group=0 \
            --numeric-owner \
            -C "$staging_root" \
            -cf - \
            LICENSE README.md regurgitate
    else
        COPYFILE_DISABLE=1 tar \
            -C "$staging_root" \
            -cf - \
            LICENSE README.md regurgitate
    fi
}

write_archive | gzip -n > "$dist_dir/$archive_name"

(
    cd -- "$dist_dir"
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$archive_name" > SHA256SUMS
    else
        shasum -a 256 "$archive_name" > SHA256SUMS
    fi
)

printf '%s\n' "$dist_dir/$archive_name"
