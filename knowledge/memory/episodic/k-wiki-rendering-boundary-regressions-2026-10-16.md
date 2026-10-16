---
agent_memory:
  version: 1
  kind: episodic
  scope: repository
  status: active
  owner: repository-agents
  created_at: 2026-10-16T03:50:00Z
  last_verified_at: 2026-10-16T03:51:44Z
  verified_by: repository-agents
  review_after: null
  supersedes: []
  superseded_by: null
  sources:
  - kind: git-commit
    reference: cbe3a8a03e83cdcbfa3dad0a53fd76190986587e:crates/k-wiki/tests/rendering.rs
    content_hash: null
  - kind: git-commit
    reference: cbe3a8a03e83cdcbfa3dad0a53fd76190986587e:crates/k-wiki/src/render/mod.rs
    content_hash: null
  - kind: wiki-concept
    reference: knowledge:architecture/knowledge-wiki#component-map
    content_hash: null
  history:
  - from: candidate
    to: active
    actor: repository-agents
    at: 2026-10-16T03:51:44Z
    reason: Reviewed the candidate against committed renderer/production-compiler regression tests at cbe3a8a03e83cdcbfa3dad0a53fd76190986587e, independent GPT-6.1 Sol/Max Unicode probe reproduction and focused fix verification, and the configured MCP3.0.3 whole-LoopAI HTML audit. The claim is explicitly historical and recommends final body URL/heading/Unicode identity checks rather than inferring navigation from metadata or sidebars. No current unresolved defect or new architectural authority is claimed.
description: 'Historical renderer regression lesson: verify final body URLs and heading identity from production compilation, including Unicode fragments.'
tags:
- k-wiki
- navigation
- regression-tests
- renderer
timestamp: 2026-10-16T03:50:00Z
title: k-wiki renderer fixtures must cross the production compiler boundary
type: agent-memory
---
The October 2026 LoopAI publication audit exposed two renderer defects that hand-built fixtures had masked: production compilation emits bundle-qualified relationship target IDs, and normalized heading metadata is sorted by ID rather than source order. Fixtures supplying bare IDs and source-ordered headings passed while actual body links routed to diagnostics and canonical IDs attached to the wrong heading text. Counting IDs or finding a destination in sidebar navigation did not establish usable source-body navigation.

The repair at commit `cbe3a8a03e83cdcbfa3dad0a53fd76190986587e` adds production `compile_projection` → `render_site` regressions. Verify labeled links in the rendered body, the ID/text/level association of source headings, and browser-decoded fragment equality with actual target IDs. Include non-ASCII concept, self, directory and root fragments, duplicate headings beyond nine, extended Markdown, nonempty base paths and collisions with generated chrome IDs. A separate Unicode URL probe caught a shared scalar-to-hex escaping bug after the first heading tests passed; UTF-8 percent encoding fixed it without changing canonical slugs.

This is a historical diagnostic lesson, not an assertion that later revisions remain defective. Preserve the compiler identity and slug contracts when extending renderer coverage, and verify final serialized HTML rather than only intermediate projection metadata.
