# codebaseGraph Engineer — independent read-only full proposal review

## Recommendation

**Proceed with specified conditions:** the full design is suitable for the user's design/creation decision after extending its validation contract to test the approved Q16 lessons workflow (F01). Do not create the skill yet: design/artifact approval, canonical-spec approval at the required committed revision, and applicable starting gates remain prerequisites. This review is not any of those approvals and is not a final artifact review.

No new product, architectural, scope or outage-policy decision is needed from this review. Q16=1 and Q4=1 are settled; do not reopen them. The owner still must approve the complete design/artifact scope and later exact committed-spec revision under the existing procedure.

## Reviewed snapshot and method

- Read the **entire** temporary proposal at `/var/folders/3y/n_wglyqn3bv8xy8s0b9p32d00000gn/T/cbg-engineer-design-bycr3upp/proposed-design.md` (53 lines).
- Snapshot SHA-256: `c48c6aaa1d4ccc9aac244c8ba7a6f8ef136a631f6b27ef0ad2caa2054e609ced`. This is a content hash, not an approved Git revision.
- Read live repository `AGENTS.md` in full, after prior graph discovery of that path; read the complete `skill-creator/SKILL.md`, the complete `adversarial-proposal-review-engineer/SKILL.md`, and its proposal-lenses reference.
- Reused this session's read-only live wiki/Scryer evidence from the targeted lessons assessment; no further broad wiki research was needed. That assessment is at `/private/tmp/codebasegraph-live-lessons-assessment-20261028.md`. Its earlier statement that Q16/outage choice remained open is superseded by the parent's explicit approval updates, not an open finding in this review.
- Applied the proposal review skill to this bounded commitment: a project-local Markdown orchestration skill plus small governed specification/review evidence, with no product implementation or live issue trial. No economics or external vendor/market premises required research.
- No repository, wiki, Scryer, Linear, integration, skill or Git state was changed. Only temporary review reports were written. No implementation tests or scenario walkthroughs were run; the artifacts do not yet exist.

## F01 — Approved lessons workflow is absent from the scenario acceptance coverage

**Type:** established validation-coverage gap. **Priority:** material; resolve before dependent skill validation/sign-off, preferably by correcting the approval draft now.

**Locator:** proposal “Validation and success contract,” lines 34–38, compared with “Approved wiki/docs lessons policy (Q16=1),” lines 47–53.

**Finding:** the scenario list tests selection, approvals, architectural conflict, anti-overengineering, fallbacks, uncertain writes, recovery and handoff, but none tests the new end-to-end requirement to retrieve **both** wiki and repository docs, preserve source authority/lifecycle distinctions, put concrete implications into each applicable Scryer brief, require worker source reading, and check the resulting implementation/test evidence. The structural checks cannot establish this behavior. A skill could pass every listed scenario while handing a worker only an unverified wiki summary and ignoring docs.

**Evidence and counterevidence:** the policy itself is strong and explicit at lines 48–53; it is not missing from the design. The design also correctly requires actual observed scenario results and independent expectations (lines 36–37). Those are good safeguards but do not test a behavior omitted from the fixtures/expectations. Live source discovery in this session showed a concrete failure mode worth exercising: general wiki search returned the Windows build-host smoke memory with `status: candidate`, while default memory recall filters to active records. Search position is not approval, verification or applicability.

**Remedy:** add a focused non-mutating lesson workflow scenario (or bounded variants), using fictitious tool/source/worker states and independently held expected outcomes:

1. A relevant wiki lesson and a separately graph-discovered repository doc must both be considered; trace selected source references and concrete implications into the mocked Scryer brief/worker assignment and resulting check evidence.
2. Include a candidate/historical or wrong-revision result so the agent must distinguish it from current approved intent, verify or reject its mutable claim, and preserve caveats rather than promote it as authority. No forced unrelated code/test work.
3. Change scope or expose a familiar failure and confirm targeted re-retrieval; make a required source unavailable and confirm the already-approved Q4 pause/independent-work behavior. An honestly empty relevant search must not generate filler lessons.

These can be concise fixtures within the existing walkthrough approach, not a new test framework or product scope. Resolution evidence is the revised acceptance contract and, after implementation, actual isolated outputs/actions mapped to the lesson requirements. No live records or real coding workers should be used for these fixtures.

## Supported strengths and challenge outcomes

### Authority and approvals

