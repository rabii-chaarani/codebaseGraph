---
agent_memory:
  version: 1
  kind: episodic
  scope: repository
  status: active
  owner: repository-maintainers
  created_at: 2026-10-20T17:42:45Z
  last_verified_at: 2026-10-20T17:42:45Z
  verified_by: codex-coordinator
  review_after: null
  supersedes: []
  superseded_by: null
  sources:
  - kind: git-source
    reference: c0741ce137e09dac309ac72f5a2c2243a92ef070:src/storage/maintenance_ownership.rs::DatabaseUseLease
    content_hash: a3ab4989eb5432d099a2aa7657796ffc6f0b63c7615dccdd9a535bf5c149666a
  - kind: test
    reference: src/storage/maintenance_ownership.rs::dc_s2_family_macos_unicode_nfc_nfd_existing_and_creation_aliases_exclude; dc_s2_family_macos_long_s_consumer_blocks_ascii_direct_even_with_valid_record at c0741ce
    content_hash: null
  - kind: executed-junit
    reference: docs/testing/graph-disk-cleanup-evidence/s2-family-focused-junit.xml
    content_hash: 6118b539934bbec11a16cacaf410fe10b12aea7d4b9e17a5ef600dc1d7941876
  - kind: independent-probe
    reference: /private/tmp/cleanup-s2-family-review-yahled4r/result.txt
    content_hash: null
  history:
  - from: candidate
    to: active
    actor: codex-coordinator
    at: 2026-10-20T17:42:45Z
    reason: Verified candidate claim against committed c0741ce source, actual Darwin focused JUnit and independent physical-alias probe output. Explicitly excludes unresolved raw helper lifetime and unrun native platforms.
description: A native macOS alias reproduction required physical-parent family exclusion while preserving raw Direct journal addresses.
tags:
- disk-cleanup
- ownership
- storage
- unicode
timestamp: 2026-10-20T17:42:45Z
title: Database basename hashes do not cover filesystem Unicode aliases
type: agent-memory
---
During the disk-cleanup guard review, native macOS resolved NFC/NFD spellings and ASCII s versus U+017F long s to the same singly linked database inode, while lowercasing and hashing the addressed basename produced different coordination names. A consumer through the alternate spelling could therefore bypass the old exclusive retirement proof even beside a valid destination cohort record.

The reviewed correction retains shared physical-parent family ownership through every guarded database use, including absent-to-created resources and conversion into a session lock. Grouped exclusive acquisition deduplicates physical parents and retains all actual family/resource guards; replacing the parent or stable coordination inode invalidates the proof. This deliberately permits conservative same-parent deferral. Preserve existing raw Direct pair spelling for journal/writer keys rather than canonicalizing those persisted addresses as an alias fix.

Verify actual filesystem aliases and full guard lifetime, not only string-case equality or Unix link counts. The focused Darwin tests and independent probes verified the fix at c0741ce; this does not establish lifetime safety for public raw native database helpers, which remain a separate unresolved API boundary, or qualify native Windows/Linux behavior.