# COD-107 Backend follow-up handoff — native retention

## Authority, ownership, and exact snapshot

- Work: COD-107 / C103-R03→A03, using existing `resp-14`/`resp-15`; Scryer binding remained only to existing `chg-xd2jtt`. No claim, topology, directive, source-map, model, Linear, JUnit attachment/ingest, fold, or Git action was performed.
- Existing approved contracts: COD-103 revision 0.3 full SHA `316be4c4bb81d4f6e025016b12374836b4dcc1a8`, separate approval record SHA `0e6008a1f46679a86aba85ac513beb05af94b0c0`, user decision `c8690556-e836-428d-bc81-079c701d2395`. Separately approved retention method freeze SHA `e5adebfa1196a2074527cec4935522c43c773e39`, approval record `08d2c060bba8bf45db069fe53185f06f5b3d6cd9`, user decision `5b4b5aad-cbe8-4265-8db9-5ea351b7f386`. Designer contract `/Users/rabii/.prime/agent/session-artifacts/01a18f18-3db0-73ba-a45a-f25faf5369ad/sub-18ebc839/cod107-strict-retention-contract.md`, SHA256 `d8556080b4b817522f12fed46e014b286ee368ce531f2f3ea6cdc7033fbbf64d`. Actual CI Production handoff `/Users/rabii/.prime/agent/session-artifacts/01a18f18-3db0-73ba-a45a-f25faf5369ad/sub-2f3ce125/cod107-retention-yaml-handoff.md`, SHA256 `e7453e47e7673e2e347992d068a6a4dce123c32e72166776c6b5a90b09382955`.
- Worktree `/private/tmp/codebasegraph-cod107-20261031`, branch `cod103-stack/cod-107`, HEAD `08d2c060bba8bf45db069fe53185f06f5b3d6cd9`, tree `09552870917f232f59005cd73e41c788b0163dca`. No Git mutation. Current final `crates/xtask/src/main.rs` SHA256 `47558f4ce54d15f874c5680fb90b9e844afa612ae50de180e45c2d308356e2c6`; `docs/release.md` SHA256 `1e67f4949647c10a48f9184db230454572d666a756b68c203690398f525c7954`. Production-owned `.github/workflows/ci.yml` readback SHA256 `3e76687feaff8021d1edc6e0d91b77444b2196e1430bd346bb232b87049cbda9`; read-only `.github/workflows/native.yml` SHA256 `cc4d668cb0dbc0d164e3f7a687f76f4020aeb5ef02b36eb56591521c75c3d683` unchanged.
- Exact scope remained only `main.rs` and `docs/release.md`. Production’s `ci.yml` change and the three separately Requirements-owned canonical documents/decision were preserved and not modified by this writer. No Cargo manifest/lockfile, native.yml, artifact/build/YAML, setting, permission, or source-policy change.
- Host had accepted `gpt-6-luna/xhigh`; fresh/default non-fork context. Runtime model metadata was not exposed. No nested dispatch.

## Implementation

- In `main.rs`, `native_retention_policy_valid` at lines 1316–1456 is an in-memory typed YAML wiring predicate called by existing `check_workflow_policy` at line 1457. It verifies the selector’s Ubuntu/single Bash step/output wiring and no job/step conditions, action, permissions, or env override; native has only the selector prerequisite and no retention-based skip; the target matrix, source SHA, run-tests input, direct `fromJSON` boolean upload binding, retention 90, reusable native boolean/number inputs, upload action/guard/name/path/missing-files behavior, and diagnostic retention 3 stay correct; and the required aggregate still has exact six dependencies, `always()`, and unchanged actual Bash/jq body. It does not parse arbitrary Actions expressions or duplicate retention decision logic in Rust.
- Added tests in the existing `#[cfg(test)]` module:
  - `workflow_policy_retention_wiring_is_closed` (line 3661): real YAML parsed through existing typed helpers, with wiring mutations killed (including absent selector dependency as static policy). It also exercises required-aggregate source preservation and unchanged native tests-before-artifact policy.
  - `native_retention_actual_body_is_cod107_only` (line 3890, Unix): extracts `decision.run` from actual parsed `ci.yml`, executes the exact Bash/jq body with isolated event/output files and cleared child environment. It covers 9 selected positives (main push and 4 permitted PR activities × both exact bases) and 30+ negative/invalid decisions including other heads/bases, case variants, forks/repo mismatches, bad activities/events, malformed/non-object/multiple JSON, missing runner inputs/files, output write failure; asserts exact `retain=true|false` outputs for valid decisions. Three semantic mutants (remove same-repo predicate, broaden base predicate, case-fold head identity) are each distinguished by the registered negative oracle.
  - `native_retention_invalid_input_cannot_succeed` (line 4276): dedicated malformed/non-object/multiple-input rejection checks; no valid output accepted.
  - `required_rejects_retention_selector_failure` (line 4305): executes the real existing aggregate Bash body with all six needs. Successful output false succeeds; present selector failure/cancelled/skipped or missing result and each other failed need reject. Entire selector-key absence is documented/tested as not rejected by unchanged runtime body; exact-needs typed/static policy rejects its missing dependency.
