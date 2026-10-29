# COD-105 Implementation Handoff

## Profile and authorization
- Required implementation role read: `codebasegraph-backend-implementation` at `/Users/rabii/Projects/Repositories/skills/codebasegraph-backend-implementation/SKILL.md`; actual child profile confirmed by Engineer: `openai-codex/gpt-6-luna`, `thinking=xhigh` (session metadata itself did not expose it). No substitution.
- Implementation approval: user “approve”, 2026-10-29, for combined COD-103 rev0.2, full SHA `7e79f7e924536f29d3212e83899b01c38afd39de`; exact record `docs/specs/COD-103/approvals/2026-10-29-combined.md`, approval record `383a481bd5a71c58dab7dd796ace92a365760da5`; proposal focused review R02 passed that exact SHA (`2e336a121e999cf7d122a2705a5a007d4dc6326c`). COD-105 is limited to C103-R01/A01 and first stack foundation as authorized by `chg-ahwwmf`.
- Worktree `/private/tmp/codebasegraph-cod103-cod105-20261029`, branch `cod103-stack/cod-105`, starting HEAD `c8b8e7395c0a83bb8a2fe53a5add97df0d9cf617`; main baseline `5889ed09954a4a07efd2590dd1bd380ee77be06a`. No Git operations performed.

## Files and hashes
Only authorized files changed:
- `crates/xtask/src/main.rs` SHA-256 `652dbeb10cfbeabef04b61fa2429356be7155f70be90d05822ff0d278df985d4`
- `.github/workflows/ci.yml` SHA-256 `98f659e98ad2060f503785390bc810b453a3eb23518b372e85122d2090f1fcda`
- `docs/release.md` SHA-256 `88efe80f119558fdc3e913211c4ebdde11a131a9d5b349248cb1b5a4b8e0c5a3`

## Implemented behavior / constants
- MCP packaged smoke now writes the same initialize ID 1 and tools/list ID 2 requests, drops stdin, drains stdout+stderr with `wait_with_output()`, requires successful child exit, and validates newline-delimited JSON-RPC 2.0 responses. It requires one valid ID 1 initialize result with protocolVersion/capabilities/serverInfo and one ID 2 result whose `tools` has an actual `name == graph_search`. Errors, malformed/non-object data, invalid response shapes, duplicate expected IDs, missing/wrong IDs and description-only decoys fail. Nonzero child status fails before considering convincing stdout. Write failure kills and reaps the child.
- Added pure validator `validate_mcp_smoke_output(&Output) -> Result<(), String>` plus portable native `ExitStatus`-based tests. New cases: `mcp_smoke_accepts_valid_initialize_and_tools`; `mcp_smoke_rejects_nonzero_child_with_success_payload`; `mcp_smoke_rejects_errors_missing_malformed_duplicate_and_decoy_responses`.
- CI ordinary `pull_request` admits exactly branches `{main, cod103-stack/cod-105, cod103-stack/cod-106, cod103-stack/cod-56, cod103-stack/cod-107}` and activities `{opened, synchronize, reopened, edited}`. `push` remains exactly `main`. Typed workflow policy validates PR bases separately from push bases, validates exact activity sets, rejects `pull_request_target`, and preserves the remaining existing workflow/required-job policy. Added policy tests `workflow_policy_accepts_exact_cod103_stack_bases`, `workflow_policy_rejects_unknown_stack_base_and_push_namespace`, `workflow_policy_rejects_privileged_pr_event`, and `workflow_policy_rejects_unapproved_pull_request_activity`.
- CI still runs the full existing job set for all PR bases; native jobs still upload only on `push`, and normal main-only release `workflow_run` behavior is unchanged. No new secrets, permissions, privileged events, PR artifact retention or release workflow edits. Documentation states stack bases and activities, full verification, main-only pushes and staging-not-qualification.

## Executed verification
Environment: macOS ARM64 developer worktree; cargo/rustc supplied by assigned repository environment; Cargo lockfile honored. No installs or dependency changes.
- Initial `cargo test --locked -p xtask`: exit 101, 50 passed / 1 failed because the newly written YAML test fixture rendered bare `*` as an alias. Raw failure was test-fixture parse error, not a product-policy assertion. Corrected fixture to quote wildcard. This historical failure is disclosed; final passing run is after the subsequent source edits.
- Initial strict xtask Clippy exposed two `useless_format` test-only warnings; replaced those calls. Final tests/Clippy run is warning-free.
- Final serialized check command: `cargo fmt --all && cargo fmt --all --check && cargo test --locked -p xtask && cargo clippy --locked -p xtask --all-targets -- -D warnings && cargo run --locked -p xtask -- check-workflows && cargo run --locked -p xtask -- release-gate` — exit 0. `cargo test`: 51 passed, 0 failed; it includes all eight new cases listed above. Strict Clippy exit 0, no warnings. Workflow policy command printed `workflow policy passed`; release gate printed `release gate passed`. `git diff --check` exit 0.
- Earlier separate `cargo fmt --all --check` exit 0; final aggregate included it. No nextest/JUnit report generated; `cargo test` was the explicitly assigned native command.

## Not executed / remaining gates
- Did not build or smoke actual packaged artifacts on macOS ARM64 and Windows x86_64; unit validator tests are not A01 artifact qualification.
- Did not execute hosted main-base PR CI, non-main-base stack PR CI, retarget/freshness event evidence, final independent review or confirmed-fix verification. COD-105 handoff can be staging-only only after those separate actual gates; COD-106’s later hosted full-CI proof remains required and pending.
- No Windows host run, publication/release, main integration, PR, merge, Linear/Scryer/wiki/settings write, or Git operation. Existing authorized non-bypass merge and Windows desktop/platform qualification gates remain unchanged and are not claimed passed.

## Assumptions and constants
- Reused exact approved five-base literal chain and four activity types from frozen design/approval; no wildcard namespaces or extra bases. `push: main`, `release.yml` main-only workflow-run, job graph, `required` context and full mandatory CI behavior remain as baseline.
- Kept existing smoke protocol negotiation/request values and generic initialize success shape; did not constrain supported protocol-version strings or alter product API.
- Build-once artifact/provenance and current two-target (`macos-arm64`, `windows-x86_64`) platform scope unchanged.

Awaiting fresh final reviewer and owner decision on any findings; implementation writes are frozen.
