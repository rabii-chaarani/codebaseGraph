# PR164 CI release-policy repair — independent review

## Verdict

**PASS for the proposed docs-only policy repair. No blocking finding.** This restores the existing Rust-only policy without changing it. It does not establish that the complete hosted cargo-publish dry-run job passes on a future commit.

Reviewed the working-tree repair against `f2936a4c956c3cd90a45089dc7f7f041e7aef477` in `C:/Users/rcha0053/Repositories/cbg-pr164`, after the root agent completed the rename and supplied both reproduction logs. This was read-only: no builds, tests, source edits, pushes, new PR, policy exemptions, or global changes were performed by this reviewer. The only file written by this reviewer is this review outside the repository.

## Findings and checks

1. **Failure attribution is appropriately bounded.** Public `job.json` identifies failure in **Check repository release policy**; package verification and size checks were skipped. The local before log reports exactly `FAIL: python-file-present: docs\specs\CBG-PLAT-01\execution\pr164-windows11-evidence\assert-supplemental.py must not be maintained source.` The reported command is `cargo run -p xtask --locked --offline -- release-gate`, before exit 1 and after exit 0 with `release gate passed`. Hosted raw logs were unavailable; the review does not represent the local diagnostic as a downloaded hosted log.

2. **The policy is unchanged and the correction is not maintained tooling hidden behind another extension.** Graph discovery of `release_gate` located `crates/xtask/src/main.rs` before source inspection. The actual reviewed `check_rust_only_files` rejects `.py` paths across documentation too. The patch changes documentation/evidence only, not this implementation, workflows, dependencies, product behavior, test selection, acceptance, constants or timeouts. The original AGENTS was read; read-only wiki recall and graph dependency context were consulted. The policy source SHA256 is `9e6cd94b73c66018acf683136bd61baaa9de09cee467cbb22b15638b8843b47f`.

   The new report explicitly identifies the `.txt` file as an immutable source transcript of a **completed external one-off check**, not a reusable repository command. Its current references are explanatory documentation, not invocation instructions. The only retained invocation record is `commands.json`'s original external `C:/Users/rcha0053/pr164-qualification/assert-supplemental.py` path. Preserving that history is correct. No current invocation was retargeted to the `.txt` archive. The Git mode is `100644`, it has no shebang, and no tracked `.py` or non-target/non-Git worktree `.py` file remains. Python syntax remains as archived evidence, not an installed, workflow-wired, or maintained Python tool. A rename alone would not satisfy this distinction; the observed immutable evidence role and absence of current consumers do.

3. **Evidence identity is preserved.** The rename is `R100`; both the baseline `.py` and renamed `.txt` have Git blob `05fcec58fd510e05d0c71680cba658e1e567afa8`. The archived source is 2,602 bytes, byte-identical to the external original, SHA256 `796f0522664831dcdf79ab67e4e3f8e7c29470d6e4821283dac7c2461f33c17c`. The original evidence manifest changes only this pathname, not its digest. All 54 original evidence checksum entries independently validate. The historical commands and execution outcomes are not rewritten. Existing qualification counts remain 21 selected units, 6 integrations and 6 supplemental checks; this repair does not rerun or expand those claims.

4. **Packaging remains separately scoped.** The supplied `cargo package --locked --offline --list` inventory and `verification.json` report exit 0. The inventory has no `docs/specs` or qualification archive paths (including Windows-separated spellings). This supports that the evidence is not shipped in the root crate, not a claim that `cargo publish --dry-run` itself was rerun or passed.

## Publication guardrails

- The repair note now records the completed local gate, identity/checksum and package-inventory results without claiming hosted success. Publish the supplied before/after logs, metadata, this review and their integrity manifest alongside it.
- Check the hosted job on the new commit before claiming full CI success. Current review approves the reproduced local release-policy repair only.
- Keep this transcript immutable. If an assertion checker becomes supported or maintained repository tooling later, implement it in the repository's Rust toolchain rather than invoking or evolving the archived Python source.

## Assumptions and limits

The completed-external-check status stated in the report is the intended continuing role of the archive. This review inspected the supplied logs and verified source/evidence identity; it did not independently rerun them. No constants were changed. No durable repository memory was recorded: the findings are PR-specific review/publication evidence, and the task is read-only.
