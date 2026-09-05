# Source format notes

Reconnaissance date: 2026-08-19.

## Agent of Empires

The inspected host runs AoE 1.14.1. Its profile session registry is a JSON array
under the XDG configuration directory. The fields needed by the adapter are:

- `id`: stable AoE session identifier;
- `project_path`: local project association, retained only in the adapter;
- `tool`: agent kind; and
- `agent_session_id`: identifier linking an AoE session to the native agent
  record.

Other observed fields include titles, commands, timestamps, group paths, and UI
state. Regurgitate does not deserialize them.

Without an explicit `XDG_CONFIG_HOME`, Regurgitate follows AoE's platform defaults:
`~/.config/agent-of-empires` on Linux and `~/.agent-of-empires` on macOS.

AoE currently provides three useful integration surfaces:

1. repo lifecycle hooks (`on_create`, `on_launch`, `on_destroy`);
2. global/profile status-transition hooks with `AOE_SESSION_ID`, `AOE_TOOL`, and
   project context; and
3. a plugin system using manifest API version 13 and JSON-RPC workers
   (plugin compatibility rechecked against AoE 1.15.3 on 2026-09-05).

The plugin system is useful for installation, commands, settings, and UI, but
has no post-install callback and its worker contract is not itself a complete
tool-event feed. Regurgitate therefore waits for an explicit setup action before
adding an agent hook and skill. Status/lifecycle hooks remain useful for
automatic session-boundary ingestion.

Regurgitate therefore ships a release-binary plugin as an operational layer over the
same executable. Its worker publishes aggregate health and handles controlled
status, refresh, and setup methods; it does not receive or reconstruct provider
tool events. The global `home-pane` hosts health and setup actions, with
contributed commands as an alternative. AoE retired `settings-page`; the plugin
now requires AoE 1.15.1 or newer within 1.x. Workers launch from `aoe serve`, so the
standalone CLI and installed recording hooks remain supported independently of
the daemon.

The implemented host integration uses global/profile `[status_hooks]` entries
for `on_idle` and `on_error`. AoE documents these hooks as best-effort,
non-blocking commands and supplies `AOE_SESSION_ID`, `AOE_PROFILE`, `AOE_TOOL`,
project/status context, and other metadata. Regurgitate reads only the first three.

Each status slot currently accepts one command string, not a command list. The
installer therefore fills only absent idle/error slots and refuses to replace
or synthesize shell composition around an existing command. It also refuses to
set `enabled = true` when that would activate another dormant hook. Global
config apply uses AoE's adjacent `.config.lock`, reloads after locking, and
atomically replaces the config while preserving unrelated TOML and symlinked
dotfile layouts. The non-mutating snippet remains available for manual global
or profile composition.

