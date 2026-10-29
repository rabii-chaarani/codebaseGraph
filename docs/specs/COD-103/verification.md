# COD-103 — independent evidence plan and capability matrix

Date: 2026-10-29. **Preparation only; no checks in this plan have been executed.**

## Scope, identity and inspection

- Evidence Verifier role, directly assigned by Engineer. Actual profile independently exposed in this child's persisted events: `openai-codex/gpt-6.1-sol`, `thinkingLevel: max`. This matches inherited coordinator settings; no profile/map changes or role dispatch.
- Proposed requirements: [spec.md](spec.md), SHA-256 `7695dad95775d674e04a4151bc90f5034c30e10dae4e2aff4962073139e5d6ac`. Proposed Design is now available and cross-checked below; combined exact-scope approval/implementation remain pending. This report supplies evidence requirements, not an implementation brief, changed acceptance, file ownership or approval.
- Ordered children: **COD-105 → COD-106 → COD-56 → COD-107 → COD-108**, true stacked PRs through the authorized complete-queue release. Preserve COD-104/admin bypass and COD-99 history. Planner owns briefs/ownership; Production owns Git, integration, baseline and durable report persistence.
- Exact inspected source: **`5889ed09954a4a07efd2590dd1bd380ee77be06a`**, Production’s isolated proposal workspace. Read-only worktree metadata resolves its branch ref to that full SHA. Production supplied clean-worktree verification. Registered older-root graph was used for discovery/orientation only; no graph build or old-root behavior inference.
- Inspected graph-discovered xtask entry point and its referenced modules/workflows/installers: `crates/xtask/src/main.rs`, `native_tests.rs`, `release.rs`, `release/workflow_tests.rs`, `ci.yml`, `native.yml`, `release.yml`, verified-release composite action, `release/install/install.ps1`, workspace and xtask manifests. Read applicable Scryer intent/rules/test radius and configured wiki memory. No Git commands, product/test/model/Linear/wiki/settings writes, tests, builds, dependencies, remote-machine probes, provisioning or credentials operations. This `/tmp` report is the sole write. No constants changed.

## Actual entry points and evidence constraints

Current xtask is Rust/libtest, **not nextest**. `main.rs:150–268` and `native_tests.rs` use `cargo test --workspace --locked --profile native-tests`; both targets retain that profile. Windows additionally supplies `codebase-graph/bundled-windows-extensions,k-wiki/bundled-windows-extensions`. `Cargo.toml:29–33` inherits release optimization with debug assertions, overflow checks and unwind enabled. Do not substitute ordinary `--release` tests: that can omit fault fixtures.

`native_tests::run` captures the exact normal CLI from current Cargo compiler-artifact messages with matching source manifest, excludes test harness binaries, requires one candidate, copies/verifies its digest, supplies `CARGO_BIN_EXE_codebase-graph`, and checks the copy remains unchanged. Retain these guarantees and the existing Windows preflight. Existing regressions are `windows_native_tests_forward_bundled_extensions`, `native_capture_reuses_the_exact_test_profile_and_features`, `native_capture_requires_unique_source_bound_normal_cli_artifact`, `native_tests::tests::captured_cli_survives_cargo_replacing_its_output`, `windows_regressions_precede_the_complete_unfiltered_suite`, and `partitions_every_test_target_and_keeps_new_integration_targets`.

Current `native.yml` runs three cohorts (`unit`, `generation`, `integration`) on each supported target; Windows uses shards 1–4 for each cohort. Native artifacts depend on successful tests and are built/smoked separately using ordinary release binaries. Targets are **macos-arm64 and windows-x86_64 only**. Hosted Windows is `windows-2022` (Server), not Windows 11 consumer qualification. Artifact installer dry-run invokes `pwsh` only; it does not establish PowerShell 5.1 coverage.

The native runner currently writes libtest listings, ignored inventories, selections and `diagnostics/tests/<target>-<cohort>[-<part>of<total>]/report.json`, plus workflow `diagnostics/platform-test.log` and `source-sha.txt`. It does **not** emit JUnit. No `.config/nextest.toml` exists in this source. Thus exact plain-Cargo commands below are valid local test entry points but are **not by themselves Scryer-ingestable evidence**. Select a bounded reporter only for focused claim tests that need JUnit; **leave ordinary Cargo/native CI unchanged** unless an approved requirement needs otherwise. Existing full native CI still supplies its separate behavior/coverage evidence. Focused JUnit cannot replace that suite, fault inventory, source-bound CLI, cohorts/shards or doctests. The follow-up capability check below found a host-local nextest executable, not repository configuration or hosted/Windows availability. Do not fabricate XML/passing verdicts or treat aggregate JSON summaries as named case results. This corrects the early in-flight progress note's premature nextest reference.

