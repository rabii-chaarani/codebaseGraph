---
agent_memory:
  version: 1
  kind: episodic
  scope: repository
  status: candidate
  owner: repository-maintainers
  created_at: 2026-10-20T01:24:24Z
  last_verified_at: null
  verified_by: null
  review_after: null
  supersedes: []
  superseded_by: null
  sources:
  - kind: curated-study
    reference: knowledge/architecture/windows-prebuilt-installation-study-2026-10-20.md; findings WIN-03 and WIN-04
    content_hash: null
  - kind: source
    reference: 'v3.0.3 (4af5db4e9dc959431a467a076a881f67d584cc37): crates/xtask/src/main.rs:382-385,561-588; .github/workflows/native.yml:43-53'
    content_hash: null
  - kind: primary-documentation
    reference: https://learn.microsoft.com/en-us/cpp/windows/determining-which-dlls-to-redistribute?view=msvc-170
    content_hash: null
  - kind: review
    reference: Read-only proposal v0.1 review on 2026-10-20; GPT-6.1 Sol / Max; source and documentation evidence only
    content_hash: null
  history: []
description: The installation study distinguished the existing prepared-host extracted-binary smoke from real consumer installation evidence.
tags:
- windows
- installation
- release
- verification
timestamp: 2026-10-20T01:24:24Z
title: Build-host smoke does not qualify installed Windows consumer behavior
type: agent-memory
---
The 2026-10-20 installation study traced released v3.0.3's native artifact path: it prepares native dependencies, runs the packaged installer with `-DryRun`, and exercises the extracted executable paths. This is useful packaged-runtime evidence but does not exercise actual destination replacement or establish operation on a clean Windows consumer environment. A green build-host smoke therefore cannot be reused as proof that an advertised prebuilt installation and upgrade work without the developer environment.

When qualifying a new binary distribution channel, inspect which paths and environment the executed checks actually cover. Preserve the existing exact-release artifact checks, then obtain separate installed-path and consumer-environment evidence for that channel. Hiding build tools on PATH alone cannot exclude runtime libraries installed elsewhere on the build host. This study executed no Windows/PowerShell consumer qualification and established no missing-DLL defect.