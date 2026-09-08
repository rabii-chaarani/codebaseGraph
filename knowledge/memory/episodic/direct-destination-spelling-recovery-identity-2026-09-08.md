---
agent_memory:
  version: 1
  kind: episodic
  scope: repository
  status: active
  owner: codex
  created_at: 2026-09-08T07:22:07Z
  last_verified_at: 2026-09-08T07:27:53Z
  verified_by: codex
  review_after: null
  supersedes: []
  superseded_by: null
  sources:
  - kind: commit
    reference: '0d07a18 fix(runtime): bind configured repository identity across MCP operations'
    content_hash: null
  - kind: source
    reference: src/api/context.rs::RepositoryIdentity and bind_repo_selector; src/storage/layout.rs::DirectLayout::destination_key
    content_hash: null
  - kind: test
    reference: api::context::tests::resolve_runtime_recovers_direct_pair_before_returning_read_lease; repository_identity_rejects_config_direct_path_rebind
    content_hash: null
  - kind: runtime-observation
    reference: 2026-09-08 integrated nextest 41-test run reproduced the recovery regression; corrected identity/coordinator 18-test run and 503-test workspace run passed.
    content_hash: null
  history:
  - from: candidate
    to: active
    actor: codex
    at: 2026-09-08T07:27:53Z
    reason: Reviewed against the reproduced interrupted-publication failure, preserved DirectLayout key implementation in 0d07a18, and passing direct recovery and identity-provenance tests in the integrated workspace suite.
description: Canonical filesystem aliases can address the same database but change the path-derived DirectLayout journal and lock keys.
tags:
- identity
- recovery
- storage
timestamp: 2026-09-08T07:22:07Z
title: Preserve direct destination spelling when resolving existing recovery journals
type: agent-memory
---
During the graph freshness identity fix, canonicalizing explicit direct destinations from /var/... to /private/var/... changed DirectLayout::destination_key and caused resolve_runtime_recovers_direct_pair_before_returning_read_lease to miss an existing interrupted-publication journal: the manifest remained at version 1 instead of recovering version 2. Preserve existing physical path spelling for direct layout/journal operations, and pin its explicit-versus-config provenance so live config edits cannot silently change that key. Canonical source/config/managed-storage identity and direct journal addressing are separate constraints. If direct addressing is ever canonicalized globally, implement and verify a recovery/locking migration first rather than treating equivalent filesystem paths as interchangeable persisted identifiers.