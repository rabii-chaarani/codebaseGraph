---
agent_memory:
  version: 1
  kind: semantic
  scope: repository
  status: active
  owner: codex
  created_at: 2026-09-23T00:00:00Z
  last_verified_at: 2026-09-23T00:00:00Z
  verified_by: codex
  review_after: null
  supersedes:
  - semantic-artifact-reuse-schema-upgrade-boundary
  superseded_by: null
  sources:
  - kind: source
    reference: src/execution/parallel.rs::build_partition_for_path_with_workers
    content_hash: null
  - kind: source
    reference: src/artifact_store.rs::artifact_key
    content_hash: null
  - kind: test
    reference: src/execution/parallel.rs::tests::prior_manifest_schema_forces_rebuild_even_when_artifact_key_matches
    content_hash: null
  - kind: test
    reference: src/execution/run.rs::tests::digest_only_build_changes_can_reuse_all_raw_artifacts
    content_hash: null
  history:
  - from: candidate
    to: active
    actor: codex
    at: 2026-09-23T00:00:00Z
    reason: Reviewed the schema-match guard in build_partition_for_path_with_workers, artifact-key material, unchanged graph digest, and the current schema-upgrade/digest-only regression tests. Removed keys use default Serde unknown-field handling.
description: A forced graph rebuild can reuse compatible raw partitions; manifest schema changes deliberately bypass reuse.
tags:
- artifacts
- compatibility
- manifest
- memory
timestamp: 2026-09-23T00:00:00Z
title: Raw artifact reuse survives graph rebuilds when keys match
type: agent-memory
---
A forced graph rebuild does not by itself invalidate raw partition artifacts. When the artifact key matches and the previous manifest schema matches the requested schema, digest-only changes and explicit full builds may reuse raw artifacts. A manifest schema mismatch suppresses reuse of the prior entry so the schema transition rebuilds partitions. Parser, ontology, profile, source content, and artifact-format changes invalidate reuse through the artifact key. Retired semantic-enrichment keys in old JSON are ignored and do not affect graph-build digests or artifact keys.