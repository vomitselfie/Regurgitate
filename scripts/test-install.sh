#!/usr/bin/env bash

set -euo pipefail

repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
binary="${1:-$repo_root/target/debug/regurgitate}"

if [[ ! -x "$binary" ]]; then
    echo "installer test binary is missing or not executable: $binary" >&2
    exit 2
fi

package_id="$(cargo pkgid --manifest-path "$repo_root/Cargo.toml" -p regurgitate)"
version="${package_id##*@}"
case "$(uname -s)-$(uname -m)" in
    Linux-x86_64) platform="linux-x86_64" ;;
    Darwin-arm64) platform="macos-aarch64" ;;
    Darwin-x86_64) platform="macos-x86_64" ;;
    *)
        echo "installer test does not support this platform" >&2
        exit 2
        ;;
esac

mkdir -p -- "$repo_root/target"
test_root="$(mktemp -d "$repo_root/target/regurgitate-installer-test.XXXXXX")"
cleanup() {
    rm -rf -- "$test_root"
}
trap cleanup EXIT

release_root="$test_root/releases"
release_directory="$release_root/v$version"
home="$test_root/home"
codex_home="$test_root/codex"
claude_home="$test_root/claude"
bin_dir="$test_root/bin with spaces"
mkdir -p -- "$release_directory" "$home"

REGURGITATE_DIST_DIR="$release_directory" \
    "$repo_root/scripts/package-release.sh" "$platform" "$binary" >/dev/null

run_installer() {
    selected_agent="${1:-codex}"
    HOME="$home" \
    CODEX_HOME="$codex_home" \
    CLAUDE_CONFIG_DIR="$claude_home" \
    REGURGITATE_RELEASE_BASE_URL="file://$release_root" \
    REGURGITATE_ALLOW_FILE_URLS=1 \
        sh "$repo_root/install.sh" \
            --version "$version" \
            --agent "$selected_agent" \
            --bin-dir "$bin_dir"
}

run_installer codex >/dev/null

installed="$bin_dir/regurgitate"
skill="$codex_home/skills/regurgitate-recall/SKILL.md"
config="$codex_home/config.toml"
test -x "$installed"
test -f "$skill"
test -f "$config"
test "$("$installed" --version)" = "regurgitate $version"
grep -F "'$installed' record-hook --agent codex" "$config" >/dev/null
grep -F "with \`'$installed'\`" "$skill" >/dev/null

before_config="$(sha256sum "$config")"
before_skill="$(sha256sum "$skill")"
run_installer codex >/dev/null
test "$(sha256sum "$config")" = "$before_config"
test "$(sha256sum "$skill")" = "$before_skill"

run_installer claude >/dev/null
claude_config="$claude_home/settings.json"
claude_skill="$claude_home/skills/regurgitate-recall/SKILL.md"
test -f "$claude_config"
test -f "$claude_skill"
grep -F "'$installed' record-hook --agent claude" "$claude_config" >/dev/null
grep -F "'$installed' preflight --agent claude" "$claude_config" >/dev/null
grep -F "with \`'$installed'\`" "$claude_skill" >/dev/null

bad_release_root="$test_root/bad-releases"
bad_release_directory="$bad_release_root/v$version"
bad_bin="$test_root/bad-bin"
mkdir -p -- "$bad_release_directory"
cp -- "$release_directory/regurgitate-v$version-$platform.tar.gz" "$bad_release_directory/"
printf '%064d  regurgitate-v%s-%s.tar.gz\n' 0 "$version" "$platform" \
    > "$bad_release_directory/SHA256SUMS"

if HOME="$home" \
    REGURGITATE_RELEASE_BASE_URL="file://$bad_release_root" \
    REGURGITATE_ALLOW_FILE_URLS=1 \
        sh "$repo_root/install.sh" \
            --version "$version" \
            --bin-dir "$bad_bin" >/dev/null 2>&1; then
    echo "installer accepted a bad release checksum" >&2
    exit 1
fi
test ! -e "$bad_bin/regurgitate"

printf 'standalone installer: OK\n'
