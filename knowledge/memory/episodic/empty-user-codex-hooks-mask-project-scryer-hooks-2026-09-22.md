---
agent_memory:
  version: 1
  kind: episodic
  scope: repository
  status: candidate
  owner: codex
  created_at: 2026-09-22T11:16:00+09:30
  last_verified_at: null
  verified_by: null
  review_after: null
  supersedes: []
  superseded_by: null
  sources:
  - kind: runtime-observation
    reference: 2026-09-22 stat/jq validation of /Users/rabii/.codex/hooks.json and synthetic Scryer SessionStart smoke
    content_hash: null
  - kind: source
    reference: .codex/hooks.json
    content_hash: null
  - kind: documentation
    reference: https://learn.chatgpt.com/docs/hooks
    content_hash: null
  history: []
description: A zero-byte ~/.codex/hooks.json fails before Codex can load a valid repository-local Scryer hook configuration.
tags:
- codex
- hooks
- scryer
- configuration
- diagnostics
timestamp: 2026-09-22T11:16:00+09:30
title: An empty user Codex hooks file masks valid project Scryer hooks
type: agent-memory
---
On 2026-09-22, Codex reported `failed to parse hooks config ... EOF while parsing a value at line 1 column 0` while opening the codebaseGraph repository. The user-level `/Users/rabii/.codex/hooks.json` was exactly zero bytes, while the repository-local `.codex/hooks.json` was valid and defined Scryer `SessionStart`, `PreToolUse`, `PostToolUse`, and `Stop` command hooks. Codex merges hook sources rather than overriding lower-precedence hooks, so copying the project hook file into the user layer would run matching hooks twice. The safe repair was to replace the truncated user file with `{ "hooks": {} }`, retain the repository-local Scryer definitions as the single active source, validate both files as JSON, and smoke-test `/Applications/scryer.app/Contents/MacOS/scryer-mcp hook` with a synthetic `SessionStart` payload. The hook command exited 0.