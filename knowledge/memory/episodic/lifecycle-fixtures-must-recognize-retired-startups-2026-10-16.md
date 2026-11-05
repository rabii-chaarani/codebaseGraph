---
agent_memory:
  version: 1
  kind: episodic
  scope: repository
  status: candidate
  owner: codex
  created_at: 2026-10-16T04:02:52Z
  last_verified_at: null
  verified_by: null
  review_after: null
  supersedes: []
  superseded_by: null
  sources:
  - kind: ci-log
    reference: https://github.com/rabii-chaarani/codebaseGraph/actions/runs/38839011807/job/116774822893
    content_hash: null
  - kind: commit
    reference: 49f294278c0ad2de34d2aa7b45e6597b0809ebc2
    content_hash: null
  - kind: source
    reference: src/storage/containment.rs::probe_lifecycle and retire_lifecycle
    content_hash: null
  - kind: test
    reference: tests/support/lifecycle_faults.rs::fixture_cleanup_accepts_retired_startup_without_terminal_receipt and fixture_cleanup_keeps_unproven_or_locked_containment
    content_hash: null
  history: []
description: A startup can be retired under its lifetime lease without a guardian ever writing a terminal receipt.
tags:
- ci
- process-lifecycle
- recovery
- testing
timestamp: 2026-10-16T04:02:52Z
title: Lifecycle fixture cleanup must recognize verified retirement
type: agent-memory
---
A Linux native CI cleanup timeout exposed a mismatch between the fixture's proof check and production recovery. Production can prove a gated startup empty and write `retired-v2` under its existing lifetime lease before any guardian writes `receipt.json`. A deterministic local regression confirmed that the old cleanup helper rejected this valid terminal state indefinitely.

Fixture cleanup now accepts a terminal empty receipt or the exact retirement marker while holding the exclusive lifetime lease through deletion. A held lease, missing proof, or invalid marker remains protected. This preserves the production fail-closed invariant instead of extending a timeout or discarding containment evidence.

The original CI job did not retain its fixture, so this establishes a confirmed helper defect rather than proving which on-disk state caused that historical timeout. The new positive and negative proof checks and the original owner-loss scenario passed under the assertion-enabled native test profile.