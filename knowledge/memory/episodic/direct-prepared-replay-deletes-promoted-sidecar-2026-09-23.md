---
agent_memory:
  version: 1
  kind: episodic
  scope: repository
  status: active
  owner: repository-maintainers
  created_at: 2026-09-23T00:12:45Z
  last_verified_at: 2026-09-23T00:14:44Z
  verified_by: codex
  review_after: null
  supersedes: []
  superseded_by: null
  sources:
  - kind: source
    reference: src/storage/direct.rs:155-165,286-300 at 885454f2eaf214c7c0df1f268da1e87955e50030
    content_hash: null
  - kind: test
    reference: src/storage/test_harness.rs:487-584 at 885454f2; coverage includes intact Prepared candidates and persisted later phases
    content_hash: null
  - kind: runtime-observation
    reference: 2026-09-23 isolated check-health reproduction with Prepared journal, already-promoted db and lexicon, and pending manifest; first replay removed lexicon and two consecutive attempts failed recovery
    content_hash: null
  history:
  - from: candidate
    to: active
    actor: codex
    at: 2026-09-23T00:14:44Z
    reason: Reviewed the Prepared replay and checksum paths at 885454f2 against phase-level tests and the isolated repeated recovery reproduction. The claim is verified as an existing defect and required invariant, not a completed repair.
description: A crash between sidecar rename and the DatabasePromoted checkpoint exposes non-idempotent Prepared recovery.
tags:
- crash-consistency
- direct
- recovery
- storage
timestamp: 2026-09-23T00:12:45Z
title: Direct publication replay must distinguish promoted sidecars from absent sidecars
type: agent-memory
---
In v1.8.1 (commit 885454f2), Direct publication writes a Prepared journal before promoting the database and its sidecars. If a crash occurs after a sidecar rename but before the DatabasePromoted checkpoint, its candidate path is absent while its valid destination exists. Prepared replay in promote_database_bundle treats that absence as an omitted sidecar and removes the destination; checksum validation then fails and repeated recovery cannot finish. An isolated synthetic interrupted-publication fixture reproduced deletion of an expected search.lexicon.bin and repeated recovery failure, without touching repository source or the managed graph. Recovery must use the journal's sidecar_sha256 membership: retain and validate an already-promoted expected sidecar when its candidate is absent, and remove destinations only for sidecars the journal omits. Verify every individual rename interruption boundary, then repeat recovery to prove idempotence. Existing phase-level tests alone miss this intra-phase crash window. This records an observed defect and required recovery invariant, not an implemented fix.