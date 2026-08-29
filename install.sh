#!/bin/sh

set -eu

repository="vomitselfie/Regurgitate"
release_base_url="${REGURGITATE_RELEASE_BASE_URL:-https://github.com/$repository/releases/download}"
latest_url="${REGURGITATE_LATEST_URL:-https://github.com/$repository/releases/latest}"
agent="none"
bin_dir="${HOME:?HOME is not set}/.local/bin"
requested_version=""
replace_skill=false
temporary_directory=""
staged_binary=""

usage() {
    cat <<'EOF'
Install a verified Regurgitate release and optionally connect one agent.

Usage: install.sh [OPTIONS]

Options:
  --agent <codex|claude|none>  Connect an agent after installing (default: none)
  --bin-dir <directory>        Install directory (default: ~/.local/bin)
  --version <version>          Install one release instead of the latest
  --replace-skill              Explicitly replace a differing Regurgitate skill
  -h, --help                   Show this help
EOF
}

fail() {
    printf 'regurgitate installer: %s\n' "$*" >&2
    exit 1
}

cleanup() {
    if [ -n "$staged_binary" ] && [ -f "$staged_binary" ]; then
        rm -f -- "$staged_binary"
    fi
    if [ -n "$temporary_directory" ] && [ -d "$temporary_directory" ]; then
        rm -rf -- "$temporary_directory"
    fi
}
trap cleanup EXIT
trap 'exit 129' HUP
trap 'exit 130' INT
trap 'exit 143' TERM

while [ "$#" -gt 0 ]; do
    case "$1" in
        --agent)
            [ "$#" -ge 2 ] || fail "--agent requires a value"
            agent="$2"
            shift 2
            ;;
        --bin-dir)
            [ "$#" -ge 2 ] || fail "--bin-dir requires a value"
            bin_dir="$2"
            shift 2
            ;;
        --version)
            [ "$#" -ge 2 ] || fail "--version requires a value"
            requested_version="$2"
            shift 2
            ;;
        --replace-skill)
            replace_skill=true
            shift
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            fail "unknown option: $1"
            ;;
    esac
done

case "$agent" in
    codex|claude|none) ;;
    *) fail "--agent must be codex, claude, or none" ;;
esac
[ -n "$bin_dir" ] || fail "--bin-dir must not be empty"

for command in curl tar install mktemp; do
    command -v "$command" >/dev/null 2>&1 || fail "required command is unavailable: $command"
done

download() {
    source_url="$1"
    destination="$2"
    case "$source_url" in
        https://*)
            curl --proto '=https' --tlsv1.2 --fail --location --silent --show-error \
                --output "$destination" "$source_url"
            ;;
        file://*)
            [ "${REGURGITATE_ALLOW_FILE_URLS:-}" = "1" ] || \
                fail "file URLs are disabled"
            curl --fail --location --silent --show-error \
                --output "$destination" "$source_url"
            ;;
        *)
            fail "release URL must use HTTPS"
            ;;
    esac
}

resolve_latest_version() {
    case "$latest_url" in
        https://*) ;;
        *) fail "latest release URL must use HTTPS" ;;
    esac
    effective_url="$(
        curl --proto '=https' --tlsv1.2 --fail --location --silent --show-error \
            --output /dev/null --write-out '%{url_effective}' "$latest_url"
    )"
    latest_tag="${effective_url##*/}"
    case "$latest_tag" in
        v*) printf '%s\n' "${latest_tag#v}" ;;
        *) fail "could not determine the latest release version" ;;
    esac
}

version="${requested_version#v}"
if [ -z "$version" ]; then
    version="$(resolve_latest_version)"
fi
case "$version" in
    ''|*[!0-9A-Za-z.+-]*) fail "invalid release version: $version" ;;
esac
case "$version" in
    *.*.*) ;;
    *) fail "invalid release version: $version" ;;
esac

