# COD-109 / PR160 baseline verification (in progress)

## Authority and scope
- Evidence Verifier; inherited actual host model/reasoning settings are unexposed.
- Diagnostic task only: no code edits, Git mutations, release, or primary checkout writes.
- Exact execution source: `/tmp/codebasegraph-pr160/worktree`, head `12fdbb28f7b44185b8de695694c02d734555caff`, clean at inspection; tree `767a0292d8a91308d9d2d54db05e21fb24a60ca1`.
- Production establishes failed CI synthetic merge `bec2d8089f64efd7f3198a88d6a03830e973a5e3` has the same tree. I independently confirmed local head/tree and the downloaded report source SHA.

## Contract and discovery
- Graph discovery: semantic search and explicit callgraph context found `tests/coordinator_process.rs::twenty_mcp_clients_share_one_coordinator_worker_and_take_over`. The graph is bound to an older same-repository checkout; exact worktree source, not graph offsets, is used for execution.
- k-wiki: `knowledge:architecture/worker-recovery-regression-contract`; active repository memory recalled. Scryer `orient` inspected exact test ownership/claims, and `get_rules` fetched verification/evidence rules. Scryer is also bound to the original checkout and shows preexisting drift/pending intent; no model writes or test-ingestion claims are made here.
- Relevant existing requirements: PR145-TREE-R2 transient resources release only after verified teardown; R3 unknown containment preserves authority/evidence and blocks conflicting writes; R4 verified teardown permits recovery. Acceptance TREE-A2/A3 covers owned-tree exit and preserved unknown containment. The coordinator test attaches existing resp-260/263/264/265/275/278/279/280/281.
- No timeout, resource threshold, assertion, fixture, or acceptance constants changed.

## Exact test and entrypoint
- The test starts20 shipping MCP stdio clients, observes one isolated worker, checks resource thresholds, verifies edited content, kills the elected owner, verifies takeover, excludes new births with actual worker/writer locks, snapshots old physical workspace incarnations, then deliberately kills remaining clients.
- After a new client publishes a graph, `registered_work_is_terminal_at_quiescence` obtains real worker/writer/workspace observation leases, checks workspace binding/selector/slot digests and sequence equality, and requires terminal registered control. Its narrow owner-loss exception accepts only the captured old physical worker incarnation, unlocked lifecycle lease(s), and `containment_empty=true` receipt(s), while requiring maintenance pending.
- A higher valid recovery-intent sequence produces `control recovery remains pending` before terminal-control/old-incarnation classification. Therefore ready/current graph health is not equivalent to workspace-owner closure. CI's final health cannot satisfy or waive that oracle by itself.
- CI entrypoint: `cargo run -p xtask -- native-test --target macos-arm64 --cohort integration`. It captures a normal source-bound CLI compiler artifact privately, hashes it, supplies `CARGO_BIN_EXE_codebase-graph`, and checks unchanged bytes at completion. `native-tests` inherits release, with debug assertions/overflow checks and unwind enabled. `.cargo/config.toml` serializes Rust tests.

## Existing failed CI evidence (not a new local execution)
- Run40822202389 / job123277004101, macOS26.6.2 build25G83 arm64.
- Downloaded report `/tmp/codebasegraph-pr160/ci-diagnostics/tests/macos-arm64-integration/report.json`: source `bec2d8089f64efd7f3198a88d6a03830e973a5e3`; captured CLI SHA256 `f1904c92f136081cad51ecac4864b30a3fff3e0906f14cba09cb0d339e5cc856`; `cli_unchanged=true`.
- Target test failed at `tests/coordinator_process.rs:284:28`: pending control recovery for the old worker root, despite current/ready serving health and no live worker PID. The report cannot establish why that durable intent remained pending.
- Other package integration commands and workspace doctests passed; that does not qualify the failed coordinator contract.

