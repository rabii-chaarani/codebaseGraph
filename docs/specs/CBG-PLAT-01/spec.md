# CBG-PLAT-01 — PR #153 platform maintenance

## 1. Identity, status and authority

- **Revision:** 0.6, proposed implementation scope with **D01–D04 policies approved** (including D03a/D03b). **Work ID:** `CBG-PLAT-01`.
- **Canonical Git path:** `docs/specs/CBG-PLAT-01/spec.md`. Supporting review evidence: [review-evidence.md](review-evidence.md) and [evidence/](evidence/).
- **Delivery:** CodebaseGraph Linear workspace (`https://linear.app/codebasegraph`), existing **Maintenance** project `P-COD-2` (`https://linear.app/codebasegraph/project/maintenance-33932479ebd0`), team `COD`. This user-selected destination supersedes older project-routing advice. No new project is created.
- **Baseline:** [PR #153](https://github.com/rabii-chaarani/codebaseGraph/pull/153), reviewed HEAD `862a8f88d485d5bd257ebd9456c194156fd4ee94`, version3.0.4. This evidence is not the prior checkout6aa5ba8 review. Revalidate changed source before implementation.
- **Approval:** the initial user request authorized planning/specification publication and issue creation. The user later selected **D01 option A: bundle required Windows prebuilt runtime libraries**, with licenses, checksums and controlled DLL loading, and **D02 option A: bundle OpenSSL dylibs for macOS prebuilt releases**, with relative runtime paths, licenses and checksums; no Homebrew prerequisite. See [D01](decisions/d01-windows-runtime-bundling.md) and [D02](decisions/d02-macos-openssl-bundling.md). The user also selected **D03a option A: explicit config path → client-specific override → native Windows user-profile directory**, ignoring HOME for the Windows default and failing clearly rather than falling back to CWD; see [D03a](decisions/d03a-windows-user-directory.md). The user then selected **D03b option B: native .exe clients plus .cmd/.bat shims through a dedicated Windows invocation adapter**, with tested argument handling, no generic shell-command evaluation and clear rejection of unsupported/unsafe inputs; see [D03b](decisions/d03b-windows-client-command-invocation.md). The explicit correction “no i meant B” supersedes the preceding A. The user also selected **D04 option A: retain and accurately report the Windows Direct-read Exclusive lock**, without a new unlock/relock transition or concurrent-reader guarantee; see [D04](decisions/d04-windows-direct-exclusive-mode.md). These approve the selected policies only, not the entire specification or implementation start. D05, implementation authorization, merge, deployment and service restarts remain unapproved. S00 records applicable exact-scope approvals without reopening settled decisions.
- **Closure owner:** repository maintainer/user; no automatic aggregate closure delegated. Assignees/dates/estimates remain unset. Draft requirements below are proposed, not approved acceptance changes.
- **State separation:** proposed → exact-scope approved → implemented → native-platform qualified → maintainer closed. Linear Backlog, attached/configured tests, commits and percentages are not evidence of later states.

## 2. Problem and observable outcome

Fresh inspection of the actual PR code found one newly introduced macOS traversal completeness defect, existing runtime packaging/discovery/installer gaps, three current native CI root causes, and a lower-priority preexisting Windows lock-mode invariant. The new guardian/Job Object and GuardedDatabase abstractions are useful and must not be replaced because an older review described code that is no longer present.

Outcome: correct the selected confirmed gaps under explicit policies, preserve current ownership/containment/cleanup safety, and qualify one exact corrected revision plus installed consumer archives on the two declared native targets. Each independently deliverable slice below has requirements, acceptance and evidence limits. No claim is made that source findings are all introduced by PR153.

## 3. Existing contracts, architectural context and scope

- Reuse the canonical [disk-cleanup contract at reviewed SHA](https://github.com/rabii-chaarani/codebaseGraph/blob/862a8f88d485d5bd257ebd9456c194156fd4ee94/knowledge/architecture/graph-disk-cleanup-contract.md), especially DC-S3-R3/DC-S4-R4 and its origin/ownership/retention invariants. It remains a wiki-authored contract at its original identity; this spec does not copy/migrate/reapprove it.
- Reuse [CBG-COPY-01](../CBG-COPY-01/spec.md), its existing approval records, and the [resource-ceiling amendment](../CBG-COPY-01/resource-ceilings-amendment.md)/[approval](../CBG-COPY-01/resource-ceilings-approval.md). Preserve no replay of uncertain COPY mutation and retained cancellation/unknown-containment behavior. Workload ceiling removal is intentional.
- Reuse the [release contract/process at reviewed SHA](https://github.com/rabii-chaarani/codebaseGraph/blob/862a8f88d485d5bd257ebd9456c194156fd4ee94/docs/release.md), current typed native target/test contracts and source evidence. Any stale four-target architecture prose is not proof of a current requirement.
- Scryer read-only orientation identified existing Graph Store `node-88`/`resp-108,109,113,232,233`; Repository Lifecycle Service `node-136`/`resp-25,30,31`; Release Gate `node-1`/`resp-16,17,254–258`; CLI Adapter `node-9`. These identifiers supply intended context, **not SHA-bound implementation proof**: the configured server is bound to the main local repository, not this detached PR checkout. No model write, sign-off, source-map repair or “implemented” transition is claimed. Revalidate applicable directives/claims and open a proportional change ledger in implementation; fixes restoring claims/docs/tests need no invented topology.
- In scope: macOS ARM64 and Windows x86_64 only; independent proposed maintenance slices S01–S09 and qualification S10. Preserve raw Direct journal compatibility separately from physical resource identity. No Ladybug/extension ABI or dependency-version upgrade is implied; any such change needs separate impact review and approval.
- Excluded: Linux/Intel/Windows ARM64 expansion, new platform trait framework, process containment redesign, policy-free DLL search fallback, raw journal migration, general SQL parsing, workload ceiling restoration, global PATH mutation, two-file installer rollback redesign, code fixes/CI reruns/deployment in this planning task.

## 4. Proposed requirements and delivery slices

All issue titles are prefixed `CBG-PLAT-01/Sxx`. S00 is the exact-scope contract/approval prerequisite, not a coding task. S06–S08 are qualification fixes, not assertions of production COPY/exclusion defects. S09 is explicitly preexisting P3 debt. Priority names are Linear Medium for reviewed P2 work and Low for P3 debt; they do not imply an urgent incident.

### S00 — Publish the proposed platform contracts and record exact-scope approval

**Outcome:** Publish/pin the proposed specification and review evidence; resolve applicable decisions and obtain explicit exact-scope approval for selected dependent slices, while recording deferral of unrelated decisions.

**Problem/evidence:** Planning and D01/D02 distribution, D03a/D03b Windows discovery/invocation, and D04 truthful Windows Exclusive lock policies are approved. D05 remains unresolved. No current message authorizes implementation, merge, deployment or service restart.

**Requirements / criteria:** R01, R02, R03, R04, R05, R06, R07, R08, R09, R10 → A00.

- Preserve the reviewed PR153 source SHA and existing CBG-COPY-01 and disk-cleanup approval histories.
- Resolve/approve only open decisions applicable to the selected slices, record explicit deferrals for the rest, and never treat recommendations as approval.
- Record canonical path, full committed specification SHA, exact approved scope and user decision reference in a separate approval record.
- Reconcile applicable Scryer intent proportionately before coding; fixes/tests restoring claims need no invented nodes.

**Acceptance (proposed):**

<a id="a00"></a>
- **A00** — Given the reviewed proposal and evidence, a maintainer can fetch the exact Git revision and all relative evidence links. Given no matching explicit approval, dependent implementation issues remain blocked; Backlog, a docs commit/PR, review, or issue creation is not approval. Given a material contract revision, retain the earlier revision and record a new approved SHA/scope before dependent work.

**Architectural context:** Planning/contract-only: no new architecture claim required; consult existing Graph Store, Repository Lifecycle Service and Release Gate intent before implementation.

**Planned verification:** Remote immutable permalink availability, Markdown evidence links/hashes, explicit approval scope and stable IDs; no product test implied.

**Source orientation at the reviewed SHA:**
- [docs/specs/CBG-COPY-01/resource-ceilings-approval.md:1–10](https://github.com/rabii-chaarani/codebaseGraph/blob/862a8f88d485d5bd257ebd9456c194156fd4ee94/docs/specs/CBG-COPY-01/resource-ceilings-approval.md#L1-L10)
- [README.md:45–65](https://github.com/rabii-chaarani/codebaseGraph/blob/862a8f88d485d5bd257ebd9456c194156fd4ee94/README.md#L45-L65)

**Dependencies:** none; planning/approval only. **Open decisions:** D05. **Resolved:** D01, D02, D03a, D03b and D04 policies approved; no implementation start authorized. No task depends on a guessed implementation date.

### S01 — Isolate macOS directory enumeration cursors for nested cleanup scans

**Outcome:** Nested rooted scans discover all directory entries without consuming an outer traversal cursor.

**Problem/evidence:** New PR153 macOS visit_entries resets/consumes the RootedDirectory FD. A current-source native 200-entry probe sees baseline200 and every inner200, but outer85. Terminal cleanup scans the same workspace-origin registry recursively.

**Requirements / criteria:** R01 → A01, A02.

- Keep the pinned rooted directory as namespace authority; use a separate open file description for each macOS traversal.
- Do not use dup/try_clone as cursor isolation, because duplicated descriptors share offsets.
- Retain fixed scratch, cancellation, no-follow/identity checks, bounded maintenance passes and conservative unknown-entry retention.

**Acceptance (proposed):**

<a id="a01"></a>
- **A01** — Given more entries than one native batch, nested and nonnested scans return the same complete entry set exactly once; do not assert the incidental85-entry batch size.
<a id="a02"></a>
- **A02** — Given terminal workspace receipts spanning multiple batches, selection can observe later eligible receipts while child receipts continue to protect parent directories. Given cancellation or namespace replacement, traversal remains bounded and does not acquire deletion authority from names alone; Windows nested-scan behavior remains green.

**Architectural context:** Graph Store node-88: resp-232 (recover abandoned workspaces), resp-233 (retire superseded generations). Reuse canonical disk contract DC-S3-R3 and DC-S4-R4; no new topology is proposed.

**Planned verification:** Execute a multi-batch native macOS regression using the actual RootedDirectory helper and a real receipt-selection case; rerun native Windows nested traversal tests and affected storage radius.

**Source orientation at the reviewed SHA:**
- [src/storage/locks.rs:2552–2559](https://github.com/rabii-chaarani/codebaseGraph/blob/862a8f88d485d5bd257ebd9456c194156fd4ee94/src/storage/locks.rs#L2552-L2559)
- [src/storage/maintenance_candidate.rs:2650–2696](https://github.com/rabii-chaarani/codebaseGraph/blob/862a8f88d485d5bd257ebd9456c194156fd4ee94/src/storage/maintenance_candidate.rs#L2650-L2696)
- [src/storage/maintenance_candidate.rs:2717–2793](https://github.com/rabii-chaarani/codebaseGraph/blob/862a8f88d485d5bd257ebd9456c194156fd4ee94/src/storage/maintenance_candidate.rs#L2717-L2793)

**Dependencies:** S00. **Open decisions:** none beyond exact-scope approval/revalidation. No task depends on a guessed implementation date.

### S02 — Define and satisfy Windows archive native-library dependencies

**Outcome:** The installed Windows archive can load required JSON/FTS native components under an explicit approved runtime-dependency contract.

**Problem/evidence:** Exact checked-in Windows JSON/FTS extension PE imports require OpenSSL3 and VC runtime DLLs. Current archive allowlist/installer contains only two EXEs, checksums and installers; static EXE linkage does not satisfy extension DLL imports. No clean-Windows load failure was locally executed.

**Requirements / criteria:** R02 → A03, A04.

- Implement the approved D01 policy for Windows prebuilt releases: bundle compatible redistributable non-system runtime libraries required by the shipped binaries and JSON/FTS extensions, including OpenSSL/VC runtime dependencies as applicable. Users must not install those runtimes separately. Include licenses, checksums and application-controlled DLL loading; do not substitute external prerequisites without a new user decision.
- Keep NativeTarget/archive provenance as the single packaging authority; include all intentionally shipped sidecars in inventory and integrity verification.
- Keep DLL resolution application-controlled; do not widen global PATH or load arbitrary caller-writable libraries as a fallback.

**Acceptance (proposed):**

<a id="a03"></a>
- **A03** — Given an approved clean Windows baseline without developer OpenSSL/VC PATH entries or separately installed required OpenSSL/VC runtimes, the exact installed prebuilt archive supplies its required compatible redistributable non-system native libraries and operates without manual runtime installation before reporting ready. The bundling policy is approved by D01; the exact consumer baseline remains D05.
<a id="a04"></a>
- **A04** — Given real setup and queries, LOAD json, LOAD fts, graph build/reopen/search and k-wiki/MCP smoke succeed through installed binaries with bundled runtimes. Given a missing/incompatible bundled sidecar, fail clearly without replacing a working installation or reporting qualification; include imports, archive hashes, licenses and application-controlled loader policy in evidence. A packaging/licensing incompatibility blocks release; it does not authorize switching to manual prerequisite installation.

**Architectural context:** Graph Store resp-113; Release Gate resp-255, resp-256, resp-257, resp-258 and resp-16/resp-17. D01 Windows prebuilt bundling policy is approved; implementation authorization, D05 baseline/source-install policy and applicable architectural reconciliation remain pending before coding.

**Planned verification:** Inspect PE imports of exact final archive components and execute clean-consumer native Windows installation/JSON/FTS/reopen/CLI/MCP cases, including missing-dependency failure.

**Source orientation at the reviewed SHA:**
- [src/db_writer/extensions.rs:98–110](https://github.com/rabii-chaarani/codebaseGraph/blob/862a8f88d485d5bd257ebd9456c194156fd4ee94/src/db_writer/extensions.rs#L98-L110)
- [crates/xtask/src/main.rs:421–441](https://github.com/rabii-chaarani/codebaseGraph/blob/862a8f88d485d5bd257ebd9456c194156fd4ee94/crates/xtask/src/main.rs#L421-L441)
- [crates/xtask/src/main.rs:596–607](https://github.com/rabii-chaarani/codebaseGraph/blob/862a8f88d485d5bd257ebd9456c194156fd4ee94/crates/xtask/src/main.rs#L596-L607)
- [release/install/install.ps1:76–89](https://github.com/rabii-chaarani/codebaseGraph/blob/862a8f88d485d5bd257ebd9456c194156fd4ee94/release/install/install.ps1#L76-L89)

**Dependencies:** S00. **Open decisions:** D05 and implementation authorization. **Resolved:** D01 Windows prebuilt runtime bundling. No task depends on a guessed implementation date.

### S03 — Remove undeclared Homebrew-path assumptions from macOS distribution

**Outcome:** macOS ARM64 binaries launch and operate on the approved consumer environment without an undeclared producer-specific Homebrew path.

**Problem/evidence:** Current build.rs requests dynamic OpenSSL and package assembly has no relocation/dylib policy. Fresh exact-head debug otool imports /opt/homebrew/opt/openssl@3 paths. This is not a release artifact or clean-host launch reproduction.

**Requirements / criteria:** R03 → A05, A06.

- Follow approved D02 option A for macOS ARM64 prebuilt releases: bundle compatible redistributable OpenSSL dylibs with relative install names/runtime paths, licenses and checksums. Users must not install Homebrew or OpenSSL separately. Do not switch to static linking or external prerequisites without a new user decision.
- Retain exact-artifact checksums/provenance and inspect final release imports, not only a debug build.
- Document the approved ARM64/minimum-OS dependency policy; do not restore intentionally removed Intel/Linux targets.

**Acceptance (proposed):**

<a id="a05"></a>
- **A05** — Given the approved clean consumer baseline without Homebrew or a separately installed OpenSSL, the final installed ARM64 archive --help, graph/JSON/FTS build and reopen, k-wiki and MCP operate using the bundled compatible OpenSSL dylibs and relative runtime paths. No producer-specific absolute Homebrew path may be required.
<a id="a06"></a>
- **A06** — Final release archive otool dependency/install-name/rpath evidence confirms relative resolution of the bundled OpenSSL dylibs; licenses/checksums cover them. Given a missing, corrupt or incompatible bundled library, fail clearly without reporting readiness or qualification. A redistribution/compatibility blocker must be reported, not converted silently into a Homebrew prerequisite.

**Architectural context:** Release Gate resp-255–resp-258, resp-16/resp-17; Graph Store resp-113. D02 dylib-bundling policy is approved; implementation authorization, D05 consumer/minimum-OS baseline and applicable architectural reconciliation remain pending. Do not change process containment.

**Planned verification:** Build the actual ARM64 release archive, inspect Mach-O imports/install names/rpaths, then execute the installed archive on a clean approved macOS consumer baseline without Homebrew/OpenSSL and with missing/corrupt bundled-library negative cases.

**Source orientation at the reviewed SHA:**
- [build.rs:10–26](https://github.com/rabii-chaarani/codebaseGraph/blob/862a8f88d485d5bd257ebd9456c194156fd4ee94/build.rs#L10-L26)
- [crates/xtask/src/main.rs:421–441](https://github.com/rabii-chaarani/codebaseGraph/blob/862a8f88d485d5bd257ebd9456c194156fd4ee94/crates/xtask/src/main.rs#L421-L441)
- [crates/xtask/src/main.rs:596–607](https://github.com/rabii-chaarani/codebaseGraph/blob/862a8f88d485d5bd257ebd9456c194156fd4ee94/crates/xtask/src/main.rs#L596-L607)
- [README.md:45–65](https://github.com/rabii-chaarani/codebaseGraph/blob/862a8f88d485d5bd257ebd9456c194156fd4ee94/README.md#L45-L65)

**Dependencies:** S00. **Open decisions:** D05 and implementation authorization. **Resolved:** D02 macOS prebuilt OpenSSL dylib bundling. No task depends on a guessed implementation date.

### S04 — Use explicit Windows user-directory and executable discovery policy

**Outcome:** User-scoped MCP registration targets the real user directory and native client discovery recognizes supported Windows executables.

**Problem/evidence:** Preexisting exact-head lifecycle.rs uses HOME only then dot and exact PATH filenames. Fresh CLI dry-run with HOME absent, USERPROFILE present and only codex.exe reports scope=user, path=./.codex/config.toml, native installer unavailable. Mac branch reproduction, not Windows runtime.

**Requirements / criteria:** R04 → A07, A08.

- Follow approved D03a Windows user-scope precedence: explicit config-path option, then client-specific override such as CODEX_HOME, then the native Windows user-profile directory. Ignore Unix-style HOME for the default Windows location; if no valid user directory can be found, fail clearly rather than fall back to CWD/repository. Preserve project scope and explicit config paths.
- Follow approved D03b option B: discover/invoke native .exe clients and supported .cmd/.bat client shims through a dedicated Windows invocation adapter. Keep native and batch argument handling distinct and tested; no generic shell-command evaluation or blind shell fallback. Unsupported or unsafe inputs fail clearly rather than executing ambiguously.
- Consolidate shared discovery semantics across registration/verification; do not change Ladybug cache home unless its native contract agrees.

**Acceptance (proposed):**

<a id="a07"></a>
- **A07** — Given real Windows user-scope install/dry-run, explicit config-path options take precedence over client-specific overrides, which take precedence over native Windows user-profile resolution. With HOME absent or pointing elsewhere, default resolution uses the native profile, never HOME/CWD/repository by fallback. When no valid directory can be found, fail clearly. Preserve project-scope and explicit-path behavior; verify with conflicting HOME/profile/override inputs.
<a id="a08"></a>
- **A08** — Given a native .exe client or .cmd/.bat client shim on Windows PATH, discovery and actual invocation/verification agree. A dedicated Windows adapter preserves spaced paths, arguments and explicit overrides for both command kinds, without generic shell-command evaluation. Test controlled shim argument capture and metacharacter/quoting boundaries; unsupported or unsafe inputs fail clearly rather than execute ambiguously. Missing home/client must not corrupt unrelated config or silently select another user/project target.

**Architectural context:** Repository Lifecycle Service node-136: resp-25/resp-30/resp-31. D03a directory and D03b native-plus-batch invocation policies are approved; implementation authorization and applicable reconciliation remain pending. Use a narrow Windows adapter, not a general shell framework; do not change macOS or native Ladybug cache-home contracts incidentally.

**Planned verification:** Native Windows process-level tests with isolated HOME/USERPROFILE/PATH/CODEX_HOME; controlled .exe, .cmd and .bat clients capturing exact arguments; spaced paths, quoting/metacharacter boundaries, rejected unsafe inputs, absent discovery and unchanged macOS/project/explicit-path cases. Check discovery, registration and verification use the same command-kind policy.

**Source orientation at the reviewed SHA:**
- [src/api/lifecycle.rs:3312–3347](https://github.com/rabii-chaarani/codebaseGraph/blob/862a8f88d485d5bd257ebd9456c194156fd4ee94/src/api/lifecycle.rs#L3312-L3347)
- [src/api/lifecycle.rs:5060–5073](https://github.com/rabii-chaarani/codebaseGraph/blob/862a8f88d485d5bd257ebd9456c194156fd4ee94/src/api/lifecycle.rs#L5060-L5073)
- [src/adapters/cli/install/verify.rs:361–369](https://github.com/rabii-chaarani/codebaseGraph/blob/862a8f88d485d5bd257ebd9456c194156fd4ee94/src/adapters/cli/install/verify.rs#L361-L369)
- [src/adapters/mcp/daemon.rs:1838–1843](https://github.com/rabii-chaarani/codebaseGraph/blob/862a8f88d485d5bd257ebd9456c194156fd4ee94/src/adapters/mcp/daemon.rs#L1838-L1843)
- [.cargo/config.toml:1–3](https://github.com/rabii-chaarani/codebaseGraph/blob/862a8f88d485d5bd257ebd9456c194156fd4ee94/.cargo/config.toml#L1-L3)

**Dependencies:** S00. **Open decisions:** implementation authorization. **Resolved:** D03a Windows user-directory precedence and D03b option B native-plus-batch client invocation. No task depends on a guessed implementation date.

### S05 — Reject failing native probes in the PowerShell installer

**Outcome:** A checksum-valid executable that exits nonzero cannot pass installer validation or replace a working installation.

**Problem/evidence:** Current Assert-Binary pipes native output to Out-Null without checking LASTEXITCODE. ErrorActionPreference=Stop alone is not a native-exit policy. Static PowerShell semantics finding; no local pwsh execution.

**Requirements / criteria:** R05 → A09.

- Check each native exit immediately and throw a clear failure for nonzero.
- Keep validation of both binaries before any destination replacement; preserve checksum checks and supported-host rejection.
- Do not conflate this fix with an unapproved two-binary rollback/atomic-upgrade redesign.

**Acceptance (proposed):**

<a id="a09"></a>
- **A09** — Given a valid checksum but a --help/--version probe returning nonzero, both DryRun and real install fail and existing destination binaries stay byte-identical. Given successful probes for both binaries, intended installation succeeds; missing/corrupt files still fail before replacement. Execute on the approved Windows PowerShell baseline and any separately claimed PowerShell version; record real exit and destination evidence.

**Architectural context:** Release Gate resp-16, resp-256; existing installer validation behavior restoration, no invented model claim required.

**Planned verification:** Native Windows installer tests using checksum-valid controlled executables with success/nonzero exits, DryRun, real destination-preservation and checksum-failure paths.

**Source orientation at the reviewed SHA:**
- [release/install/install.ps1:27–47](https://github.com/rabii-chaarani/codebaseGraph/blob/862a8f88d485d5bd257ebd9456c194156fd4ee94/release/install/install.ps1#L27-L47)
- [release/install/install.ps1:76–89](https://github.com/rabii-chaarani/codebaseGraph/blob/862a8f88d485d5bd257ebd9456c194156fd4ee94/release/install/install.ps1#L76-L89)

**Dependencies:** S00. **Open decisions:** D05. No task depends on a guessed implementation date.

### S06 — Restore watch help documentation of the parallel default

**Outcome:** Watch help states the existing default and passes the current contract test on both native platforms.

**Problem/evidence:** Current watch_help description omits ; default behavior required by watch_help_documents_parallel_default_and_opt_out. Reproduced fresh on Mac; current CI fails Mac unit and Windows unit shard4. No runtime parser failure implied.

**Requirements / criteria:** R06 → A10.

- Make watch help accurately state the existing parallel default and single-thread opt-out, consistently with build help.
- Keep retired workload-resource options removed and retain the existing help/CLI behavioral checks.

**Acceptance (proposed):**

<a id="a10"></a>
- **A10** — Given watch --help, the documented default/opt-out agrees with actual parser defaults and the existing failing test passes. Build/watch help and argument behavior remain consistent and retired options remain unsupported; do not bypass required checks or delete meaningful assertions.

**Architectural context:** CLI Adapter node-9; documentation/test correction restoring existing behavior, no new architecture entry required.

**Planned verification:** Run the exact help test plus adjacent CLI argument/default tests on Mac and Windows; attach head-bound passing cohort evidence.

**Source orientation at the reviewed SHA:**
- [src/adapters/cli/format/help.rs:21–22](https://github.com/rabii-chaarani/codebaseGraph/blob/862a8f88d485d5bd257ebd9456c194156fd4ee94/src/adapters/cli/format/help.rs#L21-L22)
- [src/adapters/cli/tests/dispatch_materialize.rs:67–79](https://github.com/rabii-chaarani/codebaseGraph/blob/862a8f88d485d5bd257ebd9456c194156fd4ee94/src/adapters/cli/tests/dispatch_materialize.rs#L67-L79)

**Dependencies:** S00. **Open decisions:** none beyond exact-scope approval/revalidation. No task depends on a guessed implementation date.

### S07 — Render valid Windows COPY paths in the trace-metadata fixture

**Outcome:** The COPY metadata regression tests actual generated SQL paths consistently on Windows and macOS.

**Problem/evidence:** The current test interpolates source.display() raw Windows backslashes; the debug parser treats them as SQL escapes and returns None. Hosted Windows unit shard1 fails; fresh Mac passes. Production COPY generator already normalizes paths.

**Requirements / criteria:** R07 → A11.

- Build the fixture statement through the existing production COPY path rendering policy, including quoting/escaping/prefix treatment.
- Retain the exact four-byte metadata oracle and bounded debug instrumentation. Do not add production loading fallbacks or alter native COPY behavior to satisfy invalid test SQL.

**Acceptance (proposed):**

<a id="a11"></a>
- **A11** — Given a Windows temp path with backslashes/spaces, the fixture statement uses the production renderer and metadata returns the exact intended file and four bytes. Native Windows and Mac trace-metadata tests pass while existing COPY failure/acknowledgement/diagnostic and path-rendering cases remain valid.

**Architectural context:** Graph Store / Graph Writer; test-only correction under existing CBG-COPY-01 contracts. No new claim or mutation-retry behavior.

**Planned verification:** Native Windows exact case and adjacent COPY renderer/instrumentation tests; verify real generated statement rather than only a magic string.

**Source orientation at the reviewed SHA:**
- [src/db_writer/phase.rs:372–399](https://github.com/rabii-chaarani/codebaseGraph/blob/862a8f88d485d5bd257ebd9456c194156fd4ee94/src/db_writer/phase.rs#L372-L399)
- [src/db_writer/phase.rs:1171–1186](https://github.com/rabii-chaarani/codebaseGraph/blob/862a8f88d485d5bd257ebd9456c194156fd4ee94/src/db_writer/phase.rs#L1171-L1186)
- [src/staging_writer/files.rs:47–54](https://github.com/rabii-chaarani/codebaseGraph/blob/862a8f88d485d5bd257ebd9456c194156fd4ee94/src/staging_writer/files.rs#L47-L54)

**Dependencies:** S00. **Open decisions:** none beyond exact-scope approval/revalidation. No task depends on a guessed implementation date.

### S08 — Recognize Windows fs2 contention in the reference-epoch observer

**Outcome:** The epoch fixture waits through genuine Windows lock contention within its existing deadline and still verifies real publication birth/epoch state.

**Problem/evidence:** Fixture retries only ErrorKind::WouldBlock; fs2 Windows reports OS33. Current Windows integration shard2 fails observing follow-up epoch. Production lock classifier already handles raw fs2 contention; no production exclusion defect established.

**Requirements / criteria:** R08 → A12.

- Use the existing fs2-aware contention semantics for the fixture: kind or lock_contended_error raw OS code.
- Preserve the five-second deadline and all actual epoch/birth/integrity assertions; propagate noncontention errors.
- If corrected retry still times out, investigate the real lock hold separately rather than widening deadline or weakening ownership.

**Acceptance (proposed):**

<a id="a12"></a>
- **A12** — Given an owned Windows contention fixture released before the deadline, observation retries and then reads/validates the real epoch. Given permanent contention, noncontention I/O failure or invalid epoch data, observation fails within the unchanged contract and preserves evidence. The actual first-creation/follow-up MCP publication test passes with the same birth and advanced epoch assertions.

**Architectural context:** Graph Store node-88 resp-109 and existing cleanup/publication contracts. Fixture correction only; do not loosen product locks.

**Planned verification:** Native Windows bounded contention/release/error cases and isolated_worker_first_creation_and_followup_keep_same_covered_publication_birth on exact source/binary provenance.

**Source orientation at the reviewed SHA:**
- [tests/support/storage_maintenance/process.rs:730–745](https://github.com/rabii-chaarani/codebaseGraph/blob/862a8f88d485d5bd257ebd9456c194156fd4ee94/tests/support/storage_maintenance/process.rs#L730-L745)
- [tests/storage_maintenance.rs:249–274](https://github.com/rabii-chaarani/codebaseGraph/blob/862a8f88d485d5bd257ebd9456c194156fd4ee94/tests/storage_maintenance.rs#L249-L274)
- [src/storage/locks.rs:340–344](https://github.com/rabii-chaarani/codebaseGraph/blob/862a8f88d485d5bd257ebd9456c194156fd4ee94/src/storage/locks.rs#L340-L344)

**Dependencies:** S00. **Open decisions:** none beyond exact-scope approval/revalidation. No task depends on a guessed implementation date.

### S09 — Make Windows Direct lease mode truthful after relock_shared

**Outcome:** Reported lease mode and held OS lock semantics agree under an explicitly chosen Direct-read contract.

**Problem/evidence:** Preexisting relock_shared layers Windows fs2 Shared over Exclusive then reports Shared. Current begin_read already acquires Exclusive on both platforms, so this is P3 mode/excess-exclusion debt, not a newly proved reader-concurrency regression.

**Requirements / criteria:** R09 → A13.

- Follow approved D04 option A for Windows Direct reads: retain the existing Exclusive lock and report/verify Exclusive truthfully. Do not add a Shared layer, an unlock/relock transition or a new concurrent-reader guarantee; preserve publication/recovery exclusion.
- Do not simply unlock before lock_shared across protected recovery state; an unvalidated gap can create a race.
- Do not reuse the fresh-creation helper without proving its preconditions apply to committed reads.

**Acceptance (proposed):**

<a id="a13"></a>
- **A13** — Given an admitted Windows Direct read, its held OS lock and mode/verifies agree on Exclusive, and second-handle Shared/Exclusive attempts contend until release. No unlock/relock gap or redundant Shared lock layer is introduced. Native Windows and Mac Direct read/replay/writer-exclusion regressions pass; preserve current macOS behavior and release semantics. No new concurrent-reader guarantee or permanent-lock-leak claim is implied.

**Architectural context:** Graph Store resp-109. D04 approves truthful retention of Windows Exclusive ownership only. Implementation authorization and applicable reconciliation remain pending; a new concurrent-reader guarantee or macOS lock-policy change is outside this decision.

**Planned verification:** Native Windows second-handle Shared/Exclusive contention, truthful mode/verifies and release tests, plus affected Direct committed-read/recovery/writer-exclusion tests on both platforms. Verify no new unlock/relock gap and unchanged macOS behavior.

**Source orientation at the reviewed SHA:**
- [src/storage/locks.rs:87–96](https://github.com/rabii-chaarani/codebaseGraph/blob/862a8f88d485d5bd257ebd9456c194156fd4ee94/src/storage/locks.rs#L87-L96)
- [src/storage/locks.rs:174–197](https://github.com/rabii-chaarani/codebaseGraph/blob/862a8f88d485d5bd257ebd9456c194156fd4ee94/src/storage/locks.rs#L174-L197)
- [src/storage/direct.rs:206–229](https://github.com/rabii-chaarani/codebaseGraph/blob/862a8f88d485d5bd257ebd9456c194156fd4ee94/src/storage/direct.rs#L206-L229)

**Dependencies:** S00. **Open decisions:** implementation authorization. **Resolved:** D04 option A, retain/report Windows Exclusive. No task depends on a guessed implementation date.

### S10 — Qualify the corrected native revision and installed consumer archives

**Outcome:** One exact corrected revision has honest native test, artifact and clean-consumer qualification before Maintenance scope closure.

**Problem/evidence:** Current head-bound CI has four failed native cohort jobs (three root causes), required failure and both artifact jobs skipped. 145 selected local Mac passes are not full workspace/Windows/release qualification.

**Requirements / criteria:** R10 → A14, A15.

- Preserve serial discovered inventories, bounded Windows sharding, captured CLI hashes, ignored-case accounting and fail-closed artifact gates.
- Run native Mac ARM64 and Windows x64 checks on the selected corrected revision; bind head/synthetic merge, binary hashes, archive hashes and environment.
- Execute real installation/upgrade/failure paths and installed CLI/MCP/JSON/FTS/reopen under approved clean-consumer and prerequisite policies. Reconcile stale four-target release documentation with the two-target scope; this qualification does not duplicate or close the broader DC-S5 cleanup/rollout programme.
- Resolve D05 and document the actual tested OS/PowerShell floors and source-install feature recipe; do not restore removed platforms or workload ceilings.

**Acceptance (proposed):**

<a id="a14"></a>
- **A14** — All required native cohorts and artifact build-and-smoke jobs actually pass for the corrected source; skipped jobs, old runs or configured tests are not accepted as passing evidence.
<a id="a15"></a>
- **A15** — The exact final immutable installed archives pass clean-consumer success and missing-prerequisite/probe-failure cases on both supported platforms under the applicable approved D01/D02/D03/D05 contracts and recorded S09 inclusion/deferral. Evidence distinguishes local implementation, native-platform qualification and aggregate closure, and rechecks retained cancellation/unknown-containment/guarded ownership contracts. A designated maintainer verifies all linked issue acceptance and approves milestone closure; issue completion percentages, commits or merges alone do not close it.

**Architectural context:** Release Gate resp-254–resp-258, resp-16/resp-17, plus Graph Store ownership/containment contracts; proportional reconciliation of changed claims before closure.

**Planned verification:** Full native CI at one corrected revision plus exact-archive clean-consumer install/upgrade/CLI/MCP/JSON/FTS checks; maintain JUnit/inventory/provenance where required and record unexecuted/ignored cases.

**Source orientation at the reviewed SHA:**
- [.github/workflows/native.yml:29–103](https://github.com/rabii-chaarani/codebaseGraph/blob/862a8f88d485d5bd257ebd9456c194156fd4ee94/.github/workflows/native.yml#L29-L103)
- [crates/xtask/src/native_tests.rs:148–187](https://github.com/rabii-chaarani/codebaseGraph/blob/862a8f88d485d5bd257ebd9456c194156fd4ee94/crates/xtask/src/native_tests.rs#L148-L187)
- [crates/xtask/src/native_tests/shards.rs:28–64](https://github.com/rabii-chaarani/codebaseGraph/blob/862a8f88d485d5bd257ebd9456c194156fd4ee94/crates/xtask/src/native_tests/shards.rs#L28-L64)

**Dependencies:** S01, S02, S03, S04, S05, S06, S07, S08. **Open decisions:** D05. No task depends on a guessed implementation date.

## 5. Failure/recovery and compatibility invariants

- Cursor repair must not broaden deletion authority, erase unknown evidence, bypass cancellation or retain unbounded directory unions. Exact multi-batch completeness is required; incidental85 batch count is not a public constant.
- Missing/incompatible native dependencies and failing installer probes must be explicit failures before readiness/replacement under the approved policy. No silent fallback to global PATH, arbitrary DLLs, downloads or alternate configuration locations.
- Discovery failures must not map user scope into repository/CWD by default. Preserve explicit client overrides, unrelated configuration and project-scoped behavior.
- Reference observer contention stays bounded by its current deadline and preserves actual epoch/birth assertions. Unknown or noncontention failures are not transformed into success. This does not weaken production locks.
- Any Windows lock transition must preserve exclusion/revalidation around changed ownership. Simply unlock/relock is not a general atomic downgrade.
- Do not change native path rejection, raw journal recovery keys, guarded handle lifetimes, Job Object/FIFO proof, no-COPY-replay, bounded diagnostics or maintenance per-pass policy incidentally. Material behavior/acceptance changes require a revised spec and user approval.

## 6. Acceptance trace and evidence status

| Criterion | Requirement/slice | Method | Executed baseline / required future evidence |
|---|---|---|---|
| A00 | S00 / R01–R10 | Audit immutable spec/review links and explicit human approval | This proposal is not approved; planning only. |
| A01–A02 | R01 / S01 | Exact rooted helper multi-batch nested test; production receipt-selection case; cancellation/namespace tests | Mac exact-source helper baseline200/outer85/inner200 fails. No complete collector-starvation/data-loss repro. Fixed native runs required. |
| A03–A04 | R02 / S02 | Final PE imports + installed clean Windows JSON/FTS/CLI/MCP and failure cases | Current asset imports inspected; no clean Windows missing-DLL execution. D01 bundling policy approved; D05/implementation and actual native qualification pending. |
| A05–A06 | R03 / S03 | Final release Mach-O/rpaths + clean approved ARM64 install/reopen/failure | Fresh debug CLI imports inspected only; no final release/clean consumer run. D02 dylib-bundling policy approved; D05/implementation and actual native qualification pending. |
| A07–A08 | R04 / S04 | Process-level native Windows env/PATH/override cases and Mac regression | Fresh CLI portable-logic reproduction points user scope to dot with HOME absent. Not Windows execution. D03a/D03b policies approved; implementation and native qualification pending. |
| A09 | R05 / S05 | Native PowerShell valid-checksum/nonzero executable preflight/destination preservation | Source semantics only; pwsh unavailable locally. D05 pending. |
| A10 | R06 / S06 | Exact watch-help and adjacent behavior tests on both native targets | Fresh Mac failure; hosted Mac+Win failure. Fixed run pending. |
| A11 | R07 / S07 | Production-rendered path fixture + native Windows/Mac metadata/COPY tests | Mac passes; Windows unit shard1 fails raw path fixture. No production loading defect inferred. |
| A12 | R08 / S08 | Windows owned contention/release/timeout/noncontention cases + actual epoch publication case | Windows integration shard2 failed OS33; product classifier already handles raw fs2 errors. Lock duration not established. |
| A13 | R09 / S09 | Native Windows second-handle Shared/Exclusive behavior + Direct read/replay/exclusion | Static fs2 semantics; no Windows local execution. D04 retain/report Windows Exclusive approved; implementation and native qualification pending. |
| A14–A15 | R10 / S10 | All required native cohorts/artifact jobs + exact installed clean-consumer success/failure; maintainer sign-off | Current CI40330652083 failed; both artifacts skipped. 145 selected local Mac passes are not qualification/closure. |

Actual verification record: [review-evidence.md](review-evidence.md). SHA-bound failed CI tested a synthetic merge; only CHANGELOG differs from reviewed implementation. Later passing evidence must identify full source/merge SHA, native OS/toolchain, CLI/archive hashes, exact command/method, result and limits. Mark planned/ignored/unexecuted cases explicitly. Ingest JUnit into Scryer only where existing claims require it, after actual execution.

## 7. Open decisions and start gate

Owner for D01–D05 is the repository maintainer/user. Recommendations are options, not preselected approvals. The approval record may approve independent exact slices without falsely approving other decisions; do not reopen unchanged approved contracts. Do not silently choose runtime/compatibility/acceptance policy while implementing a small fix.

### D01 — Windows native dependency delivery

- Affected: S02, S10; Linear COD-53 and COD-61.
- **Decision: A — bundle required runtimes with Windows prebuilt releases.** Users must not separately install OpenSSL or the required Visual C++ runtime to use the prebuilt archive.
- Include compatible redistributable required non-system libraries, their licenses, checksums and an application-controlled DLL-loading policy. Preserve the existing security constraints: no global PATH widening or arbitrary caller-writable DLL fallback.
- Exact compatible versions, app-local layout and secure loading mechanism are implementation details to validate within this policy, not already approved code. If licensing/ABI/redistribution is infeasible, report a blocker rather than silently switch to option B.
- User decision: **“A”**, answering the immediately preceding explicit D01 A-versus-B question in this conversation. See [decision/approval record](decisions/d01-windows-runtime-bundling.md).
- **Status: policy approved.** Not approval of all R02/A03–A04 implementation, the rest of this specification, merge, deployment or service restart. D05 Cargo source-install recipe and consumer/OS qualification baseline remain open.

### D02 — macOS OpenSSL delivery

- Affected: S03, S10; Linear COD-54 and COD-61.
- **Decision: A — bundle OpenSSL dylibs with macOS ARM64 prebuilt releases.** Users must not need Homebrew or a separate OpenSSL installation.
- Package compatible redistributable libraries with relative install names/runtime paths, licenses and checksums. This retains dynamic linking while removing the producer-specific Homebrew requirement.
- Exact library versions, archive layout, install-name/rpath tooling and release maintenance remain implementation details to validate within this policy. No static-linking redesign or external-prerequisite fallback was selected.
- User decision: **“A”**, answering the immediately preceding D02 A/B/C question (bundled dylibs/static linking/Homebrew prerequisites). See [decision/approval record](decisions/d02-macos-openssl-bundling.md).
- **Status: policy approved.** Not approval of all R03/A05–A06 implementation, the rest of this specification, merge, deployment or service restart. D05 consumer/OS qualification baseline remains open.

### D03 — Windows discovery precedence and command kinds

- Affected: S04/COD-55 and qualification S10/COD-61. D03 is split into independent directory and executable decisions.

#### D03a — User-directory selection (approved)

- **Decision: A — native Windows profile default.** Precedence is explicit config-path option → client-specific override such as CODEX_HOME → native Windows user-profile directory.
- Ignore Unix-style HOME for the default Windows location. If no valid directory can be found, fail clearly; never fall back to the current working directory or repository. Preserve explicit paths and project-scope behavior.
- User decision: **“A”**, answering the immediately preceding D03a A/B question (native Windows profile default versus HOME-first). See [decision/approval record](decisions/d03a-windows-user-directory.md).
- **Status: directory policy approved only.** Native profile-resolution mechanism and validation are implementation details, not selected APIs/dependencies. No incidental change to macOS or Ladybug's native cache-home contract is approved. No implementation start authorized.

#### D03b — Executable suffix and batch-command handling (approved)

- **Decision: B — native .exe clients plus .cmd/.bat client shims.** Support common Windows client installation methods through a dedicated Windows invocation adapter with separate, tested native/batch argument handling.
- Discovery, actual invocation and verification must agree. Preserve spaced executable/script paths and arguments; do not introduce generic shell-command evaluation or blind shell fallback. Unsupported or unsafe inputs fail clearly. Supporting shims does not authorize evaluating arbitrary command strings.
- User decision: initially “A”, immediately corrected to **“no i meant B”**. The correction is the effective choice and supersedes A; no native-only decision was committed. See [decision/approval record](decisions/d03b-windows-client-command-invocation.md).
- **Status: command policy approved only.** Exact adapter implementation, compatibility details and tests require validation during authorized implementation. No new dependency, general shell framework, implementation start, merge, deployment or service restart is approved.

### D04 — Direct read lock-mode contract

- Affected: S09/COD-60; disposition/qualification S10/COD-61.
- **Decision: A — retain the existing Windows Direct-read Exclusive lock and report it accurately.** Do not add an unlock/relock transition, a redundant Shared lock layer or a new concurrent-reader guarantee.
- Preserve publication/recovery exclusion and current macOS behavior. The narrower honest-mode fix does not approve a cross-platform read-concurrency redesign or changes to fresh-creation transition semantics.
- User decision: **“A”**, answering the immediately preceding D04 question offering truthful retained Exclusive versus genuine release/reacquire/revalidated Shared. See [decision/approval record](decisions/d04-windows-direct-exclusive-mode.md).
- **Status: lock policy approved only.** Exact code/visibility/name changes and native tests remain to be verified during authorized implementation. No implementation start, merge, deployment or service restart is authorized. S09 remains preexisting low-priority debt, not a cause of current CI failures.

### D05 — Supported consumer qualification matrix and Cargo-install recipe

- Affected: S02, S03, S05, S10.
- Recommendation/options: Name tested macOS ARM64/Windows x64 OS and PowerShell baselines; honor macOS13.3 native extension floor and explicitly select/document source-install bundled-feature behavior.
- Tradeoff/start constraint: No platform expansion, implicit minimum-OS guarantee, resource-ceiling restoration, blanket timeout increase or weakened test gate.
- Status: unanswered/proposed. No implementation approval recorded.

## 8. Handoff, dependencies and closure

- Publish this proposed revision and evidence in a separate docs-only review branch/PR; do not modify PR153 code or treat its existing approvals as approval of new maintenance policy. Linear issues link the remotely available full spec-commit permalink and applicable requirement/criterion IDs. Approval must later pin the exact approved content.
- Group this bounded outcome with one Maintenance milestone. S00 gates dependent implementation; S01–S08 are independently deliverable after their own contracts/approval, not a sequential implementation chain. S10 depends on the included corrected slices and actual native/consumer evidence. S09 is lower-priority residual debt: explicitly decide whether to include it in the qualified release or formally defer it; do not claim it fixed or silently omit it from milestone closure.
- There is no ready Scryer execution brief or writable-file assignment yet. Before implementation, revalidate exact current source/tools, settle affected decisions, identify existing claims, preserve other work, and use one writer per shared manifest/target/installer boundary. No new architecture entries merely to fill this table.
- Record changed files/anchors, exact approved spec SHA/scope, commits/PRs, executed criteria evidence, native environments, failure attempts, review outcomes, remaining limits and Linear transitions. Follow fresh required review gates for large implementations; planning review is not that implementation review.
- Individual issues close only on their actual acceptance; full native/consumer qualification and maintainer aggregate closure stay separate. No merge/deploy/restart authorization is granted by completion of this planning request.

### Planning authorization record

Initial user request in this conversation, 2026-10-26: create the issues using the spec workflow in **Maintenance**, **CodebaseGraph workspace**. Later separate explicit answers **“A”** selected [D01 Windows runtime bundling](decisions/d01-windows-runtime-bundling.md), [D02 macOS OpenSSL dylib bundling](decisions/d02-macos-openssl-bundling.md), and [D03a Windows native-profile directory precedence](decisions/d03a-windows-user-directory.md). The later D03b answer was explicitly corrected to **“no i meant B”**, selecting [native .exe plus .cmd/.bat client shims](decisions/d03b-windows-client-command-invocation.md) through a dedicated safe Windows adapter. The subsequent **“A”** selects [D04 truthful retained Windows Exclusive mode](decisions/d04-windows-direct-exclusive-mode.md). These policy decisions do not approve implementation of the whole spec; D05 remains open. No product constants or implementation source changed by this planning/decision work.