case "$(uname -s)-$(uname -m)" in
    Linux-x86_64) platform="linux-x86_64" ;;
    Darwin-arm64) platform="macos-aarch64" ;;
    Darwin-x86_64) platform="macos-x86_64" ;;
    *) fail "no release is available for this operating system and architecture" ;;
esac

tag="v$version"
archive="regurgitate-$tag-$platform.tar.gz"
temporary_directory="$(mktemp -d "${TMPDIR:-/tmp}/regurgitate-install.XXXXXX")"
archive_path="$temporary_directory/$archive"
checksums_path="$temporary_directory/SHA256SUMS"
unpack_directory="$temporary_directory/unpack"
mkdir -p -- "$unpack_directory"

printf 'Downloading Regurgitate %s for %s...\n' "$version" "$platform"
download "$release_base_url/$tag/$archive" "$archive_path"
download "$release_base_url/$tag/SHA256SUMS" "$checksums_path"

checksum_count="$(grep -Fc "  $archive" "$checksums_path" || true)"
[ "$checksum_count" = "1" ] || fail "SHA256SUMS must contain exactly one entry for $archive"
checksum_line="$(grep -F "  $archive" "$checksums_path")"
if command -v sha256sum >/dev/null 2>&1; then
    (cd "$temporary_directory" && printf '%s\n' "$checksum_line" | sha256sum --check -)
elif command -v shasum >/dev/null 2>&1; then
    (cd "$temporary_directory" && printf '%s\n' "$checksum_line" | shasum -a 256 --check -)
else
    fail "sha256sum or shasum is required to verify the release"
fi

tar -xzf "$archive_path" -C "$unpack_directory"
[ -f "$unpack_directory/regurgitate" ] || fail "release archive has no regurgitate binary"
[ -x "$unpack_directory/regurgitate" ] || fail "release binary is not executable"

mkdir -p -- "$bin_dir"
destination="$bin_dir/regurgitate"
staged_binary="$(mktemp "$bin_dir/.regurgitate.install.XXXXXX")"
install -m 0755 -- "$unpack_directory/regurgitate" "$staged_binary"
mv -f -- "$staged_binary" "$destination"
staged_binary=""

actual_version="$("$destination" --version)"
[ "$actual_version" = "regurgitate $version" ] || \
    fail "installed binary reported an unexpected version: $actual_version"

install_agent() {
    hook_command="$1"
    config_path="$2"
    skills_path="$3"

    "$destination" "$hook_command" --config "$config_path" \
        --executable "$destination" >/dev/null
    if [ "$replace_skill" = true ]; then
        "$destination" install-skill --target "$skills_path" \
            --executable "$destination" --replace >/dev/null
    else
        "$destination" install-skill --target "$skills_path" \
            --executable "$destination" >/dev/null
    fi

    "$destination" "$hook_command" --config "$config_path" \
        --executable "$destination" --apply
    if [ "$replace_skill" = true ]; then
        "$destination" install-skill --target "$skills_path" \
            --executable "$destination" --replace --apply
    else
        "$destination" install-skill --target "$skills_path" \
            --executable "$destination" --apply
    fi
}

case "$agent" in
    codex)
        codex_home="${CODEX_HOME:-$HOME/.codex}"
        install_agent install-codex-hook "$codex_home/config.toml" "$codex_home/skills"
        ;;
    claude)
        claude_home="${CLAUDE_CONFIG_DIR:-$HOME/.claude}"
        install_agent install-claude-hook "$claude_home/settings.json" "$claude_home/skills"
        ;;
    none) ;;
esac

printf 'Installed %s at %s\n' "$actual_version" "$destination"
if [ "$agent" != none ]; then
    printf 'Connected %s; restart it before using Regurgitate.\n' "$agent"
fi
case ":${PATH:-}:" in
    *":$bin_dir:"*) ;;
    *) printf 'Add %s to PATH to run regurgitate directly.\n' "$bin_dir" ;;
esac
