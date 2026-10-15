---
agent_memory:
  version: 1
  kind: episodic
  scope: repository
  status: active
  owner: repository-maintainers
  created_at: 2026-10-15T00:45:01Z
  last_verified_at: 2026-10-15T00:59:29Z
  verified_by: codex
  review_after: null
  supersedes: []
  superseded_by: null
  sources:
  - kind: source
    reference: src/storage/containment.rs; src/storage/run_workspace.rs; src/storage/managed.rs; src/storage/direct.rs; src/db_writer/phase.rs
    content_hash: null
  - kind: test
    reference: tests/support/lifecycle_faults.rs::standalone_owner_loss_preserves_candidate_while_guardian_is_unavailable; unavailable_guardian_preserves_paused_tree_evidence_until_emptiness_is_proven
    content_hash: null
  - kind: test
    reference: src/storage/test_harness.rs::persisted_relative_barrier_survives_cleanup_from_a_different_process_cwd; phase_registry_collection_retains_unknown_siblings_then_reclaims_empty_orphans
    content_hash: null
  - kind: review
    reference: Independent read-only GPT-6.1 Sol Max verification against v3.0.2 baseline 8ba4c316, production source snapshot fc2fd6814a8c24ad85f337384ac31ac790203967ca42f80b9ea87e780d0203ab
    content_hash: null
  history:
  - from: candidate
    to: active
    actor: codex
    at: 2026-10-15T00:59:29Z
    reason: Reviewed against the cited guarded-phase, storage cleanup/admission source, passing native retention and cross-cwd regressions, and independent GPT-6.1 Sol Max final-source verification.
description: Independent process containment is insufficient if storage abort or orphan recovery can erase its evidence and admit another writer.
tags:
- materialization
- process-lifecycle
- recovery
- storage
timestamp: 2026-10-15T00:45:01Z
title: Containment proof must survive candidate cleanup
type: agent-memory
---
During the v3.0.2 worker-recovery repair, an independent guardian correctly reported unknown standalone-loader teardown, but higher Managed/Direct abort and orphan-cleanup paths could still delete its candidate, request, and containment proof. That erased the evidence needed to prevent overlapping writes. Keep standalone phase requests and proof in a stable writer-scoped registry outside disposable runs and candidates. Writer admission, promotion, publication replay, abort, Drop, and orphan collection must all respect that registry; managed reads should retain the last committed graph and report pending cleanup truthfully.

Persist a run's private cleanup-barrier path with an absolute creation origin so a later janitor cannot reinterpret it from another working directory. Private lifecycle paths can compare by physical directory identity after rejecting leaf symlinks, missing roots, and foreign evidence. This is distinct from Direct database/manifest spelling, whose existing journal and lock keys must remain unchanged. A genuinely empty prelease startup directory can be tombstoned under newly acquired ownership; missing leases beside launch or execution evidence remain unknown. Collect proven-empty phase workspaces only under the storage writer lease.

Native paused-loader/owner-loss fixtures and a separate-process cross-cwd storage regression verified retained evidence, blocked writers, readable prior generations, and automatic retry after emptiness without another source edit. The repair does not establish the cause of the historical Qunys incident. The reusable lesson is to audit storage cleanup and admission alongside process supervision: preserving proof in the low-level launcher alone does not preserve the lifecycle contract.