## Local environment and execution
- macOS26.6.2 build25G83, Darwin25.6.0 arm64; rustc1.96.0 `ac68faa20`, cargo1.96.0 `30a34c682`.
- No CARGO/RUST/LBUG build environment variables were configured at start. No active cargo/rustc build was observed before starting.
- Shared native-tests cache was APFS clone-copied to `/tmp/codebasegraph-pr160-target/native-tests`; source cache not modified and its binaries were not accepted as provenance. Cargo exact-source compilation establishes the artifact used below.
- Build command: `CARGO_TARGET_DIR=/tmp/codebasegraph-pr160-target cargo test --locked --profile native-tests -p codebase-graph --test coordinator_process --no-run --message-format=json-render-diagnostics`.
- Build passed (exit0), compiler reported33.33s, wall33.415s. Normal CLI and exact test were rebuilt (`fresh=false`). Source hashes unchanged after build.
- Normal compiler artifact: `/tmp/codebasegraph-pr160-target/native-tests/codebase-graph`; private capture: `/tmp/codebasegraph-pr160/baseline-evidence/captured-cli/codebase-graph`, SHA256 `f2fc23004b4138f7ce56a1121c0caa113615f58d2c5ec16fd8bf67c0b3e5b0a1`. Test binary: `/tmp/codebasegraph-pr160-target/native-tests/deps/coordinator_process-0ab63c88e81094e5`, SHA256 `e4ba189ca1bc8b14d9ebb62176294ca6396bb9d2498f39d65bcc05c95beaeacc`. `baseline-evidence/capture.json` records profile/source binding.
- First invocation stopped at shell parsing before Cargo (exit127) because the hyphenated runtime override name cannot use shell assignment syntax. Preserved in `baseline-evidence/coordinator-invocation-error.log`; this did not execute the test.
- Corrected focused command: `env CARGO_TARGET_DIR=/tmp/codebasegraph-pr160-target CARGO_BIN_EXE_codebase-graph=/tmp/codebasegraph-pr160/baseline-evidence/captured-cli/codebase-graph cargo test --locked --profile native-tests -p codebase-graph --test coordinator_process twenty_mcp_clients_share_one_coordinator_worker_and_take_over -- --exact --nocapture`.
- Corrected exact regression **failed**, exit101, wall57.249s / test56.43s; 0passed,1failed,1filtered. Log: `baseline-evidence/coordinator-focused.log`; machine-readable result: `baseline-evidence/result.json`.
- It reached the same final owner-closure assertion at line284, but with **`preexisting owner-loss containment unproven`**, not CI's pending-intent reason. Current/ready health, pending=false, worker=null, cleanup_pending=true were again visible. Resource gates passed (follower p95=12,386,304; owner idle=18,513,920; owner growth=901,120; worker peak=166,002,688 bytes).
- Source hashes and captured CLI hash stayed unchanged. No rerun was attempted. This reproduces an owner-closure failure family, not the exact CI control-intent interleaving.

## Passive residual state after test cleanup
- Fixture root: `/private/var/folders/3y/n_wglyqn3bv8xy8s0b9p32d00000gn/T/codebase_graph_coordinator_process_34319_0`.
- Failure root: `storage/workers/worker-34320-1793259113475860000-1-34320-1`; stable physical identity `(dev16777232, ino76470104)` at observation.
- Its workspace binding, selector and active slot checksums verify. Selector sequence0 / active slot0 / Building phase0 / closure0; no valid pending recovery intent. Other seven registered roots have terminal phase4 and closure1/2 with valid control.
- Its receipt reports cancelled=true, success=false, containment_empty=false, with `containment_unknown: killpg for owned worker34519: Operation not permitted (os error1)`. Receipt SHA256 `38c6fc584a03a7d1094ba46173ddc22978c20ad50c68952582a2ac3182fb1ac9`.
- All actual worker/writer, root and lifecycle locks could be acquired noncreating/exclusive after test cleanup and were immediately released. No remaining target PIDs were observed. Numeric-PID absence and unlocked leases are diagnostic facts, not authenticated empty-containment proof, and were not used to authorize mutation.
- Full public digests/sequences/paths/locks: `baseline-evidence/residual-locked-snapshot.json`. Additional passive public facts and process observation are retained alongside it. No private capability bytes copied into evidence.
- Local unknown-containment behavior must not be “repaired” by treating the false receipt as empty or relaxing final owner closure. Backend must separate actual retained runtime authority from test preconditions that deliberately kill owners mid-build.

## Bounded next verification
- Await backend's diagnosis and proposal. Prefer deterministic valid pending-intent/owner-loss fixtures that preserve unknown containment and require verified teardown for closure, then execute the exact coordinator regression on the approved final snapshot.
- Keep the baseline failure and CI's different reason visible; a passing repeat would not settle either cause. No open-ended suite run is justified in this diagnostic phase.

