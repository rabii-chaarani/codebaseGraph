---
agent_memory:
  version: 1
  kind: episodic
  scope: repository
  status: active
  owner: repository-maintainers
  created_at: 2026-10-21T01:45:48Z
  last_verified_at: 2026-10-21T01:45:48Z
  verified_by: codex-coordinator
  review_after: null
  supersedes: []
  superseded_by: null
  sources:
  - kind: git-source
    reference: 7b0ad57ba26152c9fe19a9145a7c83c363f87b23:src/db_writer/access.rs::guarded_native_path
    content_hash: 104d9ed3ecc6b3b3c3ecef64a16b462464465d6f7aa0cbbaa2dc3d9dbc8b2a1d
  - kind: independent-review
    reference: docs/testing/graph-disk-cleanup-v2-evidence/s2-wrapper-review.md
    content_hash: null
  - kind: executed-junit
    reference: docs/testing/graph-disk-cleanup-v2-evidence/s2-wrapper-review-junit.xml
    content_hash: b539f8fa4d2cc86f107177ed8951a5858797c34e70bb17e43dcf260c1b4a6fee
  - kind: historical-native-probe
    reference: /private/tmp/cleanup-s2-wrapper-review-ccjn2do9/tilde-probe.log
    content_hash: null
  history:
  - from: candidate
    to: active
    actor: codex-coordinator
    at: 2026-10-21T01:45:48Z
    reason: Reviewed against exact committed wrapper/path source, historical real native tilde counterexample and independent final29case JUnit/report; qualification limits are explicit.
description: A native tilde reproduction showed that guarding the caller's path can exclude a different resource from the SDK's transformed path.
tags:
- graph-storage
- native-database
- ownership
- path-identity
timestamp: 2026-10-21T01:45:48Z
title: Bind database exclusion to the path actually opened by the native SDK
type: agent-memory
---
During the guarded database review, the pinned Ladybug Rust SDK converted a Path through display(), while native path expansion interpreted a leading tilde and lexically normalized components. A real Darwin native query succeeded against a different physical parent from the one guarded using the caller's original spelling. Lifetime exclusion alone was therefore insufficient.

The reviewed correction validates and captures one absolute native spelling before coordination, then uses that same spelling for both exclusion and native open. Reject ambiguous/non-UTF-8 forms rather than silently projecting them. Preserve existing raw Direct journal keys separately. Windows canonicalize returns extended namespaces: blanket rejection breaks ordinary resolver paths, while stripping prefixes can change length and parsing semantics. The correction preserves supported canonical prefixes and checks effective ANSI/OEM FileAPI encoding without changing process mode.

Independent Darwin execution verified the final reviewed wrapper and path-rejection cases. Windows prefix/encoding correction was inspected statically only; native Windows/Linux qualification and the author's parallel output-handle LEAK observations remained open. Reuse the lesson by tracing the exact pinned SDK and native filename pipeline before choosing a resource identity, then test actual native open plus competing exclusion.