These assumptions were rechecked against the current upstream
[configuration reference](https://github.com/agent-of-empires/agent-of-empires/blob/main/docs/guides/configuration.md)
and [plugin internals](https://github.com/agent-of-empires/agent-of-empires/blob/main/docs/development/internals/plugin-system.md)
on the reconnaissance date above.

## Codex

Current Codex JSONL records are stored below `CODEX_HOME/sessions` and use
top-level record types including `session_meta`, `turn_context`, `event_msg`,
and `response_item`. Tool activity appears as paired response items:

- `function_call` / `function_call_output` in older records; and
- `custom_tool_call` / `custom_tool_call_output` in current code-mode records.

Calls carry a tool name and call identifier. They also carry arguments or input,
which Regurgitate must discard. Outputs may be a string or content-block array and
must also be discarded after deriving a controlled outcome. Older shell results
contain a stable `Process exited with code N` marker. Newer code-mode transcript
output does not always expose an exit code, so the transcript fallback reports
`unknown` rather than manufacturing success.

Codex lifecycle hooks are the preferred source. `PostToolUse` supplies a stable
session id, tool name, tool-use id, and tool response. It runs for successful and
non-zero Bash commands and covers `apply_patch`, MCP calls, and most local
function tools. The direct-recording adapter also allowlists `cwd`, but carries
it only in a non-serializable `ProjectLocator`. Raw `tool_input` and transcript
location have no deserialization fields.

Hosted tools such as web search do not currently use the local tool hook path.
Transcript fallback may observe them, but should retain only a generic
controlled operation. The official hook reference also describes
`transcript_path` as a convenience rather than a stable API, reinforcing that
the native hook payload—not transcript parsing—is the primary integration.

Current [upstream hook reports](https://github.com/openai/codex/issues/34289)
show that shell `tool_response` can be plain output text without an exit status,
and native `tool_use_id` is not guaranteed to equal the corresponding JSONL
call ID. Regurgitate therefore keeps those outcomes `unknown` and does not attempt
content heuristics or cross-source joins. Native hooks and AoE transcript
ingestion are alternative sources for a session, not a combined feed.

Codex loads additive lifecycle hooks from inline `config.toml` tables and other
active hook sources. A matcherless `[[hooks.PostToolUse]]` group observes all
supported local tools without replacing existing matcher groups. Non-managed
command hooks must be reviewed and trusted in Codex before they run. These
assumptions were checked against the current official
[Codex hooks reference](https://learn.chatgpt.com/docs/hooks) on the
reconnaissance date above.

Tool identity is the only provider field used for automatic strategy tagging:
`apply_patch` maps to `structured_patch`, while explicit edit/write tools map to
`direct_text_mutation`. Shell commands and unknown/MCP tool names receive no
strategy because deriving one would require inspecting private arguments or
content. Strategies such as `atomic_write`, `preview_then_apply`, and
verification scope enter history only through the fixed-vocabulary `learn`
command after a directly established result. Analysis methods—including
`reproduce_then_compare`, `per_subject_streaming`, and `resource_cap_first`—are
also explicit-only and use the shared `research` / `analyze` classification;
provider payloads are never inspected to infer them.

## Claude Code

Claude Code command hooks receive JSON on stdin. The documented common input
includes `session_id`, `transcript_path`, `cwd`, `permission_mode`, and
`hook_event_name`. `PostToolUse` additionally provides `tool_name`,
`tool_input`, `tool_response`, and `tool_use_id`; `PostToolUseFailure` provides
the same tool identity plus an `error`, optional interruption state, and
optional duration.

Regurgitate registers both terminal events. It allowlists only the session ID,
working directory, event name, tool name, tool-use ID, and optional duration.
The distinct hook event names are sufficient to derive provider-reported tool
execution status, so the adapter has no fields for tool input, tool response,
error, transcript path, or permission mode. This status is never treated as
semantic approach correctness. The working directory becomes only a
non-serializable project locator.

These assumptions were checked against the current official
[Claude Code hooks reference](https://code.claude.com/docs/en/hooks) on the
reconnaissance date above. No Claude transcript parser is implemented.

## Current ingestion compatibility

`regurgitate ingest --session <id>` composes the AoE registry lookup and Codex
transcript normalizer through a generic session-event source. Its encrypted
cursor records a committed byte offset, the SHA-256 digest of the committed
prefix, the last observed source length, and any pending call as controlled
metadata. It never stores a transcript path or raw JSONL fragment.

Only newline-terminated records advance the cursor. Appended records are parsed
incrementally; a shorter source or changed committed prefix triggers a reset
and safe reparse. Deterministic event UUIDs keep this interruption-safe because
SQLite ignores an event already committed before the cursor advances.

## Fields that must never cross normalization

- user and assistant messages;
- reasoning, summaries, or instructions;
- tool arguments and tool input;
- raw tool responses or output;
- working directories in events or persisted plaintext, and all transcript or
  source paths;
- git metadata;
- URLs, command strings, environment values, and credentials;
- model context or token accounting payloads.

Rust adapter structs omit these fields instead of deserializing and then trying
to redact them. The only exception is a hook working directory, which is
deserialized directly into the non-serializable `ProjectLocator` path and
consumed solely by encrypted project identity resolution.
