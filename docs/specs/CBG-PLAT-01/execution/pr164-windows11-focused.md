# PR #164 — focused Windows 11 qualification

## Identity and boundary

- Scope: [COD-55](https://linear.app/codebasegraph/issue/COD-55), CBG-PLAT-01/S04, R04 / A07–A08; [approved rev0.11](https://github.com/rabii-chaarani/codebaseGraph/blob/8a410c7a22f5291f06a1cece5cd9e7c0be255e72/docs/specs/CBG-PLAT-01/spec.md#s04--use-explicit-windows-user-directory-and-executable-discovery-policy), [exact approval](https://github.com/rabii-chaarani/codebaseGraph/blob/949db168fcf128cc1fe6e73cb70a14db832d50b5/docs/specs/CBG-PLAT-01/approvals/cod55-windows-client-discovery.md).
- Tested source: `4e42468d904c18a5f3fd58c0809212977a561727`; isolated worktree, unchanged product source. This follow-up adds documentation/evidence only to the existing PR branch.
- Host: Windows 11 Enterprise 23H2 x64, build 22631.7652. [Host/source record](pr164-windows11-evidence/host-and-source-complete.log), [OS record](pr164-windows11-evidence/host-os.log).
- Rust/Cargo 1.96.0, MSVC 14.39.33519, SDK 10.0.22621.0; `x86_64-pc-windows-msvc`, `native-tests` profile (debug assertions/overflow checks enabled) and `bundled-windows-extensions`; locked offline dependency resolution.
- Native prerequisites reuse checksum-recorded Ladybug 0.19.0 shared and existing Anaconda OpenSSL 3.3.2. Test-only sibling DLL copies in `target/native-tests` and `deps` allow PATH-sanitizing cases to start. These are harness preparation, **not shipped assets or packaging qualification**. [Inputs](pr164-windows11-evidence/native-inputs.json).
- No product constants, real client registrations, global environment, services, merge, release or issue closure changed. Synthetic test directories and process-local environment only; real-profile resolution uses dry-run.

## Completed execution

```text
cargo test --locked --offline --profile native-tests --features bundled-windows-extensions --lib --test windows_client_discovery --no-run --message-format json
```

Native build completed exit0. Its exact CLI, unit and integration artifacts are hashed in [binaries.json](pr164-windows11-evidence/binaries.json); source inputs are in [source-hashes.json](pr164-windows11-evidence/source-hashes.json). The final invocations use [run-native.ps1](pr164-windows11-evidence/run-native.ps1) with captured native environment restored after Git Bash starts. See [commands/results](pr164-windows11-evidence/commands.json), [unit request](pr164-windows11-evidence/focused-unit-native-env-request.json), [integration request](pr164-windows11-evidence/focused-integration-native-env-request.json), [exact selection](pr164-windows11-evidence/selection.json).

| Slice | Completed result | Evidence |
|---|---|---|
| Selected native units: client_config9, client_process6, lifecycle6 | **21/21 pass**, zero ignored; 822 other units filtered, not claimed passing | [Log](pr164-windows11-evidence/focused-unit-native-env.log) |
| All Windows client discovery integration cases | **6/6 pass**, zero ignored/filtered | [Log](pr164-windows11-evidence/focused-integration-native-env.log) |
| Supplemental CLI scenarios | **6/6 checked**, all commands exit0 and recorded assertions pass | [Assertions](pr164-windows11-evidence/supplemental-assertions.log), [script](pr164-windows11-evidence/assert-supplemental.py) |

Each Rust test binary ran serially; the two independent binaries ran concurrently. Supplemental scenarios are separate CLI checks, **not six extra Rust tests**. No other integration suite, full workspace test run, new macOS execution, fmt or Clippy run is claimed here. Earlier [macOS61/61 and strict checks](pr164-unix-path-review.md) are separate evidence. The captured current-source CI snapshot has22 successful checks ([snapshot](pr164-windows11-evidence/pr-and-ci.json)); that Windows Server CI is not substituted for this desktop run.

### A07 — directory policy

The native integration case covers conflicting/absent HOME, valid USERPROFILE, CODEX_HOME, explicit file precedence, project target without a valid profile, unrelated HOME config preservation, and actual token-profile lookup with USERPROFILE absent. Supplemental dry-runs also set USERPROFILE to an absolute existing **file** and a relative invalid value. Both resolve to the same actual native profile as the missing-variable control, not HOME/CWD/repository. See [missing](pr164-windows11-evidence/profile-missing.log), [file](pr164-windows11-evidence/profile-absolute-file.log), [relative](pr164-windows11-evidence/profile-relative.log).

Injected native unit negatives verify clear failure when profile resolution is unavailable or not absolute. This is **not** a claim that the logged-in host's real token/profile was made unavailable.

### A08 — command and selected-target policy

- Native `.exe` arguments retain spaces, empty strings, trailing backslashes, quotes, CR/LF and metacharacters literally. `.cmd` and `.bat` forwarding tests capture exact argv including quotes/metacharacters and assert no injection marker. Fixed PATH/suffix order, explicit pinned paths and unsupported/ambiguous/unsafe command rejection pass in the selected process units.
- Integration add/list and fixture stdio verification plumbing pass for `.exe`, `.cmd` and `.bat`. The fixture returns five canned JSON-RPC responses; this is not real graph/server-health qualification. Native failure23 followed by file fallback preserves the relative CODEX_HOME target despite child CWD changes. Explicit custom-file visibility is deliberately skipped instead of checking a different default. Both `claude` and `claude-project` list checks use the selected project; list failure reports false, not skipped. Unsafe batch input leaves existing config byte-identical for dry-run and write modes.
- Supplemental missing-client execution uses an empty PATH: same selected synthetic user config is written by `file_adapter`, with `codex executable not found`, and the unrelated HOME config stays unchanged. [Output](pr164-windows11-evidence/missing-client-same-target.log).
- Supplemental controlled OpenClaw fixture: relative/spaced OPENCLAW_HOME, native set exit23, same-target file fallback, then successful list visibility. Relative `--client-config-path explicit space/custom.json5` wins over that override; file adapter writes that exact file, registration/list client calls are absent (canned stdio fixture verification still runs), unsupported visibility is explicitly skipped. Synthetic profile/HOME defaults stay byte-identical. [Relative output](pr164-windows11-evidence/openclaw-relative-fallback.log), [explicit output](pr164-windows11-evidence/openclaw-explicit-custom.log). The external fixture adapts the PR fixture only for OpenClaw's `set` grammar, OPENCLAW_HOME and `mcp.json5`; [source](pr164-windows11-evidence/supplemental/openclaw_fixture.rs) is evidence, not a product change.

## Retained failed attempts

The first execution selected Git's `usr/bin/link.exe` when fixture tests invoked bare `rustc`; Cargo's build used an explicit MSVC linker and had passed. Initial unit19/21 and integration1/6 results are retained, **not green evidence**. Restoring recorded native PATH/LIB/INCLUDE inside the process-only PowerShell wrapper corrected fixture compilation. The unchanged source and same test/CLI binaries then passed21/21+6/6. No product retry policy, timeout, dependency, behavior or resource-limit change was made. [Initial unit](pr164-windows11-evidence/focused-unit.log), [initial integration](pr164-windows11-evidence/focused-integration.log), [attempt details](pr164-windows11-evidence/attempts.json).

The first host probe also failed because MSYS converted `cl /Bv` into a pathname; its full output is retained separately and the corrected identity command completed exit0. This was not a test or product failure.

## Independent review and limits

[Independent criteria review](pr164-windows11-evidence/criteria-review.md) established the exact21+6 selection and recommended process-only additions. Its in-flight notes describe that review's point in time. A separate [completed-execution review](pr164-windows11-evidence/independent-execution-review.md) audits the final results, supplemental assertions, source/binary identity and limitations.

This qualifies only the executed focused source-build behavior on this Windows 11 x64 host with controlled fixtures, not actual vendor client releases, clean-machine prerequisites, archives/installers, other Windows versions/architectures, full application behavior or aggregate CBG-PLAT-01 closure. Native-profile unavailability is injected-helper coverage. No merge/release/deploy/restart or COD-55 closure is performed. Reusable-knowledge assessment: the native PATH-vs-bare-rustc pitfall and its bounded correction are retained here; no new repository memory candidate is needed for this execution-only follow-up.

Evidence raw bytes are protected by local `.gitattributes` (`* -text`); [SHA256SUMS](pr164-windows11-evidence/SHA256SUMS) binds all evidence files except itself. Narrative whitespace checks exclude raw evidence: native CRLF and trailing empty captured environment fields are preserved, not reformatted. Git index blob hashes are checked against the exact local bytes.
