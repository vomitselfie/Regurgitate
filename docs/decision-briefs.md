# Memory that earns attention

The useful unit is a reminder that changes a decision. Storage volume, hook
activity, and the number of agent invocations are not success criteria.

Automatic preflight supplies at most one note before a recognizable task in
Codex or Claude Code. It prefers eligible project-local advice and never forces
a reminder when there is no match. The bundled skill can request at most two
notes manually before consequential
choices, skips familiar routine work, and records only verified lessons that
would have changed the approach if known earlier. It does not narrate ordinary
memory use or ask users to fill the notebook.

`recall --brief` renders contextual lessons as prose. It keeps conditions,
caveats, uncertainty, counterevidence, challenged status, failure reasons, and
confirmation references; it omits posterior arithmetic and execution telemetry.
The full diagnostic JSON remains the default CLI representation for compatibility.

The same renderer serves automatic preflight. A bounded internal recall uses
the existing maximum diagnostic budget, then budgets the final representation.
This avoids prematurely dropping a note just because its JSON is larger than
its prose. Whole notes are removed under budget pressure; caveats are never
trimmed away to make a recommendation fit. Context-free legacy aggregates do
not enter briefs. Their data remains accessible through ordinary recall.

There is no extra model call, daemon, network service, or memory migration.
Known untouched skills from 0.10.3, 0.10.4, and 0.10.9 upgrade through the existing
preserving installer. Changes to personal content still require explicit review.

## Quiet by default

Automatic hooks have a 350 ms lookup deadline and skip malformed/oversized
input, unavailable storage, and locked credentials without output or approval
requests. Greetings skip credential access altogether. They neither write recall
telemetry nor grant hook trust. The skill treats permission failures as optional
memory, not an interruption to the user's work. Explicit human diagnostics
remain available. On macOS the non-interactive keychain adapter is built by the
release matrix; a live locked-keychain test still needs a Mac.

## Proving benefit

The isolated regression scenario saves a verified lesson, opens a fresh project,
checks the host's pre-decision context, and verifies authenticated confirmation
and replay behavior. Other tests cover unrelated prompts, a one-note/local
preference, bounded waiting, and unchanged storage during lookup. This proves
delivery mechanics, not that an AI actually avoided a mistake.

The next evaluation is normal work over ten suitable tasks. Count a success
only when a lesson changes the approach and the result is verified. Confirm
real reuse through the existing receipt; never seed the real notebook with test
results. A brief mention in the normal task handoff is enough when genuinely
useful; no extra user-maintained log or routine memory announcement is needed.
Look for a few concrete avoided mistakes with negligible intervention. If the
trial produces no such evidence, simplify or retire the experiment rather than
add more collection or dashboards. This trial has not been completed by unit tests.

Verification covers complete notes under budget pressure, preservation of
negative evidence and receipts, quiet empty/unavailable CLI behavior, invalid
argument handling, and exact stock-skill upgrades. These checks establish
behavior, not measured real-world savings; usefulness still depends on natural
reuse and authenticated feedback.

## Relevance before ranking

An unrecognized query no longer acts like an unfiltered lesson listing. Without
recognized task/domain hints or explicit filters, it yields no lessons even when
project defaults match local history. Ordinary diagnostic recall still retains
its separate hook summary; briefs stay quiet. Omitting the query altogether
continues to support intentional inspection.

Known ecosystem conflicts are excluded before ranking, with explicit flags
overriding query hints. Related ecosystems (JavaScript/TypeScript, PHP/Laravel,
and C/C++/CUDA) remain eligible. An explicit tool-family selection excludes a
different known applicability tag. Unknown and generic metadata remain eligible;
project defaults and inferred tool mentions remain soft hints.

The classifier does not equate Python with Pytest, schema with SQL, or kernel/GPU
with CUDA. Concrete tool names still classify normally. Regression cases cover
both rejected mismatches and retained related, generic, and multi-ecosystem
matches. This is controlled-category filtering, not semantic understanding of
every query or proof that every surviving lesson is useful.
