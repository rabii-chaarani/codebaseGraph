# PR164 macOS unit failure: completed execution evidence review

## Verdict

**Accept the completed local Windows evidence for the reviewed test-only patch. No source or runtime blocker found in the selected scope.** This is not a strict Clippy pass, full unit/workspace run, native macOS qualification, or confirmation of the original macOS root cause.

## Source and binary identity

Recomputed SHA-256 directly from both current source files and the actual executable:

| Item | SHA-256 |
| --- | --- |
| `src/adapters/cli/tests/mcp.rs` | `0b3fdd1a58939c3dc28ab04fd528fb68ea0dd893c1445c99815036f1e9f1f651` |
| `src/coordinator/tests.rs` | `a57902231b187e47b48188156eeb9133d7d431470bd4a9c6bc17633a40515d75` |
| `codebase_graph-7ed996dfd9332328.exe` | `70be058acc52129ad3eaa08065424e674c969d638c7c9ef93471eae5be5def56` |

Both source hashes match `integrated-review.md`, `final-identity.json`, `source-hashes.json`, and the final build command record. The executable is 46,150,656 bytes and matches `binary.json`.

Parsed `final-build.log`: the final compiler artifact is this exact executable, built with `bundled-windows-extensions` under the native-tests optimized test profile; its artifact record is not fresh/cached. The log ends with `build-finished` success true. `commands.json` records final build exit 0. The intermediate build is explicitly labeled nonfinal, so it is not substituted for this final evidence.

## Executed scope and result

`affected-tests.log` records:

> 37 passed; 0 failed; 0 ignored; 0 measured; 808 filtered out; finished in 159.60s.

The 37 executed names match `selection.json` exactly. The final compiled inventory contains 845 tests: 37 selected plus 808 not selected. The command selects the CLI MCP and coordinator modules and runs serially with `--test-threads=1 --nocapture`; `commands.json` records exit 0.

Confirmed passing entries include:

- original `mcp_stdio_serves_tools_and_tool_errors`;
- new exact typed-busy matcher test;
- new real-maintenance-permit admission/release regression;
- existing ambiguous-disconnect no-replay test;
- existing deadline/actual-permit retention tests;
- existing real execution/maintenance mutual-exclusion tests.

Several existing panic-path fixtures print panic text under `--nocapture`, then report `ok`. The final result has zero failures. These messages must not be reported as runtime test failures.

The deterministic regression supports the independently reviewed admission mechanism. The ordinary stdio pass does not establish that this execution exercised a busy retry, and no original macOS response is available.

## Formatting, release policy, and Clippy boundaries

- `commands.json` records exit 0 for cargo fmt check, the release gate, and scoped diff whitespace check; `fmt-policy.log` includes `release gate passed`.
- Strict Windows Clippy **failed**, exit 101. `clippy.log` preserves four diagnostics: `needless_return` and `unnecessary_map_or` in `src/db_writer/access.rs`, `useless_conversion` in `src/storage/locks.rs`, and `permissions_set_readonly_false` in `src/storage/maintenance_ownership.rs`. These are outside the reviewed test-file deltas. This review did not read the graph-missing maintenance source or rerun an unchanged baseline to independently establish when those diagnostics originated.
- A separate Clippy run passed, exit 0, with those **four entire lint categories explicitly allowed globally**, including in tests. `clippy-known-baseline-exclusions.log` records successful completion. This is not a strict pass and does not independently prove absence of those categories in the changed tests.
- No unrelated source or lint policy change is warranted to make this scoped patch appear strict-clean.

## Reporting and residual requirements

Use the following claim boundary:

> Windows: final reviewed-source build, formatting/release policy checks, and all 37 selected MCP/coordinator tests passed. Strict Clippy failed on four diagnostics outside the patch; a separately labeled run with four global category allowances passed. Native macOS rerun is pending. The archived failure's error payload is missing, so its original cause is unconfirmed.

The 15-second deadline/socket I/O policy is new and test-side only; no product constant changed. It is not an exact whole-test wall-clock or native-cancellation guarantee.

The parent corrected `validation.json` after the initial evidence read: it now records the completed 37-test pass, strict Clippy exit 101, and separately allowed-category Clippy exit 0. Also read `post-execution-identity.json`, which records unchanged reviewed sources/binary and confirms all selected names were registered. These records agree with the independent hash/count checks above. The historical `native-test-report.json` belongs to the original failed macOS merge execution and must not be presented as a repaired native run.

This reviewer ran no tests/builds, modified no repository source, and pushed nothing. The review independently read/parsed the retained evidence and recomputed source/binary hashes. The only new file written is this requested addendum. No new durable wiki memory was recorded: the historical root cause remains unconfirmed.
