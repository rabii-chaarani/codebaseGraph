# COD-107 failed attempt 1 (preserved)

- Command: `cargo test --locked -p xtask docs_python -- --nocapture`
- Exit: 101
- Source SHA-256 at failure: `49cad26f6028aae47a15c2a3e3077f69e72e8b652c21687f21b4fbddae406f8c`
- Focused test-region SHA-256: `c8637b11128bed010dd118ab15f28ba90b44ae02ca202acb37941b328c2f0974`
- Failed test: `tests::docs_python_exception_rejects_component_aliases_and_symlinks`
- Exact output:

```text
   Compiling xtask v4.0.5 (/private/tmp/codebasegraph-cod107-20261031/crates/xtask)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 2.48s
     Running unittests src/main.rs (target/debug/deps/xtask-269d5b6e7ec16933)

running 3 tests
test tests::docs_python_exception_rejects_component_aliases_and_symlinks ... 
thread 'tests::docs_python_exception_rejects_component_aliases_and_symlinks' (40169472) panicked at crates/xtask/src/main.rs:3820:9:
assertion failed: super::is_docs_helper_path(&aliases, &aliases.join("docs/helper.py")).unwrap()
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
FAILED
test tests::docs_python_gate_accepts_root_and_nested_helpers_in_both_modes ... ok
test tests::docs_python_gate_rejects_neighbors_root_tooling_and_workflow_python ... ok

failures:

failures:
    tests::docs_python_exception_rejects_component_aliases_and_symlinks

test result: FAILED. 2 passed; 1 failed; 0 ignored; 0 measured; 76 filtered out; finished in 0.08s

error: test failed, to rerun pass `-p xtask --bin xtask`
```

## Causal diagnosis

The failing assertion expected an absolute candidate beneath a temporary fixture root to qualify. On macOS, the fixture root was lexically under `/var/folders/...`, while `canonicalize(root)` produced its `/private/var/folders/...` spelling. The first implementation compared the candidate's original spelling only against the canonical root and returned `Ok(false)` before checking the actual docs path. This was a helper's absolute-root prefix-handling defect, not an incorrect test setup or a weakened oracle. The correction first strips the supplied lexical root when it prefixes an absolute candidate, then uses canonical-root prefix matching as the fallback; subsequent canonical containment and no-symlink checks remain unchanged.

The complete attempt-1 source snapshot and unified diff against the approved starting main.rs are in `cod107-failed-attempt-1-main.rs` and `cod107-failed-attempt-1.diff`.
