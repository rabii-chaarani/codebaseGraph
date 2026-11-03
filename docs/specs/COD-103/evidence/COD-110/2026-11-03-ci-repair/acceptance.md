# COD-110 / PR178 — CI-repair acceptance addendum

**Narrow outcome: the executed carrier/checkout and dependency-patch checks are satisfied at private candidate `a3b51c25bbea5e137bdeeac572a138c064834c9a`, tree `01ffaa61cd444f16284319bcc4799ff1865d88e9`.** This advances only these affected local criteria. It does not rewrite historical BLOCKED reports, accept every application behavior, approve a new source revision or claim PR/hosted/release closure.

Authority remains COD-103 spec revision0.3 at `316be4c4bb81d4f6e025016b12374836b4dcc1a8`, separate approval `0e6008a1f46679a86aba85ac513beb05af94b0c0`; COD-110 focused repair ledger `chg-0p9sje`. No source/Git/model/tracking write or new execution occurred for this addendum. Required and parent-accepted verifier settings remain Sol/XHIGH.

## Accepted executed evidence

| Affected criterion | Actual result and source binding |
|---|---|
| Preserve evidence and restore full checkout | Both hosts materialized all3,213 candidate files with no omissions. Mac bytes/modes/Gitblobs all exact. Windows706 rawexact +2,507 CRLF-only conversions; all3,213 index modes/blobs exact, clean status. Native NTFS root33 characters including separator; command-scoped `core.longpaths=false`, no global change. Original checkout128 (17file errors +1fatal directory) and124-tree-path inventory remain distinct. Corrected maxrelative223/absolute256. |
| Faithful inert carriers | Both final carrier hashes match prior independent inspection:200 unique safe regular0644 members, all original bytes/size/hash/mode; all711 logical historical payloads resolve unchanged. No extraction/execution. |
| Minimal authorized change | Relative to original `8f8838a06fab1aafc91962f306924a92c7a5b806`:200 mapped removals,5new transport files, only changed existing file Cargo.lock. pulldown-cmark0.13.5→0.13.6 plus checksum only; lockLF SHA256 `ffc44cf15ff000954c4caf15d017df5c15564e5b7b4fd0dc376b4ce6f2a5e181`. |
| Mac affected behavior and regression gates | `cargo test --locked -p k-wiki --test compilation --test rendering --test projection`: exit0,15 actual passes (compilation5/projection4/rendering6),0failed/ignored/filtered. `cargo fmt --check`:0. `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`:0,0compilerwarnings. `cargo test --locked -p xtask`:0,158 actual passes,0failed/ignored/filtered.327 Rust/manifest/lock files matched frozen candidate before/after. |
| Windows affected behavior | Same three wiki targets through accepted offline entry: **host0 and numeric innerexitfile0**, all15 actual expected names,0failed/ignored/filtered. Exact bundled-extension case separately listed1 and ran1pass/0failed/ignored (829filtered). These are distinct checks, not a claimed full Windows suite. Fresh normalCLI compiler-artifact exact source manifest/non-test and matched immutablecopy SHA256 `03be889f09a87bc87032bcfe7653be71f12b1df916c34efa2c56fcd358ecaab9`; external native SDK binding recorded. Actual poststate0: clean same HEAD/tree, heavy0, dependency metadata unchanged. |

Mac environment was native ARM64 macOS26.6.2/Rust1.96.0. Windows was Windows11 ARM64 with existing x64-emulated Rust1.99.1/MSVC; no native-x64 OS or MSRV1.82 qualification claim. No synthetic JUnit. Compilerwarnings0 is not a supply-chain audit pass.

## Historical failures and separate environment outcome

The original135-payload report and its explicit false-PASS retraction stay immutable. Their Cargo101/linker failure and earlier unknown top-level Cargo exit remain historical. The later87-payload Windows report remains **native-setup BLOCKED** at its actual JSON extension dependent-module load failure (CLI1). Its15+unit1 PASS is independently supported; FTS/PE/warm were NOT RUN there. This addendum reconciles those dates/scopes rather than relabelling a failure.

Engineer/root later performed environment preparation directly: `/private/tmp/c110-win-ready-direct-20261103/ready.json` and `WINDOWS-SETUP.md` report runtime-DLL preparation, DB/sidecar success and separate native-FTS fixture first/warm passes. **Those are root-owned execution, not my independent execution.** That record also preserves a manual null-backend query with empty results (CLI0/assertion1) and invalid/pending maintenance. They are separate limitations; no source-regression or whole-product acceptance conclusion is drawn here. This addendum neither re-reviews environment setup nor creates a new environment gate.

Local cargo-audit remained unavailable/notpass; fresh hosted supply-chain/CI and final implementation review/delivery gates remain pending. The eventual normal reviewed commit must retain this exact approved content/source closure; this private transport commit is not a pushed PR head. Existing prior unchanged xtask/CLI/index claims are not broadened by these results.

## Raw relative input list for faithful persistence

Root aliases below identify existing immutable inputs. Production may preserve the selected files under matching aliases, without rewriting their substance or historical statuses. Each original `files.json` authenticates its full closed payload; retain references to full inventories rather than duplicating source/binaries/bundles.

- `baseline` = `/private/tmp/c110-ev-checkout-20261102T121249Z-c8cebc`
- `carriers` = `/private/tmp/c110-ev-carriers-20261102T122424Z-3273b8`
- `combined` = `/private/tmp/c110-ev-final-20261102T204847Z-b37cb6`
- `mac-lock` = `/private/tmp/c110-ev-mac-lock-20261102T213046Z-b88c24`
- `windows` = `/private/tmp/c110-win-follow-20261102T234408Z-26a017`

```text
baseline/files.json
baseline/payload/baseline-result.json
baseline/payload/win-checkout.stderr
carriers/files.json
carriers/payload/report.md
carriers/payload/member-checks.json
carriers/payload/logical-payload-checks.json
combined/files.json
combined/payload/acceptance-result.json
combined/payload/candidate-delta-check.json
combined/payload/mac-file-check.json
combined/payload/windows-file-check.json
combined/payload/windows-index-check.json
combined/payload/guest-checkout-steps.json
combined/payload/mac-focused-tests.stdout
combined/payload/mac-focused-tests.stderr
combined/payload/executed-test-inventory.json
mac-lock/files.json
mac-lock/payload/report.md
mac-lock/payload/result.json
mac-lock/payload/fmt.result.json
mac-lock/payload/clippy.stderr
mac-lock/payload/clippy.result.json
mac-lock/payload/xtask-full.stdout
mac-lock/payload/xtask-full.result.json
mac-lock/payload/xtask-executed-inventory.json
windows/files.json
windows/payload/report.md
windows/payload/result.json
windows/payload/wiki-tests-attempt2.stdout
windows/payload/wiki-tests-attempt2.stderr
windows/payload/guest-wiki-tests-attempt2.exit
windows/payload/bundle-list.stdout
windows/payload/bundle-list.result.json
windows/payload/bundle-run.stdout
windows/payload/bundle-run.result.json
windows/payload/native-build-binding.json
windows/payload/db-build.stderr
windows/payload/db-build.result.json
windows/payload/post-state-short.stdout
windows/payload/post-state-short.result.json
windows/payload/post-integrity.stdout
windows/payload/post-integrity.result.json
```

Historical retraction: `/private/tmp/c110-ev-retraction-20261102T210914Z/retraction.json` with its own files.json. Separate root environment records above remain attributed to root. All selected paths were read/checked for this documentary closure; **no VM, test, build, retry or new source check was run**. Reusable-knowledge assessment: no new verified procedure arises from this documentary consolidation; no memory mutation.
