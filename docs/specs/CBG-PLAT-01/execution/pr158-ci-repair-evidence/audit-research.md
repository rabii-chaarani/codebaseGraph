# PR158 / COD-53 independent CI audit: pulldown-cmark

## Scope and finding

This is a read-only audit of PR #158 head `5f68b41cebc327072cf71be770c9e5a9d1c0dad2` for CI run 40798473142. The captured `cargo audit` job scans 223 locked crate dependencies and fails on `pulldown-cmark 0.13.4`, `RUSTSEC-2026-0414` (“Exponential event growth with `ENABLE_WIKILINKS`”), CVSS 7.5 high; RustSec says upgrade to `>=0.13.5`. Evidence: `/private/tmp/codebasegraph-pr158-ci-40798473142/baseline-audit-job.log` and `baseline-head-audit.log`.

The same scan lists `cxx 1.0.138` / RUSTSEC-2026-0202 and `tokio 1.53.1` / RUSTSEC-2026-0386 as **two allowed warnings**, not the failing vulnerability. This diagnosis does not resolve or expand scope to those warnings. No ignore for 0414 is appropriate.

## Primary sources and patch facts

- RustSec advisory page: https://rustsec.org/advisories/RUSTSEC-2026-0414
- Official advisory record: https://github.com/RustSec/advisory-db/blob/main/crates/pulldown-cmark/RUSTSEC-2026-0414.md . It documents exponential parser-event growth from empty-display wikilinks such as `[[a|]]` when `ENABLE_WIKILINKS` is enabled, potentially exhausting memory on a small document. It says this option is off by default and identifies fix #1102 / 0.13.5.
- Official 0.13.5 release (published 2026-10-21): https://github.com/pulldown-cmark/pulldown-cmark/releases/tag/v0.13.5 . Release notes say security issues were fixed and refer to #1181.
- Stable-branch security fix: https://github.com/pulldown-cmark/pulldown-cmark/pull/1181 . Original upstream fix and regression details: https://github.com/pulldown-cmark/pulldown-cmark/pull/1102 .
- Crates.io sparse index record: https://index.crates.io/pu/ll/pulldown-cmark . Version 0.13.5 checksum is `278e9268c93b9e498a2da46c34d90ba93a3255eb99009646398f34bc73ab36eb`; `rust_version` is 1.71.1, same as 0.13.4. The normal dependency requirements and feature sets are unchanged from 0.13.4. Upstream manifests: [0.13.4](https://github.com/pulldown-cmark/pulldown-cmark/blob/v0.13.4/pulldown-cmark/Cargo.toml), [0.13.5](https://github.com/pulldown-cmark/pulldown-cmark/blob/v0.13.5/pulldown-cmark/Cargo.toml).

## Consumer and minimum patch

Graph-first semantic search located the consumers in `crates/k-wiki/src/compiler/mod.rs` and `crates/k-wiki/src/render/mod.rs`; semantic dependency and change-impact contexts were requested. The registered graph is not exact-head proof, so I checked these paths in the isolated PR-head worktree. Compiler parsing uses `Parser::new(...).into_offset_iter()` (default options). Rendering explicitly enables tables, strikethrough, task lists, footnotes and GFM; it does not enable `ENABLE_WIKILINKS` or use `Options::all()`. Both graph-discovered source files are byte-identical to PR head. The application’s current parser options do not activate the advisory’s vulnerable feature, but the lockfile still triggers a valid advisory rejection and should be patched.

The sole direct declaration is in `crates/k-wiki/Cargo.toml`: `pulldown-cmark = { version = "0.13", default-features = false, features = ["html"] }`. The lock pins 0.13.4 with checksum `e9f068eba8e7071c5f9511831b44f32c740d5adf574e990f946ddb53db2f314e`; `pulldown-cmark-escape` is 0.11.0.

**Recommended minimum product-file set:** `crates/k-wiki/Cargo.toml` and `Cargo.lock` only. Raise the manifest minimum from `0.13` to `0.13.5`, preserving `default-features = false` and `features = ["html"]`; update the lock entry precisely to 0.13.5 and its verified checksum. The existing 0.13 semver requirement already permits 0.13.5, so a lock-only update can make this exact build pass; raising the lower bound additionally prevents future resolution below the fixed version. No feature, API call, ABI, or other dependency change is supported or needed by these findings. MSRV 1.71.1 is below this workspace’s Rust 1.82 floor. This is an upstream patch release with unchanged package dependency requirements/features; no compatibility blocker was found for the use-sites inspected.

## Limits and handoff

I made no product edits and ran no build, dependency resolution, or post-fix audit. This report independently verifies the baseline finding and upstream patch facts; the coordinator’s post-patch audit/test results should be recorded from their actual logs, not attributed to this audit. Official primary sources were reachable, so there is no external-source blocker. Keep the other PR158 package/release-gate diagnosis separate; it is not part of this advisory finding.
