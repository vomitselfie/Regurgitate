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

### A shared local notebook

Lessons belong to the local encrypted store, not to an agent or session. Connect
each agent once using AoE setup or the standalone installer. Agents running as
the same OS user with the same data home and credential store then use the same
notebook. Custom data homes and separate OS accounts remain separate stores.

For a verified lesson that applies across projects, agents can add `--shared`:

```bash
regurgitate experience record --shared --task integration \
  --situation "A stock integration is migrated by replacing its bundle directory." \
  --lesson "Check the entire directory layout before migration; extra personal files require explicit replacement." \
  --procedure targeted-verification --outcome success --artifact config
```

This illustrates the syntax; record only after observing and verifying the
lesson. `--shared` means `--scope machine`. Omitting it keeps the lesson scoped
to its project. Existing memories keep their scope. Recall consults broader
scopes when relevant local evidence is sparse, including in a new project with
no recorded history; task and applicability filters still apply.

The bundled skill chooses shared scope for portable lessons and keeps recording
selective. A single procedure dimension is enough; caveats and ordered steps are
optional. Hooks are optional telemetry, not a prerequisite for sharing lessons.
Any agent capable of invoking the CLI can participate. Forgetting an origin
project removes its lessons even when they were shared. There is no network
service, background lesson generator, or automatic promotion of old memories.

After updating the binary, refresh each agent's recall skill and restart the
agent. AoE users run its setup action; standalone users rerun the installer with
their agent. The exact previous bundled skill upgrades automatically. Personal
edits or extra files require review and explicit replacement (`--replace-skill`
for standalone installations).

### Stored evidence

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

You can install Regurgitate without AoE in one command. Choose the agent to
connect; the installer downloads into a temporary directory, verifies the
release archive against `SHA256SUMS`, installs the binary atomically under
`~/.local/bin`, and safely adds the agent's hook and recall skill.

#### Codex

```bash
curl --proto '=https' --tlsv1.2 -LsSf \
  https://github.com/vomitselfie/Regurgitate/releases/latest/download/install.sh |
  sh -s -- --agent codex
```

#### Claude Code

```bash
curl --proto '=https' --tlsv1.2 -LsSf \
  https://github.com/vomitselfie/Regurgitate/releases/latest/download/install.sh |
  sh -s -- --agent claude
```

Restart the connected agent. The first recall or learning request may ask for
permission to reach your operating system's credential store. Approve only the
exact Regurgitate command, never a general shell command.

The installer pins both the hook and skill to the verified executable, so the
agent does not depend on shell `PATH` configuration. It never uses `sudo`, never
edits a shell profile, preserves unrelated agent configuration, and refuses a
differing skill unless you explicitly add `--replace-skill`.

To update a standalone installation, rerun the same command. To install only
the binary, use `--agent none`. `--bin-dir <directory>` changes the destination,
and `--version <version>` installs a specific release.

<details>
<summary>Inspect the installer before running it</summary>

```bash
curl --proto '=https' --tlsv1.2 -LsSf \
  https://github.com/vomitselfie/Regurgitate/releases/latest/download/install.sh \
  -o regurgitate-install.sh
less regurgitate-install.sh
sh regurgitate-install.sh --agent codex
```

</details>

<details>
<summary>Manual verified installation</summary>

Regurgitate still ships three platform archives:

Open the [latest release](https://github.com/vomitselfie/Regurgitate/releases/latest)
and download `SHA256SUMS` plus the archive for your computer:

| Computer | Asset name ends with |
| --- | --- |
| Linux, x86-64 | `linux-x86_64.tar.gz` |
| Apple Silicon Mac | `macos-aarch64.tar.gz` |
| Intel Mac | `macos-x86_64.tar.gz` |

Verify the archive, extract it, place `regurgitate` in a directory on your
`PATH`, then use the binary's preview-first `install-codex-hook`,
`install-claude-hook`, and `install-skill` commands. Their `--help` output lists
the explicit config and skill destinations.

</details>

#### Avoid mixed installations

AoE and standalone installations are separate update channels. AoE's **Set up
Codex/Claude Code** action pins the agent to AoE's managed binary; the standalone
installer pins it to the selected `--bin-dir`. Use one setup path per agent.
`aoe plugin update` does not update a separately installed standalone binary,
and rerunning the standalone installer does not update AoE's plugin copy.

An untouched older PATH-based Regurgitate hook and skill are migrated safely to
the chosen executable. Personally modified or restricted integrations remain
untouched and produce a conflict instead.

#### Optional AoE transcript fallback

Older AoE/Codex setups can record at session boundaries instead of using the
native Codex hook:

```bash
regurgitate install-aoe-hook \
  --config "$HOME/.config/agent-of-empires/config.toml" \
  --apply
```

Use this only when the native Codex hook is not installed.

#### Check it

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
agent recalls only when prior experience could materially change non-trivial
work, confirms a recalled lesson by its `ref` when it applied it, and records
at most one new capsule only when a milestone produced a novel reusable
lesson. With
Claude Code, the installed `UserPromptSubmit` hook injects a short brief
automatically when a relevant lesson has confirmed evidence, shows at most
two `unconfirmed` lessons while a project is still bootstrapping, and stays
silent for unrelated prompts.

Useful commands:

| Command | What it does |
| --- | --- |
| `regurgitate status` | Checks the key store and encrypted history |
| `regurgitate recall --query "csv importer" --best-effort --token-budget 240` | Shows ranked lessons, or a compact non-blocking status |
| `regurgitate recall --failures --task data-import` | Shows failed lessons for this task |
| `regurgitate recall --risk version-sensitive --tool-family cargo --tool-major 1` | Corrects inferred risk or tool context explicitly |
| `regurgitate experience record --task data-import --situation "…" --lesson "…" --procedure per-subject-streaming --outcome success` | Records, deduplicates, or rejects one bounded capsule |
| `regurgitate experience confirm --match <ref> --outcome success` | Confirms or refutes one authenticated receipt; replay is idempotent |
| `regurgitate experience metrics` | Reports aggregate lifecycle and authenticated-confirmation usefulness |
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

Hook recording adds no chat output: successful hooks are silent. Direct recall
defaults to 300 approximate tokens, while the bundled skill requests 240 and
the preflight brief requests 220. All trim low-ranked lessons instead of
replaying history; no-match and unavailable recall produce one tiny status
object. The core skill plus its normal recall has a Regurgitate-controlled
ceiling of about 935 tokens. AoE setup adds one
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
