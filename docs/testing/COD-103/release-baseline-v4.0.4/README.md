# v4.0.4 independently verified publication baseline

This directory versions the accepted independent Evidence Verifier audit for COD-103's release-baseline prerequisite only. Selected source: **`7125900c740297be5c60821dbeb9811f32a5894a`**. Approved native scope: **macos-arm64 and windows-x86_64**.

## Preserved reports

- [report.md](report.md): authored human-readable conclusion and evidence.
- [result.json](result.json): authored machine-readable observations, public URLs, source/run/attempt identities, artifact IDs, checksums and limits.
- [evidence-file-manifest.json](evidence-file-manifest.json): authored inventory of the original audit artifacts.

Production copied these three files byte-for-byte without rewriting the Verifier's conclusions. This README is a separate Production index.

## Durable files versus retained raw artifacts

The manifest enumerates files in the Verifier's **original session artifact directory**, not files all committed adjacent to this README:

`/Users/rabii/.prime/agent/session-artifacts/01a18f18-3db0-73ba-a45a-f25faf5369ad/sub-a2a991d7/release-baseline-v4.0.4/`

Only `report.md`, `result.json`, `evidence-file-manifest.json` and this index are versioned here. Raw API inventories, inspections, authority-read receipts, actual public downloads, native archives, crate and artifact ZIPs remain in that original directory. Relative raw filenames in the preserved reports/manifest refer to that directory. No binaries, ZIPs or crates are committed to Git.

For reproduction, use the actual public URLs, artifact IDs and independently calculated checksums in the report/result. Native source CI is **40997892360 / attempt 1**; original native artifacts are **12955489108** (macOS) and **12957135488** (Windows). Governed publisher is **41010094891 / attempt 1**; staged-set artifact **12957666252** and metadata artifact **12958080289**. The original raw downloads are retained in the session artifact directory. Hosted Actions artifacts and session files are retention-bound and may later expire or become unavailable; this index does not promise permanent raw-artifact availability. Public release and registry URLs supply independently verifiable published bytes while available; recheck availability and hashes rather than substitute a different run, attempt, source or target.

## Authority and limits

- [Canonical approved COD-103 spec revision 0.2](https://github.com/rabii-chaarani/codebaseGraph/blob/7e79f7e924536f29d3212e83899b01c38afd39de/docs/specs/COD-103/spec.md).
- [Preserved combined approval record](https://github.com/rabii-chaarani/codebaseGraph/blob/383a481bd5a71c58dab7dd796ace92a365760da5/docs/specs/COD-103/approvals/2026-10-29-combined.md).
- [Current canonical spec](../../../specs/COD-103/spec.md) remains the delivery-scope authority; this audit does not approve a later amendment.

The accepted audit resolves only the published-baseline prerequisite for **C103-R06/C103-A06**. It does not qualify later main/COD-54 changes, consumer platforms, Windows 11 desktop/PowerShell A09, the complete COD-103 queue, a final release candidate or aggregate closure. Historical partial v4.0.3 is separate; no retry, upload, retag or other recovery is authorized here. No publication or endpoint authority follows from versioning these reports.
