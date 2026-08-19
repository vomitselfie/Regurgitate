# Praxis

**Praxis helps your AI coding agents remember what worked—without saving your
conversations, code, commands, or tool output.**

Coding agents often repeat the same investigation every time you open a new
session. Praxis gives them a small, private notebook of procedural evidence:
which kind of approach was tried, whether it worked, and how often.

It remembers things like “targeted verification has worked reliably in this
project.” It does not remember what you said, what the agent said, or the
contents of your files.

## What changes for you?

| Without Praxis | With Praxis |
| --- | --- |
| A new session starts from zero | A new session can check what worked before |
| You paste old logs or explain past attempts | The agent requests a small aggregate summary |
| One agent's lessons stay with that agent | Codex, Claude Code, AoE, and other CLI-capable agents can share the same local evidence |
| “Memory” may mean storing entire conversations | Praxis stores only controlled categories and outcomes |

The practical goal is less repeated exploration, fewer wasted tokens, and more
consistent choices across sessions. Praxis is procedural memory, not a chat
archive or a replacement for project documentation.

```text
agent hook → safe category + outcome → encrypted local history → bounded recall
```

## What does it store?

Praxis stores a deliberately small vocabulary: categories such as `patch`,
`test`, or `research`; controlled strategies; and `success`, `failure`, or
`unknown` outcomes. Recall returns counts and guidance, never individual
events.

Praxis never stores:

- prompts, responses, or reasoning;
- source code or file contents;
- commands, arguments, or terminal output;
- tool inputs, tool results, URLs, or environment values.

Private event data and project mappings are encrypted before they reach SQLite.
The encryption key stays in Linux Secret Service or macOS Keychain. Nothing is
sent to a Praxis cloud service because there is no Praxis cloud service.

For the exact boundary—including the small amount of structural database
metadata—see [Architecture](docs/architecture.md).

## Supported today

- Linux x86-64
- macOS on Apple Silicon or Intel
- Codex native recording hooks
- Claude Code native recording hooks
- Agent of Empires plugin health UI and Codex transcript fallback
- Bounded recall from any agent that can run the CLI

Windows is not supported. Praxis is still an early project, so keep normal
project documentation and backups.

## Get Praxis running

There are two pieces:

1. The `praxis` program records and reads the encrypted history.
2. A small agent skill tells your agent when to use it.

The Agent of Empires plugin is optional. It provides health and status inside
AoE, but does not replace the recording hook.

### 1. Install the program

