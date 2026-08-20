---
agent_memory:
  version: 1
  kind: procedural
  scope: repository
  status: candidate
  owner: release-engineering
  created_at: 2026-08-17T06:18:49Z
  last_verified_at: null
  verified_by: null
  review_after: 2027-02-17
  supersedes: []
  superseded_by: null
  sources:
  - kind: github-actions-run
    reference: https://github.com/rabii-chaarani/codebaseGraph/actions/runs/32000102755/job/95298986437
    content_hash: null
  - kind: repository-commit
    reference: 31a87fe:.github/workflows/release.yml
    content_hash: null
  - kind: repository-commit
    reference: 31a87fe:crates/xtask/src/main.rs
    content_hash: null
  history: []
description: Prevent GitHub CLI release publication from depending on local Git metadata in artifact-only jobs.
tags:
- release
- github-actions
- github-cli
- workflow-policy
- publishing
timestamp: 2026-08-17T06:18:49Z
title: Checkout-free GitHub Release publishers must select the repository explicitly
type: agent-memory
---
## Reusable rule

A GitHub Actions job that intentionally omits `actions/checkout` must not rely on GitHub CLI repository inference. For `gh release upload`, pass `--repo "$GITHUB_REPOSITORY"` (or enforce an equivalent explicit `GH_REPO` binding) alongside `GH_TOKEN`.

## Failure signal

The artifact download and validation steps succeed, then `gh release upload` exits with `fatal: not a git repository`. This means the release and artifacts are valid; repository discovery failed before upload.

## Verification procedure

1. Keep the publisher checkout-free when it only consumes an Actions artifact.
2. Bind the upload to the workflow repository explicitly.
3. Add a workflow-policy regression that removes the repository binding and expects rejection.
4. Validate repository resolution from a directory without `.git` using a read-only `gh release view --repo OWNER/REPO` call.
5. Run the workflow policy, release gate, `xtask` tests, formatting, and Clippy before publication.