The proposal keeps the live `AGENTS.md`/tool rules authoritative, Scryer as architectural authority/brief owner, specs as contract/approval authority, and graph/source/executed evidence as implementation evidence (lines 7, 14, 16–17, 22–24). Issue choice is not implementation approval; a named issue is selection only. Missing gates are visible rather than inferred from labels. This directly addresses the risk that orchestration becomes unauthorized issue execution. No demonstrated conflict with live repository rules was found.

### Feasibility and progressive disclosure

The chosen path is consistent with skill-creator: project-local `.prime/agent/skills/codebasegraph-engineer/SKILL.md`, matching valid machine name, Markdown rather than a needless Python service, trigger-specific frontmatter, and references loaded only when needed (lines 4–5, 27–38). It does not promise deterministic automatic routing or treat static YAML validation as successful loading. No new enforcement mechanism, daemon, credentials or knowledge store is required. The minimum-change alternative of general instructions alone remains less task-specific; the proposal's benefit is focused routing and a concise decision flow, not a claimed new security boundary or proven productivity gain.

### Scope, workers and recovery

One selected issue, exact file ownership, preserved shared work, serialized Git mutation, no overlapping replacement writers, and no automatic issue chaining provide concrete stop boundaries. The inability to contact services or obtain prescribed reviewers pauses only dependent work and permits only already-authorized independent work (line 21; Q4=1). The existing outage decision is sufficient; no fresh threshold or fallback choice is necessary.

### Q16 source use

The approved policy retrieves wiki/memory **and** graph-discovered docs, follows canonical links, checks revision/environment and mutable facts, uses source links and concrete implications rather than copied knowledge, requires workers to read sources, and refreshes at defined task transitions (lines 48–53). It retains memory as advisory and avoids a second durable brief. It also permits no relevant lessons without filler. These safeguards resolve the material authority/staleness concerns raised in the earlier targeted assessment. F01 concerns proving that behavior, not its stated policy.

### Validation boundary

Non-mutating isolated walkthroughs are proportionate to an initial Markdown skill. Independent expectations, actual recorded actions, load verification and explicit limits on live issue trials prevent a false claim that a successful mock proves production orchestration. Final artifact/acceptance review must remain separate and fresh. Do not report this design review as structural, loading or behavioral execution evidence.

## Remaining conditions, not new owner choices

1. Incorporate/accept the F01 validation-coverage correction. It implements already-approved Q16, not a new behavior choice.
2. Obtain the user's full design/artifact creation approval; commit the reviewable canonical spec and obtain exact revision approval before dependent skill creation as live rules require. The proposed stable work ID is still provisional, not an established tracking identity.
3. Confirm actual Linear workspace/data access before relying on it for selected issue readiness or record creation. The proposal already reports that access as unconfirmed rather than successful; this is an execution prerequisite, not a reason to choose another workspace.
4. At artifact completion, run the structural/load/non-mutating behavior checks, review the actual final artifact snapshot and evidence with the mandated fresh final reviewer, and retain any unavailable gates as pending.

## Actual reviewer settings and independence

- Provider/model: `openai-codex` / `gpt-6.1-sol`.
- Reasoning effort: `high`.
- Session service tier: `default` (this is **not** evidence of an `agent_type` parameter).
- Runtime: a separate depth-1 subagent named `engineer-proposal-review`, session `01a180e6-328c-76f2-8052-f3f69d93f2ae`, started 2026-10-28T05:05:32.300Z.
- Freshness: this child session's persisted transcript begins with session/model/effort/tier metadata and the parent's new lessons-assessment message; it contains no forked parent conversation. I was not a proposal author or implementation writer. The full proposal arrived later and was explicitly read in full before this full assessment. This is still the same read-only child that performed the earlier targeted assessment, not a new final reviewer.
- `agent_type: default` and `fork_turns: none` are the required launch settings, but neither explicit parameter is exposed in the child metadata inspected here. The observed independent session/no-fork transcript supports freshness; the parent must attest the exact launch arguments if required for the gate. Do not conflate service tier with agent type.
- Required proposal-review skill was explicitly read and used. No silent model substitution occurred. The initial targeted report's claim that this was not the Sol reviewer was overcautious and is corrected by inspected actual session metadata above: it **is** Sol/High. It did not yet contain a full proposal review then; this report supplies that assessment now.

## Reusable knowledge assessment

No new repository fact/runbook needs recording. Existing source-lifecycle and authority guidance covers the discovered lessons. No filler memory, source copy, candidate promotion or wiki write was made.
