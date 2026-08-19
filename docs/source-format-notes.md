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
state. Praxis does not deserialize them.

AoE currently provides three useful integration surfaces:

1. repo lifecycle hooks (`on_create`, `on_launch`, `on_destroy`);
2. global/profile status-transition hooks with `AOE_SESSION_ID`, `AOE_TOOL`, and
   project context; and
3. a stable plugin system using manifest API version 8 and JSON-RPC workers.

The plugin system is useful for installation, commands, settings, and UI, but
the documented worker contract is not itself a complete tool-event feed.
Status/lifecycle hooks remain useful for automatic session-boundary ingestion.

The implemented host integration uses global/profile `[status_hooks]` entries
for `on_idle` and `on_error`. AoE documents these hooks as best-effort,
non-blocking commands and supplies `AOE_SESSION_ID`, `AOE_PROFILE`, `AOE_TOOL`,
project/status context, and other metadata. Praxis reads only the first three.

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
and config persistence implementation on the reconnaissance date above.

## Codex

Current Codex JSONL records are stored below `CODEX_HOME/sessions` and use
top-level record types including `session_meta`, `turn_context`, `event_msg`,
and `response_item`. Tool activity appears as paired response items:

- `function_call` / `function_call_output` in older records; and
- `custom_tool_call` / `custom_tool_call_output` in current code-mode records.

Calls carry a tool name and call identifier. They also carry arguments or input,
which Praxis must discard. Outputs may be a string or content-block array and
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
call ID. Praxis therefore keeps those outcomes `unknown` and does not attempt
content heuristics or cross-source joins. Native hooks and AoE transcript
ingestion are alternative sources for a session, not a combined feed.

Codex loads additive lifecycle hooks from inline `config.toml` tables and other
active hook sources. A matcherless `[[hooks.PostToolUse]]` group observes all
supported local tools without replacing existing matcher groups. Non-managed
command hooks must be reviewed and trusted in Codex before they run. These
assumptions were checked against the current official
[Codex hooks reference](https://learn.chatgpt.com/docs/hooks) on the
reconnaissance date above.

## Claude Code

Claude Code command hooks receive JSON on stdin. The documented common input
includes `session_id`, `transcript_path`, `cwd`, `permission_mode`, and
`hook_event_name`. `PostToolUse` additionally provides `tool_name`,
`tool_input`, `tool_response`, and `tool_use_id`; `PostToolUseFailure` provides
the same tool identity plus an `error`, optional interruption state, and
optional duration.

Praxis registers both terminal events. It allowlists only the session ID,
working directory, event name, tool name, tool-use ID, and optional duration.
The distinct hook event names are sufficient to derive success or failure, so
the adapter has no fields for tool input, tool response, error, transcript path,
or permission mode. The working directory becomes only a non-serializable
project locator.

These assumptions were checked against the current official
[Claude Code hooks reference](https://code.claude.com/docs/en/hooks) on the
reconnaissance date above. No Claude transcript parser is implemented.

## Current ingestion compatibility

`praxis ingest --session <id>` composes the AoE registry lookup and Codex
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
