---
agent_memory:
  version: 1
  kind: semantic
  scope: repository
  status: candidate
  owner: codex
  created_at: 2026-08-17T00:00:00+09:30
  last_verified_at: null
  verified_by: null
  review_after: null
  supersedes: []
  superseded_by: null
  sources:
  - kind: source
    reference: src/execution/parallel.rs
    content_hash: null
  - kind: test
    reference: src/execution/parallel.rs::tests::prior_manifest_schema_forces_rebuild_even_when_artifact_key_matches
    content_hash: null
  - kind: test
    reference: src/execution/run.rs::tests::digest_only_build_changes_can_reuse_all_raw_artifacts
    content_hash: null
  - kind: test
    reference: src/execution/run.rs::tests::semantic_enrichment_changes_reuse_raw_artifacts_and_rerun_global_enrichment
    content_hash: null
  history: []
description: Defines the compatibility boundary between forced graph rebuilds and raw partition artifact reuse.
tags:
- artifacts
- manifest
- compatibility
- memory
timestamp: 2026-08-17T00:00:00+09:30
title: Raw artifact reuse is invalidated only by manifest schema upgrades
type: agent-memory
---
A forced graph rebuild does not by itself make raw partition artifacts unusable. Reuse remains allowed when the artifact key matches for digest-only changes, semantic-enrichment changes, and explicit full builds. Suppress raw-artifact reuse when the previous manifest schema differs from the requested manifest schema so the v4→v5 transition performs one bounded full partition rebuild. Parser, ontology, and profile changes normally invalidate reuse through the artifact key.