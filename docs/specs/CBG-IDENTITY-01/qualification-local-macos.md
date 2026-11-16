# COD-135/Q01 — Local macOS qualification

Revision0.1, 2026-11-17 Australia/Darwin. This is the macOS policy delta from CBG-IDENTITY-01/spec.md revision0.1 at `067f3486fc136493d8d240d109fdb85a919d541f` to logical revision0.2; [Q02](qualification-hosted-windows.md) records the later Windows decision. The original exact approval and history remain in [approval.md](approval.md).

The maintainer answered **“use the local macos”** to request_user_input_async `call_a04ae21a6f9d4356a211cedac01ffd87`, item0. The exact question asked for a current macOS27 ARM64 qualification host, stated Apple's latest27.2.1 and that the local/observed-CI hosts were on26, and distinguished the pending Windows11 question. This actual decision selects the existing local macOS26.6.2 build25G83 ARM64 for COD-135's final Mac checks. It does not claim that host is current stable.

Only ID-A09's macOS environment is changed. ID-R24, all behavior/safety requirements, native helper/old-binary refusal/failure tests, genuine JUnit, independent Terra/medium execution, fresh Sol/xhigh adversarial review and confirmed-defect resolution remain required against the final relevant snapshot. Earlier moving-source tests do not become final acceptance by this decision.

This Mac decision alone changed no Windows requirement; the later [Q02](qualification-hosted-windows.md) explicitly selects hosted Windows CI for this issue. The global supported-platform policy, CBG-PLAT-01, other issues (including cancelled COD-113 and CBG-REBUILD-01), aggregate qualification/closure, merge/release and unrelated installations are unchanged. No current-store apply or runtime installation is approved; the actual preview-fingerprint decision remains separate.

Linked-issue impact is limited to COD-135 ID-A09 and the corresponding Mac execution prerequisite in Scryer chg-r9y7bh's same-change brief. Its three architecture responsibility statements and owners are unchanged. Root will bind this faithful codification and scope decision to its committed full SHA in a separate approval record before counting final Mac qualification. No new or uncreated behavior is approved by that record.
