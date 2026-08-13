---
agent_memory:
  version: 1
  kind: episodic
  scope: repository
  status: active
  owner: codex
  created_at: 2026-08-13T00:00:00+09:30
  last_verified_at: 2026-08-13T00:00:00+09:30
  verified_by: codex
  review_after: null
  supersedes: []
  superseded_by: null
  sources:
  - kind: commit
    reference: 'ffbbc7d test(storage): shorten Windows staging path'
    content_hash: null
  - kind: ci-log
    reference: GitHub Actions run 31666611652, cargo test (windows-2022)
    content_hash: null
  - kind: ci-log
    reference: GitHub Actions run 31667751952, cargo test (windows-2022)
    content_hash: null
  history:
  - from: candidate
    to: active
    actor: codex
    at: 2026-08-13T00:00:00+09:30
    reason: Verified against the MAX_PATH failure in GitHub Actions run 31666611652, the focused regression change in commit ffbbc7d, and the successful Windows workspace-test job in run 31667751952.
description: The direct-run staging layout amplifies a test temporary-directory name into connector CSV paths on Windows.
tags:
- ci
- github-actions
- max-path
- staging
- tests
- windows
timestamp: 2026-08-13T00:00:00+09:30
title: Keep direct-materialization test workspace paths below Windows MAX_PATH
type: agent-memory
---
A Windows CI regression in `direct_materialization_uses_custom_paths_and_cleans_shadow_files` was caused by `MAX_PATH`, not a missing staging file. Direct-run workspaces are nested below the test temporary-repository name, then write long connector filenames such as `from_syntaxchild__syntaxcapture__syntaxchild.csv`. Keep test workspace stems short (for example, `temp_repo("direct_materialization")`) so the resulting staging path stays below 260 characters; avoid deriving the directory name from the full test function name.