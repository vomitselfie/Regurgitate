# Memory that earns attention

The useful unit is a reminder that changes a decision. Storage volume, hook
activity, and the number of agent invocations are not success criteria.

The bundled skill therefore requests at most two notes before consequential
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
Known untouched skills from 0.10.3 and 0.10.4 upgrade through the existing
preserving installer. Changes to personal content still require explicit review.

Verification covers complete notes under budget pressure, preservation of
negative evidence and receipts, quiet empty/unavailable CLI behavior, invalid
argument handling, and exact stock-skill upgrades. These checks establish
behavior, not measured real-world savings; usefulness still depends on natural
reuse and authenticated feedback.
