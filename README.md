# Regurgitate

**Regurgitate remembers what worked—not what you wrote.**

Coding agents often repeat the same investigation every time you open a new
session. Regurgitate gives them a small, private notebook of procedural
evidence: which kind of approach was tried, whether it worked, and how often.

It remembers things like “for generated native artifacts, change one
placement class at a time and run the native check—parser acceptance alone
was not enough.” It does not remember what you said, what the agent said, or
the contents of your files.

## What changes for you?

| Without Regurgitate | With Regurgitate |
| --- | --- |
| A new session starts from zero | A new session can check what worked before |
| You paste old logs or explain past attempts | The agent requests a small aggregate summary |
| One agent's lessons stay with that agent | Codex, Claude Code, AoE, and other CLI-capable agents can share the same local evidence |
| “Memory” may mean storing entire conversations | Regurgitate stores controlled categories, outcomes, and three bounded sentences per lesson |

The practical goal is less repeated exploration, fewer wasted tokens, and more
consistent choices across sessions. Regurgitate is procedural memory, not a
chat archive or a replacement for project documentation.

```text
agent hook → encrypted execution summary
verified agent judgment → situation + procedure + outcome + caveat → encrypted experience capsule
new task → scoped, recency- and evidence-weighted recall → 100–300 token brief
```

## What does it store?

Regurgitate keeps two deliberately separate ledgers. Hooks record controlled
tool-execution categories and provider-reported status. Explicit experience
records an **experience capsule**: a controlled task, a compositional
procedure, a semantic `success` or `failure`, controlled applicability tags,
and three bounded sentences—when the lesson applies, what to do, and a
caveat. Those sentences are capped, structurally checked so that code,
commands, paths, URLs, secrets, payloads, and conversation are rejected, and
encrypted before they reach SQLite. A command exiting successfully does not
make a bad approach successful. Recall returns ranked lessons with an
explicit posterior, credible interval, and effective evidence size; hook
activity appears only in a separately labeled summary. Neither returns
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
regurgitate recall --project "$PWD" --query "data import"
```

`not_configured` before the first recorded event is normal. Once a hook records
an event, status should report ready history and separate `hook_event_count`
from `experience_count`. Recall may still be empty until the agent has
recorded a useful lesson for that task. Regurgitate does not retain the query
text, although your shell may keep commands you type in its own history.

## Day-to-day use

Once the hook and skill are installed, there is usually nothing to manage.
Hooks record tiny sanitized execution observations in the background. The
agent recalls once before exploring a non-trivial task, confirms a recalled
lesson by its `ref` when it applied it, and records one new bounded
experience capsule after a meaningful milestone or failed approach. With
Claude Code, the installed `UserPromptSubmit` hook injects a short brief
automatically when a relevant lesson has confirmed evidence, shows at most
two `unconfirmed` lessons while a project is still bootstrapping, and stays
silent for unrelated prompts.

Useful commands:

| Command | What it does |
| --- | --- |
| `regurgitate status` | Checks the key store and encrypted history |
| `regurgitate recall --project "$PWD" --task data-import --query "csv importer"` | Shows ranked lessons relevant to this task |
| `regurgitate recall --project "$PWD" --failures --task data-import` | Shows lessons with failed evidence for this task |
| `regurgitate recall --project "$PWD" --risk version-sensitive --tool-family cargo --tool-major 1` | Uses controlled risk and tool-version context |
| `regurgitate experience record --project "$PWD" --task data-import --situation "…" --lesson "…" --procedure per-subject-streaming --outcome success` | Records or confirms one experience capsule |
| `regurgitate experience confirm --match <ref> --outcome success` | Confirms or refutes one authenticated receipt; replay is idempotent |
| `regurgitate experience list --project "$PWD"` | Lists capsule status and shape (never lesson text) |
| `regurgitate experience challenge\|obsolete --project "$PWD" --match <selector>` | Marks a capsule challenged or obsolete |
| `regurgitate experience supersede --project "$PWD" --old <sel> --new <sel>` | Replaces one capsule with another |
| `regurgitate preflight --project "$PWD" --query "…"` | Prints the plain-text brief a host could inject |
| `regurgitate learn --project "$PWD" --task data-import --strategy per-subject-streaming --outcome success` | Text-free shorthand for `experience record` |
| `regurgitate bench-report --runs runs.jsonl` | Summarizes paired cold/warm benchmark runs |
| `regurgitate forget --project "$PWD"` | Previews deleting this project's history |
| `regurgitate forget --project "$PWD" --apply` | Deletes this project's history |
| `regurgitate prune --keep-recent 10000` | Previews a global retention cleanup |
| `regurgitate prune --keep-recent 10000 --apply` | Applies that cleanup |
| `regurgitate --help` | Lists every command |

Destructive commands preview what they will remove unless you add `--apply`.

## Token impact

Hook recording adds no chat output: successful hooks are silent. Recall
defaults to an approximate 300-token output limit, the preflight brief to
220, and both trim lowest-ranked lessons first instead of replaying history.
An irrelevant task gets no brief at all. The core skill plus a default recall
has a Regurgitate-controlled ceiling of about 1,050 tokens. AoE setup adds one
short instruction containing its local executable path, so that exact total
varies slightly by computer. Whether that context pays for itself is what the
[paired benchmark](benchmarks/README.md) measures.

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
