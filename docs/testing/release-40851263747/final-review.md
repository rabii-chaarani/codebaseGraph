# Independent final review — immutable release publication correction

## Outcome

**PASS for the reviewed implementation snapshot. No confirmed P0–P3 defects.** The changed publisher implements draft → upload the four validated native archive/sidecar assets → remote size and byte comparison → publish. The independent local evidence is current. This result supports the required final-review gate for the authorized tested/reviewed PR handoff; it is not user approval, merge/publication authorization, hosted qualification or aggregate closure.

## Scope and canonical inputs

| Field | Reviewed input |
|---|---|
| Repository/worktree | `rabii-chaarani/codebaseGraph`; `/private/tmp/codebasegraph-immutable-release-assets-20261029` |
| Endpoint | Tested, independently reviewed PR only. No merge, live release recovery or v4.0.3 mutation. |
| Baseline | `5889ed09954a4a07efd2590dd1bd380ee77be06a` |
| Candidate | Branch `fix/immutable-release-assets`; HEAD `ad3beec36502448c238f236c9af16702ddce107c` (documentation-only approval record), plus exactly three unstaged product files below. Staged/untracked inventories empty. |
| Canonical contract | `docs/release.md` at full baseline SHA. Baseline content SHA-256 `5f867117736b465211c677ae7bbb19aec5d47ec7837228d7f50cfe0ba7fb3991`. |
| Frozen scope/acceptance | `docs/testing/release-40851263747/approval-and-acceptance.md` at full candidate HEAD; read bytes match that committed version. Acceptance items 1–5. |
| Explicit decision references | Engineer-supplied user “try the required workflow correction” and “yes i approve the inspection”; conversation `01a1876f-3b43-750e-a9c9-c326dc4e6e10`. Review does not create approval. |
| Stable proposal | SHA-256 `c552e03401ad3160b0472499df0a97f356e658f6affb12269a27c0526860eeb6`, independently checked. |
| Intended responsibilities/brief | Existing `resp-259` / `resp-276`; restorative parent `chg-4kq3vn`, execution brief `chg-6qg638`. Scoped pending reads contain no claim changes; no new claims are needed for this restoration. |
| Review boundary | Actual three-file product diff and committed scope record, with unchanged nearby artifact validation/job gates as needed. No repository-wide audit. |

### Frozen product bytes

| File | SHA-256 |
|---|---|
| `.github/workflows/release.yml` | `aca94a20d46b2ecca85a9c112bb21daa7df1fb418e84f44d28bd35037c706bf3` |
| `crates/xtask/src/release/workflow_tests.rs` | `e567ddb0bb1679c81566497607dc14e54207de8e341c5ab18116f93f23f53de7` |
| `docs/release.md` | `abe55daa1bd234badd8707e0b1f47a77dd90e37b632a3bdf3cd42e28d97ed948` |

These hashes matched before source review and after the final review checks. HEAD/status stayed unchanged. `crates/xtask/src/main.rs` is unchanged at SHA-256 `70507a624361a97a7bcdb28412b31112258fcb023fa8e80dccd943b113b335ad`.

The actual Git product diff SHA-256 is `60cbb764495f8b579012bdcbbfe8f3f76a35300c583a107ef88405e033324632`, retained in `actual-product-diff.patch`. Production's supplied `final-diff.patch` has the same patch plus a four-line Git status trailer; it contains no different product change.

## Findings

**None confirmed.** No P0–P3 finding IDs were issued. No fixes were made or requested on speculative grounds.

## Consequential paths challenged

