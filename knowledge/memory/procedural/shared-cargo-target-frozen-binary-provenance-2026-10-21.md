---
agent_memory:
  version: 1
  kind: procedural
  scope: repository
  status: active
  owner: repository-maintainers
  created_at: 2026-10-20T18:36:45Z
  last_verified_at: 2026-10-20T18:36:45Z
  verified_by: codex-coordinator
  review_after: null
  supersedes: []
  superseded_by: null
  sources:
  - kind: independent-review-evidence
    reference: /private/tmp/codebasegraph-s4-postpublication-review-et3thm39/review-evidence.json
    content_hash: 6101c50b30b24228d473ff86ab98c4d6d16128fd8e98195705df595ea5d22729
  - kind: executed-junit
    reference: /private/tmp/codebasegraph-s4-postpublication-review-et3thm39/nextest/s4-postpublication-review/junit.xml
    content_hash: 3df4dc85b9039e597ac040cfca0615d7e4e0cbb05b60b2c94ea7d321c66b6216
  - kind: excluded-historical-report
    reference: /private/tmp/codebasegraph-s4-postpublication-review-et3thm39/first-reused-target-junit.xml
    content_hash: ed70ff0327920ea194f4df6fd76ceac547cd67db2ccdc03b96c06f8c9c4d408c
  history:
  - from: candidate
    to: active
    actor: codex-coordinator
    at: 2026-10-20T18:36:45Z
    reason: Reviewed against independent S4 exact-source manifest, explicit package-only rebuild command and source-specific passing JUnit; the ambiguous earlier report is separately preserved and excluded. Procedure is scoped to provenance checks, not a universal Cargo implementation diagnosis.
description: A shared Cargo target reused a test inventory from another checkout until the root package was explicitly rebuilt.
tags:
- cargo
- provenance
- reviews
- testing
timestamp: 2026-10-20T18:36:45Z
title: Verify the rebuilt binary for frozen Rust reviews
type: agent-memory
---
When several checkouts share CARGO_TARGET_DIR, matching source hashes and a passing selected test count alone do not establish which frozen snapshot executed. In the 2026-10-21 S4 review, an initial no-rebuild run registered 559 tests while the frozen reconstruction expected 555. Both runs selected the same 17 passing cases. An explicit root-package rebuild from the reconstructed path restored the expected inventory; the first run was preserved and excluded from frozen-snapshot verification.

For an independent reconstruction, retain the source/dependency manifest before execution, then force and verify an actual root-package compilation. A package-specific profile override such as --config 'profile.test.package.codebase-graph.codegen-units=128' can retain dependency caches; choose an uncached value when targets are shared and require the log's Compiling line to name the intended checkout. Reusing an override is not proof of a rebuild. Do not disable debug assertions or test-feature gates to change the cache key.

Record the exact command/profile, root compilation line, binary hash and registered test inventory alongside JUnit and before/after source identities. Check named cases and inventory, not only selected pass totals. Preserve ambiguous or mismatched reports as historical evidence, explicitly excluded from source-specific qualification. Distinguish a rerun of an already captured binary from a fresh source compilation; both may be useful, but establish different facts.