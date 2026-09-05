#!/usr/bin/env bash
# Exercise AoE's real parser without installing/granting/running a plugin.
set -euo pipefail

repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
aoe_binary="${1:-aoe}"
if [[ "$(uname -s)" != Linux ]]; then
    echo "This isolated XDG fixture currently requires Linux." >&2
    exit 2
fi

test_root="$(mktemp -d)"
trap 'rm -rf -- "$test_root"' EXIT
plugin_dir="$test_root/config/agent-of-empires/plugins/vomitselfie.regurgitate"
mkdir -p -- "$plugin_dir"
cp -- "$repo_root/aoe-plugin.toml" "$plugin_dir/aoe-plugin.toml"

inspect_manifest() {
    XDG_CONFIG_HOME="$test_root/config" \
    XDG_DATA_HOME="$test_root/data" \
    XDG_CACHE_HOME="$test_root/cache" \
    XDG_STATE_HOME="$test_root/state" \
        "$aoe_binary" plugin info vomitselfie.regurgitate
}

info="$(inspect_manifest)"
grep -F -- 'home-pane (overview)' <<<"$info" >/dev/null
grep -F -- 'status-bar (health)' <<<"$info" >/dev/null
grep -F -- 'needs approval' <<<"$info" >/dev/null

# Prove this check catches the retired slot that our TOML-only test missed.
sed -i 's/slot = "home-pane"/slot = "settings-page"/' "$plugin_dir/aoe-plugin.toml"
if inspect_manifest >/dev/null 2>&1; then
    echo "AoE unexpectedly accepted the retired settings-page slot." >&2
    exit 1
fi
echo "AoE manifest compatibility passed ($("$aoe_binary" --version))."