| Contract/hypothesis | Safeguards and evidence examined | Result |
|---|---|---|
| Publishing still freezes assets too early (acceptance 1/2) | `release.yml:479–486` POSTs `draft:true`; `:510–511` completes upload and calls verification; the only draft publication PATCH is `:520–522`. `:488–500` enumerates expected asset names, rejects absent/duplicate/wrong-sized matches, downloads by asset ID and uses `cmp -s` on each archive **and** sidecar. Production's old-workflow RED log fails on post-freeze upload; the independent final freeze-on-publish test passes. | Defeated. No filename/checksum-text-only success path is present. |
| Existing drafts are invisible or recreated; lost acknowledgement replays a write (acceptance 2/5) | Authenticated `--paginate --slurp` listing and exact-tag cardinality check `:461–465`; existing release/tag preflight `:466–485`. Accepted/lost POST or PATCH responses stop the invocation under `set -euo pipefail`; a later independent invocation observes the existing draft or immutable release. Actual Bash tests cover partial draft and both lost-response cases. | Defeated. No automatic write-retry loop is added. |
| An immutable retry clobbers or silently accepts corrupt assets (acceptance 3) | Published `.immutable == true` branch `:502–505` can only verify and exit; mismatch returns nonzero before `gh release upload`. Exact tag/source preflight happens first. Independent cases include missing archive/sidecar and same-size corruption of each; no-write assertion checks POST/PATCH/DELETE/upload. | Defeated for the expected validated asset set. |
| Shared implementation publishes legacy drafts or blocks allowed mutable repair (acceptance 4) | Metadata/PR checks remain automatic-only `:446–449`; missing legacy release/tag fails without creation `:468–480`; publication PATCH additionally requires automatic mode `:512`. Mutable published asset repair `:507–511` never republishes; matching sets skip writes. Actual Bash manual draft/mutable/immutable cases pass. | Defeated. Existing-tag remains native-assets-only. |
| Latest, provenance, dry-run, crate or finalization gates are weakened (acceptance 4) | Latest stable-version ordering is retained and recomputed immediately before PATCH `:513–521`. Existing exact tag SHA is checked `:474–478`. Upstream CI/source/PR/metadata/complete-artifact gates are unchanged in the actual diff. Current source has exactly two targets; `main.rs:683–777` validates both exact provenance/archives/sidecars and rejects unexpected input. Four-file staging remains `release.yml:371–384`. Dry-run output and publisher condition remain `:135–146,394–400`; crate/finalization conditions remain `:525–539,609–630`. | Defeated within reviewed scope. Source checks and executed behavior/policy cases agree. |
| Real `gh release upload` cannot resolve the draft, or readback alters bytes | Version-matched GitHub CLI **v2.98.0** primary source: `shared.FetchRelease` performs published REST and draft GraphQL lookups; draft lookup resolves the database ID and fetches it by REST. `uploadRun` uses that release's upload URL, honors `--clobber`, uploads without publishing. The workflow uses the same step-scoped `GH_TOKEN` for list/create/upload/readback/PATCH. GitHub REST documents push-authorized draft listing and octet-stream asset downloads with 200/302; the Go HTTP client follows redirects and does not reattach auth across a host change. `gh api` raw binary copy streams bytes unchanged to redirected stdout; strict generated sidecars contain no terminal control content. | External compatibility hypothesis defeated by source/documentation, not live API execution. Hosted runner/token qualification remains a separate limit. |
| Bash 3 or `set -e` conditional behavior bypasses failure | Bash 3.2 syntax check on the exact extracted publisher passed. Actual independent tests execute that body under Bash 3.2. For `verify_assets` called in an `if`/`||` context, consequential listing/download/file/empty-entry/cmp failures have explicit returns; the function does not depend on inherited errexit for these checks. Upload and PATCH are top-level guarded by the script's errexit. | No supported causal failure found. |

Remote asset verification intentionally checks all expected staged files; no new policy about unrelated public extra assets was imposed. Existing source and approved proposal are the oracle, not stale wiki target counts.

## Actual executed evidence

Independent Evidence Verifier report: `/private/tmp/codebasegraph-immutable-release-evidence-20261029/final-report.md`, SHA-256 `29b8662d958b7cbb661fd00be479b5fd799d591c1ed82430913750b8a9111bf0`. I read the report, raw suite/policy/release-gate/Clippy logs and genuine JUnit; I did not substitute the worker's tests for independent execution.

- Native project environment: macOS 26.6.2 ARM64; Cargo/rustc 1.96.0; Bash 3.2.57; jq 1.7.1; dedicated verifier Cargo target outside the repository; `--locked --offline`, closed mock-gh, no live tokens.
- Full `cargo nextest run -p xtask` inventory: **52 passed, 0 failed, 0 skipped**. Includes 18 actual-workflow Bash tests and 13 workflow-policy tests.
- Genuine JUnit run `55d7263f-ab59-47b9-ad8d-8cbefe4fc455`; file SHA-256 `1c614c1eab8ab239d8b546601e77af248ff59f178c859c32e3e89b9e42f15606`. Parsed testcases=52, failures=0, errors=0, skipped=0.
- Independent `check-workflows`, local `release-gate`, `cargo fmt --all -- --check`, strict `cargo clippy -p xtask --all-targets --locked --offline -- -D warnings`, and `git diff --check`: **pass**, no warnings. Exact commands/environment are in the verifier's unchanged report.
- Baseline independent 24-case selected suite/policy passed before implementation; it did not establish immutable ordering. Production's separately attributed enhanced-fixture RED run failed against the old workflow with `Immutable release rejects asset upload`; this is worker evidence, not my execution.
- My additional executed check: `/bin/bash -n` on the exact extracted new publisher body, exit 0. Other review evidence is direct source/diff trace and read-only primary-document retrieval. No redundant full test rerun or live probe was made.