Open the [latest release](https://github.com/vomitselfie/aoe-praxis/releases/latest)
and download the archive for your computer:

| Computer | Asset name ends with |
| --- | --- |
| Linux, x86-64 | `linux-x86_64.tar.gz` |
| Apple Silicon Mac | `macos-aarch64.tar.gz` |
| Intel Mac | `macos-x86_64.tar.gz` |

Extract the archive and put the `praxis` file in a directory on your `PATH`,
such as `~/.local/bin`. Then check it:

```bash
praxis --version
```

If that says `command not found`, reopen your terminal or ask your agent to add
`~/.local/bin` to your `PATH`.

If “put it on your PATH” is unfamiliar, ask your coding agent:

> Install the latest Praxis release from vomitselfie/aoe-praxis for this
> computer. Verify it against SHA256SUMS, place it in ~/.local/bin, and confirm
> that `praxis --version` works.

<details>
<summary>Terminal install using GitHub CLI</summary>

```bash
release="$(gh release view --repo vomitselfie/aoe-praxis --json tagName --jq .tagName)"
case "$(uname -s)-$(uname -m)" in
  Linux-x86_64) platform="linux-x86_64" ;;
  Darwin-arm64) platform="macos-aarch64" ;;
  Darwin-x86_64) platform="macos-x86_64" ;;
  *) echo "Praxis has no release for this platform." >&2; return 1 ;;
esac
archive="praxis-${release}-${platform}.tar.gz"
gh release download "$release" --repo vomitselfie/aoe-praxis \
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
install -m 0755 "$unpack_dir/praxis" "$HOME/.local/bin/praxis"
praxis --version
```

</details>

### 2. Connect your agent

Choose the agent you use. You can connect more than one; they share the same
local encrypted history.

#### Codex

```bash
codex_home="${CODEX_HOME:-$HOME/.codex}"
praxis install-codex-hook --config "$codex_home/config.toml" --apply
praxis install-skill --target "$codex_home/skills" --apply
```

Restart Codex. The first recall or learning request may ask for permission to
reach your operating system's credential store. Approve only the exact
`praxis recall` or `praxis learn` command—not a general shell command. Codex's
[command rules](https://learn.chatgpt.com/docs/agent-configuration/rules) can
remember that narrow approval.

#### Claude Code

Install the skill:

```bash
praxis install-skill --target "$HOME/.claude/skills" --apply
```

Then run `praxis print-claude-config` and merge the printed `hooks` entries into
`~/.claude/settings.json`. Preserve any hooks already there. If you would rather
not edit JSON, ask Claude. These are Claude Code's documented user-level
[skill](https://code.claude.com/docs/en/skills) and
[hook](https://code.claude.com/docs/en/hooks) locations.

> Run `praxis print-claude-config` and safely merge its hooks into my user
> settings without replacing anything already configured.

#### Agent of Empires

With AoE 1.14 or newer, install the optional plugin:

```bash
aoe plugin install gh:vomitselfie/aoe-praxis
aoe plugin info vomitselfie.praxis
```

If you already installed the Codex hook above, stop here. Do not also enable the
AoE transcript recorder for the same Codex sessions.

To use AoE's transcript fallback instead of the Codex hook:

```bash
praxis install-aoe-hook \
  --config "$HOME/.config/agent-of-empires/config.toml" \
  --apply
```

The plugin worker runs with `aoe serve`. A TUI-only AoE process can install the
plugin but does not run plugin workers.

### 3. Check it

Use your agent normally for a moment, then run:

```bash
praxis status
praxis recall --project "$PWD" --query "what approach should I try?"
```

`not_configured` before the first recorded event is normal. Once a hook records
an event, status should report ready history. Recall may still be empty until
Praxis has useful evidence for that project. Praxis does not retain the query
text, although your shell may keep commands you type in its own history.

## Day-to-day use

Once the hook and skill are installed, there is usually nothing to manage. The
agent records tiny sanitized observations in the background and requests a
bounded recall when prior evidence could help.

Useful commands:

| Command | What it does |
| --- | --- |
| `praxis status` | Checks the key store and encrypted history |
| `praxis recall --project "$PWD"` | Shows bounded aggregate evidence for this project |
| `praxis forget --project "$PWD"` | Previews deleting this project's history |
| `praxis forget --project "$PWD" --apply` | Deletes this project's history |
| `praxis prune --keep-recent 10000` | Previews a global retention cleanup |
| `praxis prune --keep-recent 10000 --apply` | Applies that cleanup |
| `praxis --help` | Lists every command |

Destructive commands preview what they will remove unless you add `--apply`.

## Token impact

Recording adds no chat output: successful hooks are silent. Recall defaults to
an approximate 300-token output limit and returns aggregates instead of replaying
history. The installed skill plus a default recall has a Praxis-controlled
ceiling of about 1,031 tokens, 43% smaller than the original implementation.

That is a context-size ceiling, not a promise about provider billing. The
payoff comes when a small recall prevents an agent from repeating a much larger
investigation or asking you to paste old context.

## Safety behavior

- Praxis fails closed if Secret Service or Keychain is unavailable or locked.
- It never falls back to a plaintext key or plaintext history.
- Agent sandbox access should be granted only to exact Praxis subcommands.
- `status`, recall, and deletion previews do not create missing state.
- Hook input is reduced to an allowlisted event before storage.

## More detail

- [Architecture and privacy boundaries](docs/architecture.md)
- [Roadmap](docs/roadmap.md)
- [Upstream source-format notes](docs/source-format-notes.md)
- [Contributor and release guide](docs/releasing.md)
- [`praxis-recall` skill](skills/praxis-recall/SKILL.md)

Praxis is MIT licensed.
