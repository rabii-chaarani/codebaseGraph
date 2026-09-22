---
agent_memory:
  version: 1
  kind: episodic
  scope: repository
  status: quarantined
  owner: codex
  created_at: 2026-09-22T00:00:00Z
  last_verified_at: null
  verified_by: null
  review_after: null
  supersedes: []
  superseded_by: null
  sources:
  - kind: source
    reference: src/agent_hooks.rs::event_from_input and event_names
    content_hash: null
  - kind: test
    reference: manual native sessionStart stdin smoke on 2026-09-22
    content_hash: null
  - kind: documentation
    reference: https://docs.github.com/en/copilot/reference/hooks-reference
    content_hash: null
  history:
  - from: candidate
    to: quarantined
    actor: codex
    at: 2026-09-22T12:00:00+09:30
    reason: The candidate overstates that out-of-band identity is required; the implementation now safely infers native Copilot events from documented mutually distinct payload shapes and has regression coverage.
description: A shared Copilot hook command cannot infer sessionStart/userPromptSubmitted/postToolUse from native camelCase stdin alone.
tags:
- agent-hooks
- copilot
- hooks
- integration
- payload
timestamp: 2026-09-22T00:00:00Z
title: Copilot CLI native hook payloads omit the event name
type: agent-memory
---
GitHub Copilot CLI native hook payloads are selected by the configured event name but do not include that event name in stdin. For example, sessionStart supplies sessionId, timestamp, cwd, source, and initialPrompt; userPromptSubmitted supplies sessionId, timestamp, cwd, and prompt; preToolUse/postToolUse supply sessionId, timestamp, cwd, toolName/toolArgs or toolResult. A command reused for multiple lowerCamel events must receive event identity out-of-band (for example an event-specific argument or wrapper), otherwise a normalizer that only inspects hook_event_name/hookEventName/event/type/name classifies native payloads as unknown and returns no context. VS Code-compatible PascalCase payloads do include hook_event_name, so tests must cover both formats.