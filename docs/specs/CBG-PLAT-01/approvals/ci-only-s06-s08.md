# CBG-PLAT-01 — CI-only S06–S08 implementation approval

- Approved canonical specification: [spec.md at revision0.10](https://github.com/rabii-chaarani/codebaseGraph/blob/1a9db7b558b60248888408daa6268a3b45f2f0df/docs/specs/CBG-PLAT-01/spec.md), full Git commit `1a9db7b558b60248888408daa6268a3b45f2f0df`.
- Canonical path: `docs/specs/CBG-PLAT-01/spec.md`; SHA-256 `13aa21bb99084865557b8f9db2d39d51298e27e28ce9f60c2c6de4b7ae540030`.
- **Exact approved scope:** S06/COD-57/R06/A10 (watch help states existing parallel default), S07/COD-58/R07/A11 (COPY metadata fixture uses production-rendered Windows paths with unchanged four-byte oracle), S08/COD-59/R08/A12 (reference-epoch test observer recognizes fs2 Windows contention, retaining the five-second deadline and epoch/birth assertions). Verification and normal Git/PR delivery for these fixes are included.
- User decision: **“I approve CI-only implementation scope”**, immediately answering the assistant's proposal to implement COD-57/COD-58/COD-59 first under the linked revision0.10 and rerun affected checks.
- Source baseline rechecked before implementation: PR153 remains `862a8f88d485d5bd257ebd9456c194156fd4ee94` (3.0.4). Docs decisions live in PR155; do not substitute old review code or unrelated pending Scryer/resource-ceiling work.
- **Excluded:** S01–S05, S09/S10 implementation, runtime packaging/dependencies, discovery/shim behavior, lock-mode changes, directory-cursor repair, broad qualification closure, unrelated specs, merge, deployment and live-service restarts. Existing D01–D05 policy decisions remain selected, not newly implemented by this approval.
- Scryer ledger: `chg-pzt87f`, signed off for this exact fixes/docs/tests scope. No new responsibilities/topology are needed: these restore existing help/test contracts. The configured Scryer project is repository-bound rather than this PR worktree; no current-PR model implementation/reconciliation result is claimed.

## Verification and closure limits

Keep the source/library mutation behavior unchanged. Do not change production COPY SQL/parser/lock policy to accommodate invalid fixture inputs, delete meaningful assertions, increase the five-second observer deadline, bypass required CI, or mark Windows/platform qualification passed without execution. If real contention still exceeds the existing deadline, investigate and report it separately.

Record actual source/binary hashes, environment, commands and results; distinguish local macOS checks from hosted Windows/native cohorts and from previously failed CI. Move only these implementation records to active/review states as evidence warrants. COD-51 remains the gate for all unapproved implementation slices; it is not globally completed by this subset approval. No aggregate milestone closure, merge/deployment or service restart is authorized.
