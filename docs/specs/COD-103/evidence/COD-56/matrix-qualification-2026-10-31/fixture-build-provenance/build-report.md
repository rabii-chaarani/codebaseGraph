# COD-56 native fixture build handoff

## Scope and authority

- Assigned writable product file: `release/install/tests/native_probe_fixture.rs` only.
- Checkout: `/private/tmp/codebasegraph-cod56-20261031`, HEAD `9ed7f117bc605d5a365ab5b669d54e56182d8e07`, tree `7a3ed8cb31c6937483f96a600c2b00bfba967b83`, parent `653177127041caed55b6628ff3a596a013ef0cd4` (method freeze). The assignment's base SHA was abbreviated; I did not resolve it independently.
- The COD-103 rev0.3 source commit and separate approval are present; the COD-56 rev0.4 method record on this HEAD was read. Scryer carrier fingerprint matched `0fbf815aa736dd4d6d704bfcbe1e763462510cb9c723ebaaa1c10421cf561fa5`; the new emulated-matrix section fingerprint matched `7f5569636c4331d91c75698d881db79d789a82fda80d8777cc286bbd68ffe439`.
- Resumed only existing Scryer change `chg-2z3ngs`. `locate` reports this new test-only file has no mapped claim or boundary; no Scryer claim/source-map/anchor/report/fold changes were made. The initial `orient` also surfaced pending Release Gate work and drift in other files/scopes; these are outside the assigned fixture. Linear connection remains pending/unavailable.

## Source and checks

- Source: `/private/tmp/codebasegraph-cod56-20261031/release/install/tests/native_probe_fixture.rs`
- Final source SHA-256: `7bfd7e558803824c5594335c350f317def747b338a80d7d3dee0d59f30b7c314`
- Anchors: `mainCRTStartup` line 58; `run` line 68; `parse_environment` line 177; `write_record` line 370; `write_all` line 419.
- `rustfmt --edition 2021 --check release/install/tests/native_probe_fixture.rs`: **passed**, after the final source edit.
- No Cargo command, dependency install/download, VM command or guest execution was run.

## Build provenance

- Host: `aarch64-apple-darwin`.
- `rustc 1.96.0 (ac68faa20 2026-05-25)`, LLVM 22.1.2; compiler path `/Users/rabii/.rustup/toolchains/stable-aarch64-apple-darwin/bin/rustc`; SHA-256 `c5922366bfe3d6d028a65d626f4e629b3adad066995cf0b60c8a4b617bba5ffe`.
- Target: installed `x86_64-pc-windows-msvc`; target libdir `/Users/rabii/.rustup/toolchains/stable-aarch64-apple-darwin/lib/rustlib/x86_64-pc-windows-msvc/lib`.
- Linker: `/Users/rabii/.rustup/toolchains/stable-aarch64-apple-darwin/lib/rustlib/aarch64-apple-darwin/bin/rust-lld`, LLD 22.1.2; SHA-256 `613eb4a0b9a297e3efb58e5ddf311091c7ab606bc846584f2ea27125f4e97063`.
- Accepted command (attempt 003):

```text
rustc --edition=2021 --target x86_64-pc-windows-msvc -C panic=abort -C opt-level=s -C linker=/Users/rabii/.rustup/toolchains/stable-aarch64-apple-darwin/lib/rustlib/aarch64-apple-darwin/bin/rust-lld -C linker-flavor=lld-link -C link-arg=/nodefaultlib -C link-arg=/entry:mainCRTStartup -C link-arg=/subsystem:console -D warnings release/install/tests/native_probe_fixture.rs -o /Users/rabii/.prime/agent/session-artifacts/01a18f18-3db0-73ba-a45a-f25faf5369ad/sub-6fd69609/native-fixture-build/attempt-003/native_probe_fixture.exe
```

- Result: exit 0 in 2.955s; compiler stdout/stderr are empty. `-D warnings` was active, so this compilation emitted no warnings. Logs: `/Users/rabii/.prime/agent/session-artifacts/01a18f18-3db0-73ba-a45a-f25faf5369ad/sub-6fd69609/native-fixture-build/attempt-003/rustc.stdout.txt` and `/Users/rabii/.prime/agent/session-artifacts/01a18f18-3db0-73ba-a45a-f25faf5369ad/sub-6fd69609/native-fixture-build/attempt-003/rustc.stderr.txt`.
- Earlier source-level attempts are preserved, not overwritten: attempt 001 used source SHA `542aee2f278d0283eef0b600623900e65840d4648b17cac4b0697a991dc0ac6a` and failed before link due to assigning `parse_exit_value(...)?` (`u32`) into `Option<u32>` at both control keys; attempt 002 used SHA `239b3ac0596c95d640c57bd56a0607e47fbfba51e23fdf2f47b32ee528d558be` and failed before link because `trace_path` needed an explicit `*const u16` type. These were local syntax/type corrections in the assigned file; no link failure occurred.

## PE inspection

- Output: `/Users/rabii/.prime/agent/session-artifacts/01a18f18-3db0-73ba-a45a-f25faf5369ad/sub-6fd69609/native-fixture-build/attempt-003/native_probe_fixture.exe`; size 4608 bytes; SHA-256 `08e9773855c82ded4837968683cfa7895d17fb7d9d3283b9670ce4dc0f1fca80`.
- `file`: PE32+ Windows console executable, x86-64.
- Parsed headers: machine `0x8664` (AMD64), optional-header magic `0x20b` (PE32+), entry RVA `0x1752`, subsystem `3` (console), CLR directory `('0x0', '0x0')` (absent).
- Imports: `kernel32.dll` (`CloseHandle`, `CreateFileW`, `ExitProcess`, `FlushFileBuffers`, `FreeEnvironmentStringsW`, `GetCommandLineW`, `GetEnvironmentStringsW`, `LocalFree`, `WriteFile`) and `shell32.dll` (`CommandLineToArgvW`).
- Compiler-produced PDB is also private at `/Users/rabii/.prime/agent/session-artifacts/01a18f18-3db0-73ba-a45a-f25faf5369ad/sub-6fd69609/native-fixture-build/attempt-003/native_probe_fixture.pdb` (not a product file).

## Independent Windows guest smoke requested — NOT run here

Use the exact compiled PE bytes (`/Users/rabii/.prime/agent/session-artifacts/01a18f18-3db0-73ba-a45a-f25faf5369ad/sub-6fd69609/native-fixture-build/attempt-003/native_probe_fixture.exe`) copied unchanged to `SourceDir\codebase-graph.exe` and `SourceDir\k-wiki.exe`. For each invocation, set child-only `CBG_A09_GRAPH_EXIT`, `CBG_A09_WIKI_EXIT`, and a fresh absolute, initially absent `CBG_A09_TRACE` path. Capture actual process status and decode the trace; expected cases:

1. `codebase-graph.exe --help`, graph exit `0` → actual status 0; one record version 1, role 1, argc 1, UTF-16 arg `--help`, selected exit 0.
2. Same call with graph exit `23` → actual status 23; same arg/role, selected exit 23.
3. `k-wiki.exe --version`, wiki exit `0` → actual status 0; one record version 1, role 2, argc 1, UTF-16 arg `--version`, selected exit 0.
4. Same call with wiki exit `29` → actual status 29; same arg/role, selected exit 29.
5. `codebase-graph.exe --unexpected` with otherwise valid configuration → actual status 64; trace records actual `--unexpected`, role 1, argc 1, selected exit 64.

These are requests for the independent guest executor, not evidence of native execution. No Mac-side emulation/run was attempted.
