---
agent_memory:
  version: 1
  kind: procedural
  scope: repository
  status: candidate
  owner: repository-agents
  created_at: 2026-10-21T00:42:30Z
  last_verified_at: null
  verified_by: null
  review_after: null
  supersedes: []
  superseded_by: null
  sources:
  - kind: source
    reference: https://github.com/rabii-chaarani/codebaseGraph/blob/018df35e72447fc3e014e950d1d3c6648bbd4820/.github/actions/verified-release/action.yml
    content_hash: null
  - kind: source
    reference: https://github.com/rabii-chaarani/codebaseGraph/blob/018df35e72447fc3e014e950d1d3c6648bbd4820/.github/workflows/release.yml
    content_hash: null
  - kind: pull-request
    reference: https://github.com/rabii-chaarani/codebaseGraph/pull/154
    content_hash: null
  - kind: commit
    reference: d71e314be474e06a74385cfec8e6d7955f9eabc2
    content_hash: null
  - kind: executed-check
    reference: python3 /private/tmp/codebase-graph-release-3.0.4-20261021/verify_recognition.py; seven checks passed; /private/tmp/codebase-graph-release-3.0.4-20261021/verification.json
    content_hash: null
  history: []
description: Use the exact trusted release branch and distinguish projected recognition from actual publication qualification.
tags:
- ci
- provenance
- release
- replacement
- verification
timestamp: 2026-10-21T00:42:30Z
title: Preflight a same-version replacement release before merging
type: agent-memory
---
When an original release merge never achieves successful exact-SHA CI, a dependency fix on a later ordinary main commit cannot authorise that original release. For a version that is absent from both GitHub tags/releases and the package registry, prepare a replacement release proposal from patched main while retaining the same package, dependency and release-manifest versions.

Before reusing the generated release branch, verify that no open PR owns it, retain its prior head identity, and update it with an expected-SHA lease. The existing recogniser requires the exact repository-owned `release-please--branches--main--components--codebase-graph` branch, base `main`, and a pending or tagged release label. A conventional `codex/` replacement branch is not recognised by this contract.

Preflight the unmodified recognition action with the new PR's real repository, branch, base and label fields. A fixture that projects a merged PR and successful main-push CI at the same SHA must select only the replacement, including when the original pending PR is present in the associated-PR response. Confirm rejection of the actual open PR, an untrusted branch, a missing release label, a fork, the original merge at a different SHA, and failed CI. Keep this projection explicitly separate from observed GitHub execution: it proves selection behaviour, not an actual merge, CI success, main-history membership, complete artifacts or publication.

Keep the original pending release tracked until the replacement's native assets and crate publication have both succeeded. The current finalizer updates only the selected PR; stale pending state on the original still needs explicit supersession reconciliation after successful publication. Do not mark the original source SHA as published or clear its label merely to bypass the proposal guard.

The preflight was exercised for replacement PR #154 using the action at `018df35e72447fc3e014e950d1d3c6648bbd4820`; all seven checks passed. Its publication remained a later gate.