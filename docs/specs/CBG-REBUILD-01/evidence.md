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
