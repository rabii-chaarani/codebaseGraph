---
agent_memory:
  version: 1
  kind: episodic
  scope: repository
  status: candidate
  owner: codex
  created_at: 2026-10-16T02:08:03Z
  last_verified_at: null
  verified_by: null
  review_after: null
  supersedes: []
  superseded_by: null
  sources:
  - kind: ci-log
    reference: https://github.com/rabii-chaarani/codebaseGraph/actions/runs/38837159211/job/116768743205
    content_hash: null
  - kind: commit
    reference: '062aad8: correct smoke_wiki_artifact fixture description; PR149'
    content_hash: null
  - kind: test-run
    reference: '2026-10-16 cargo run -p xtask --locked --offline -- smoke-wiki-artifact <built-k-wiki-binary>: exit 0 on macOS ARM64'
    content_hash: null
  history: []
description: A missing recommended description had been hidden by successful CLI exit status despite rejected validation.
tags:
- k-wiki
- ci
- validation
- native-artifacts
timestamp: 2026-10-16T02:08:03Z
title: Strict validation exits expose invalid native smoke fixtures
type: agent-memory
---
The PR #149 native artifact job failed after the CLI began returning a nonzero exit status for rejected validation. Its wiki smoke fixture omitted a description required by the recommended profile. The CLI correctly returned `accepted: false` with `recommended_description_missing`; the earlier always-successful exit status had hidden that invalid fixture from the artifact gate.

The correction supplies complete metadata in the shared native smoke fixture while preserving recommended-profile validation. The existing `smoke-wiki-artifact` entry point then passed against the built binary, including validation, rendering, isolated MCP registration, loopback HTTP, and stdio MCP checks.

When correcting a command's exit semantics, inspect downstream smoke fixtures as well as unit tests. A newly failing artifact gate can expose a previously ignored rejection payload. Repair the fixture's promised valid input rather than weakening the rejection contract.