## Smallest per-child checks

These are planned checks, not newly invented test names. Existing command filters below are real; names/commands for new regressions remain design/implementation-dependent until actual tests exist. Run from the exact assigned source root through its native Cargo environment. First capture the exact full SHA, toolchain, OS/shell and test inventory.

### COD-105 — C103-R01 / A01: packaged MCP acceptance

1. Add/execute the smallest parser/process regression group at the affected xtask smoke path. Pass only successful exit plus valid expected-ID initialize and tools/list successes with an actual tool entry `name: graph_search`. Fail: nonzero with `graph_search` in error/output; zero with protocol error; missing initialize; malformed protocol/read failure; missing graph_search. Cover both required request IDs rather than finding a string in another field. New group name/interface is pending Design, not an existing command.
2. Exercise the real packaged graph MCP smoke, not parser fixtures alone, within each target's existing artifact job. `cargo run -p xtask -- native-artifact --target <native-target> --commit-sha <full-sha> --output dist/<native-target>` is the existing entry point; use each target's native host and preserve both-binary/archive/wiki smoke coverage. No parallel new benchmark or unrelated product protocol test suite.
3. Diagnostics must retain child exit/status and useful parsed protocol failure context without secrets. Trace to `resp-16`, `resp-17`, `resp-256`. Existing archive-entry tests are not MCP-failure evidence.

### COD-106 — C103-R02 / A02: legacy existing-tag ancestry

Existing Unix-only fixture module `crates/xtask/src/release/workflow_tests.rs` extracts and **executes actual YAML Bash bodies** with closed local `gh` fixtures; it removes GH/GITHUB tokens and rejects unexpected calls. Keep that execution model, not a test of source text/magic strings or a separately reimplemented predicate.

- Existing targeted baseline: `cargo test --locked -p xtask release::workflow_tests:: -- --nocapture` (Unix host with Bash and jq; JUnit route still pending). It includes automatic/resume main-advance, removed-history, exact original source/idempotence, dry-run/recovery-input and complete asset-set regressions.
- Extend that same fixture to actual existing-tag resolution **and the actual final pre-upload ancestry guard**. Positive comparisons: identical and source-ancestor/current-main-ahead. Negatives: behind/diverged/unavailable/malformed comparison and failed source acquisition. Case: initial ancestry valid, current main changes/removes source before mutation; assert no upload/mutation. Log call order and require comparison before authorization and again before legacy upload, not merely somewhere in the workflow.
- Preserve strict tag resolution, exact successful main-push CI, historical tag without release-PR association, all-artifact/provenance checks, dry-run non-mutation and native-assets-only scope. An eligible old source remains valid after ordinary main advance; do not impose tip equality or crate publishing.
- Existing automatic guard is `.github/actions/verified-release/action.yml:43–45`; legacy resolution is `release.yml:115–125`; publication revalidation currently applies only when automatic (`:414–423`), upload at `:487–491`. New fixture names are pending Design. Trace to `resp-259`; present attached automatic-release tests do not prove the legacy path.

### COD-56 — unchanged CBG-PLAT-01/S05 R05 → A09

**Reuse canonical revision0.11 at full SHA `8a410c7a22f5291f06a1cece5cd9e7c0be255e72`, original path/approval/COD-51 prerequisite and closure owner. Do not replace or renumber A09.**

