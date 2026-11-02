# COD-111 — Windows qualification-method approval, 2026-11-02

## Exact amended specification

- Work: [COD-111](https://linear.app/codebasegraph/issue/COD-111), isolated Git worktree graphs and live refresh. User retains aggregate closure.
- Canonical specification: `docs/specs/COD-111/spec.md`, logical revision **0.2**, frozen full Git SHA **`415e9d24b508182801c7462a37510ce7c1009723`**.
  [Immutable amended specification](https://github.com/rabii-chaarani/codebaseGraph/blob/415e9d24b508182801c7462a37510ce7c1009723/docs/specs/COD-111/spec.md).
  Exact content: **35606 UTF-8 bytes**, SHA-256 **`ea2901b6e2a6ad28c2b57b9119ab6af0a9a18650fa168ff57b34f5ba72636b77`**.
- This record binds the user's narrow qualification-method approval to the exact faithful amendment authored by Requirements and inspected by Engineer before Production froze it. It does not assert a new whole-product approval or advance the original approval pointer automatically.

## Actual user decision and narrow scope

In session **`01a19ac7-b463-7200-9829-08d043ecb2b1`**, the user directed exactly:

> yes you should run the full Windows suite under x64 emulation in the VM. the emulated should be considered as native qualification

Engineer recorded this actual decision on [COD-111](https://linear.app/codebasegraph/issue/COD-111), stable comment **`579d8b2a-b3c8-4796-8e6d-e469ce2a580a`**. The decision settles the method; it is not inferred from a commit, status, model sign-off or passing test.

For **COD-111 only**, successful execution of the full applicable Windows x64 suite and all applicable COD-111 acceptance, including A12, on the existing local Parallels Windows 11 **ARM64 OS / x64-emulated processes** VM, UUID **`7eb56d19-dbd8-4a09-b6e8-85bf4f08a5a0`**, counts as Windows qualification. No additional native x86_64 host run is required for COD-111 after that successful execution. Evidence must report the actual architecture truthfully; this does not make the VM an actual native x86_64 OS.

Full applicable local **macOS ARM64** suite and acceptance execution remains required at the **same final source snapshot**. Hosted execution substitutes for neither local run. Evidence must identify source/content/artifact identities, actual OS and process architecture, toolchain, exact commands, profiles/features, test inventory and outcomes, compiler warnings, failures, ignored/skipped/inapplicable cases with reasons, and unavailable or unrun work. Missing or narrower execution is not a pass.

Direct method impact is COD111-A12 / R02, R04, R06, R07 and R13; the same approved Windows method applies to all other applicable COD-111 acceptance and full-suite evidence. COD111-R01–R13, observable A01–A12 outcomes, D01–D07, public/source contracts, product scope, exclusions, architecture/claims and closure owners remain unchanged. This is not Windows ARM64 product/support expansion and does not amend or close CBG-PLAT-01, global platform policy or any other issue.

## Preserved original authority

- Revision **0.1** remains approved at full SHA **`57d89df6f5bb8e10473b8813d61d754ca792ade1`**, canonical path `docs/specs/COD-111/spec.md`, content SHA-256 **`d5c21568c5c1ec2b8823d0e22fb132a4221eae9824d4f840184ebabb9c504098`**.
  [Immutable original specification](https://github.com/rabii-chaarani/codebaseGraph/blob/57d89df6f5bb8e10473b8813d61d754ca792ade1/docs/specs/COD-111/spec.md).
- The [original implementation approval](https://github.com/rabii-chaarani/codebaseGraph/blob/92893e1ecd7c943010c2db3ccd61fd4ca272ee4f/docs/specs/COD-111/approvals/2026-11-02-implementation.md) remains unchanged at full SHA **`92893e1ecd7c943010c2db3ccd61fd4ca272ee4f`**, path `docs/specs/COD-111/approvals/2026-11-02-implementation.md`, content SHA-256 **`9c2b46412ae1a3b483df8001c863c471a2596f4ef091af483a7e56f16dfc3298`**; original user decision **`c8c59db5-2516-4902-8b0a-d922fa7a0a43`** is preserved.
- Scryer project `/Users/rabii/Projects/Repositories/codebaseGraph`, change **`chg-dek36q`**, Repository Runtime Resolver **`node-143.description`**, original signed complete carrier **145971 UTF-8 bytes / SHA-256 `b005a98efeb5ae439b0376dbc2018d00f1ac69cc769f0fa4258fd1158f649603`**, and original 19-entry sign-off remain historical authority. This record makes no model, claim, sign-off, source-anchor, test-attachment, verdict or fold change. Engineer owns any method-reference reconciliation.
- Approved implementation baseline remains main **`01792cb097f6ca23f679396871ef273006732120`**, implementation branch `feat/cod-111-worktree-graphs`. Unrelated work is not imported.

## Endpoint, exclusions and evidence state

The endpoint remains a **reviewed, verified implementation PR only** after applicable acceptance, independent Evidence Verifier execution, genuine claim/evidence reconciliation, fresh mapped Final Reviewer review and delivery gates. Keep this amendment and approval with the related eventual implementation PR. Merged proposal PR #176 is not reopened; this record does not create an implementation PR.

No merge, release/publication, live installation, daemon restart, dependency/tool installation, VM/service/configuration/credential/settings change, permission/deletion expansion, unrelated work or aggregate closure is authorized. Existing owner and prerequisite gates remain.

**No full-suite pass, acceptance pass or Windows/macOS qualification is asserted by this method approval or its Git/hash readbacks.** Actual execution and independent verification remain pending at the final source snapshot. No product constant changes. This record deliberately does not embed its own containing commit SHA.