- Docs at lines 34–53 explain the COD-107-only internal access purpose, no artifact promotion/publication/new permissions, same-source native archives/sidecars/provenance availability, full native verification for all PRs, unchanged main retention 90 days, and all other PR retention disabled. Existing optional docs-helper text remains.
- New constants are test-only input/output fixture naming and explicit policy literals already settled in the approved contract. Existing main retention 90 and diagnostic retention 3 remain unchanged.

Exact two-file diff `/private/tmp/cod107-backend-output/cod107-retention-final-two-file.diff`, SHA256 `4ac0521ef4c39af5c0d8722d96b728d893015c9736adb827a888421b6efead0b`; frozen source/docs copies are in that directory. `git diff --check -- crates/xtask/src/main.rs docs/release.md` passes. Working status also includes Production’s `.github/workflows/ci.yml`; it is not included in the two-file diff or attributable to this worker.

## Executed evidence

Environment: macOS ARM64 (`aarch64-apple-darwin`), Rust/Cargo 1.96.0; Bash 3.2.57 and jq 1.7.1. `xtask` dev/test profile, no explicit features (xtask declares none), commands locked. No installation/dependency operation.

After latest source edit:

- `cargo fmt --package xtask -- --check` — exit 0.
- `cargo test --locked -p xtask native_retention_ -- --nocapture` — exit 0; 2 passed (selector body and invalid input).
- `cargo test --locked -p xtask workflow_policy_retention_wiring_is_closed -- --nocapture` — exit 0; 1 passed.
- `cargo test --locked -p xtask required_rejects_retention_selector_failure -- --nocapture` — exit 0; 1 passed.
- `cargo test --locked -p xtask` — exit 0; **83 passed, 0 failed, 0 ignored**.
- `cargo clippy --locked -p xtask --all-targets -- -D warnings` — exit 0; no warnings. A first strict-Clippy pass surfaced `too_many_arguments` for the test runner and `type_complexity` for mutation tables; refactored to a fixture-input struct and local mutation alias without suppressions, then reran and passed.
- `cargo run -p xtask --locked -- check-workflows` — exit 0, `workflow policy passed`.
- `cargo run -p xtask --locked -- release-gate` — exit 0, `release gate passed`.
- Exact actual Bash body tests verify case-sensitive identity and valid false exit 0; malformed/unreadable/runner-input/output failures are nonzero and do not write a valid output. Required body fixture explicitly confirms selector-key absence is not rejected at runtime, so no unsupported runtime claim is made.

Windows Bash/jq/host execution remains unavailable. Unix-only tests are not Windows passes; later Windows/native CI and actual hosted same-source both-target archive retrieval/sidecar/provenance plus root/wiki crate payload inventories remain independently required. No `.crate` or native package/archive was built or inspected here, no hosted CI was run, and no A03 acceptance/staging/PR completion is claimed. The actual output file harness used ephemeral temporary files; it did not edit or commit a workflow artifact.

## Preserved failures/limitations and next owner

The earlier COD-107 feature baseline remains preserved in `/private/tmp/cod107-backend-output/cod107-backend-handoff.md` with original failed-attempt-1 source/diff and exact initial source hashes; this follow-up’s implementation changes its frozen hashes but did not overwrite that baseline record.

During this follow-up, a temporary first version of the test-only predicate helper rejected actual YAML because the helper was compared in pieces and initially omitted the required job wiring in its helper predicate. The actual closed test caught it; the helper was corrected to validate its scoped typed fields independently and the wiring mutation caught regressions. Test harness authoring also had intermediate escaping/tuple and Clippy errors; each was corrected in the assigned file, and the final focus/full checks above passed. No workflow or production selector behavior change was needed or made. The failure was a backend policy/test implementation defect within assigned ownership, not a defect in Production’s frozen YAML.

Next: the independent Evidence Verifier should execute the required independent acceptance/JUnit/archive inventories against exact final source; fresh Final Reviewer should adversarially review the final integrated diff and confirmed fixes. Production owns Git, related PR/evidence persistence, hosted full CI and exact same-source native artifact readback. Parent/Engineer owns any later stage decisions. This handoff does not grant more writes; source/Cargo are now stopped. No reusable memory entry was warranted.