- Required execution is actual **Windows 11 x86_64 desktop**, built-in **Windows PowerShell 5.1** and supported **PowerShell 7**. Record OS edition/version/build, process/OS architecture and full shell versions. Server CI/pwsh/macOS/cross-build/source review cannot replace it.
- Use checksum-valid **native controlled-exit executables**, not script-function mocks, for graph `--help` and wiki `--version`. Retain A09's passing and failing candidates, success/failure, DryRun/real-install, validation of both binaries/checksums, and byte-identical existing destinations before/after failure. Independently fail each probe, including wiki failure after graph success, before either replacement. Keep checksum and supported-host barriers; no atomic two-binary rollback redesign.
- Current source `Assert-Binary` checks bytes/hash then invokes the native process through `Out-Null` without checking the native exit. Both Assert calls precede destination creation/replacement. Qualify the corrected exact script/archive and record candidate/archive/script hashes, actual native exits, before/after bytes and installer result for every required case.
- Existing direct invocation surface: `powershell.exe -NoProfile -File <exact-install.ps1> -SourceDir <fixture> -BinDir <destination> [-DryRun]`, then the corresponding `pwsh -NoProfile -File ...`. These are **invocation templates, not an existing complete qualification harness**. Design/Planner must supply the controlled-exit harness/report command and canonical A09 case mapping; this report invents neither a filename nor an execution pass. No automatic execution-policy change is authorized.
- Trace applicable `resp-16`/`resp-256`; no direct installer model anchor currently surfaced. Planner/Architect reconcile applicable existing anchors/tests proportionately, without a new topology. The Windows desktop gate belongs to **COD-56**, not COD-107.

### COD-107 — C103-R03 / A03: docs-only helpers

After combined approval settles optional stdlib/local-only/invocation/package details:

- Execute source-policy fixtures with one genuine allowed helper under `docs/` and nearest disallowed Python neighbors outside it. Retain root tooling (`pyproject.toml`, `requirements-dev.txt`, prohibited directories) and workflow Python prohibitions. Test the actual policy function/release-gate route, ordinary and confirmed-production modes. A docs path cannot exempt product implementation or workflow/security checks. New fixture names remain pending Design.
- Existing real commands: `cargo run -p xtask --locked -- release-gate`; and `cargo run -p xtask --locked -- release-gate --production --confirm release-environment --confirm private-vulnerability-reporting`. For fixture-level policy testing these confirmation flags exercise parser/policy branches only; they do not attest real protected-environment/private-reporting configuration. Genuine production confirmation stays with authorized owners.
- Prove package exclusion by inventory of **actual generated** native archives and publishable `.crate` payloads, not only manifests or a `--list` prediction. Existing commands are `cargo package --locked --no-verify` and `cargo package -p k-wiki --locked --no-verify --config 'patch.crates-io.codebase-graph.path="."'`, plus native-artifact jobs. Inspect payloads and assert no helper/Python dependency; retain existing crate-size/archive-contract checks. Git/tag source snapshots may contain docs, subject to approved contract.
- Preserve archive bytes/history and do not restore archived helpers. Trace `resp-14` and applicable workflow `resp-15`; neither current source inspection nor unrelated passing workflow tests establish the new positive boundary.

### COD-108 — C103-R04/R05 / A04/A05: trusted semantic mode and required

The now-available proposed Design defines the CLI/schema/trust/guard seams in the focused cross-check below. **Implementation test names remain pending; this evidence report does not assign ownership or create a benchmark.** The requirements/design field allowlist, observed bot identity, output contract and constraints remain proposals until combined approval. Bind tests to that exact approved interface:

- One positive actual base/head semantic diff covering the real generated release shape (manifest; root/wiki/xtask versions; wiki local dependency version; corresponding workspace lock package identities; consistent new changelog section). Verify configured release-token identity rather than guessing a bot username. Versions must agree.
- Boundary negatives: code/workflow/installer/build/package/feature changes; disallowed metadata field; external lock source/checksum/resolution/dependency-edge change; historical changelog rewrite; mixed/untrusted/fork/wrong-identity/wrong-base PR; rename/delete; failed/partial/uncertain diff acquisition. Branch/title/label alone cannot grant lightweight mode. Fall back to full CI on ineligibility/uncertainty; invalid/missing classification must never create green `required`.
- Execute the real aggregate/guard body with mode/check result fixtures: valid full success; valid lightweight with exactly authorized heavy skips; failed/cancelled/missing/unexpected mandatory result; missing/invalid classification; stale head/base. Require one stable `required`, with fresh classification/qualification for updated head/base. Any changed policy validators receive positive baseline and focused negative mutation tests.
- Hosted event evidence after approval: trusted allowed metadata PR selects lightweight and heavy jobs are intentionally skipped/not run; mixed/untrusted PR selects full; ordinary PR and **every main push** use full CI. Capture run/attempt/event/base+head identities, per-job conclusions and actual required context, not just green UI or configuration. Mandatory lightweight policy/consistency/fmt/advisory checks must actually pass. No privileged PR trigger/new permissions or PR-artifact promotion.
- Preserve native selection/provenance (`resp-254`–`resp-256`) and workflow policy (`resp-15`). Architect has drafted the genuine eligibility/fallback responsibilities in proposal-only **`chg-9xdkd4`** on `node-1`: **`resp-1rnbs9`** / **`resp-tddbkj`**, before combined approval/sign-off. These are real draft intent IDs, not approval, implementation or verdicts. Approval/sign-off must identify those exact proposed claims; only then may dependent implementation begin. No verifier claim writes or fabricated IDs/verdicts.

