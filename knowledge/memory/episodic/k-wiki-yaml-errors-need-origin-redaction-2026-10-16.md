---
agent_memory:
  version: 1
  kind: episodic
  scope: repository
  status: candidate
  owner: codex
  created_at: 2026-10-16T01:46:17Z
  last_verified_at: null
  verified_by: null
  review_after: null
  supersedes: []
  superseded_by: null
  sources:
  - kind: commit
    reference: '8531bd8: safe YAML/I/O diagnostics and shared delimiter parsing; included in 93be5c3'
    content_hash: null
  - kind: test
    reference: crates/k-wiki/tests/authoring.rs::malformed_existing_frontmatter_does_not_echo_source_values
    content_hash: null
  - kind: test
    reference: crates/k-wiki/tests/bundle_reader.rs diagnostic regressions for duplicate YAML keys and unreadable directories
    content_hash: null
  - kind: test
    reference: crates/k-wiki/tests/diagnostics.rs::malformed_source_errors_are_safe_through_api_mcp_and_http and incomplete_scan_messages_are_safe_through_api_mcp_and_http at 93be5c3
    content_hash: null
  history: []
description: YAML error display text can disclose scalar values and duplicate keys even when path filtering succeeds.
tags:
- k-wiki
- diagnostics
- verification
- redaction
timestamp: 2026-10-16T01:46:17Z
title: Sanitize YAML diagnostics before transport conversion
type: agent-memory
---
The diagnostic remediation reproduced two independent ways that raw YAML error formatting disclosed source content: deserializing a scalar where a mapping was required echoed the scalar value, and duplicate-key errors echoed the key. A path-pattern filter cannot identify these disclosures because the exposed value need not resemble a path.

The verified correction constructs public messages from a safe error category and typed line/column information at the parser boundary. Filesystem errors similarly use `io::ErrorKind` categories and an independently derived relative source path. This keeps useful failure identity and location without forwarding dependency display text.

Regression checks must cover both operation failures and successful responses containing diagnostics. In k-wiki, a loader diagnostic can travel inside a successful API/MCP/HTTP payload, bypassing an HTTP error-only sanitizer. Marker values in malformed YAML and a disposable unreadable directory exercise these distinct paths. The checks used isolated fixtures and never real secrets.