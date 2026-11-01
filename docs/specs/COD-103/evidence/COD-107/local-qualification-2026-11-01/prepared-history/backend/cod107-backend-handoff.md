# COD-107 Backend implementation handoff

## Scope, authority, and endpoint

- Work: COD-107, C103-R03/A03; existing Scryer `resp-14` (`check_rust_only_files`) and unchanged workflow accountability `resp-15`. No topology or new claim was added.
- Approved authority: `docs/specs/COD-103` revision 0.3 and all five contracts at full Git SHA `316be4c4bb81d4f6e025016b12374836b4dcc1a8`; separate approval record `docs/specs/COD-103/approvals/2026-10-31-revision-0.3.md` at `0e6008a1f46679a86aba85ac513beb05af94b0c0`; user decision `c8690556-e836-428d-bc81-079c701d2395` (“i already approved revision 0.3”). Approved design §6 bytes matched the supplied SHA `8ae7be05e8dafe6865656dbdcbbbdf13d270cf92e51a327a58884bea8abab8a8`.
- Worktree: `/private/tmp/codebasegraph-cod107-20261031`, branch `cod103-stack/cod-107`, HEAD `f69fc133c544e2ecd16a825a1e4c8f448c8d81b7`, tree `47f5e934dc7e4fb616b193cd1bcfd26400761ab8`. No Git mutation was performed.
- Host accepted the supplied `gpt-6-luna/xhigh` override; context was fresh/default, non-fork. Runtime model metadata was not exposed. No nested agents were started.
- The connected Linear entry reported pending verification; no Linear read/write was claimed. Wiki/Scryer were read-only; `chg-xd2jtt` was the only bound Scryer change. `locate` still maps `check_rust_only_files` to `resp-14`; helper `is_docs_helper_path` has no separate claim. No sign-off, anchor, test attachment, JUnit ingestion, fold, or model edit was made.
- The user-approved native-archive retention method and its separate Requirements/Production ownership are not implemented here. No artifact-retention/YAML predicate or its tests were guessed or changed. No `.crate`, native package/archive, CI, Windows, publication, or release qualification is claimed.

## Files and implementation

Only the two assigned files were edited by this worker:

1. `crates/xtask/src/main.rs` — final SHA-256 `ec60404c430b36fce5619796b05cf0e9281ea24f04b2e5d7a513df2356091a4a`.
   - `is_docs_helper_path` (lines 1005–1097): component-based eligibility; requires the first repository-relative component `docs`, at least one child, exact `.py` extension, a regular file, no parent/root/prefix aliases, canonical containment under the real literal `docs`, and no symlink in `docs` or candidate components. Metadata/canonicalization errors return an actionable error and the gate fails closed. Absolute fixture paths handle both lexical and canonical root spellings; the no-symlink and containment checks remain in force.
   - `has_generated_component` (1098–1107) preserves `.git`, `target`, `.codebaseGraph`, and `.kwiki` exclusions by `Path::components` on all platforms.
   - `check_rust_only_files` (1108–1143) uses the exception only for `.py` candidates. Root manifests/directories and every non-docs Python path remain prohibited. The existing walker was not redesigned; it still has its prior traversal/error behavior.
   - Added actual test cases: `docs_python_gate_accepts_root_and_nested_helpers_in_both_modes` (3747), `docs_python_gate_rejects_neighbors_root_tooling_and_workflow_python` (3771), and `docs_python_exception_rejects_component_aliases_and_symlinks` (3819). They invoke `release_gate` in a child test process using a complete isolated fixture and a fixture-local marker file; neither the real checkout nor process-global cwd/environment is changed. Fixtures contain the real gate inputs (security policy, manifests, release-please config, all three workflows and files read by the legacy-surface check). The route runs ordinary and fully confirmed `--production` modes.
2. `docs/release.md` — final SHA-256 `c4267010d8fa0746b39ab28e78f85ecc8659a4b18e5e8d5b25db81316867d931`, local helper section begins line 116. It states the docs-only/support-only boundary, optional explicit Python 3 standard-library local use, no setup/dependency/manifest/helper invocation in CI/Release/product build/runtime/install, no shipped archive/`.crate` helpers, ordinary source-snapshot allowance, preserved security/workflow/root checks, and no helper/command added.

Test constants added only inside `#[cfg(test)]`: `GATE_FIXTURE_CHILD_MARKER` and Windows symlink privilege error code `1314` (`ERROR_PRIVILEGE_NOT_HELD`). No existing production constant or runtime setting changed. Assumptions: tests run on the supplied native Darwin host; Windows symlink privilege/platform execution remains unverified here.

The exact two-file diff is `/private/tmp/cod107-backend-output/cod107-final-files.diff`; frozen source/docs copies are `cod107-final-main.rs` and `cod107-final-release.md` in that directory. The main source and diff are scoped to the two assigned paths. `git diff --check` passed for those paths.

