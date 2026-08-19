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
function tools. The hook input also contains raw `tool_input`, `cwd`, and
transcript location; Praxis intentionally has no deserialization fields for
those values.

Hosted tools such as web search do not currently use the local tool hook path.
Transcript fallback may observe them, but should retain only a generic
controlled operation.

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
- working directories, transcript paths, and source paths;
- git metadata;
- URLs, command strings, environment values, and credentials;
- model context or token accounting payloads.

The Rust adapter structs omit these fields instead of deserializing and then
trying to redact them.
