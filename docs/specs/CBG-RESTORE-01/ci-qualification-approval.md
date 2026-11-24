# CBG-RESTORE-01 actual dev-CI qualification approval

Revision CI-approval-0.1, 17 November 2026 (Australia/Darwin). Work: CBG-RESTORE-01 / COD-136; CI-R01–CI-R05 and CI-A01–CI-A03.

## Actual human decision

The maintainer answered **Approve the bounded dev CI exception** to request_user_input_async call_d5b6ea5cdb8e4600af8642084e57e382, item 0.

The exact question named full commit **d091767dd26d5b25f963d20129bd0abc982fc46b** and [ci-qualification-amendment.md](https://github.com/rabii-chaarani/codebaseGraph/blob/d091767dd26d5b25f963d20129bd0abc982fc46b/docs/specs/CBG-RESTORE-01/ci-qualification-amendment.md), revision CI-0.1. It described adding only dev to the strict CI PR-target list/policy/tests, full Mac/Windows testing on dev, preservation of all existing checks, main-only push policy and release restrictions, and the exact-set COD-103 constraint requiring this exception.

Approved file blob: 5539c056ace376106d4eff4b75f8a41d420515e6. Approved tree: 206639d0a16e674299eec9e9810d26586bdddd52. This is actual human input, not a selected default, the original restoration approval or a reviewer verdict. The historical proposal and earlier pending-state records keep their original meanings.

## Bounded implementation GO

Once this actual decision/GO record is committed, published and read back, the same sole technical owner /root/restoration_implementation (native default/gpt-6.1-terra/medium/fork_turns:none) may implement the approved exception inside the already-assigned paths:
- .github/workflows/ci.yml: add literal dev to the existing pull_request base set.
- crates/xtask/src/main.rs: update both exact expected sets, diagnostics and affected policy tests.
- crates/xtask/src/ci.rs: enforce/test the approved full classification/freshness behavior for dev if needed, without broadening other policy.

The exact set becomes main, dev, cod103-stack/cod-105, cod103-stack/cod-106, cod103-stack/cod-56, cod103-stack/cod-107 and cod103-stack/cod-108. Keep existing activity types, main-only pushes, full native run-tests=true for Mac ARM64/Windows x64 on dev, strict policy checks, source/base/head freshness, required aggregation, security/artifact boundaries and release authorization.

Do not add wildcard/other bases, permissions, credentials, privileged/manual triggers, lightweight dev exemptions or a release/publishing/merge route. Run affected positive and negative policy/classifier/required regressions and actual hosted CI at the final exact source and dev base/head. Configured tests or another source/base are not passing qualification.

The original restoration approval at d809fe0f8289bbf7d167a891bd7c31160f27f0e1 remains unchanged. The non-CI GO at 0c814942fbad63aede9ee978e53529b395ff3b12 remains in force; this record releases only its separate CI-exception hold. The owner remains sole writer; root remains product-quiescent and serializes Git/tracking. Original branches/models/directives/evidence, native C4 scope/order and no-live-change limits are retained.

This exception narrowly supersedes only COD-103's prior six-base CI-set constraint. It does not rewrite the original COD-103 contracts, approve new product scope or claim its aggregate closure. Separate independent execution, fresh final review and publication verification remain required. No merge, release, installation, daemon/client change or live store operation is authorized.
