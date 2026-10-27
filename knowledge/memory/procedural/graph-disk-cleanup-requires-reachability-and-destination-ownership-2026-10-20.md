---
agent_memory:
  version: 1
  kind: procedural
  scope: repository
  status: active
  owner: repository-maintainers
  created_at: 2026-10-20T01:50:01Z
  last_verified_at: 2026-10-20T01:50:01Z
  verified_by: codex
  review_after: null
  supersedes: []
  superseded_by: null
  sources:
  - kind: review
    reference: knowledge/architecture/graph-disk-retention-review-2026-10-20.md
    content_hash: f8952424f5f1f64d388ab3ed5ebf82c74c62548e44abb47496d66344dca32dc3
  - kind: source
    reference: src/storage/layout.rs::DirectLayout::destination_key at 6aa5ba8bd99c3071e106a8bfc7a408652a8e4bb5
    content_hash: null
  - kind: test
    reference: /private/tmp/codebasegraph-storage-review-wihkqic4/result.json; extracted-function collection check, synthetic residue, no runtime crash injection
    content_hash: null
  history:
  - from: candidate
    to: active
    actor: codex
    at: 2026-10-20T01:50:01Z
    reason: Reviewed the focused diagnostic procedure against the measured active-manifest inventory, exact Direct destination-key match at 6aa5ba8, and fresh GPT-6.1 Sol Max source/harness findings. It distinguishes observed state from deletion authority and from absent containment guarantees.
description: Separate legacy debris, active parsing cache and live build state before designing or executing graph cleanup.
tags:
- cleanup
- diagnosis
- graph-storage
- ownership
timestamp: 2026-10-20T01:50:01Z
title: Classify graph disk usage by reachability and destination ownership
type: agent-memory
---
## Reusable procedure

Before treating graph state size as reclaimable waste, resolve the active pointer and classify allocated bytes against active-manifest artifact keys, live run ownership, leased generations, and recovery evidence. Recheck the pointer after measurement; record any concurrent mutation. Independently identify Direct cache destinations using their exact database/manifest pair: managed configuration alone does not retire an explicitly addressable Direct store.

The 2026-10-20 review found that large historical staging and a separate Direct cache coexisted with an overwhelmingly current parsing cache and live candidate space. A collector that sees only valid artifact keys can also miss interrupted temporary/replacement writes. Review all owned namespaces and failure exits, then require the relevant destination's own ownership and recovery proof before deletion. Preserve a replacement backup when a referenced destination is missing, and preserve stable lock inodes. Directory age and numeric PIDs do not prove abandonment.

Use version-specific source evidence for cleanup authority: newer guardian claims in wiki or memory cannot authorize destructive cleanup against a baseline that lacks them. Test total-state convergence and interrupted writes as well as active database size. The isolated extracted-function check established enumeration behavior only; it did not qualify a new runtime collector.