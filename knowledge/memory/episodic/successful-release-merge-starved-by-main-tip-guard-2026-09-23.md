---
agent_memory:
  version: 1
  kind: episodic
  scope: repository
  status: superseded
  owner: codex
  created_at: 2026-09-23T06:00:00Z
  last_verified_at: 2026-09-23T06:00:00Z
  verified_by: codex
  review_after: null
  supersedes: []
  superseded_by: exact-commit-release-publication-and-recovery-2026-09-24
  sources:
  - kind: ci-log
    reference: https://github.com/rabii-chaarani/codebaseGraph/actions/runs/35820576114/job/107057232758
    content_hash: null
  - kind: ci-run
    reference: https://github.com/rabii-chaarani/codebaseGraph/actions/runs/35818714588
    content_hash: null
  - kind: source
    reference: https://github.com/rabii-chaarani/codebaseGraph/blob/8c3bcf317d2bc8e5d803ee59b8f86678c1a55a1c/.github/workflows/release.yml
    content_hash: null
  - kind: documentation
    reference: knowledge/architecture/release-verification.md
    content_hash: null
  history:
  - from: candidate
    to: active
    actor: codex
    at: 2026-09-23T06:00:00Z
    reason: Reviewed against both GitHub Actions runs, the release job's exact skip log, release PR and descendant commit metadata, and the workflow at 8c3bcf3. Local workflow Git blob hash matches the immutable remote revision; all three tip guards and manual crate exclusion verified.
  - from: active
    to: superseded
    actor: codex
    at: 2026-09-24T05:13:02Z
    reason: The merged exact-commit publisher replaces current-tip-dependent release-please publication and adds verified full CI-run recovery. Historical incident and policy evidence remains available for audit.
description: The current-tip publication policy can skip an otherwise verified release permanently; later ordinary CI and existing-tag recovery do not complete it.
tags:
- ci
- freshness
- release
- workflow-run
timestamp: 2026-09-23T06:00:00Z
title: A successful release merge can be stranded when main advances during CI
type: agent-memory
---
Release run 35820576114 was triggered by successful main-push CI run 35818714588 for release PR #124 (2.0.0, c32f6ae3e2cf51f25278ceb3902e35c1cc1c9ac8). PR #123 advanced main to 8c3bcf317d2bc8e5d803ee59b8f86678c1a55a1c before that CI completed. The initial main-tip guard exited successfully with current-tip=false, so release-please and all publishers were skipped. This is a liveness limitation of the authored current-tip policy, not a CI failure or artifact failure.

Later ordinary commits run release-please with tag creation disabled, so their successful CI cannot recover the pending release. Retrying the same stale Release logic also cannot help. Current-tip equality is enforced in three places: before release-please, after release-please, and in the automatic exact-SHA CI gate; changing only the first check is insufficient. Existing manual dispatch requires an existing tag and never publishes the crate.

When investigating this pattern, distinguish a release merge whose own exact CI succeeded from an older release merge whose CI failed. Any proposed relaxation must retain exact merge identity, successful main-push CI, tag/SHA equality and same-run artifact provenance. Allowing publication after main advances would change the current documented architectural policy and has not been implemented by this investigation.