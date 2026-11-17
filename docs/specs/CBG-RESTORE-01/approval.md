# CBG-RESTORE-01 actual implementation approval

## Actual decision and exact subject

Work: CBG-RESTORE-01 / COD-136. Date: 17 November 2026, Australia/Darwin.

The maintainer answered **Approve this exact revision and scoped implementation** to request_user_input_async call_64c7348695384bd08dd527d17f26db20, item 0.

The question explicitly named the full Git commit **d809fe0f8289bbf7d167a891bd7c31160f27f0e1**, [PR #200](https://github.com/rabii-chaarani/codebaseGraph/pull/200), specification revision 0.1 and its ledger, execution brief, authority proposal and file ownership. It stated: “This authorizes isolated product reconstruction, tests and a reviewed PR to dev; C4 stays separate, with no merge, release or live installation/store changes.”

This is actual human input, not inferred from a selected default, the earlier plan, issue status, review or commit. It supersedes only the approval-pending status recorded at the earlier preparation snapshot. Historical preparation and failed-attempt records retain their original meanings.

| Approved path at the full SHA above | Git blob |
| --- | --- |
| docs/specs/CBG-RESTORE-01/spec.md | ad46f77e1f0f7f37db83a332b8c5eea9ddfcea68 |
| docs/specs/CBG-RESTORE-01/preservation-ledger.md | ab030e69fcced000fc67bafca69004ce6723f74f |
| docs/specs/CBG-RESTORE-01/execution.md | b957c33a43b0a131dda806d412b06d95878f4df2 |
| docs/specs/CBG-RESTORE-01/authority-plan.json | 93a745f3e41bbdb71e2a322a3ddb2f6b9d314a54 |
| docs/specs/CBG-RESTORE-01/file-ownership.json | df7aadcb8c75228658bd0ab9494235a42dae6cee |

Approved tree: abc388210784d9fbb5b0e8e779670f6dfe30d2b8. Parent/source baseline: 90e2ab6fae2767bdb158e19f0072d43568e80dbc. Pre-collector reference: ca115690b226384164cf364458a0e3255b8908d3. PR target: dev, frozen baseline 2f1fe10f6fc5f275f61e4a9bc2d38b4b33a7b5a9.

## Scope and retained gates

Approval covers REST-R01–R13 and REST-A01–A12: explicit removal of the added collector and cleanup-only interfaces, reconstruction from the reference with retained unrelated behavior, isolated tests and reviewed PR delivery. Preserve ordinary publication recovery, reader/ownership/containment guarantees, operation-owned cleanup, all named retained contracts, version/history and original working/architectural evidence.

The current isolated branch is codex/restore-v304. No original branch is reset or rewritten. The approved package remains byte-identical at its immutable subject; this later approval/assignment record does not silently advance that subject.

Native C4 keeps its original approvals, authority and separate companion order. COD-25 remains WIP. COD-125/126 and COD-135 work/status/closure owners are not adopted or cancelled.

All verification, source/caller discovery, drift reconciliation, one-owner/file, independent execution and fresh adversarial final-review gates remain. Newly discovered material contract or authority conflicts require an actual maintainer decision. The agreed preservation limitations are retained, not widened.

No merge, release, installed-runtime activation, daemon/client reconfiguration, live graph repair/enrollment/migration/deletion or other-repository operation is authorized. Do not run old or reconstructed binaries against the current store.

The root/coordinator records bounded readiness and the actual technical assignment in [implementation-go.md](implementation-go.md). No source-edit GO is inferred merely from creating that record.
