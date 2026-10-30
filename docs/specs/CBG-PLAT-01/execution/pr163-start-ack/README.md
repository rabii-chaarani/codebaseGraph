# PR163 START acknowledgment repair — evidence index

**Status: bounded local verification passed and fresh final source review GREEN; head-bound full CI and archive qualification pending.** Reports below keep their original wording and execution-time status, including preparation-time “pending” statements.

## Authority and exact scope

- Work: [COD-54](https://linear.app/codebasegraph/issue/COD-54), `CBG-PLAT-01/S03`, R03 / A05–A06; endpoint existing [PR163](https://github.com/rabii-chaarani/codebaseGraph/pull/163) only. No merge, release, deployment, restart or aggregate closure.
- [Focused approval](../../approvals/pr163-start-ack.md), committed before source implementation at `84f860cbee741dbcbd0b66eb78c6640e924a6fef`. The user approved **“approve the 5-second”** on2026-10-30. The approval records immutable original specification revisions0.10/0.11 and their approvals; it is a narrow internal timing exception, not a replacement specification.
- Scryer execution brief `chg-1k9jre`, diagnosis `chg-mk8x5r`, project `/Users/rabii/Projects/Repositories/codebaseGraph`; no new claims/topology.
- Only exact `START` response-frame read budget changes250ms→existing5s `DEADLINE`. Connect/write250ms, non-START response250ms, inbound frame100ms, authentication/framing, one-send/no-replay, containment and proven-empty cleanup rules stay unchanged.

## Source and CI identities

| Identity | Value |
| --- | --- |
| Repair PR baseline | `b603712e09a3e56651d0b5918af7e13a96d75bed` |
| Original CI actual synthetic merge | `4162dd67e1720f552bc3e2f3424192343d93d41d` |
| Original CI base | `7125900c740297be5c60821dbeb9811f32a5894a` |
| Baseline and original merge identical source tree | `a7b5908da9a9f900a748746c24264e5c54930314` |
| Approval-first implementation baseline | `84f860cbee741dbcbd0b66eb78c6640e924a6fef` |
| Frozen implementation file | `src/process_lifecycle/mod.rs` |
| Frozen implementation SHA256 | `59c66cb5906eeb13e32c4daa0ca08d26fb416b3e86c14a1b004b930959284fc3` |
| Final integrated commit and head CI | Commit/push authorized after final report copies; head-bound full CI pending |
| Reviewed source identity | Exact frozen lifecycle SHA256 above; fresh GPT-6.1 Sol / Max review GREEN, no confirmed P0–P3 |

The frozen-file identity above is not a commit identity or full qualification claim. Source/diff provenance is in [Backend handoff](implementation/backend-handoff.md) and [native test provenance](implementation/native-test-provenance.json).

## Diagnosis and actual pre-fix evidence

- [Diagnosis and final decision options](diagnosis/diagnosis.md), [bounded recommendation](diagnosis/recommendation.md).
- Native Darwin socket-timeout probe: [source](diagnosis/darwin_socket_timeout.rs), [result](diagnosis/darwin_socket_timeout.txt).
- Authenticated delayed acknowledgment probe: [source](diagnosis/authenticated_delayed_ack.rs), [result](diagnosis/authenticated_delayed_ack.txt). These standalone probes explain the IO mechanism; they are not product acceptance tests.
- Original failed [job metadata](original-ci/job-123951179128.json), [run metadata](original-ci/run-41013561222.json), and [exact bounded failure-log tail](original-ci/failure-tail.log). Original source: [run41013561222/job123951179128](https://github.com/rabii-chaarani/codebaseGraph/actions/runs/41013561222/job/123951179128). The tail starts at archive validation; compilation noise is omitted.
- Actual-product [baseline regression failure](implementation/start-ack-baseline-failure.txt), [final focused output](implementation/final-test-logs.txt), [source diff](implementation/source-diff.patch), [source SHA256](implementation/source-sha256.txt). Backend reported delayed-ACK regressions2/2 and lifecycle module8/8; these are Backend execution, not independent final verification.

The original delay remains unestablished. START committed, and response receive timeout is the strongest source/syscall inference; CI did not capture the exact failing syscall. Packaging/sign/install succeeded before runtime smoke failed. No OpenSSL root cause is claimed.

## Independent evidence and review

- [Verification preparation](verification/preparation/verification-preparation.md) and [native environment readiness](verification/preparation/native-environment-readiness.md) are preparation artifacts, not final passing execution. Their original “no builds/tests yet” status describes that earlier phase.
- [Native dependency identities](verification/preparation/native-environment-identity.json), [direct cache checks](verification/preparation/native-cache-direct-checks.txt) and [official release API response](verification/preparation/ladybug-official-release-api.json) preserve dependency provenance. Local development link prerequisites are not consumer runtime qualification.
- [Independent core report](verification/core/core-verification-report.md), [state](verification/core/core-verification-state.json), [genuine runner-written lifecycle JUnit](verification/core/focused-process-lifecycle.junit.xml), [runner log](verification/core/focused-process-lifecycle.log), [configuration](verification/core/nextest-verification.toml), [format output](verification/core/fmt-final.txt), [Clippy output](verification/core/clippy-final.log), [Clippy exit](verification/core/clippy-final.exit) and [candidate diff](verification/core/candidate-source.diff) are copied unchanged at their current partial status. The final accepted report records the completed bounded core checks. Complete native/artifact checks and final executable/archive qualification remain pending head-bound hosted CI after reviewed push, per coordinator priority.
- Independent accepted core results so far: lifecycle13/13 and xtask64/64 passed; formatting and strict workspace/all-target/all-feature Clippy passed with no warnings. [Exact commands](verification/core/core-commands.json), [xtask log](verification/core/xtask-final.log), [xtask genuine JUnit](verification/core/xtask-final.junit.xml), [runner JUnit audit](verification/core/junit-audit-core.json). Four bounded auth/cancellation/owner-loss/recovery integration cases also passed4/4. [Integration JUnit](verification/core/integration-selected.junit.xml), [log](verification/core/integration-selected.log), [CLI executable provenance](verification/core/integration-executable-provenance.json), [final source identity](verification/core/candidate-final-identity.txt) and [environment](verification/core/core-environment.json) bind execution to the frozen source.
- [Fresh independent final review](review/final-review.md), [reviewed source patch](review/reviewed-source.patch), [initial snapshot](review/initial-snapshot.json) and [static hypotheses](review/static-hypotheses.json): **GREEN, no confirmed P0–P3**. Required GPT-6.1 Sol / Max, default agent, fresh independent context/no fork; exact source SHA256 remains `59c66cb5906eeb13e32c4daa0ca08d26fb416b3e86c14a1b004b930959284fc3`. Production did not substitute its own review.
- Head-bound PR CI: **pending**; no passing CI claim follows from local tests or configured workflows.

## Execution-path mapping and retained substance

Copied reports/provenance remain byte-for-byte as supplied. Absolute paths in them identify the original execution context, not durable clickable links. Use these mappings after checkout:

| Original execution prefix | Versioned destination |
| --- | --- |
| `/tmp/codebasegraph-pr163-ci-investigation-20261030/production/` | [`diagnosis/`](diagnosis/) — Markdown, probe `.rs` and `.txt`; compiled probe binaries intentionally excluded |
| `/tmp/codebasegraph-pr163-ci-investigation-20261030/implementation/` | [`implementation/`](implementation/) |
| `/tmp/codebasegraph-pr163-ci-investigation-20261030/verification/` | [`verification/preparation/`](verification/preparation/) for preparation/environment files; [`verification/core/`](verification/core/) for core commands/logs/JUnit/reports |
| `/tmp/codebasegraph-pr163-ci-investigation-20261030/review/` | [`review/`](review/) |
| `/private/tmp/codebasegraph-pr163-ci-repair-20261030/` | Repository checkout root; original source/build execution directory, not a shipped binary |

Reports mention additional discovery/tool context files that are not acceptance evidence and are not copied here. Canonical specs/approvals remain at their existing immutable references; raw chats, memory dumps, unrelated tracking state and build binaries are excluded. File provenance and checksums are in [inventory.json](inventory.json).

## Delivery and remaining owners

All durable documents for this repair, including the already-committed approval, must ship in PR163 with the source. The temporary `docs/pr163-start-ack-approval` remote reference only made the approval available before implementation; it is not a separate final PR endpoint. Production stages/version-controls supplied evidence without rewriting its substance, after coordinator acceptance and inventory review. Coordinator retains tracking/ledger/closure decisions. No reusable memory is warranted while the originating CI delay is unresolved.
