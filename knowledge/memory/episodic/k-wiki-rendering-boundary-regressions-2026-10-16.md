---
agent_memory:
  version: 1
  kind: episodic
  scope: repository
  status: active
  owner: repository-agents
  created_at: 2026-10-16T03:50:00Z
  last_verified_at: 2026-10-16T03:56:33Z
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
  - from: active
    to: quarantined
    actor: repository-agents
    at: 2026-10-16T03:55:08Z
    reason: 'Final independent review contradicted the sentence implying source-ordered heading metadata reached the renderer in old fixtures: sample_projection calls projection.normalize, and Concept::normalize sorts headings by ID. The historical failures are verified, but that sentence overstates their masking mechanism. Quarantine pending precise correction and renewed evidence review.'
  - from: quarantined
    to: candidate
    actor: repository-agents
    at: 2026-10-16T03:55:43Z
    reason: Freshly reviewed the corrected masking mechanism against sample_projection normalization and the independent final reviewer counterevidence. Removed the assertion that source-ordered fixture metadata reached rendering; the candidate now attributes the missed heading defect to absent ID/text association checks and the target defect to bare fixture IDs plus whole-page assertions. Quarantine history retains the original correction reason. Ready for focused final review before activation.
  - from: candidate
    to: active
    actor: repository-agents
    at: 2026-10-16T03:56:33Z
    reason: Fresh independent GPT-6.1 Sol/Max reviewer fetched and verified the corrected candidate against production regression tests and sample_projection/Concept normalization. The corrected wording accurately distinguishes bare fixture IDs from missing final ID/text association checks; the earlier mechanism is retained only in lifecycle correction history. Final configured-MCP3.0.3 source-body/heading/Unicode audit passes. Historical scope, source provenance and architecture authority remain explicit.
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
The October 2026 LoopAI publication audit exposed two renderer defects that hand-built fixtures had masked: production compilation emits bundle-qualified relationship target IDs, and normalized heading metadata is sorted by ID rather than source order. The hand-built fixtures used bare relationship IDs, while whole-page URL assertions and missing ID/text association checks allowed both production target mismatches and misassigned heading IDs to pass. Counting IDs or finding a destination in sidebar navigation did not establish usable source-body navigation.

The repair at commit `cbe3a8a03e83cdcbfa3dad0a53fd76190986587e` adds production `compile_projection` → `render_site` regressions. Verify labeled links in the rendered body, the ID/text/level association of source headings, and browser-decoded fragment equality with actual target IDs. Include non-ASCII concept, self, directory and root fragments, duplicate headings beyond nine, extended Markdown, nonempty base paths and collisions with generated chrome IDs. A separate Unicode URL probe caught a shared scalar-to-hex escaping bug after the first heading tests passed; UTF-8 percent encoding fixed it without changing canonical slugs.

This is a historical diagnostic lesson, not an assertion that later revisions remain defective. Preserve the compiler identity and slug contracts when extending renderer coverage, and verify final serialized HTML rather than only intermediate projection metadata.
