---
agent_memory:
  version: 1
  kind: episodic
  scope: repository
  status: candidate
  owner: codex
  created_at: 2026-10-16T01:19:39Z
  last_verified_at: null
  verified_by: null
  review_after: null
  supersedes: []
  superseded_by: null
  sources:
  - kind: source
    reference: crates/k-wiki/src/adapters/mcp.rs:181-194 at 55abb2522d410580d96861cd0894127ec7c87206
    content_hash: null
  - kind: test
    reference: crates/k-wiki/tests/transports.rs::mcp_tool_calls_dispatch_exactly_once_and_preserve_error_codes at 55abb2522d410580d96861cd0894127ec7c87206
    content_hash: null
  - kind: isolated-reproduction
    reference: /private/tmp/k-wiki-diagnostic-review-ysl7yi_o/diagnostics.rs, run.py and results.json::mcp_error_loss; 2026-10-16 local library harness
    content_hash: null
  history: []
description: Successful and unknown-tool transport checks can pass while real wiki domain errors lose their codes and recovery metadata.
tags:
- k-wiki
- diagnostics
- mcp
- verification
timestamp: 2026-10-16T01:19:39Z
title: Test k-wiki domain failures at the final MCP response boundary
type: agent-memory
---
The diagnostics review at commit `55abb2522d410580d96861cd0894127ec7c87206` reproduced a gap between typed API errors and the final MCP response: a real missing-concept request lost `concept_not_found`, and a separate injected retryable transport failure lost its domain code, retryability, and recovery details. Both became an RPC error containing only `-32602` and a message.

The existing transport regression exercises successful dispatch and an unknown tool; its passing result does not establish preservation of wiki domain failures. The reusable verification lesson is to exercise a real API failure through `dispatch_api` and `handle_message`, then assert the final serialized response retains the intended domain identity and recovery information. Include a distinct retryable failure, and distinguish it from malformed arguments and unknown tools. Testing an intermediate typed error alone cannot detect information discarded by the last response conversion.

This is a historical diagnostic discovery, not an assertion about later revisions. Recheck the current response conversion before relying on it.