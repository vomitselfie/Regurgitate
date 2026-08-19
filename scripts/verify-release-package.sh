#!/usr/bin/env bash

set -euo pipefail

if [[ $# -ne 1 ]]; then
    echo "usage: $0 <platform-id>" >&2
    exit 2
fi

platform="$1"
repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
dist_dir="${REGURGITATE_DIST_DIR:-$repo_root/dist}"

if [[ ! "$platform" =~ ^(linux|macos)-(x86_64|aarch64)$ ]]; then
    echo "invalid release platform: $platform" >&2
    exit 2
fi

package_id="$(cargo pkgid --manifest-path "$repo_root/Cargo.toml" -p regurgitate)"
version="${package_id##*@}"
archive="$dist_dir/regurgitate-v${version}-${platform}.tar.gz"

if [[ ! -f "$archive" ]]; then
    echo "release archive is missing: $archive" >&2
    exit 1
fi

(
    cd -- "$dist_dir"
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum --check SHA256SUMS
    else
        shasum -a 256 --check SHA256SUMS
    fi
)

mkdir -p -- "$repo_root/target"
smoke_dir="$(mktemp -d "$repo_root/target/regurgitate-package-smoke.XXXXXX")"
cleanup() {
    rm -rf -- "$smoke_dir"
}
trap cleanup EXIT

tar -xzf "$archive" -C "$smoke_dir"
for packaged_file in LICENSE README.md regurgitate; do
    if [[ ! -f "$smoke_dir/$packaged_file" ]]; then
        echo "release archive is missing $packaged_file" >&2
        exit 1
    fi
done
if [[ ! -x "$smoke_dir/regurgitate" ]]; then
    echo "packaged Regurgitate binary is not executable" >&2
    exit 1
fi

actual="$($smoke_dir/regurgitate --version)"
if [[ "$actual" != "regurgitate $version" ]]; then
    echo "packaged binary reported an unexpected version: $actual" >&2
    exit 1
fi
