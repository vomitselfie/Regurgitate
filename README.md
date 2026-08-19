# Regurgitate

**Regurgitate remembers what worked—not what you wrote.**

Coding agents often repeat the same investigation every time you open a new
session. Regurgitate gives them a small, private notebook of procedural
evidence: which kind of approach was tried, whether it worked, and how often.

It remembers things like “targeted verification has worked reliably in this
project.” It does not remember what you said, what the agent said, or the
contents of your files.

## What changes for you?

| Without Regurgitate | With Regurgitate |
| --- | --- |
| A new session starts from zero | A new session can check what worked before |
| You paste old logs or explain past attempts | The agent requests a small aggregate summary |
| One agent's lessons stay with that agent | Codex, Claude Code, AoE, and other CLI-capable agents can share the same local evidence |
| “Memory” may mean storing entire conversations | Regurgitate stores only controlled categories and outcomes |

The practical goal is less repeated exploration, fewer wasted tokens, and more
consistent choices across sessions. Regurgitate is procedural memory, not a
chat archive or a replacement for project documentation.

```text
agent hook → safe category + outcome → encrypted local history → bounded recall
```

## What does it store?

Regurgitate stores a deliberately small vocabulary: categories such as
`patch`, `test`, or `research`; controlled strategies; and `success`,
`failure`, or `unknown` outcomes. Recall returns counts and guidance, never
individual events.

Regurgitate never stores:

- prompts, responses, or reasoning;
- source code or file contents;
- commands, arguments, or terminal output;
- tool inputs, tool results, URLs, or environment values.

Private event data and project mappings are encrypted before they reach SQLite.
The encryption key stays in Linux Secret Service or macOS Keychain. Nothing is
sent to a Regurgitate cloud service because there is no Regurgitate cloud
service.

For the exact boundary—including the small amount of structural database
metadata—see [Architecture](docs/architecture.md).

## Supported today

- Linux x86-64
- macOS on Apple Silicon or Intel
- Codex native recording hooks
- Claude Code native recording hooks
- Agent of Empires plugin with guided Codex and Claude Code setup
- Bounded recall from any agent that can run the CLI

Windows is not supported. Regurgitate is still an early project, so keep
normal project documentation and backups.

## Get Regurgitate running

### Easiest: install through Agent of Empires

If you use AoE 1.14 or newer, it can download Regurgitate and connect your
agent:

```bash
aoe plugin install gh:vomitselfie/Regurgitate
aoe serve
```

Open the plugin settings page in AoE and click **Set up Codex** or **Set up
Claude Code**. The same actions are available in AoE's command palette as
`Regurgitate: set up Codex` and `Regurgitate: set up Claude Code`.

That setup action installs both pieces an agent needs: a recording hook and a
small recall skill. It uses the executable AoE already downloaded, so you do
not need to install `regurgitate` on your `PATH` or merge configuration by hand.
Existing settings and personal hooks are preserved; if Regurgitate cannot add
itself safely, it stops and reports that the setup needs attention. Restart
the selected agent afterward.

AoE will ask to approve `runtime.worker`, `fs.read`, and `fs.write`. The worker
permission powers the plugin's status/setup page. File access is used only to
inspect and add the hook and skill beneath the selected agent's user config.

Installing the plugin alone downloads the complete Regurgitate program but
does not silently edit Codex or Claude Code. The explicit setup action makes
that final connection. Once connected, recording and recall continue to work
without the AoE page being open. Future Regurgitate releases update with:

```bash
aoe plugin update vomitselfie.regurgitate
```

#### Moving from Praxis

The new plugin ID means AoE treats Regurgitate as a new plugin rather than an
update to Praxis. Install Regurgitate, use its settings page to set up each
agent you previously connected, then remove the old plugin:

```bash
aoe plugin install gh:vomitselfie/Regurgitate
aoe serve
# After completing agent setup in AoE and closing it:
aoe plugin uninstall vomitselfie.praxis
```

Setup replaces old Praxis hook commands instead of adding duplicates. It moves
an existing `praxis-recall` skill into a hidden `.regurgitate-retired`
directory before installing `regurgitate-recall`, preserving any personal
changes for review. Existing encrypted history remains readable immediately;
the data directory and credential-store key migrate on the next recorded
event. The encryption format retains compatibility with all existing events.

Do not also enable the AoE transcript fallback for Codex after connecting the
native Codex hook; the two sources can observe the same session.

### Standalone installation

You can also install Regurgitate without AoE. There are two pieces:

1. The `regurgitate` program records and reads encrypted history.
2. A small agent skill tells your agent when to use it.

#### 1. Install the program

