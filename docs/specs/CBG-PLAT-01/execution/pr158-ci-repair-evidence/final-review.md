# PR158 / COD-53 — final engineering review

## Verdict

**No confirmed P0–P3 defect found. Proceed with coordinator commit/push of the bounded CI repair to existing PR158.** This is not merge/release approval, a passing future native CI result, Windows 11 qualification, or COD-53/S02 aggregate closure.

Required reviewer execution: **GPT-6.1 Sol / Max**, fresh independent default-agent RLM session with no inherited transcript; reviewer did not implement this change. Applied `adversarial-review-engineer` and `codebasegraph-engineer`. Source/Git access stayed read-only. No builds, tests, installs, settings changes or external writes were run by this reviewer; only this report was written.

## Reviewed identity and authority

- Baseline PR head: `5f68b41cebc327072cf71be770c9e5a9d1c0dad2`.
- Integrated staged tree: **`649f78da13f2e044309ebf78953730aa57b1b25c`**, 34 changed paths: the frozen five repair files, original proxy deletion, and bounded canonical repair report/evidence additions.
- `final-full-snapshot.json` SHA-256: `c8181dc3b879dc232768fc1597ce4d737599ebd1fa5d1724137baffda4cac7db`. All listed file digests match disk; every reviewed new/modified file's Git blob matches the index. The historical archive is explicitly staged despite the existing `*.tar.gz` ignore. `git diff --cached --check` passes.
- Reviewed canonical report: `docs/specs/CBG-PLAT-01/execution/pr158-ci-repair.md`, SHA-256 `a8085ce27a2237f0c64a0d56f3c32629a10e342de01db50b0e99966d96743624`.
- Governing scope: user CI-debug/fix request dated 2026-10-29; COD-53 / CBG-PLAT-01/S02; unchanged spec rev0.10 at approved SHA `1a9db7b558b60248888408daa6268a3b45f2f0df`, R02 / A03 / A04 / A16; existing exact S02 approval. Read live Scryer rules, Release Gate intent and `chg-fq8t0y` / `chg-h07kt8`. The repair restores existing controls and needs no invented topology/claim.

## Consequential hypotheses examined

### 1. The dependency change could evade audit or change unrelated resolution

Defeated for this snapshot. The complete manifest/lock diff changes only the wiki lower bound from `0.13` to `0.13.5`, and the locked parser version/checksum from 0.13.4 to 0.13.5. `default-features = false` and `features = ["html"]` are unchanged; no other package/features changed. The cached published crate hashes to `278e9268c93b9e498a2da46c34d90ba93a3255eb99009646398f34bc73ab36eb`, identical to the sparse registry record and lock entry. Current RustSec primary advisory identifies `>=0.13.5` as patched. Both tagged manifests declare Rust1.71.1 with unchanged dependency/features declarations. No full-workspace Rust1.82 execution is inferred.

Graph-discovered compiler/render consumers use default/explicit options without vulnerable wikilinks; this reduces direct exposure but does not excuse the demonstrated audit failure. The patch correctly removes the advisory blocker rather than adding an ignore. Captured baseline/final audit logs preserve the same guarded time exception and existing two allowed cxx/tokio advisory warnings. Those are not compiler warnings and are not claimed repaired.

### 2. Archiving the helper could corrupt or silently rewrite historical proof

Defeated by direct byte checks. The staged 998-byte archive SHA-256 is `d37b3c86d978018d137375e7a30316b3ce779b2787df5037237032bdec892752`. It has exactly one regular file, at the original full repository-relative proxy path, with only parent directory entries alongside it. The 1,311-byte member SHA-256 is `ca7ada29f99cb0a6da0807d6f468c22ad81d1bce59abe5c9f743c1b03b83588f`; its Git blob is `deeddafd5070b476931f44c8cea189755a7d9ec1`, identical to the deleted baseline file. Both independently generated archive artifacts and the staged archive are byte-identical.

Independently resolved all **65** original index records, substituting only that member; every length/digest matches. The index retains Git blob `9b177b21a49b7e331ca109ad272ec940fdd47ec3` and all **330 CRLF** endings. No historical report/log/command bytes changed. `archived-inputs.json` supplies source commit, archive/member paths, lengths/digests and separate-checkout reconstruction guidance. The qualification note preserves original tested-source identity and partial Windows verdicts. New captured CI log gzip files decode byte-for-byte to the captures and declared hashes.

### 3. The repair could bypass the Rust-only gate or rely on an unrelated main fix

Defeated within the reviewed scope. Gate/workflow/ignore/security files and product source are unchanged. The original unchanged gate scans loose `.py` files without an evidence-directory exemption. Parent-executed synthetic-merge logs show pass; a temporary loose Python file causes the expected rejection and removal restores pass. Verified synthetic merge `f49fbb5afe9e6c24ac410a65b61a646d72078aa2` parents are pinned main `9f69a0bd4b468eaa8652d933d518fafb8c596b6a` and baseline PR head. The repaired archive/sidecar/qualification bytes match in that merged worktree. Four older head-only helpers were already archived on main by PR159; this patch neither imports nor duplicates their unrelated repair.