## Focused owner-kill timing finding and scope check
- Exact trace: edited generation change/search visibility at `tests/coordinator_process.rs:163-196` → abrupt first-owner `kill_pid` at198 → replacement election → worker/writer snapshot observations at212-228 → abrupt `kill_all` at229. Thus the existing locks do not cover first-owner death.
- `src/materialization_worker.rs:135-139` holds the real worker lease through execution; successful result/publication at277-295 precedes `finish_empty_worker_workspace`. Unknown supervision returns at263-265 without terminal closure, as required by the guard comment564-570. Search visibility alone is not a teardown boundary.
- The failed local root belongs to original owner34320/edit build#1, not the replacement. `result.json` SHA256 `8252df227c41450d1e7b133373d7634d8d428932bd2ba30a8873394967d250e1` is Ok for active generation `34519-1793259114010563000-1`. That generation's manifest SHA256 `6eef357d638179bb0a86d6b73c9073104ed363acbeea43bae5247938911ac42a` records `src/lib.rs` hash `b6079da826011cf7c1035e0bc4857d598540fa2c9575204b5150acd2b4ab5cff`, exactly the SHA256 of the fixture's `coordinated_graph_v2` edit. Public matching evidence: `baseline-evidence/edited-generation-match.json`. No capability bytes are included.
- This establishes publication of the edited generation before the first-owner loss with later false-empty receipt/Building residue. It supports an accidental active-cleanup kill window, not proof of CI's pending-intent root cause.

### Canonical scope and preservation of failure coverage
- No separate canonical spec names this exact test or requires that its takeover kill hit a live worker. Scryer committed `node-209` attaches it to routing/publication/client-memory resp-278/279/280; none says takeover must interrupt an active build. The test itself waits for changed publication and searchable content before killing and has no positive active-worker/phase fault barrier.
- The current approved resource-ceiling amendment `docs/specs/CBG-COPY-01/resource-ceilings-amendment.md` R1 (approval full SHA `c8dd4ea9ca2a6f992756a2a41828396a27eabb6d`) explicitly preserves owner-loss/cancellation/unknown evidence (RM2,RM5,RA2,RA4) and forbids weakened unrelated assertions (RA5).
- k-wiki `architecture/worker-recovery-regression-contract` PR145-TREE-R1/R2/R3/R4 and TREE-A2/A3 require active contained-tree teardown and unknown retention overall. Their named active-fault fixtures are separate from this test. Failed CI inventory confirms normal integration coverage includes `lifecycle_faults::owner_exit_cleans_paused_worker_and_loader_before_recovery`, `owner_exit_after_state_before_start_cleans_gated_tree`, `owner_exit_before_guardian_readiness_cleans_gated_tree`, `paused_database_loader_exits_after_outer_worker_failure_and_refresh_recovers`, `authenticated_cancellation_cleans_loader_and_retries_pending_edit`, `unavailable_guardian_preserves_paused_tree_evidence_until_emptiness_is_proven`, and `standalone_owner_loss_preserves_candidate_while_guardian_is_unavailable`. These tests must remain unchanged and applicable final evidence must still execute active/unknown paths.
- Bounded recommendation: before the first abrupt kill, await the actual worker/writer lease boundary and strict terminal-control/no-pending-intent checks under observation locks; hold the exclusion through kill so no new build starts. Retain crash takeover and existing deadlines, later process/failure coverage, and final strict oracle. Do not substitute current/ready, dead PIDs, elapsed time or relaxed receipts. This makes an existing post-publication takeover deterministic; it does not qualify active-worker death by itself.
- If the coordinator interprets this test as specifically requiring active-build death, that intent is not stated in the found canonical material and must be resolved before changing its trigger. Keep dedicated active-tree tests regardless. A potentially real terminal publication/runtime defect remains separately diagnosable and cannot be hidden by ignoring pending control intent.
- Full focused read-only analysis: `/tmp/codebasegraph-pr160/fixture-owner-loss-review.md`. No additional builds or code edits performed.

## Qualification limits
- One new exact native macOS baseline failure is recorded. No Linux/Windows qualification and no final fixed-revision verdict. No Scryer ingestion because the configured model/source is not the isolated execution revision. No code change or acceptance weakening permitted. Reusable-knowledge assessment: no new memory candidate; this single unresolved local outcome does not yet justify a distilled root-cause claim.
