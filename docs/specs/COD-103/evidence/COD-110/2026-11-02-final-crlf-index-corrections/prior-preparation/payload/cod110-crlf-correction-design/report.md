# COD-110 CRLF correction — bounded design handoff

**Recommendation / disposition:** routine restoration within the existing approved candidate/selected-source behavior. Normalize **only CRLF pairs → LF in the six in-memory filesystem metadata buffers** in `ci.rs::validate_release_candidate`, after the existing **raw-byte** size limit. Keep shared recognizers and immutable Git-blob classification unchanged. No new behavior approval, planning/proposal-review stage, architecture claim, parser dependency, setting, `.gitattributes` or source-index write is needed.

## Authority and exact basis

- Five contracts: approved `316be4c4bb81d4f6e025016b12374836b4dcc1a8`; separate approval `0e6008a1f46679a86aba85ac513beb05af94b0c0`, user confirmation “i already approved revision 0.3”, COD-103 decision `c8690556-e836-428d-bc81-079c701d2395`.
- `docs/specs/COD-103/spec.md`, C103-R07, lines136–138: validate “actual tested checkout identity and proposed version” with “the same core metadata validation as the final selected main source”; candidate validity gives neither lightweight nor publication authority.
- Approved `design.md` §9.1, line175: candidate/final callers reuse `ci.rs`; “its immutable B/H classifier adds the strict semantic diff/span proof.” Ordinary compatibility remains unchanged. This supports adapting the filesystem representation before the same core, not changing blob proof.
- Approved Design §8.2, lines119,126–132: conservative generated-layout recognizers; reject duplicate/ambiguous literal/multiline context; preserve every non-version byte and historical changelog suffix in classification; unsupported formats select full. These constraints stay exact.
- Compatibility decision `49612ec66814ad6b883a0c4c3c624911bd5a8825`, separate approval `754c82…`, user “i approve your recommendations”, COD-110 decision `2917ddb7-8746-407a-a37d-23b1e72396bf`: exact tracked-byte/HEAD proof without real-index mutation and unchanged ordinary helpers. Its §1 explicitly adds “no format or metadata-parser policy.” No new parser format is proposed.
- Immutable approved Design and compatibility Git objects were read. Relevant current Design §§8–9, R07 and compatibility text match their approved objects. Scryer node-1 COD-110 DESIGN/EXECUTION / `chg-wzt5hk` and scoped intent are reused from Engineer's current matching read, with no new model calls/writes or intent changes.

## Exact source / ownership

Source `/private/tmp/codebasegraph-cod110-20261101`, parent-bound HEAD `862bb3b3c40c36b21cdcd388f596d52ad482530b`, tree `02cd3ebed5a1861bef4506f678449d836e7e2bf8`. `ci.rs` independently read SHA-256 `7db803caf383175b0842c9b01e2725588a96794fc8e9b021db550fd4e129139a`.

**Production implementation ownership:** only `crates/xtask/src/ci.rs`. Engineer subsequently assigned the same Backend owner bounded candidate regression tests in `crates/xtask/src/release.rs` as well; no production `release.rs` change is needed. No `main.rs`, workflow, metadata-file or docs edit is needed for the fix. Any later related evidence documentation remains in its existing PR under Production ownership.

Graph MCP unavailable: semantic CLI search found `ci.rs` and `release.rs`; explicit semantic dependencies/change-impact contexts were requested before source reads. Registered canonical graph is the old same-project baseline, not current862bb3 evidence; no rebuild/rebind was performed. No unrelated model audit was added.

### Concrete algorithm / interfaces

Keep `pub(super) fn validate_release_candidate(root: &Path, expected_version: Option<&str>) -> Result<ReleaseCandidate, String>` unchanged (ci.rs:74–77).

At the existing read loop (88–95):
1. Read the raw bytes, preserving existing path-specific read errors.
2. Apply existing `MAX_BLOB_SIZE` to the raw byte length **before** conversion.
3. Produce a private validation buffer by one left-to-right byte scan: on adjacent `0x0d 0x0a`, emit `0x0a` and consume both; otherwise copy that byte. Do not trim, split/rejoin, remove all CR, normalize other whitespace, decode/re-encode lossy UTF-8, or touch disk.
4. Insert that buffer into the existing local map. Continue existing recognizers/version/lock/notes validation and errors unchanged. A small private pure helper in `ci.rs` is sufficient; no new public API, error type, mode flag, framework or dependency.

All six buffers must pass this boundary. Besides three TOMLs and lock, **manifest recognition hardcodes LF prefix/suffix** (`recognize_manifest`, 997–1004), so a TOML/lock-only change is incomplete. Changelog `release_notes` already uses `str::lines` and joins LF (158–181), so paired-ending conversion preserves its returned notes.

O(n) time and at most one owned output buffer per file, bounded by the existing raw-size limit. The local map owns buffers until return. No persistent state, locks, retries, cancellation protocol or migration is applicable.

### Preserved semantics

