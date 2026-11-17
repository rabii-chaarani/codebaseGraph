# CBG-RESTORE-01 — Dev PR native-CI qualification amendment

Revision CI-0.1, 17 November 2026 (Australia/Darwin). Work: [COD-136](https://linear.app/codebasegraph/issue/COD-136), CBG-RESTORE-01 / REST-R11, REST-A10, REST-A12. **Proposed only: no CI implementation approval is recorded.**

## Existing authority and concrete conflict

The maintainer already approved the restoration package at d809fe0f8289bbf7d167a891bd7c31160f27f0e1, including a dev-targeted PR and genuine Mac/Windows CI qualification. That approval and all other restoration work remain valid.

Frozen main90e2ab6fae2767bdb158e19f0072d43568e80dbc accepts CI pull requests against exactly six bases: main, cod103-stack/cod-105, cod103-stack/cod-106, cod103-stack/cod-56, cod103-stack/cod-107 and cod103-stack/cod-108. Push CI is main only. The [COD-103 exact-set contract](https://github.com/rabii-chaarani/codebaseGraph/blob/90e2ab6fae2767bdb158e19f0072d43568e80dbc/docs/specs/COD-103/design.md#L92) excludes unrelated bases and requires typed policy checks for that exact set. Therefore adding dev requires a specific maintainer decision rather than an inferred routine permission.

The [CI workflow](https://github.com/rabii-chaarani/codebaseGraph/blob/90e2ab6fae2767bdb158e19f0072d43568e80dbc/.github/workflows/ci.yml#L4) invokes the native Mac ARM64 and Windows x64 matrix with run-tests=true on its accepted events. Native is workflow_call only. Release manual dispatch uses run-tests=false and is not a substitute for this qualification; this amendment does not use or alter the release route.

PR #200 currently targets dev and no CI runs/checks were observed at its contract heads. This is an actual route gap, not failing product evidence or a reason to weaken the required checks.

## Proposed exact-scope exception

Recommendation: add the one literal base **dev** for CI pull_request events. This applies to dev-targeted PRs generally and supplies the already-selected development qualification route. Preserve the six current bases and every existing gate.

| ID | Required behavior |
| --- | --- |
| CI-R01 | The CI pull_request base list becomes exactly main, dev, cod103-stack/cod-105, cod103-stack/cod-106, cod103-stack/cod-56, cod103-stack/cod-107 and cod103-stack/cod-108. No wildcard, additional base or new event is introduced. |
| CI-R02 | Update both typed expected-base sets in xtask's ci_mode_policy_valid and check_workflow_policy, diagnostics and affected policy tests to this exact seven-base list. Do not bypass validation or make the matcher permissive. |
| CI-R03 | A dev-targeted PR selects the full CI path, including both native targets with run-tests=true, and existing policy/classification/base-head freshness/required aggregation/security/artifact checks. No new lightweight exemption or passing result from missing, cancelled or stale checks. |
| CI-R04 | Preserve push main only, current pull_request activity types, token permissions, trust boundaries, artifact policy and all release/publication authorization. Do not add pull_request_target, workflow_dispatch, a release path, credentials or a merge permission. |
| CI-R05 | Execute affected policy/classifier/aggregation regressions and obtain actual hosted native CI on the final restoration SHA with its exact PR base/head. Independent execution/review/publication gates remain. Configured jobs are not passing tests. |

This is a narrow exception to COD-103's six-base CI validation contract, not a revision of its other requirements or historical approvals. Keep the original COD-103 documents and immutable references unchanged; this additive record supplies the new selected precedence only after approval.

## Placement, ownership and prerequisites

The existing Release Gate/xtask CI owners retain their responsibilities and dependency direction. No new component/claim is invented. The current sole technical owner /root/restoration_implementation (default/Terra/medium) remains within the approved 137-file ownership.

Affected production/configuration files: .github/workflows/ci.yml and crates/xtask/src/main.rs. Classifier/required tests in the already-owned crates/xtask/src/ci.rs may be extended if needed; no production classifier policy beyond CI-R03 is broadened. Source/caller discovery and exact current anchors remain prerequisites.

The root owns this contract/decision record and serialized Git/tracking. CI source edits wait for actual exact-revision approval and root's bounded GO. Independent non-CI reconstruction may proceed under the original approval once its own readiness passes; this proposal does not suspend that authorized work.

## Acceptance and failure behavior

| ID | Given / When / Then |
| --- | --- |
| CI-A01 | Given the exact seven bases, workflow policy passes. Omitting dev or another required base, adding an eighth/wildcard base, expanding push or adding a privileged trigger fails the unchanged strict gate. |
| CI-A02 | Given a dev PR, including metadata-only input, full classification selects native tests for Mac ARM64 and Windows x64. Missing/cancelled/stale base-head evidence cannot make required succeed. Existing main/named-stack event regressions still pass. |
| CI-A03 | Given final integrated restoration source and actual dev PR, hosted Mac/Windows CI executes required suites, and passing evidence binds the exact head/base. No release/publication/merge or live operation occurs. |

If the branch/event route fails or required jobs do not run, preserve the actual failure and keep qualification pending. Never use Release's non-test native route or a broad policy exemption to manufacture a pass.

## Human decision still required

Approve this exact committed amendment to permit the bounded dev CI exception, or retain the existing exact six-base policy and leave the current native-CI route unresolved. A preselected answer, code commit, review or original restoration approval is not approval of this new exception.

Later actual approval must record this path, full Git SHA, precise scope and human question/answer reference. This file does not embed its own containing SHA. All other approved restoration requirements, native C4 separation and no-merge/release/live-change limits remain unchanged.
