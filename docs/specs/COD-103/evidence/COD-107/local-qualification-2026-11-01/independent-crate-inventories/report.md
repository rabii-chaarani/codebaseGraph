# COD-107 — actual clean root/wiki .crate inventories

**PASS for the assigned actual package-inventory portion of C103-A03.** Both real generated archives contain **zero Python/docs-helper executable members**. A03/native/full qualification is not complete.

## Exact clean source and commands

Worktree `/private/tmp/codebasegraph-cod107-20261031`, branch`cod103-stack/cod-107`, clean source **`2c4ed59bacfdc316f61e202eb6b5eaaa6acca767`**, tree **`86b0d6aa4bebf645a59dd5bb12bd7beeffd8ca2d`**, parent08d2. Before/after nonignored status is clean; exact HEAD/tree and source/manifests/lock/config hashes unchanged. Actual Production receiptSHA256`0e5a3444f8e9085511a6f82d6a0a5f1c8a0b736854256e42f56b1d56210f4d52` independently verified.

Original316be/0e6008/design§6andapprovedretentione5ade/08d2apply; same EvidenceVerifierinherit/inherit, no overrides/delegation, runtime self-metadata unexposed. Previous discovery/native environment reused; no new source design or approval.

| Actual command | Exit | Duration |
| --- | ---: | ---: |
| `cargo package --offline --locked -p codebase-graph --no-verify` |0|0.599s|
| `cargo package --offline --locked -p k-wiki --no-verify --config 'patch.crates-io.codebase-graph.path="."'` |0|0.167s|

Command-local wiki patch is the existing CI in-tree dependency route, not persistent configuration. No`--allow-dirty`, package-list-only substitute, dependency install or upload. Exact version4.0.5 came from both actual manifests; exact `target/package/<name>-4.0.5.crate` files were **absent before** and fresh after successful commands. Raw stdout/stderr, timestamps and requests in `commands.json`/`package-output-before.json`. No find-first/stale archive.

## Actual archive results

| Package | Compressed bytes | SHA256 | Regular members |
| --- | ---: | --- | ---: |
| codebase-graph4.0.5 | **8485616** | `d5c5da36855c8d5ea7084cf29ab2bebb8107fa35852b8b130a860320c10c211e` |220|
| k-wiki4.0.5 | **140437** | `45ca763a9899074799ccb2366b7239969570a0e01bf4eee5dfa782b8b477b2fd` |64|

Both actual `.crate` files copied byte-for-byte into this private directory. **Every284member's full bytes were read and SHA256-hashed**, with0duplicate/unsafe/nonregular/link entries and exactly the expected one top-level package directory. Every non-generated payload member equals its actual source bytes; originalCargo.toml.orig bytes exactly match source manifests. Actual normalized Cargo.toml, Cargo.lock and VCS JSON were independently read/parsed, not assumed.

Both `.cargo_vcs_info.json` files name exact **2c4ed59bacfdc316f61e202eb6b5eaaa6acca767**, with no dirty flag (`false` semantics); path_in_vcs is`""`forroot and`"crates/k-wiki"`forwiki. No future-head identity is fabricated. Root is **8485616 <=10485760bytes** by the unchanged exact10MiB gate. Wiki is unpublished; **no wiki10MiB policy** was invented.

Actual packaged locks are not workspace-byte-identical: root127/wiki211 packages vs workspace223. Both lock contents/source+checksum identities were inspected and retained; root5entries drop redundant`syntype2`dependency qualification from`"syn 2.0.117"`to`"syn"`, with source/checksum unchanged. Wiki source-backed records match workspace. This is Cargo's actual per-crate lock output; locked workspaceCargo.lock remained unchanged. Full observations/diffs are adjacent.

## Actual Markdown inclusion — no blanket docs exclusion

The initial in-memory analyzer predicate incorrectly treated unqualified`README.md`include as top-level-only and assumed all docs entries must be absent. It stopped on the first docsREADME; **Cargo commands succeeded** and no source/archive input or command was retried/changed. Exact assumption/error/correction preserved in `preserved-inspection-assumption.json`.

Actual Cargo filename-style include permits nested README basenames. Root has the following **16ordinary Markdown README entries**, all source-byte-equal; this does not mean Python/helper execution content is shipped. The approved criterion is zero Python/docs-helper executable payload, not zero Markdown. Wiki has0docs entries. Actual include/default package semantics, not an imaginary top-level-only allowlist, govern the comparison.

