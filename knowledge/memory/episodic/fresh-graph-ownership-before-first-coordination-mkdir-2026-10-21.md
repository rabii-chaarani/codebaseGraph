---
agent_memory:
  version: 1
  kind: episodic
  scope: repository
  status: active
  owner: repository-maintainers
  created_at: 2026-10-21T13:47:21Z
  last_verified_at: 2026-10-21T13:49:00Z
  verified_by: codex-coordinator
  review_after: null
  supersedes: []
  superseded_by: null
  sources:
  - kind: executed-evidence-index
    reference: docs/testing/graph-disk-cleanup-v2-evidence/implementation-checkpoints-212-216.json
    content_hash: null
  - kind: immutable-evidence-archive
    reference: docs/testing/graph-disk-cleanup-v2-evidence/implementation-checkpoints-212-216.tar.gz
    content_hash: d4175cb036f4dff1f16576d7d5883b115a6a6ea3a6758eb43cf52cf876edd339
  - kind: executed-junit
    reference: 216/nextest/s5-worker216/worker-junit.xml inside the evidence archive
    content_hash: 30c0b17f218bfc14953b1e8adb7c0c8e9fc419c99d7b028007f5bb8e1ab205c7
  - kind: source-snapshot
    reference: 216-source/src/storage/maintenance_ownership.rs::initialize_fresh_managed_domain inside the evidence archive
    content_hash: 77087773f50ce6180f95bce7e557a323860ebabb1ee92f4bc02977f2efc5750c
  history:
  - from: candidate
    to: active
    actor: codex-coordinator
    at: 2026-10-21T13:49:00Z
    reason: Verified the archived actual 216 shipping MCP first-birth/followup JUnit has one passing named case and exact recorded hash, with actual coordinator/refresh initializer calls in frozen source. The archived 214 failure distinguishes valid graph publication from absent epoch/origin coverage; the record preserves Darwin-only qualification and no existing-root adoption.
description: A real MCP first-start test exposed ownership initialization that unit factories missed because the coordinator created storage first.
tags:
- graph-storage
- initialization
- ownership
- regression-testing
timestamp: 2026-10-21T13:47:21Z
title: Establish fresh graph ownership before the first coordination lock creates storage
type: agent-memory
---
During automatic graph cleanup implementation, direct ManagedStore tests established a covered reference epoch, but the real shipping MCP coordinator created the missing storage directory for coordinator.lock before the isolated worker reached the genuine fresh-root initializer. The graph still published successfully with artifact publication controls, while reference epoch/activity controls and per-entry origins were absent. Existing-directory checks could not safely recover that lost creation authority.

The verified correction calls the genuine fresh Managed-domain initializer at each actual first mutating coordinator/refresh/worker acquisition boundary, before its lock can create storage. Keep path resolution and observational probes noncreating. Existing roots remain unadopted; directory emptiness or a successful current lock is not fresh-creation proof.

Verify this through the real shipping coordinator first activation and a same-root worker follow-up, not only a direct storage factory. The retained 214 failure and passing 216 MCP/coordinator tests distinguish successful graph publication from cleanup ownership coverage. These executions were Darwin ARM64; native qualification elsewhere remains separate.