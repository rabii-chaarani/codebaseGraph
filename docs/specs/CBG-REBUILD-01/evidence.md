# COD-126 preparation evidence and remaining gates

Date: 2026-11-16, Australia/Darwin. Scope: CBG-REBUILD-01/D00 specification/model-proposal authoring only. This record is not product acceptance or final specification approval.

## Verified baseline and method

- Authoring worktree: `/private/tmp/codebasegraph-cod126-specs-20261116`, branch `codex/cod-126-rebuild-specs`, based on remote dev `2f1fe10f6fc5f275f61e4a9bc2d38b4b33a7b5a9`.
- Canonical comparison checkout remains on main `90e2ab6fae2767bdb158e19f0072d43568e80dbc`, with pre-existing dirty/untracked knowledge changes preserved.
- GitHub connector readback verified those remote main/dev SHAs. `git rev-list --left-right --count main...origin/dev` returned `3 0`. `git log`/`git diff --stat` established release 4.2.1, cc and release-action dependency differences; no tracked product Rust source differs.
- Existing worktree list was inspected, not pruned, moved or deleted. COD-111/COD-113 and other branches/evidence remain separate.
- k-wiki bundle/memory/original concepts, COD-125/126 and related Linear issue records, COD-19's immutable transition contract, and Scryer committed/plan ownership were read. Other projects were not substituted for this repository.
- CodebaseGraph MCP tools are not exposed in this session. Repository CLI discovery and explicit context returned `generation_gate_binding_changed`. No graph repair/rebuild/reinstall/direct-database bypass was performed. Prior graph-discovered source anchors at the unchanged comparison SHA were inspected proportionally, but this does not establish a complete fresh caller/retired-path inventory.
- Scryer preparation change: `chg-c9eh2t`; detailed unsigned execution record: `chg-e1cw51`. The focused [architecture snapshot](architecture-proposal.json) captures 17 draft entries. Readback established all 25 pre-existing pending entries unchanged, including their ownership. Pending resource-removal and COD-111 claims retain their original ownership/history.
- Scryer structural validation executed after authoring: no blocking finding; three unchanged pre-existing long-description advisories on `container-release-verifier`, `node-1` and `node-143`. The new proposal adds no advisory. Structural validity is not behavioral acceptance or sign-off.
- The referenced adversarial-review skill is present. The user's AGENTS final-review table selects Sol/xhigh, overriding the skill's Max default. No product claim is marked implemented and no test/JUnit verdict is invented.
- GitHub returned 404, “No commit found”, for COD-111's locally present `d05df72076b5a2016b95a70dd0cbe55bd9ac9f33`. Its local approvals directory contains only the November 2 implementation/emulation records, not the November 3 record referenced by spec 0.3. Later semantics are Linear-reported; exact-revision approval/publication remains unverified, and original approvals are preserved.

## Independent checks and corrections

Initial frozen proposal: `5b4c2427eac46fd0d846db7aaeff56267a9987d8`, against authoring base above. The branch was pushed; GitHub immutable readback returned the same spec Git blob identity as local `git rev-parse` (`5d1dc480e20733c530fb718c142c7ad098e9fdf0`). No PR or human approval is inferred from a branch push.

The independent GPT-6.1 Terra/medium verifier passed diff scope/check, JSON parsing, 37 requirement-to-acceptance mappings, eight S-series/four D00 criteria and three local links. Live read-only Scryer comparison matched all 17 captured entries. This was static verification; no product/native tests ran and original acceptance identity provenance was not independently certified.

The first Sol/xhigh review attempt ended incomplete after partial/truncated artifact inspection and supplies no Final Review clearance. A fresh Sol/xhigh bounded retry completed the static review of all three artifacts at the initial frozen commit. Two P2 document defects were confirmed: the broad `resp-237` legacy-migration claim contradicted v2-only migration, and the spec overstated COD-111 exact approval. No additional confirmed defect survived that document review.

Corrections at `178e20c9f9a3be29d2b03281d23a571d121e6019` narrow the live/captured claim to managed-v2 and qualify the unverified approval/publication input. Focused independent Terra/medium verification passed the correction diff, JSON, local links, retained criteria/requirement mappings and a fresh 17:17 live Scryer comparison with zero mismatches. The contracts at that revision have SHA-256 `278bbbe8cf09db19d2e12159608e35c3b466cbd861caaa331b30f9351ffa56a5`; the architecture snapshot has SHA-256 `73d7901161481fe5119b5570c9291cc7336fb364d74cf6580959035566bdf136`.

The reviewer identified a conditional ambiguity in migration's public publication boundary. The final clarification explicitly binds migration result certainty/active identity to installation configuration cutover, treats a new root's internal pointer as prepared participant state and adds the corresponding pre-cutover failure oracle. The original reviewer has not yet focused-verified these final corrections; no final-snapshot clearance is claimed until that check actually returns. This remains a static proposal review, not product/native qualification or approval of future implementation.

## Original proposal acceptance status

| Gate | Status at initial draft |
| --- | --- |
| D00-A01: complete traced specification | Authored; independent review pending |
| D00-A02: settled contracts, complete inventory and bounded ownership | Contracts proposed; complete fresh graph inventory/exact downstream file bindings remain pending |
| D00-A03: linked impacts resolved | Explicit impact/disposition table authored; overlapping pending intent requires owner/human reconciliation before product implementation |
| D00-A04: immutable approved revision and signed intent | Not satisfied; no final exact-revision human approval exists |
| Product/native/packaged acceptance | Not executed by this docs-only task |

## Approval and authorization boundary

The 2026-11-16 “Please implement the plan” authorizes the D00 authoring/review/PR work. The earlier multiple-choice answers select proposal direction. Neither is approval of uncreated final committed bytes or permission to start D01. No approval record is pre-created.

Any proposal PR must remain an explicitly proposed contract package. COD-126 must stay open while its completeness/approval gates remain unmet; dependent issues remain blocked by it. The concrete reviewed commit, PR, verification report and reviewer outcome will be recorded after they exist.