- `docs/specs/CBG-PLAT-01/execution/pr163-start-ack/README.md` — 9115 bytes, SHA256 `370fd554d48c93507bb04ea1375648d37f3c5ee7be149eee16832aca1e5a42ea`
- `docs/specs/COD-103/evidence/COD-107/local-qualification-2026-11-01/README.md` — 5831 bytes, SHA256 `d7297946acfe411fedd532a388d2a878bf220652dc16827bab11f5615ed40b22`
- `docs/specs/COD-103/evidence/COD-107/local-qualification-2026-11-01/prepared-history/README.md` — 6012 bytes, SHA256 `a4826502b0dcd66fd7efbef393da1563f99da53dda154711bdb90890b85614f1`
- `docs/specs/COD-103/evidence/COD-107/local-qualification-2026-11-01/prepared-history/backend/retention-raw-evidence/README.md` — 1830 bytes, SHA256 `12a9dbd695a2570bbb55728435d1e01308e1b6f2dfe71b42f2a728d473bed472`
- `docs/specs/COD-103/evidence/COD-56/README.md` — 13642 bytes, SHA256 `0a150c53bde4dfeb6eeeec252bbe589b4b329d09bc7c9cf200b77f258bd45fc1`
- `docs/specs/COD-103/evidence/COD-56/hosted-41192405055-attempt1/README.md` — 3782 bytes, SHA256 `10951f5903e35efc1ac1559422337363f73b70e5bd0b71575b5c6c75d0e9aa15`
- `docs/specs/COD-103/evidence/COD-56/matrix-qualification-2026-10-31/README.md` — 6000 bytes, SHA256 `de7f68c5da6bcb5279ae1636e5bd3cae27f571d68975e648a786a8ca3b01fa69`
- `docs/testing/COD-103/COD-105/README.md` — 5409 bytes, SHA256 `0f0a23197ce31c9ff9fb089ca125ed33b07af34ff93df20bd5df225f44d2576a`
- `docs/testing/COD-103/COD-105/integration-2026-10-31/README.md` — 3802 bytes, SHA256 `daa999ff0b43d5e34407fa0316e17f13da549b3526f62eab75588fa0a5acb7bd`
- `docs/testing/COD-103/COD-105/warning-hygiene-2026-10-31/README.md` — 7089 bytes, SHA256 `224d30b10dbce3ce3e769ead749bf9aedd3cece744e750b64b4969eadd64abb3`
- `docs/testing/COD-103/COD-106/local-verification-2026-10-31/README.md` — 7041 bytes, SHA256 `3caeb9bd00cf211fbee0e7b1cb130073864d3d34e630d089accc14f939781ec1`
- `docs/testing/COD-103/release-baseline-v4.0.4/README.md` — 3310 bytes, SHA256 `1993ae408bd051850613a5d3a9ccdea67b4d8ed065b3886f7a7a3fb7c91ac6f8`
- `docs/testing/graph-disk-cleanup-evidence/README.md` — 3056 bytes, SHA256 `1c7212d00b8793b00a7672482bb77fee508fac8bbe80f0ee305eec12f4c3a7bc`
- `docs/testing/pr153-review-fixes/README.md` — 5960 bytes, SHA256 `085374754c5363c810812cd34b21f375619c1dc46ec2fc68d6b55b845611e56f`
- `docs/testing/release-40851263747/README.md` — 1652 bytes, SHA256 `7e6493dbf2f92aadde3e3fa0cb1535c5959e47d11808a785d5edc0e58bd38bf6`
- `docs/testing/release-pr-162/README.md` — 4242 bytes, SHA256 `78b0391e82fdf7e3cf65bb27210c9ac819b5f913d07427d06fa9492326160acc`

Root otherpayloads match actual src/tests/assets/vendor/build/LICENSE/include patterns. Wiki has no explicit include/exclude and`publish=false`; actual64-member default inventory was checked. No.py/.pyw/.pyc/.pyo/helper-executable member exists in either; no historical helper archive restored/extracted. Complete member types/sizes/fullhashes in `root-member-inventory.json`/`wiki-member-inventory.json`; exact actual Cargo metadata in corresponding`*-cargo-metadata.json`.

## Limits, later source binding and STOP

`--no-verify` is actual archive inventory **not** extracted-package compilation, runtime/install/registry publish verification. No package code executed/imported; orchestration stdlibtar reads only. Existing hosted publish-dryrun/compile/size/main/release gates remain. Previous83genuineJUnit/strictClippy0warnings and18rawCLI supplement were neither altered nor rerun.

Future evidence-only PR head may reuse these actual clean2c4packages only with proof unchanged relevant **payload inputs**, not product-code hashes alone: newly added/changed nestedREADME can affect root contents. Keep originalVCS2c4binding truthful; compare actual package-selecting source/README/manifests/lock inputs and final-head relationship. If real inputs change, Engineer assigns one necessary cleanfinalpackage slot; no automatic endlesspackage/evidence/CIloop.

Both actual same-source native archives/sidecars/provenance require necessary real107PR/fullCI and independent retained-byte readback later. FreshFinalReviewer/final-headallCI/current-sourceScryer/acceptance gates remain. **No A03complete/PR/107→108/Done/main/release/platformaggregate assertion.**

Only normaldefaultworktreetarget/package outputs and this newprivateevidence directory were written. No source/Gitrefs/Linear/wiki/model/maps/ingest/fold/CI/VM/settings/credentials/dependency/install changes. Existing nativeenv/resources reused, no constants/policies changed. Reusable-memory assessment: transient per-sourceinventory, no write warranted/authorized.

**Engineer/FinalReviewer/Production next. Package Cargo lease released; all Cargo/privatewrites STOPafterhandoff.** `result.json` contains exact source/checks; `files.json` binds all closed reports/raw logs/metadata/member manifests/privatearchives without recursive selfhash.
