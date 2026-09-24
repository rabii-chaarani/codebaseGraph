---
agent_memory:
  version: 1
  kind: procedural
  scope: repository
  status: active
  owner: codex
  created_at: 2026-09-24T05:13:02Z
  last_verified_at: 2026-09-24T05:13:02Z
  verified_by: codex
  review_after: null
  supersedes:
  - workflow-run-release-exact-run-binding-2026-08-14
  - release-please-stale-pending-tag-guard-2026-08-17
  - successful-release-merge-starved-by-main-tip-guard-2026-09-23
  superseded_by: null
  sources:
  - kind: commit
    reference: 4594f384488e4764c18ca41ac69be146b328c18c
    content_hash: null
  - kind: source
    reference: https://github.com/rabii-chaarani/codebaseGraph/blob/4594f384488e4764c18ca41ac69be146b328c18c/.github/workflows/release.yml
    content_hash: null
  - kind: test
    reference: crates/xtask/src/release/workflow_tests.rs at 4594f384488e4764c18ca41ac69be146b328c18c
    content_hash: null
  - kind: ci-run
    reference: https://github.com/rabii-chaarani/codebaseGraph/actions/runs/35958468113
    content_hash: null
  - kind: published-package
    reference: https://crates.io/crates/codebase-graph/2.0.0
    content_hash: null
  history:
  - from: candidate
    to: active
    actor: codex
    at: 2026-09-24T05:13:02Z
    reason: Verified against merged implementation, 31 passing targeted tests, full PR CI, successful GitHub dry-run/publication, public archive digests, and the published crate's original source SHA.
description: Bind publication to the release merge's successful CI; recover pending releases before maintaining subsequent proposals.
tags:
- ci
- provenance
- recovery
- release
timestamp: 2026-09-24T05:13:02Z
title: Publish and resume verified releases independently of the current main tip
type: agent-memory
---
Publication is bound to one successful main-push ci.yml run and the exact merge SHA of one trusted repository-owned release PR. The commit must remain in main history, but need not remain its tip. Release-please only manages proposals; the targeted publisher creates the selected tag/release after production and native artifact validation. Matching tags/releases and already-published crate versions can be retried; conflicting tags are never moved. The pending label is removed only after native assets and crates.io succeed.

To recover a blocked release, dispatch Release on main with resume-ci-run set to that release merge's own successful CI run, artifact-source=promote, and dry-run=true. After the complete dry-run succeeds, dispatch the same target with dry-run=false. This path can create a missing tag/release and publish both native assets and the crate. The separate publish-existing-tag mode remains native-assets-only and retains all-target rebuild-if-missing; exactly one target is allowed. Never substitute a newer ordinary CI run or clear the pending label just to unblock release-please.

If GitHub rejects a historical tag/release because its workflow files differ from current main, use an appropriately scoped RELEASE_PUBLISH_TOKEN in the cargo environment, or have the authorized owner create only the verified tag/release after dry-run validation and then resume the same CI target. Do not upload a local CLI login credential into Actions secrets.

After finalization, proposal maintenance requires successful CI for current main. If that CI is still running, proposal creation waits for its completion event. Outstanding merged pending release PRs are explicit errors with recovery instructions. The shared release-main concurrency group uses cancellation disabled and queue=max to avoid replacement of pending release runs.

Verified in the 2.0.0 recovery: full dry-run 35958192962 and publishing run 35958468113 succeeded; all four public native archive digests matched the original retained CI artifacts, and the crates.io package's .cargo_vcs_info.json identified c32f6ae3e2cf51f25278ceb3902e35c1cc1c9ac8. PR #124 was finalized as tagged.