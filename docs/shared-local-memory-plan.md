# Shared local memory

Goal: let verified procedural lessons benefit other agents and projects under
one OS account, with minimal agent input and bounded recall.

- [x] Keep the existing shared encrypted store and origin-based deletion.
- [x] Let new projects recall broader scopes without creating local history.
- [x] Add `experience record --shared` as an explicit machine-scope shortcut.
- [x] Retain project scope by default and preserve existing lesson scope.
- [x] Teach the bundled skill to share portable lessons and omit optional fields.
- [x] Keep the skill within its existing 3,000-byte budget.
- [x] Test first-project recall, private-scope exclusion, relevance filtering,
      authenticated feedback, replay protection, and origin deletion.

The existing sparse-local-evidence policy limits broader retrieval. This release
does not turn execution telemetry into successful procedural evidence. It does
not pool OS accounts or publish lessons to a network. Real usefulness still needs
to be measured through natural recall, application, and authenticated feedback;
fixture confirmations are not production success evidence.