At the recorded status snapshot, the worktree also contained these separate Requirements-owner edits, which were left untouched: modified `docs/specs/COD-103/{delivery.md,spec.md,verification.md}` and untracked `docs/specs/COD-103/decisions/2026-10-31-cod107-native-artifact-retention.md`. Preserve them; do not revert, stage, or attribute them to this implementation.

## Baseline, execution, and actual results

Environment: macOS ARM64 (`aarch64-apple-darwin`), Rust/Cargo `1.96.0`, debug `dev` profile. `xtask` has no feature declarations; commands used `--locked`. No dependency was installed. Build output was in the worktree’s ordinary Cargo `target/` directory.

**Unfixed baseline, before source edits:**

- `cargo build -p xtask --locked` — exit 0; fresh `xtask v4.0.5` compilation. Baseline binary SHA-256 `1b160c91b10f4d479f9889ca031b3ebaabc580a8a0b67b24c0e543e286adaffa`.
- On `/private/tmp/cod107-baseline-fixture`, the actual unfixed CLI `target/debug/xtask release-gate` and `release-gate --production --confirm release-environment --confirm private-vulnerability-reporting` each passed with no Python file and each rejected `docs/nested/helper.py` with `FAIL: python-file-present: docs/nested/helper.py must not be maintained source.` This establishes the approved regression on both CLI routes.
- Initial fixture-copy setup referenced absent optional `src/ladybug_writer.rs`; the project’s legacy-surface check tolerates that missing file. The fixture was corrected to omit it, then the complete ordinary/production fixture passed. This was fixture setup only, not a project test failure.

**Final implementation checks (after the latest source/test edit):**

- `cargo fmt --package xtask -- --check` — exit 0.
- `cargo test --locked -p xtask docs_python -- --nocapture` — exit 0; 3 passed, 0 failed, 76 filtered. This executed all positive/negative routes and the acyclic docs→outside, outside→docs, and symlinked-docs-directory cases on Darwin.
- `cargo test --locked -p xtask` — exit 0; **79 passed, 0 failed, 0 ignored**.
- `cargo clippy --locked -p xtask --all-targets -- -D warnings` — exit 0; no warnings.
- `cargo build --locked -p xtask` — exit 0, rebuilt `xtask v4.0.5` from the final source; binary `target/debug/xtask` SHA-256 `bc7d0052c0545420f20030802c632909260fb82b8562fd0d81b476cd9801495d`.
- The final built CLI on isolated complete fixtures passed ordinary and fully confirmed-production routes with root and nested `docs/*.py`. The same two routes rejected `docs-extra/neighbor.py` with exit 1 and the existing `python-file-present` diagnostic. Exact output was `release gate passed` for positives and `FAIL: python-file-present: docs-extra/neighbor.py must not be maintained source.\nrelease gate failed` for negatives.
- Actual repo commands `cargo run -p xtask --locked -- check-workflows` and `cargo run -p xtask --locked -- release-gate` passed after the production logic was current; the later test-marker-only refinement did not change production code. No production confirmation flags were run against the actual repository.
- The actual read-only `git diff --check -- crates/xtask/src/main.rs docs/release.md` passed. No JUnit report was produced or ingested; independent Evidence Verifier owns that work.

Windows execution was unavailable in this session. The Windows-specific symlink code is present; on Windows the test reports and skips only error 1314 (`ERROR_PRIVILEGE_NOT_HELD`). Windows host/CI coverage remains pending, not passed. No full workspace/product/native tests, native archive or `.crate` build/inventory, hosted CI, or archive-retention verification was run; do not infer these from the 79 xtask tests.

## Failed attempt preserved

The first focused post-edit run was not green: 1 test failed, 2 passed, 76 filtered. The valid absolute temporary-root case exposed that `canonicalize(root)` changed `/var/folders/...` to `/private/var/folders/...`; attempt 1 compared only the canonical spelling against the original candidate and returned false. This was an implementation root-prefix defect, not a test expectation adjustment. The fix checks the supplied lexical root prefix first and uses canonical-root matching only as fallback; canonical containment, component checks, and symlink rejection were not loosened. Exact source SHA-256 `49cad26f6028aae47a15c2a3e3077f69e72e8b652c21687f21b4fbddae406f8c`, test-region SHA-256 `c8637b11128bed010dd118ab15f28ba90b44ae02ca202acb37941b328c2f0974`, original output, source snapshot, and exact diff are preserved at `/private/tmp/cod107-backend-output/cod107-failed-attempt-1.md` and the adjacent `-main.rs`/`.diff` files.

## Remaining owners and stop

- Implementation files are frozen at the hashes above. Independent Evidence Verifier, current-host native archive retention/inventory, the separate approved automation/predicate contract and any later shared policy tests, fresh Final Reviewer, Production Git/PR/CI, and all merge/release gates remain with their assigned owners. Any follow-up to main.rs/docs/release for the separately approved retention workflow requires the serialized grant described by Engineer; this worker did not start it.
- No reusable knowledge/memory entry was warranted. No claim that COD-107, A03 archive acceptance, PR handoff, CI, merge, or release is complete.