## Primary external references

Read-only current GitHub REST/immutable-release documents and local-version-matched CLI source were used; URLs, content hashes and inspected copies are retained outside the repository in this report directory.

- [Immutable releases](https://docs.github.com/en/code-security/concepts/supply-chain-security/immutable-releases): draft → attach assets → publish; tag/assets lock after publication.
- [REST releases](https://docs.github.com/en/rest/releases/releases?apiVersion=2022-11-28): draft listing visibility and `draft:false`/`make_latest` update behavior.
- [REST release assets](https://docs.github.com/en/rest/releases/assets?apiVersion=2022-11-28): `Accept: application/octet-stream`, 200 or 302 download response.
- [gh v2.98.0 draft/published lookup](https://github.com/cli/cli/blob/v2.98.0/pkg/cmd/release/shared/fetch.go), [upload](https://github.com/cli/cli/blob/v2.98.0/pkg/cmd/release/upload/upload.go), [API output](https://github.com/cli/cli/blob/v2.98.0/pkg/cmd/api/api.go), [binary output guard](https://github.com/cli/cli/blob/v2.98.0/pkg/iostreams/content.go), [auth redirect handling](https://github.com/cli/cli/blob/v2.98.0/api/http_client.go).

The hosted `ubuntu-latest` CLI version is not pinned or observed here. These references establish the inspected CLI/API behavior; they do not claim a hosted release was run.

## Pending gates, limitations and conditional risks

- **No confirmed product defect remains.** No separate conditional defect was scored; hosted service/token/runner semantics are unexecuted qualification, not proof of a source failure.
- Local Bash/mock-gh tests are not hosted GitHub immutable-release execution, Windows native packaging qualification, full workspace checks, production gate confirmations or publication verification. The closed mock does not execute gh's GraphQL/upload transport; primary source was consulted to challenge that boundary.
- Exact-source Scryer binding/ingestion/reconciliation remains pending: the target worktree has no model, while the registered primary-root model/graph is older with unrelated work. I fetched governing rules, scoped pending changes, plan/committed Release Verifier intent and drift read-only. No false same-snapshot ingestion/fold/reconciliation or new claims were made. Engineer retains this integration limit.
- Linear is unavailable; tracking synchronization remains with Engineer. `actionlint` is unavailable; no install was attempted. YAML parsing, workflow policy and Bash fixtures are not described as actionlint execution.
- Graph semantic search/context was available to this reviewer and used for `docs/release.md`, `main.rs`, change impact and artifact dependencies. It supplies older-root orientation only. The exact user exception authorizes the named workflow/test module inspection; no broad source exception was used.
- Current wiki release architecture still lists four historical targets. Current canonical contract/source validate two targets/four assets; no implementation decision was taken from the stale list and no wiki write was made.
- This review changed **no** source, model, tracking, Git state, constants, dependencies, release settings, credentials or live release state. Only outside-repository report/evidence copies were written. No delegation.

## Settings, reusable knowledge and next owner

- Fresh independent `final_reviewer`, required `adversarial-review-engineer` method. Role map and required method agree: **gpt-6.1-sol / max**, `agent_type: default`, `fork_turns: none`. Parent explicitly reports host acceptance of this exact dispatch pair; runtime model/effort self-inspection is unexposed, with no known mismatch. I did not implement this change or dispatch another reviewer.
- Reusable knowledge assessment: freeze-on-publication tests plus downloaded-byte comparison are a useful durable release lesson, but this read-only assignment makes no wiki/memory write. Engineer may distill it after reconciling remaining gates.
- **Next owner: Engineer**, to reconcile the pending tool/platform/endpoint gates and authorize Production's Conventional Commit/push/PR operations. Production must preserve these reviewed product bytes; any product/test change needs affected independent checks and focused final re-review. No merge/release/live v4.0.3 action is authorized by this result.
