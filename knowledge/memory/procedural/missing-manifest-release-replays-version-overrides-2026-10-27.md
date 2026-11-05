---
agent_memory:
  version: 1
  kind: procedural
  scope: repository
  status: active
  owner: repository-maintainers
  created_at: 2026-10-27T09:07:00Z
  last_verified_at: 2026-10-27T09:08:00Z
  verified_by: repository-maintainers
  review_after: null
  supersedes: []
  superseded_by: null
  sources:
  - kind: executed-evidence
    reference: https://github.com/rabii-chaarani/codebaseGraph/tree/39e817427bd9cd8fb65f6ff907599e0b7ff6ec6f/docs/testing/release-pr-162
    content_hash: null
  - kind: source
    reference: https://github.com/googleapis/release-please/blob/712fcf01effd08d7b0e7b1fd3861f2cb388bc8d1/src/manifest.ts#L537-L727
    content_hash: null
  - kind: source
    reference: https://github.com/googleapis/release-please/blob/712fcf01effd08d7b0e7b1fd3861f2cb388bc8d1/src/strategies/base.ts#L543-L570
    content_hash: null
  - kind: contract
    reference: https://github.com/rabii-chaarani/codebaseGraph/blob/39e817427bd9cd8fb65f6ff907599e0b7ff6ec6f/docs/release.md
    content_hash: null
  history:
  - from: candidate
    to: active
    actor: repository-maintainers
    at: 2026-10-27T09:08:00Z
    reason: Verified against pinned release-please17.6.0 source and executed read-only generator evidence committed at39e8174; independent final reviewer verified causal path and source snapshot. Scoped to version17.6.0 behavior; no publication authority inferred.
description: Bound release-please fallback history without fabricating publication; verify the actual pinned generator before repairing a release proposal.
tags:
- provenance
- recovery
- release
- release-please
- versioning
timestamp: 2026-10-27T09:07:00Z
title: Missing manifest-matching release tags can replay old Release-As trailers
type: agent-memory
---
When release-please 17.6.0 has a manifest version but cannot find that exact version's tag/release, it can scan all history and replay an old `Release-As` trailer before normal semantic version bumping. The manifest alone is not a commit boundary. Diagnose the missing exact tag and generator logs before treating a downgrade as a cosmetic PR-title error.

For a maintainer-approved replacement of a failed unpublished release, a `bootstrap-sha` at the failed release merge can serve only as the commit cutoff for subsequent version calculation. It does not prove publication, authorize that old SHA, or replace exact-source CI/production/artifact gates. Verify the pinned generator with outbound mutations blocked. In the observed case, manifest4.0.0 plus the failed4.0.0 merge cutoff generated4.0.1 naturally; a last-published3.0.4 cutoff instead included the breaking change and generated5.0.0. Do not add a permanent `release-as` or `last-release-sha` pin as a cosmetic workaround.

Bootstrap is ignored once a manifest-matching tag/release is found. Curate replacement notes to retain unreleased features/breaking changes and compare from the actual published baseline. Preserve historical sections and describe failed proposals as unpublished; never fabricate a tag or mark their source as tagged to unblock the bot. Open PR config is not read until it reaches the target branch, and extra hold/snooze/draft labels do not stop updates to a pending open PR in this version. Keep the overwrite window explicit rather than weakening trusted branch/label recognition or changing integration settings.