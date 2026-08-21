---
agent_memory:
  version: 1
  kind: episodic
  scope: repository
  status: candidate
  owner: codebaseGraph maintainers
  created_at: 2026-08-20T14:01:28+09:30
  last_verified_at: null
  verified_by: null
  review_after: null
  supersedes: []
  superseded_by: null
  sources:
  - kind: source
    reference: src/coordinator.rs
    content_hash: null
  - kind: commit
    reference: 48d9c99a5c2f5a0c7ba1b6e44532ecee0e1ab610
    content_hash: null
  - kind: commit
    reference: 7dad07d2be3677d4fded82fb715fd4350880b389
    content_hash: null
  - kind: ci-log
    reference: GitHub Actions run 32331167537 job 96311967342
    content_hash: null
  - kind: ci-log
    reference: GitHub Actions run 32331746250 jobs 96313611925 and 96313611971
    content_hash: null
  history: []
description: A cached coordinator endpoint can remain connectable while its ownership token has changed; route recovery must treat authentication failure as stale routing, not as an ordinary operation failure.
tags:
- coordinator
- ci
- macos
- race
- routing
timestamp: 2026-08-20T14:01:28+09:30
title: Coordinator stale routes must recover from authentication failures
type: agent-memory
---
PR #88 exposed a macOS ARM race where a follower connected to a valid loopback endpoint but received `coordinator_authentication_failed` rather than pong because its cached coordinator token no longer matched the current owner. Bare TCP reachability is insufficient: owner monitors must use owned-thread liveness, followers must use authenticated protocol pings, and operation forwarding may refresh and retry only when authentication failed before execution. Ordinary application failures must not be replayed. A deterministic regression invalidates a follower token and verifies both operation forwarding and ping recovery; hosted run 32331746250 passed Clippy and the complete macOS ARM native artifact job.