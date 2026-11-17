# CBG-RESTORE-01 actual minimal sanitizer patch approval

Revision SEC-approval-0.1, 17 November 2026 (Australia/Darwin). Work: CBG-RESTORE-01 / COD-136.

## Actual decision and immutable subject

The maintainer answered **Approve the minimal ammonia security patch** to request_user_input_async call_6f8ff61b16f14e41a581ddc866381c6b, item 0.

The question explicitly named full commit **fa25f63d989bf8984d808b9fe84a4f39d2fb05dc** and [security-audit-amendment.md](https://github.com/rabii-chaarani/codebaseGraph/blob/fa25f63d989bf8984d808b9fe84a4f39d2fb05dc/docs/specs/CBG-RESTORE-01/security-audit-amendment.md), revision SEC-0.1. It described updating only ammonia 4.1.8 to 4.1.9 and its wiki manifest lower bound, preserving all 222 other package blocks and audit rules, and resolving the actual RUSTSEC-2026-0533 CI blocker. Runtime corrections were to continue independently.

Approved file blob: 51a6b00c39e9ecece57b7da66055825cfed5876c. Approved tree: d506d390e6da63a53e511b7d3cbeaecaa7ed1193. This records actual human input, not a selected default, reviewer conclusion or earlier restoration/CI approval. The proposal and earlier pending-state evidence retain their historical meanings.

## Bounded dependency implementation GO

After this actual decision record is committed, published and checked, the same sole technical owner /root/restoration_implementation, native default/Terra/medium/fork_turns:none, may implement SEC-R01–SEC-R05 in the two already-owned paths:
- crates/k-wiki/Cargo.toml: ammonia ~4.1.8 to ~4.1.9.
- Cargo.lock: ammonia 4.1.9 with registry checksum 935c841d86ee68ba89a983ea6e4255436f769ab1e2b701574653baf4b30739ce.

Use the actual Cargo resolution or checked exact inert proposal. Prove all 222 other package blocks/dependency edges remain byte-identical, with no feature/backend/ABI/native asset/sanitizer allowlist or other member-minimum change. Root Rust1.98 and explicit1.98.1 builds remain.

No advisory ignore, new warning exception, weakened policy or release authorization is granted. Execute the unchanged audit and affected wiki rendering/sanitization/API/package checks, plus the required final-source native, independent review and publication gates. Report existing warning handling and application-exposure limits accurately.

Original restoration approval d809fe0f8289bbf7d167a891bd7c31160f27f0e1 and dev-CI approval d091767dd26d5b25f963d20129bd0abc982fc46b remain unchanged. Root owns this decision/Git/tracking and remains product-quiescent; the technical owner remains sole product writer. No merge, release, installation, daemon/client change, live graph operation or unrelated issue closure is authorized.