## Hosted stack, main CI and final publication — separate evidence stages

1. **Local baseline / focused regressions:** exact-source fixture groups above; actual failed/passed named cases; real reporter/JUnit ingestion where required. Static source inspection performed here is not a baseline test execution.
2. **PR stack qualification:** approved bounded stack-base trigger/staging strategy must exist before dependent PRs rely on it. Follow the [first-child stage rule](delivery.md#first-child-staging-clarification-pr-01): COD-105 staging-only handoff may precede the first child event after its applicable local/artifact/full-main-base-PR checks and independent final review/fix verification. Production obtains the first actual fresh COD-106 non-main-base event; Evidence Verifier records full-CI/stable-required base/head/run/attempt evidence and Engineer links it to C103-A05 and both existing ledgers. Missing/failed/stale proof blocks full COD-105 qualification, COD-106 handoff to COD-56, every main integration, Done and release. This deferred proof is not passed or waived; subsequent retarget proof is due at its actual per-PR stage, not before its own PR may open. Current CI is main-base-only and policy enforces this; it cannot qualify literal non-main-base children unchanged. For each actual PR head/base and update/retarget, record fresh required context, selected mode and mandatory job evidence. Do not replace literal stacking with cumulative main PRs or missing/greenless checks.
3. **Serial main integration:** Production verifies compliant non-bypass merge path. Merge each authorized child serially; each **actual merge SHA** must pass full main-push CI before the next integration step. Main native diagnostics must retain the native profile/features, source-bound CLI and fault-case inventory. Both target artifacts must be produced/smoked from that same source; PR lightweight results never authorize release.
4. **Consumer Windows qualification:** COD-56 A09 stage above remains an independent desktop gate. Passing windows-2022 unit/artifact CI is useful evidence only within its Server environment; it is not a product defect if desktop capability is not supplied.
5. **Final candidate and independent publication:** after all five children, acceptance/review/evidence gates and exact approved delta reconciliation, Final Reviewer reviews the immutable complete candidate before its release merge can trigger unattended publication. Production owns baseline refresh, allowed release contents/actions and durable evidence. Independent verifier subsequently compares selected release SHA/own successful main CI/run attempt, tag target, version, exact complete two-target artifacts and SHA-256 sidecars against retained CI provenance, public release inventories, crates.io downloaded package bytes/version and embedded `.cargo_vcs_info.json` source SHA, plus successful finalization/labels. Report missing/mixed/source-mismatched/pending publication honestly. No verifier publication or retries.

Production's dated refresh says merged release167 source `5889ed0…` had own CI40837906797 queued, pending label, last fully verified publication4.0.2. **Do not call4.0.3 published or refresh/dispatch it here.** Production re-verifies any normal publication and freezes the final baseline/delta. Preserve approved PR158 subset inclusion without implying COD-53 closure or republishing an already-completed baseline. No partial-queue release, new target, unapproved delta or aggregate platform/COD-103 closure.

## Capability matrix / next owner

| Capability | Current evidence / limit | Gate / owner |
|---|---|---|
| Verifier required profile | Independently exposed Sol/max; ready | No model blocker |
| Exact source/native test interfaces | Inspected at5889ed0; no test/build executed | Ready for planning, not qualification |
| Local Rust/Bash/jq/JUnit execution | Entry points inspected; host-local cargo-nextest0.9.140 version/help checked without tests/builds; ordinary native runner has no JUnit; Bash/jq/test execution not qualified | Planner selects focused reporting option below; future execution/Windows tool availability remains conditional, not a proposal blocker |
| Hosted PR-stack CI | Current main-only PR trigger cannot qualify parent-branch PRs | Design/Planner/Production settle approved staging; hosted permissions/runs not claimed available |
| Hosted native/main CI | Existing macOS ARM64 and Windows Server workflow only; proposed future runs are not passes | Production schedules authorized exact revisions; verifier checks results |
| Non-bypass merge actor | Prior Production refresh found only owner/admin always-bypass actor; no enforceable actor established | **Integration/release pending**; Engineer/owner provides authorized non-bypass actor, Production verifies; retain admin bypass |
| Windows11 x86_64 + PS5.1/7 | Prior Production refresh found0 repository self-hosted runners; no available desktop interface established | **COD-56 A09 and complete-queue release pending**; Engineer/owner supplies existing suitable desktop or separately authorizes provisioning. No re-enumeration/provision here |
| Release baseline/publication | PR167 queued/pending at prior refresh, no independent new publication result | Production refreshes baseline; verifier independently checks final publication only when assigned |
| Combined approval / final reviews | Requirements/design/plan and exact-source proposal review/approval pending | Engineer coordinates; no implementation readiness asserted |

## Scryer test radius and evidence handoff

Fetched binding `test-verdicts`, `test-attachment`, `fold-evidence-gate`, `anchor-completeness`, proportionality and closure rules. Current older-root radius lists7 files: xtask `main.rs`, `release.rs`, `release/workflow_tests.rs`, CLI install tests, execution/run, semantic enrichment and storage harness. Relevant release entries are stale; unrelated radius/drift is **not** permission to run broad suites, reconcile globally or attach unrelated tests. Refresh applicable scope against final source before execution; execute applicable required radius plus acceptance-specific new tests. Do not treat an empty radius or absent matching test as pass.

Observed scoped attachments: `resp-15` attaches `workflow_policy_rejects_unconditional_release_publication`; `resp-254` attaches Windows feature mapping; `resp-256` attaches unexpected archive entries; `resp-259` attaches main-advance and exact-source/idempotence fixtures. `resp-14`, `resp-16`, `resp-17` did not surface matching acceptance tests. Existing tests do not mechanically cover every new failure/positive case above. Anchor/attach only genuine implemented behavior; record actual named case output/JUnit after last source/test edit, ingest **one call per executed report file**, inspect unmatched/ambiguous/unmentioned cases and remaining radius. No model/verdict writes are authorized in this preparation assignment; do not force-fold or infer passing from attachment/configuration.

Each eventual evidence record must identify work/spec/criterion (or unchanged A09), applicable claim, exact full SHA and archive/script hashes, environment, command, named cases, actual exit/result, report/run/attempt links and limitations. Production persists/version-controls reports at the authorized canonical location. Required order: Architect drafts real proposed claim changes and IDs **before combined approval/sign-off**; approval/sign-off identifies that exact intent; dependent code/tests start only afterward; applicable owner attaches genuine implemented-source anchors/tests; verifier executes actual named cases with JUnit and authorized ingestion records current verdicts; only then may the authorized owner fold a testable claim with its passing evidence. Proposals/test checker IDs are not verdicts. This verifier preparation writes no claims, attachments or verdicts. No new repository memory is warranted: this is transient, unapproved preparation; durable knowledge follows verified outcomes.


## Focused reporting options — follow-up, 2026-10-29

Engineer requested a bounded capability check, not a native-pipeline retrofit. Executed **only** these mechanical commands from the exact proposal workspace:

- `cargo nextest --version` — **exit0**, `cargo-nextest 0.9.140 (a9fef2964 2026-07-05)`, host `aarch64-apple-darwin`.
- `cargo nextest run --help` — **exit0**. Confirmed `--cargo-profile`, `--config-file`, `--user-config-file none`, nextest `--profile`, `--no-tests fail`, package/bin selection and `-E/--filterset` interfaces.

No tests, compilation, installation, nextest config/report creation, PowerShell execution, native-machine check or CI change occurred. Availability is **only this macOS host**. The absence of nextest/configuration on another assigned host is a conditional focused-execution capability gate, **not a proposal blocker** and not permission to install it. Ordinary Cargo/native CI remains unchanged.

### Option1 — existing nextest for focused Rust claim tests (recommended locally)

Use the already available runner with an explicit temporary evidence config outside the repository and a named-case filter. No Cargo dependency, product manifest, repository nextest policy, global settings, or new full-pipeline reporting component is needed. Planner pins the approved case list, environment and report destination in its brief. For example, the minimal **proposed**, unexecuted nextest config is:

```toml
[profile.cod103]
retries = 0

[profile.cod103.junit]
path = "/absolute/evidence/cod103-focused.xml"
```

Corresponding proposed command shape:

```text
cargo nextest run --locked -p xtask --bin xtask --cargo-profile native-tests --config-file /absolute/evidence/cod103-focused-nextest.toml --user-config-file none --profile cod103 --no-tests fail -E '<approved actual test filterset>'
```

`native-tests` is the **Cargo** profile; `cod103` is only the temporary **nextest reporting** profile. They are not interchangeable. Existing `tests::workflow_policy_rejects_unconditional_release_publication` is a real selectable baseline case; the new parser/classifier/legacy cases need actual implemented names before pinning their filterset. Current workflow Bash fixture module is Unix-only, so execute those actual-body cases on Unix with Bash/jq, not pretend their exclusion on Windows is a pass.

Before the first authorized run, validate config acceptance and ensure the filter selects the intended named cases; retain the runner exit status, inventory and report. No-tests, build/config/setup errors and missing report fail evidence collection rather than generating a synthetic pass. Inspect report case identifiers against Scryer's real attachments; ingest each actual report once and examine unmatched/ambiguous/unmentioned results. `retries=0` avoids quietly accepting a retry without preserving it. This option qualifies **only the selected Rust fixtures**. It does not replace ordinary native-test execution, process fault coverage, captured source-bound CLI or doctests. No nextest migration of `native_tests::run` is proposed.

### Option2a — Windows A09 cases as focused Rust wrappers, JUnit from existing runner

If the owner-supplied Windows11 desktop already has an authorized Rust test environment and nextest available, Planner can place each exact canonical A09 case in the assigned existing Rust test surface. That case launches the **actual** PowerShell5.1 or7 process, actual exact installer script and controlled-exit **native executable** fixtures; asserts the expected installer outcome plus checksums/destination bytes; and retains native exits/stdout/stderr/source and OS/shell identities. Nextest then emits JUnit for the real Rust assertion outcomes using the same bounded approach.

This has reliable named-test/claim attachment without a separate XML writer. Its cost is declared Rust/nextest tooling on the desktop; do not infer that capability, install it or substitute Server CI. If wrappers ever exercise product process tests, preserve that test's required source-bound CLI/profile/features; an independent installer-native fixture must not be confused with the shipped CLI. All platform-required A09 cases must run under **both shell families on the actual desktop**. A selected subset or local tool check does not qualify A09.

### Option2b — bounded PowerShell actual-case harness, built-in XML writer

If a separate Rust runner is unsuitable or unavailable on the qualified desktop, an approved **single A09 test harness**, not a general reporter project, can execute the exact same canonical cases and serialize their actual outcomes using .NET `System.Xml.XmlWriter` available to PowerShell5.1/7. No Pester/module availability or install is presumed, and no product dependency is added. Planner selects its assigned test/evidence location and validates case-name/claim mapping.

The harness must record structured results **at the point assertions execute**, not infer passes from loose log strings or aggregate success:

1. Register the canonical required case/shell set before execution. For each case, stage its real native controlled-exit candidates/checksums and pre-existing destinations; record hashes/bytes and source/archive/fixture identity.
2. Launch the actual installer in a fresh approved shell process and capture the true process exit, native-probe evidence, stdout/stderr and destination state. Each case passes only after all expected assertions. A deliberate nonzero probe case is a passing *test* only when it actually rejects the installer and preserves both destinations as required; a nonzero exit alone is insufficient.
3. Append a result object containing stable case name/class, shell/OS/source identity, assertion outcome, failure/error details and elapsed time. Serialize those objects into JUnit test suites/test cases using XML-safe APIs; include diagnostic output/properties without credentials.
4. Compute counts from those actual results. Assertion mismatch is failure; setup/fixture/unexpected harness error is error. Missing/unrun/skipped required cases cannot count as qualification. On abort, preserve executed results and an explicit incomplete/error outcome, and return nonzero. Write a complete report safely; never write a green placeholder when process creation/configuration/reporting fails.
5. Independently compare the report against the required A09 case/shell matrix and retained raw evidence, then ingest genuine named results only where valid Scryer test anchors exist. An unmatched report remains evidence but does not settle the claim. Do not attach fabricated source/test names just to make ingestion green.

A tiny direct actual-result XML serializer is legitimate test-harness output; hand-authored pass records or conversion of an aggregate CI result into invented testcase passes are not. Either2a or2b can satisfy reporting once approved and actually executed; neither changes A09, lowers its desktop requirement, or creates new acceptance. Actual harness command/names remain Planner/Design-dependent. **Recommendation:** Option1 for focused xtask claims; select2a only if the supplied desktop already supports it, otherwise the narrower2b harness. Keep both choices conditional at proposal freeze, with the chosen route pinned before dependent claim execution/fold.


## Focused Design cross-check — CLI and executable guards

Cross-checked final proposed [design.md](design.md), SHA-256 **`5fa6cbd4fc226de259a83444b87e1fb117582aaa67193670276ed9c45688cf12`**, independently matching Engineer's final notification. Earlier notified/intermediate design hashes are superseded for this cross-check, not approved revisions. No material behavior contradiction with the proposed Requirements/evidence outcomes was found. Design is still proposed, not approved. This cross-check adds no policy or new claim/test names.

### Bind focused checks to the proposed actual interface

Proposed invocation, **not executed**:

```text
cargo run -p xtask --locked -- ci-mode --repo <git-object-checkout> --base-sha <B> --head-sha <H>
```

Execute the helper and its manifests/lock/build context from protected-main **B**; read H only as Git object data in the separate store. Test explicit repo-path use from an unrelated cwd, strict full SHA validation, actual B→H rather than merge-base-only proof, B ancestry and non-execution of head code. The new command is absent from inspected5889ed0; no claim that it already runs.

Successful CLI output is **one** JSON object with `schema_version:1`, `mode:"lightweight"|"full"`, bounded `reason_code`, exact `base_sha:B` and `head_sha:H`. Test semantic decisions through `classify_metadata_diff` and test the real CLI output separately; unit decisions alone do not prove workflow wiring. Required negative/output cases include wrong/missing/unknown schema/mode, incorrect types/SHAs, malformed/extra nonprotocol output, failed invocation and omitted outputs. Actual reason-code values/test identifiers await implementation and the brief; this evidence plan does not invent them.

Distinguish these outcomes explicitly:

- Unsupported/untrusted/mixed/uncertain eligibility → **fresh valid full** decision. A failed eligibility attempt may be converted only by the designed workflow path into a fresh well-formed full selector result with bounded reason and correct event identity. It cannot become lightweight.
- Failed/cancelled/skipped selector, malformed/missing decision or unknown identity → **required fails**, not an implicit successful full mode. Freshness/current API inability at the final guard also fails; full mode does not excuse stale base/head.

Retain complete real six-file PR165/167-shaped positive fixtures. Negative semantic checks include same/decreasing/inconsistent versions, incomplete set, wrong package/table context, duplicate/decoy/multiline keys, unsupported layout, any external lock/source/checksum/edge change, changelog-history edit, rename/delete/type/symlink/submodule data and malformed acquisition. The conservative recognizer may safely choose full for unsupported valid metadata syntax; do not widen it merely to make a fixture pass.

### Actual selector / aggregate Bash, not a second implementation

Extend the existing Unix closed-API actual-YAML-body fixture pattern for the eventual `classify` and `required` job guards. Pin real implementation identifiers only when those bodies/tests exist. Execute the actual API/JSON/provenance/fallback branches with allowlisted mocked requests and a call log; unexpected API calls fail. No live token, head execution or CI mutation is needed.

The selector's trust fixture must arrange actual current repo/PR/main/B/H/event agreement, approved release ref/label and proposed observed creator **ID41898282/login/type**, protected-B helper path and missing-helper bootstrap. Foreign/stale/draft/ordinary/non-main-base inputs select full; guarded code must not learn a new identity from a PR. Missing classifier on B makes COD-108's own PR full. PR-head repository-policy/release-gate and classifier unit tests remain ordinary untrusted, secret-free full-PR checks; **they cannot supply lightweight admission before the protected-B helper exists or override the selector**. None of these tests approve that identity constant or strengthen GitHub's head-editable workflow guarantees beyond Design.

Execute aggregate cases against the proposed exact state table:

| Selector + freshness | Common jobs | Heavy jobs | Expected required |
|---|---|---|---|
| Valid full, correct current event/B/H | classify/fmt/supply-chain/repository-policy all success | clippy/publish-dry-run/native all success | Pass |
| Valid trusted lightweight, final API identity/B/H/trust revalidated | All common success | Every heavy job exactly skipped | Pass with explicit skips |
| Missing/failed/cancelled/invalid selector or stale/unavailable final identity | Any | Any | Fail |
| Either valid mode | Any failed/cancelled/skipped/missing common result | Any | Fail |
| Valid full | Common success | Any failed/cancelled/skipped/missing/unexpected heavy state | Fail |
| Valid lightweight | Common success | Any heavy state other than exactly skipped, including unexpected success | Fail |

Change head/base/main/label/creator identity between selection and final re-read and require rejection/rerun as designed. For full PRs, verify current tested B/H; for all pushes, prove explicit full plus all heavy successes. No conversion of final identity failure into successful fallback. Keep stable `required`, normal read-only events/permissions, no PR artifact retention and no privileged PR trigger. The final API read is not a merge transaction; strict main freshness through the owner-provided non-bypass actor remains separate.

### Exact existing-case nextest baseline filter (new cases pending)

The existing nextest `help filterset` command completed **exit0** in the project workspace and confirms `test(=name)` equality and `|` union. This is help only, not compilation/inventory/test execution. A concrete **unexecuted**, focused Unix baseline invocation using already inspected real test identifiers is:

```text
cargo nextest run --locked -p xtask --bin xtask --cargo-profile native-tests --config-file /absolute/evidence/cod103-focused-nextest.toml --user-config-file none --profile cod103 --no-tests fail -E 'test(=tests::workflow_policy_rejects_unconditional_release_publication) | test(=release::workflow_tests::verified_release_survives_main_advancing_and_ignores_other_pending_releases) | test(=release::workflow_tests::verified_release_rejects_untrusted_prs_and_removed_history) | test(=release::workflow_tests::publisher_targets_original_sha_and_is_idempotent)'
```

This baseline covers relevant existing workflow/ancestry/source-identity regressions, **not** the new semantic classifier, selector, aggregate table, legacy uploader or MCP validator. Planner appends the actual new named cases after their implementation/attachment and pins the expected inventory; no generic reporter framework or native-pipeline replacement. A name change requires updating the evidence/filter/attachment honestly, not accepting an empty selection.

A09 remains unchanged: real checksum-valid native executable fixtures, actual exact installer, both DryRun/real cases, destination preservation, actual Windows11 desktop and PowerShell5.1 plus7. The bounded actual-result reporting options above still apply. Parent's last observed publication was queued/pending; no publication refresh or new release claim occurred here.


### Final sequence and evidence status alignment

Final binding Design identity is **`5fa6cbd4fc226de259a83444b87e1fb117582aaa67193670276ed9c45688cf12`**, confirmed by rereading the actual file bytes at this correction. [Architecture proposal](architecture.md) records genuine draft change **`chg-9xdkd4`**, claims **`resp-1rnbs9`** / **`resp-tddbkj`**, created **before** combined approval and `sign_off`; these are neither placeholder checker IDs nor passing verdicts. Approval records must identify those exact draft claims/immutable proposal. Then user sign-off; dependent implementation only afterward; genuine applicable anchor/test attachment and actual focused execution/JUnit/report ingestion; current passing verdict before authorized fold. Proportional restorations retain existing claims without manufactured additions. This verifier does no model writes.

Final Design clarifies that eligibility API/diff uncertainty may yield a valid full selector result, but final current B/H/API uncertainty or mismatch fails `required` in **both** modes. The state table above already enforces that distinction. Its source inspection notes current workspace lock dependency edges are unqualified; **actual** PR165/167 generated-layout positive fixture execution remains pending. Do not turn that source observation, proposed checker identifier, attachment, or plan table into a passing decision or hosted rollout result.

## Canonical rendering provenance

Production preserved the Evidence Verifier source artifact SHA-256 `685084ede2d86ab8eb3003f826b0c5f043628585018d9da14e6f8ca76f81c505`. Only canonical document/workspace references were normalized for this Git copy; the verification plan, evidence limits and author conclusions are unchanged. Input artifact hashes identify the author inputs, not this rendered file or an approval. Combined Git snapshot and user approval remain pending.
