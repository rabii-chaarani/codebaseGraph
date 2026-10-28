# Focused F01 verification — codebaseGraph Engineer proposal

**Result: F01 resolved at the proposal/acceptance-contract level.** Recommend proceeding to the pending user design/spec decision. No new material issue found in the changed or directly affected text.

## Snapshot and scope

Read the actual entire updated file `/var/folders/3y/n_wglyqn3bv8xy8s0b9p32d00000gn/T/cbg-engineer-design-bycr3upp/proposed-design.md`. SHA-256 verified: `5c239d7bca51977533ac501038a042d411e379d7cee6c4f19bd440d32a6729fa` (matches parent-supplied hash). Compared against the prior fully reviewed snapshot `c48c6aaa1d4ccc9aac244c8ba7a6f8ef136a631f6b27ef0ad2caa2054e609ced`. The only substantive addition is the “Approved lessons policy scenarios” bullet in “Validation and success contract.” Q16=1 and Q4=1 remain settled and were not reopened.

## Focused verification

The added acceptance text explicitly covers every material omission identified by F01:

- Task-specific retrieval of **both** wiki and docs.
- Lifecycle/applicability treatment of candidate, stale and wrong-revision sources, with source status and authority checks rather than ritual read confirmations.
- Source links and concrete implications in the canonical Scryer brief.
- Worker reading of applicable selected sources.
- Implementation/verification reflection without unrelated scope.
- Refresh at subsystem/scope change or familiar failure.
- Missing-source outage allowing independent already-approved work only.
- Fixtures without expected answers and capture/checking of actual responses.

This is a proportionate correction inside the existing non-mutating scenario approach, not new product behavior or a test framework. Existing requirements to keep walkthroughs fictitious, avoid production changes/real coding agents, and have a reviewer inspect actual final artifacts and evidence still apply.

**What is resolved:** the design no longer omits Q16 from its behavioral validation contract.

**What is not claimed:** scenarios have not been implemented or executed; no skill files exist. This review does not establish loading, behavioral passing results, implementation completion, user creation approval or committed-spec approval. Later evidence must demonstrate the fixtures/actions/results against the actual final snapshot.

## Conditions and owner decisions

No new owner question arose from this revision. The existing pending choices about an isolated current-main worktree and Maintenance issue destination belong to the user/coordinator and were not assumed resolved here. Required design/artifact approval and exact committed-spec approval remain separate gates. The prior report's other supported conclusions stand.

## Settings, limits and writes

Same independent read-only proposal reviewer session: openai-codex / gpt-6.1-sol, high effort, default service tier, no delegation. Runtime history/model/effort were confirmed; explicit `agent_type` and `fork_turns` launch parameters are not exposed and are not asserted. This is focused follow-up verification by the proposal reviewer, not a new final-artifact review. No repository, external record, Git, product or integration writes were performed; only this new Python-tempfile report was written.