Open the [latest release](https://github.com/vomitselfie/Regurgitate/releases/latest)
and download the archive for your computer:

| Computer | Asset name ends with |
| --- | --- |
| Linux, x86-64 | `linux-x86_64.tar.gz` |
| Apple Silicon Mac | `macos-aarch64.tar.gz` |
| Intel Mac | `macos-x86_64.tar.gz` |

Extract the archive and put the `regurgitate` file in a directory on your `PATH`,
such as `~/.local/bin`. Then check it:

```bash
regurgitate --version
```

If that says `command not found`, reopen your terminal or ask your agent to add
`~/.local/bin` to your `PATH`.

If “put it on your PATH” is unfamiliar, ask your coding agent:

> Install the latest Regurgitate release from vomitselfie/Regurgitate for this
> computer. Verify it against SHA256SUMS, place it in ~/.local/bin, and confirm
> that `regurgitate --version` works.

<details>
<summary>Terminal install using GitHub CLI</summary>

```bash
release="$(gh release view --repo vomitselfie/Regurgitate --json tagName --jq .tagName)"
case "$(uname -s)-$(uname -m)" in
  Linux-x86_64) platform="linux-x86_64" ;;
  Darwin-arm64) platform="macos-aarch64" ;;
  Darwin-x86_64) platform="macos-x86_64" ;;
  *) echo "Regurgitate has no release for this platform." >&2; return 1 ;;
esac
archive="regurgitate-${release}-${platform}.tar.gz"
gh release download "$release" --repo vomitselfie/Regurgitate \
  --pattern "$archive" --pattern SHA256SUMS
checksum="$(grep -F "  $archive" SHA256SUMS)"
if command -v sha256sum >/dev/null 2>&1; then
  printf '%s\n' "$checksum" | sha256sum --check
else
  printf '%s\n' "$checksum" | shasum -a 256 --check
fi
unpack_dir="$(mktemp -d)"
tar -xzf "$archive" -C "$unpack_dir"
mkdir -p "$HOME/.local/bin"
install -m 0755 "$unpack_dir/regurgitate" "$HOME/.local/bin/regurgitate"
regurgitate --version
```

</details>

#### 2. Connect your agent

Choose the agent you use. You can connect more than one; they share the same
local encrypted history.

#### Codex

```bash
codex_home="${CODEX_HOME:-$HOME/.codex}"
regurgitate install-codex-hook --config "$codex_home/config.toml" --apply
regurgitate install-skill --target "$codex_home/skills" --apply
```

Restart Codex. The first recall or learning request may ask for permission to
reach your operating system's credential store. Approve only the exact
`regurgitate recall` or `regurgitate learn` command—not a general shell
command. Codex's
[command rules](https://learn.chatgpt.com/docs/agent-configuration/rules) can
remember that narrow approval.

#### Claude Code

```bash
claude_home="${CLAUDE_CONFIG_DIR:-$HOME/.claude}"
regurgitate install-claude-hook --config "$claude_home/settings.json" --apply
regurgitate install-skill --target "$claude_home/skills" --apply
```

Restart Claude Code. The installer adds to both terminal tool events while
preserving existing settings and personal hooks. These are Claude Code's
documented user-level [skill](https://code.claude.com/docs/en/skills) and
[hook](https://code.claude.com/docs/en/hooks) locations.

#### Optional AoE transcript fallback

Older AoE/Codex setups can record at session boundaries instead of using the
native Codex hook:

```bash
regurgitate install-aoe-hook \
  --config "$HOME/.config/agent-of-empires/config.toml" \
  --apply
```

Use this only when the native Codex hook is not installed.

#### 3. Check it

Use your agent normally for a moment, then run:

```bash
regurgitate status
regurgitate recall --project "$PWD" --query "what approach should I try?"
```

`not_configured` before the first recorded event is normal. Once a hook records
an event, status should report ready history. Recall may still be empty until
Regurgitate has useful evidence for that project. Regurgitate does not retain
the query text, although your shell may keep commands you type in its own
history.

## Day-to-day use

Once the hook and skill are installed, there is usually nothing to manage. The
agent records tiny sanitized observations in the background and requests a
bounded recall when prior evidence could help.

Useful commands:

| Command | What it does |
| --- | --- |
| `regurgitate status` | Checks the key store and encrypted history |
| `regurgitate recall --project "$PWD"` | Shows bounded aggregate evidence for this project |
| `regurgitate forget --project "$PWD"` | Previews deleting this project's history |
| `regurgitate forget --project "$PWD" --apply` | Deletes this project's history |
| `regurgitate prune --keep-recent 10000` | Previews a global retention cleanup |
| `regurgitate prune --keep-recent 10000 --apply` | Applies that cleanup |
| `regurgitate --help` | Lists every command |

Destructive commands preview what they will remove unless you add `--apply`.

## Token impact

Recording adds no chat output: successful hooks are silent. Recall defaults to
an approximate 300-token output limit and returns aggregates instead of
replaying history. The core skill plus a default recall has a
Regurgitate-controlled ceiling of about 1,046 tokens, 42% smaller than the
original implementation. AoE setup adds one short instruction containing its
local executable path, so that exact total varies slightly by computer.

That is a context-size ceiling, not a promise about provider billing. The
payoff comes when a small recall prevents an agent from repeating a much larger
investigation or asking you to paste old context.

## Safety behavior

- Regurgitate fails closed if Secret Service or Keychain is unavailable or
  locked.
- It never falls back to a plaintext key or plaintext history.
- Agent sandbox access should be granted only to exact `regurgitate`
  subcommands.
- `status`, recall, and deletion previews do not create missing state.
- Hook input is reduced to an allowlisted event before storage.

## More detail

- [Architecture and privacy boundaries](docs/architecture.md)
- [Roadmap](docs/roadmap.md)
- [Upstream source-format notes](docs/source-format-notes.md)
- [Contributor and release guide](docs/releasing.md)
- [`regurgitate-recall` skill](skills/regurgitate-recall/SKILL.md)

Regurgitate is MIT licensed.
