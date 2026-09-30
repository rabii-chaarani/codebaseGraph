---
agent_memory:
  version: 1
  kind: episodic
  scope: repository
  status: active
  owner: repository-maintainers
  created_at: 2026-09-30T21:48:51Z
  last_verified_at: 2026-09-30T21:48:51Z
  verified_by: codex
  review_after: null
  supersedes: []
  superseded_by: null
  sources:
  - kind: source
    reference: crates/k-wiki/src/search/mod.rs::highlight_match
    content_hash: null
  - kind: test
    reference: crates/k-wiki/tests/search.rs::snippet_windows_preserve_utf8_boundaries
    content_hash: null
  - kind: test
    reference: crates/k-wiki/tests/api_integration.rs::mcp_stdio_memory_recall_with_unicode_keeps_the_transport_open
    content_hash: null
  - kind: test-run
    reference: '2026-10-01 cargo nextest run -p k-wiki: 98 passed; cargo fmt --all --check; cargo clippy -p k-wiki --all-targets -- -D warnings'
    content_hash: null
  history:
  - from: candidate
    to: active
    actor: codex
    at: 2026-09-30T21:48:51Z
    reason: Reviewed the panic reproduction, UTF-8 boundary repair, direct-search matrix, and real MCP recall/ping regression against source; all 98 k-wiki tests, formatting, and strict Clippy passed.
description: Memory recall can close stdio through a shared search-snippet panic when a byte-based excerpt boundary splits a UTF-8 character.
tags:
- k-wiki
- mcp
- memory
- search
- unicode
timestamp: 2026-09-30T21:48:51Z
title: Unicode snippet boundaries can terminate the wiki MCP transport
type: agent-memory
---
A `wiki_memory_recall` transport closure can originate in shared search snippet generation. A disposable MCP process with an activated Unicode memory reproduced a main-thread panic when the snippet context boundary split a multibyte character. The process exited during recall before answering the request, which leaves clients with a closed transport rather than a typed wiki error.

The repair moves the context window endpoints outward to valid UTF-8 character boundaries before slicing. Regression coverage exercises every interior byte boundary of two-, three-, and four-byte characters on both sides of ASCII and Unicode matches. A real stdio process records and activates a Unicode memory, recalls it with both text and structured output, and successfully answers a subsequent ping. The full k-wiki suite passed 98 tests; formatting and strict Clippy also passed.

When diagnosing a closed stdio transport during text retrieval, reproduce the call in an isolated MCP process and inspect its exit status and stderr before assuming an idle timeout or connection lifecycle defect. Preserve Unicode in the reproduction because an ASCII-only fixture hides this failure.