- `recognize_toml` (1028–1111), `parse_lock_packages` (1228–1301), `recognize_manifest`, `recognize_candidate_lock`, version checks and `release_notes` unchanged. Normalized-buffer spans are local to candidate extraction; never reuse them against raw blobs.
- Lone CR is copied, not removed: TOML/lock continue their explicit CR rejection (1039,1235); manifest keeps existing layout rejection; changelog keeps its existing behavior. **No blanket new bare-CR policy** is asserted.
- LF and CRLF pairs may coexist: scan pairs independently and preserve existing LF; do not add a uniform-ending requirement absent from approved intent. This is representation handling, not a general parser expansion.
- Literal/multiline markers, duplicate identities/keys, malformed quoting, wrong versions, external/source/checksum workspace records remain invalid under existing checks. Pair conversion does not remove those tokens.
- `verify_checkout` and `verify_worktree_blob` (664–687) remain authoritative for actual source/HEAD/index/modes, including Git-normalized working bytes. Preserve raw-size checking, tracked source and real index unchanged. No conversion is used as cleanliness proof.
- `classify_metadata_diff` (853–971), raw Git reads, spans and `masked_equal` (974–995) are untouched. Its raw unsupported CRLF blobs still choose full; original non-version byte/history equality remains mandatory.
- Four ordinary helpers and caller output schemas/source SHA/version/tag/notes remain unchanged. PR readiness and selected-source package/publication gates are unchanged.

## Three bounded regression groups (proposed, not executed)

1. **Filesystem positive equivalence / real failure path:** in ci.rs tests use existing complete six-file fixtures, write LF vs all-six CRLF vs alternating LF/CRLF, and call the actual `validate_release_candidate` with None and matching expected version. Assert identical complete `ReleaseCandidate {version,notes}`; include manifest conversion. On both local hosts rerun unchanged `release::tests::candidate_metadata_accepts_current_real_generated_six_file_layout` on actual product-root bytes; never replace it with only a minimal fixture, skip it, or normalize the test oracle.
2. **Malformed bytes remain invalid / identity unchanged:** a compact table retains lone CR in relevant TOML/lock/manifest layout, multiline/decoy or duplicate context, mismatched version and source/checksum-bearing workspace lock failures. Preserve existing UTF-8/read/size errors. Exercise real `release-metadata` identity path in a disposable clean tracked checkout with Git-normalized CRLF; compare metadata result and real index bytes/flags before/after. Actual tracked dirt still fails. Existing comprehensive negative/ordinary tests should be reused, not duplicated exhaustively.
3. **Classifier strictness counterexamples:** raw LF base→CRLF bumped head (or raw CRLF base/head) still returns Full; raw LF valid six-file bump remains Eligible. A non-version whitespace/comment/lock-edge edit and historical changelog rewrite remain Full; existing duplicate/multiline/external-lock/strict-version suites retain their assertions. This proves candidate adaptation never leaks into byte-span eligibility.

## Evidence / actual verification plan

No tests/builds/VM commands executed by Designer. Baseline proof only, not fixed-source acceptance:
- macOS existing CLI binary `63df67451f452d32fc670d9e4572ef25c8ac51bc2ea91ee1caaf1a710e5bd69b`: LF success vs controlled CRLF exact root-Cargo rejection. Private `pr178-existing-cli-newline-corroboration/outcome.json` and raw outputs under the parent session artifact root.
- Closed local Windows proof: `sub-b5aa2377/cod110-local-windows-baseline-repro/{report.md,result.json,files.json}`. Windows11 ARM64 / existing x64 Rust1.99.1 emulation, exact2696-file source verified; actual LF test stdout1pass/90filtered (Cargo numeric exit unavailable); same compiled PE `0772b115b581c048707c1868d217f82a60a0ddabbdbe6d72ea68b8455f215823` controlled-six-CRLF direct exit101,0pass1fail90filtered, release.rs551. Six LF-roundtrip hashes match source; private index unchanged. Wrapper cleanup255 is not Cargo result. No actual hosted Windows raw checkout bytes were captured.
- Failed own full CI41393999523/attempt1 and native runf3f15422 remain failed historical evidence; no flaky label, retry-as-pass or waiver.

After Backend's minimal fix, **before any PR update**, Evidence Verifier must run affected behavior on BOTH local macOS and the local Windows VM (strict user rule), recording exact corrected source/binary/runtime, actual input EOL counts/hashes, selected test inventory, numeric exit/results and index nonmutation. Execute the three groups, unchanged real-product-root regression, actual release-metadata interface and relevant xtask suite/format/lint checks using existing project methods; prove genuine all-six CRLF (not intended case labels). Preserve ordinary compatibility and package-preflight/workflow regressions relevant to the changed shared core. Do not synthesize JUnit.

Then obtain focused fix verification from the **original Sol/MAX Final Reviewer** on the final corrected snapshot/affected paths, and a **new own full CI** at that corrected head under Production. These are existing correction gates, not a new full proposal/design/review stage. Fresh hosted common-preflight/lightweight/full proofs remain due where already required by R04–R08; local fixtures do not replace hosted evidence. No PR update/merge/release/closure is authorized by this proposal alone.

## Closure / settings

Software Designer required/parent-assigned `host openai-codex/gpt-6.1-sol`, XHIGH, no nested dispatch; actual self-runtime metadata is unexposed. No source/model/Git/API/tracker/wiki/memory writes; only this new private handoff bundle. No constants/settings changed. No invented acceptance IDs or claims. Reusable knowledge assessed: no new memory warranted; canonical contracts and the causal evidence own this bounded restoration. Next owner: Engineer → one Backend owner for this exact ci.rs correction, then existing verification/review/Production owners. No unresolved new behavior question remains for this pair-only boundary correction. Private/source lease released; no running owned Bash handles.