## Executed evidence inspected

Checks below were run by the coordinator, not re-executed by this reviewer. I inspected retained logs/JUnit, commands/results in `pr158-ci-repair-evidence/verification.json`, staged bytes and relevant local package archives.

- Full wiki + xtask regression: **174 passed, 0 failed/errors/skipped**, JUnit run `737a3796-b8c5-4d4e-953a-44c248621ab9`; includes compiler and renderer behavior cases. Log explicitly recompiles pulldown-cmark0.13.5 and k-wiki from the intended isolated checkout.
- Formatting and strict workspace/all-target/all-feature Clippy: recorded exit0; compiler log completes without warnings.
- Audit: baseline failure reproduced, repaired exit0 with unchanged exceptions/warnings as qualified above; time feature tree omits parsing.
- Unchanged merged-tree release gate plus positive/negative policy controls: expected results above.
- Normal root `cargo publish --dry-run --locked --allow-dirty`: success; log explicitly compiles extracted source package, then aborts upload due to dry-run. Preexisting published-version/dry-run messages are not compiler failures.
- Independently read root `.crate`: **8,458,182 bytes**, digest matches evidence, 207 members and all four bundled XZ payload bytes match source. Wiki `.crate`: **140,452 bytes**, digest matches evidence, 64 members; normalized manifest retains lower bound0.13.5 and unchanged features. Wiki archive creation is not misrepresented as isolated archive compilation/smoke.
- Windows-filtered default/opt-out feature resolution is recorded as resolution, not native execution.
- Existing pinned Ladybug0.19.0 static library SHA-256 matches `a840ea7c538e67a8b9fc0d58495b5226dc9c0a5f2f144ff0c48c0f6ca1193ae6`. Initial missing-header baseline compile failure and corrected successful retry are both retained; no source/dependency/ABI change was used to hide it.

## Conditions and limits

1. Final pushed-head macOS/Windows native CI remains **pending** and must assess the actual pushed revision/merge. Static review and local compatibility evidence cannot predeclare it green.
2. Original prepared-environment Windows evidence remains bound to `47808ac34bcff79882abee4ab796c16c0621ed05`; this repair does not repeat/promote it. Clean-consumer qualification, full S02/A03/A04/fullA16 and maintainer aggregate closure remain open. No merge, release, deployment or restart is authorized.
3. The connected graph/Scryer binds the older original repository, not the detached review worktree. Explicit semantic discovery/context was used before source inspection; relevant source/diff/byte checks establish this snapshot, not a branch-specific graph/test fingerprint. Linear connection is pending, so no live tracking success is asserted by this reviewer.
4. Final report-retention/link-only edits after this tree require focused verification; substantive repair/source changes require renewed affected review. The review recommendation does not replace owner approval or delivery reconciliation.

Applied lessons: auditable dependency metadata and exact-source/extracted-package evidence from live wiki memory and `docs/release.md`; do not treat historical target lists or a shared cache as current-platform proof. No new durable memory is warranted: this is version-bound/task-local evidence, and the archival procedure already exists.

## Focused final evidence verification — 2026-10-29

**No new confirmed P0–P3 defect; commit/push recommendation unchanged.** Reviewed the subsequent canonical report's review-link/verdict paragraph, clarified audit-warning wording and additive combined-tree evidence. Current canonical report SHA-256: `9c55e674cc21fd59c392156617f7759d61f7896009cd3283447a69190dc8bd31`. It accurately cites the prior reviewed tree, records no confirmed defect, and keeps actual pushed-head native CI/platform/aggregate gates pending. Final-review retention at the linked canonical path is a coordinator delivery step, not a new source change.

Directly inspected `merged-checks.json`, `merged-xtask-tests.log`, `merged-xtask.junit.xml` and `combined-evidence-release-gate.log`: nextest run `d424b847-8f02-48c0-9ec3-9f23a180ba84` explicitly recompiles xtask4.0.2 from the synthetic merged worktree and contains **44 test cases, 44 passed, 0 failures/errors/skipped**. The post-evidence unchanged release gate passes. These remain coordinator-executed local checks, not native CI or desktop qualification.

All five frozen repair-file hashes and every previously reviewed non-report evidence-file hash remain unchanged. The new report and additive evidence are staged; `git diff --cached --check` still passes and there are no unstaged tracked edits. No builds/tests or repository writes were performed by the reviewer.

Additive evidence SHA-256:

- `merged-checks.json`: `bb8c658afc018edc8d745e5f0b4b24f2aae51c96810bc5406a94574553a86483`
- `merged-xtask-tests.log`: `d1bb1ee54c96d104063b8b07e6078b505228e37efa15a989110cdabb9a55dbc4`
- `merged-xtask.junit.xml`: `e5e060b6142d99979388b349374520de18273df574333d36baab15f053d0ed88`
- `combined-evidence-release-gate.log`: `5899714f6181a86b883602a3dd9b321883ca9e4084db16c902cf56856e633340`
