---
agent_memory:
  version: 1
  kind: semantic
  scope: repository
  status: active
  owner: repository-maintainers
  created_at: 2026-10-23T03:57:12Z
  last_verified_at: 2026-10-23T03:57:50Z
  verified_by: codex
  review_after: null
  supersedes: []
  superseded_by: null
  sources:
  - kind: user-decision
    reference: '2026-10-23, this chat: explicit selection of CodebaseGraph workspace/team COD and creation of a new CodebaseGraph project.'
    content_hash: null
  - kind: linear-project
    reference: https://linear.app/codebasegraph/project/codebasegraph-2cf77b39fc8b
    content_hash: null
  - kind: repository-handoff
    reference: docs/specs/CBG-C4-01/linear-delivery.md
    content_hash: 9d35052c79f40b8268777de1d18cba85ff0dd51ad788fdf983ddf4ce73921742
  history:
  - from: candidate
    to: active
    actor: codex
    at: 2026-10-23T03:57:50Z
    reason: Verified against the user's explicit destination selection, the live COD project UUID/team/URL returned by Linear, and the delivery-index content hash. This records delivery identity only, not implementation approval.
description: The user selected a new COD workspace project rather than the separately connected LoopConnect project.
tags:
- cbg-c4-01
- delivery
- linear
- project-identity
timestamp: 2026-10-23T03:57:12Z
title: CBG-C4-01 delivery uses the CodebaseGraph workspace and COD project
type: agent-memory
---
For CBG-C4-01 delivery, use the CodebaseGraph workspace, team `COD` (`6f16e639-47e4-4b66-aa1c-177d7da4a8da`), and project `P-COD-1` (`8949d9f9-6c53-45ea-824b-89d766688e5d`): https://linear.app/codebasegraph/project/codebasegraph-2cf77b39fc8b.

On 23 October 2026 the user explicitly chose: “CodebaseGraph workspace, team COD: create a CodebaseGraph project first, then create the hierarchy there.” This resolves the old P-RAB-1 destination ambiguity for this programme. The separate LoopConnect P-LC-1 project was not used or migrated.

Verify the workspace/team/project identities against the live connector rather than choosing by project name alone. This destination decision authorises delivery planning; it is not implementation approval or approval of an uncommitted specification.