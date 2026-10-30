# COD-54 / CBG-PLAT-01 S03 — final integrated review

## Outcome

**GREEN for the bounded implementation and local/native artifact handoff. No unresolved introduced P0–P3 defect was established.** Reviewed source: `855c7e282a27de4afbad2d4252d7692de4b42ecb`; baseline: `34eac4f8644adc046093babd84ee1fdbffdae853`.

This is not support qualification, release/merge approval, COD-54 closure or aggregate closure. Clean minimum/current macOS qualification and the remaining full native/hosted CI gates are still pending.

## Reviewer and authority

Fresh independent reviewer: `openai-codex/gpt-6.1-sol`, thinking **max**, child `sub-b0e5e01b`, confirmed against the coordinator’s actual spawn call; no implementation context fork. Used `/Users/rabii/.codex/skills/adversarial-review-engineer/SKILL.md` and its engineering lenses.

Approved scope: `docs/specs/CBG-PLAT-01/spec.md` revision **0.10** at `1a9db7b558b60248888408daa6268a3b45f2f0df`, **R03 / A05–A06**, approval record commit `71dc27e92857c4215bbbe583f104de7f341dc816`. Examined initial proposal review conditions **SC-01–SC-04**. PR: [#163](https://github.com/rabii-chaarani/codebaseGraph/pull/163). Neither PR creation nor this review changes closure ownership.

Product source, tests, Git and Scryer were not changed by this reviewer; this report is the only reviewer file write. No reviewer Cargo/build/full-suite execution. No constants or behavior assumptions were changed by the reviewer.

## Findings and focused fix verification

**Confirmed unresolved findings: none.** No conditional code defect or unresolved design decision was established within the authorized scope. Platform/verification limits below remain material gates, not invented product defects.

- Verified the repaired official-source extraction path accepts only the expected PAX comment metadata and does not extract it, while ordinary entries retain strict source-root/type/path validation. The current first-loop binding is mutable (`macos.rs:701`); the transient FR-01 observation from an older in-flight read is withdrawn. Inspected preserved E0596/PAX failure history and the passing PAX regression; they are not presented as final passing runs.
- Verified the installer mixed-version fixture helper now derives the version from the canonical SSL prefix/suffix; its final regression passes without weakened assertions.
- Focused review of `a2005dbb..855c7e28`: the sole source delta removes `&` from an already-borrowed `binary` argument to `change_dependency` (`macos.rs:168`). It changes no behavior and adds no lint suppression. Strict Clippy was rerun successfully.

## Contracts traced in the final source

1. **Supply/build:** `macos.rs:24–122`, `main.rs:425–461`. Exact official OpenSSL 3.6.2 source digest is checked before fresh-root extraction; same-source license is copied. Source build retains upstream defaults except approved shared/no-tests/no-apps/no-module configuration. Both Cargo binary commands receive the prepared `OPENSSL_LIB_DIR`, deployment target 13.3 and header padding; no new consumer Homebrew prerequisite or static-link/dependency/ABI change.
2. **Relocation/audit:** `macos.rs:125–238,267–337`. Binary imports are executable-relative; SSL-to-crypto and dylib identities are loader-relative. Rpaths are removed; unknown non-system imports, wrong architecture, excessive minimum OS or bad signatures fail. Mutation precedes ad-hoc re-signing/strict verification, then checksums. Native audit is confined to native macOS build/execution, not cross-target artifact-set validation.
3. **Inventory/provenance:** `main.rs:70–91,464–535,610–704,800–880`. One NativeTarget inventory produces five macOS payload checksum rows and eight regular flat archive entries. Windows retains two payloads/five entries; provenance remains schema 1. Archive and internal hashes cover final bytes.
4. **Installer:** `install.sh:82–194`. Exact categories/common constrained numeric version, no duplicate/unknown/path/missing rows, regular non-symlink payloads and all five hashes are validated before either executable probe or any destination write. Compiled probes run without inherited runtime overrides. Real install puts version-qualified runtime/license files before binaries using per-file temporary-copy/chmod/rename; dry run creates no destination. No all-file rollback or old-sidecar garbage collection is promised.
5. **Runtime evidence:** `main.rs:751–789,1779–1998,2001–2118,2321–2357,2430–2527`. Actual relocated installed pair is exercised, not just extracted binaries. All direct graph/wiki/MCP/HTTP runtime-child sites use the cleaned smoke environment and fixture home. Negative checks use the actual incompatible Mach-O load failure and compare every installed payload hash after both dry-run and real rejection.
6. **Scope:** `materialization_worker.rs` changes only the import cfg; all uses were already debug/test-only. Cargo/lock/dependency versions, extension assets/ABI, Windows behavior/workflows and process containment remain unchanged.

## Executed evidence

All evidence filenames below are under `/private/tmp/codebasegraph-cod54-review/` unless otherwise stated.

### Coordinator-executed, reviewer-inspected

- Baseline: 43 passing xtask tests. Final `final-xtask.log` / `final-xtask.junit.xml`: **55 tests, 0 failures, 0 errors, 0 skipped** after the final lint-only commit. Includes archive/checksum/cross-target contract, PAX extraction and installer dry-run/install/preflight preservation cases. Unit installer fixture binaries are shell scripts; native compatibility is established separately below.
- `clippy-final.json` / `.log`: `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`, **exit 0**, Rust 1.96.0, source `855c7e28`. Coordinator reports final formatting check **exit 0**; reviewer independently ran shell syntax and diff-whitespace checks.
- `source-bound-artifact.json` / `.log`: `cargo run -p xtask --locked -- native-artifact --target macos-arm64 --commit-sha 855c7e282a27de4afbad2d4252d7692de4b42ecb --output .../dist/cod54-source-bound`, **exit 0**, 309.5 seconds, macOS **26.6.2 / 25G83, ARM64**. It builds the official runtime and both optimized binaries, audits/signs/packages/extracts, dry-runs and really installs to a relocated spaced path, exercises graph/wiki/MCP/HTTP and rejects missing, corrupt and checksum-valid non-MachO runtime files in both installer modes with installed hashes unchanged. The negative dyld aborts in this successful log are expected preflight failures, not passing probes.
- Earlier provisional artifact runs were inspected as historical evidence only; their approval-SHA provenance labels are excluded from final artifact proof. The final run uses the actual implementation SHA.

### Reviewer-executed / independently checked

- Target-local graph semantic search/context preceded source inspection. Adjacent runtime paths were discovered with MCP graph; wiki memory/search and Scryer rules/model were consulted read-only. The configured wiki/Scryer/MCP graph refer to an older root checkout, so they are architectural/discovery context, not final-SHA behavior proof. Linear was not mutated.
- `bash -n release/install/install.sh`, `git diff --check`, and final product-source comparison against `855c7e28`: **exit 0**. Actual host read with `sw_vers`/`uname`: macOS 26.6.2 / 25G83, ARM64.
- Independently hashed the official source tarball: `aaf51a1fe064384f811daeaeb4ec4dce7340ec8bd893027eee676af31e83a04f`, matching the saved official release asset digest. Compared upstream archive `LICENSE.txt` to the supplied/shipped license: byte-identical, SHA-256 `7d5450cb2d142651b8afa315b5f238efc805dad827d91ba367d8516bc9d49e7a`; no top-level NOTICE file.
- Final archive `/private/tmp/codebasegraph-cod54-review/final-archive/codebase-graph-4.0.0-macos-arm64.tar.gz`: independently SHA-256 hashed as **`e5f32abbf8124584d5f6f2df403091c3323d16a1359a70f387fc99eeeb6e4282`**. Schema-1 provenance names exact source `855c7e282a27de4afbad2d4252d7692de4b42ecb`, version 4.0.0 and target macos-arm64 with matching filename/hash.
- Read tar directly without writing/extracting: exactly eight regular flat entries with prescribed modes; all five internal payload hashes match actual bytes and upstream license identity.
- Independently decoded all four native files’ Mach-O commands from the exact tar: thin ARM64, macOS minOS **13.3**, no LC_RPATH, both executables’ OpenSSL imports through `@executable_path`, SSL’s crypto import and dylib IDs through `@loader_path`, all other imports Apple system libraries.
- Preserved `final-archive/installed bin`: all five payloads byte-identical to the exact archive. Actual `otool -L/-D` confirms import/ID closure; `codesign --verify --strict` **exit 0** on all four native files. Independent `env -i` installed `codebase-graph --help` / `k-wiki --version` probes both **exit 0**, with controlled HOME/system PATH; wiki reports 4.0.0. These are real compiled binary probes.

## Qualification, limits and remaining gates

- **Clean-host A05 qualification remains open:** actual final installed archive execution on clean Apple Silicon **macOS 13.3 and a current macOS release**, without Homebrew/OpenSSL, has not been supplied. Deployment metadata and a developer-host pass cannot substitute. Do not close COD-54 or claim supported-platform qualification from this review.
- **Full native and hosted CI remain pending** at this report’s handoff. Passing targeted/local artifact checks do not imply those broader suites passed. Record any subsequent failures and evaluate introduced effects before merge/completion.
- Normal populated build/reopen/search exercises JSON and **disk-BM25**. The legacy-manifest branch executes real **LOAD fts** with fresh, hash-checked official bytes; native FTS indices are intentionally absent, so it does **not** prove a populated native-FTS-index query. No product fallback/assertion was weakened to hide that distinction.
- Ad-hoc signature verification is not Developer ID signing, notarization, legal certification or artifact authentication. Source/package checksums prove integrity, not a new signature policy.
- No full-file transaction, concurrent cohort upgrade guarantee, old-sidecar cleanup, static linking, source-install redesign or dependency/ABI upgrade was required or inferred. No new owner decision or waiver is implied.

## Reviewed final source hashes (SHA-256)

- `crates/xtask/src/main.rs`: `6610d3c571b2a39af7f50cc2703cb9a5cb5431573c0dd46af43dcb9669bd35dd`
- `crates/xtask/src/macos.rs`: `810255a2714e8aed31b77b26ac5260747fb68d5c6c5f85bc3e17757a97c1f0cf`
- `crates/xtask/src/macos_installer_tests.rs`: `10646723ce3db83a5a70e51769957214801d4ef7533362eab1035c31ce7fab17`
- `release/install/install.sh`: `7ae5c7c5e08fa4c025b0fe5f5b2da605caccdde1a0aefcec148a272530c260e9`
- `README.md`: `0f784e2a63656208ae19137e3d745dcc7c3f3132a404f417a3697d836c620323`
- `docs/release.md`: `6c673b23376ad03a2c1c26c2c7ab01351a56c54ed71cbbfe084b1c073d1d05db`
- `src/materialization_worker.rs`: `5124af6f9df654761bfd09cd46fbf68f9dacb4efc7d0dfc11b9b9ddcb9bc386e`

## Handoff

Proceed with the bounded implementation/local-artifact handoff, preserving the pending qualification and broader verification gates above. No unresolved reviewer defect requires a source fix. Re-review any later material source change against this snapshot. Reusable knowledge assessment: no new memory candidate is warranted; this task-specific evidence does not replace the existing repository single-artifact-recipe invariant.
