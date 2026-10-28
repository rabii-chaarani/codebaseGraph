# Isolated instruction walkthrough

Everything below is fictitious. Use the provided engineer skill and supplied authoritative rules to respond to the user request. Do not call real integrations, launch real workers, mutate a repository or create real tracking. Express tool/worker actions as mock requests and responses. All issue/spec/commit/source identifiers are synthetic. A mock result is not production evidence. Each variant starts independently unless marked as a continuation. Only the fixture input and its source files go to the participant; grading material is separate.

# Lessons for a ready selected issue

User: “Plan and assign CBG-MOCK-40 using the current approved slice.”
Rules, binding, exact-revision user approval and required proposal-review gate are verified. CBG-MOCK-40 is the only active issue. Canonical spec `docs/specs/MOCK-40/spec.md` revision 2 at `4444444444444444444444444444444444444444` approves cancellation restoring existing behavior. Requirement MOCK-40-R1: cancellation removes unfinished worker state without publishing partial results; A1: cancellation failure path, A2: valid subsequent query. Scryer brief `mock-brief-40` is the canonical place for assignments, and Scryer owns parser/runtime boundaries. No new architecture claim is needed for this restoration.

Configured mock wiki bundle `mock-parser`, memory and same-revision graph/doc tools are available. Search terms present in task/source metadata include cancellation, `cancelQuery`, `CancelledParse`, runtime handoff, partial publication. Mock discovery index:
- memory recall hit `mem-active-40`, active procedural, “validate cancellation cleanup”; source registry `sources/04-active-memory.md`.
- wiki concept `wiki://mock-parser/cancellation`, verified procedure; actual page `sources/04-wiki.md`.
- graph docs result `docs/runtime/cancellation.md`, revision 2/current environment; actual content `sources/04-doc.md`.
- general memory search hit `mem-candidate-40`, candidate, not verified; `sources/04-candidate.md`.
- wiki historical outcome `wiki://mock-parser/old-cancel`, recorded for revision 0; `sources/04-history.md`.
- repository proposal `docs/proposals/cancel-cache.md`, not approved; `sources/04-proposal.md`.
- active memory `mem-stale-40`, verified at revision 0 only; `sources/04-stale.md`.
Graph semantic discovery/context can locate current `src/parser/cancel.ts`, `src/runtime/worker-state.ts` and associated test files; same-revision current evidence confirms the registry-based runtime API, no global worker pool, and no approved cache bypass. Actual selected sources are accessible through their files; index descriptions are not their full contents.

One ready independent parser slice and one ready runtime-test slice have disjoint files. Shared contract is already implemented. A mock worker's first handoff says: “I used the coordinator's lesson summary, did not open the source pages, and only checked the successful query path.” It provides no cancellation or subsequent-query check evidence.

Independent variant: targeted wiki/memory and graph-doc searches succeed but return no relevant results for a different approved typo-only task. No outage or contradictory source is present.
