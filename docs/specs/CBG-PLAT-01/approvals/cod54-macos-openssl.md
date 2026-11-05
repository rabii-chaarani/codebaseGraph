# COD-54 — macOS bundled OpenSSL implementation approval

- Work: `CBG-PLAT-01/S03`, [COD-54](https://linear.app/codebasegraph/issue/COD-54), Maintenance `P-COD-2`, CodebaseGraph workspace/team COD.
- Canonical specification: `docs/specs/CBG-PLAT-01/spec.md`, revision **0.10**, full approved Git SHA **`1a9db7b558b60248888408daa6268a3b45f2f0df`**.
- Immutable specification: https://github.com/rabii-chaarani/codebaseGraph/blob/1a9db7b558b60248888408daa6268a3b45f2f0df/docs/specs/CBG-PLAT-01/spec.md
- Verified Git blob: `3e3bc03546607c4d6cc558fa34699f4349688640`.
- Exact approved scope: **COD-54/S03 only; R03 / A05–A06**.
- Decision reference: 2026-10-27, this Prime Agent conversation. The assistant asked “Approve implementation of COD-54 only?” and named revision0.10, R03/A05–A06, bundled compatible OpenSSL libraries with relative paths/licenses/checksums and final-archive verification on clean Apple Silicon macOS13.3/current release. The user answered **“approved”**.

## Included

Reuse settled D02: compatible redistributable OpenSSL dylibs in macOS ARM64 prebuilt releases, relative install names/runtime paths, licenses and checksums, with no separate Homebrew/OpenSSL prerequisite. Reuse settled D05a: target macOS13.3+ Apple Silicon and require actual final installed archive execution on the minimum and a current macOS release before claiming support. Inspect final Mach-O dependencies and cover missing/corrupt/incompatible-library failures. Unavailable qualification environments remain pending, not waived.

## Excluded and remaining gates

No other slice, static-linking switch, ABI/dependency-version upgrade, new macOS Cargo-install contract, platform expansion, process-containment change, package/release publication, merge, deployment, service restart or aggregate closure. Existing approvals and maintainer closure ownership remain unchanged. Implementation/review, local/native qualification and aggregate closure remain separate states.

Initial source baseline: main `34eac4f8644adc046093babd84ee1fdbffdae853` (isolated branch `fix/cod-54-macos-distribution`). Original dirty user checkout is preserved. Scryer change `chg-h7rgh8` records the approved existing-claim restoration; Release Gate `resp-16/17/255–258`, Graph Store `resp-113`. No new claims or topology are invented. Detailed execution ownership and prerequisites will be recorded in Scryer before dependent implementation; applicable fresh proposal/final review gates remain mandatory.
