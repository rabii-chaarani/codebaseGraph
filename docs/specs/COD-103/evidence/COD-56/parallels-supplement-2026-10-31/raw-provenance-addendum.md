# Raw result provenance addendum

Original report.md, manifest.json and all prior artifacts are unchanged. No guest commands or tests ran for this addendum.

Guest Set-Content -Encoding UTF8 wrote original JSON to the owned mirrored Documents staging directory. Host derived copies were produced by read_text(encoding="utf-8-sig"), json.loads, then json.dumps(indent=2) and write_text. This removes the BOM and changes whitespace/newline serialization; it is not a test rerun. Byte-exact originals are now copied under original-raw. Decoded raw and derived JSON values compare equal for both reports.

12 is the four-case Continue invocation; 18 is a distinct four-case Stop + Set-StrictMode -Version Latest invocation. Exact owned scripts 12-mechanism.ps1 and18-strict-mechanism.ps1, and launch commands12-memory-launch/18-strict-launch in commands.json identify separate executions. Result PayloadBase64 stores exact function-plus-caller text; stdout records ErrorActionPreference2 (Continue) vs1 (Stop). Native system where.exe and unchanged extracted function definitions are retained as described in report.md. Neither set is full installer File/A09 qualification.

The earlier parent size/hash concern mixed the staged original size with the derived hash. No evidence hash change or rerun was established. Earlier FIRST-chunk message was also corrected separately; historical raw files remain intact.

## Exact identities

[
  {
    "source": "/Users/rabii/Documents/codebasegraph-cod56-vm-11023706-4804-4438-b044-e9768538a0c8/12-mechanism-result.json",
    "raw_copy": "/Users/rabii/.prime/agent/session-artifacts/01a18f18-3db0-73ba-a45a-f25faf5369ad/sub-575ea352/cod56-parallels-arm64-supplement/original-raw/12-mechanism-result.json",
    "raw_bytes": 17853,
    "raw_sha256": "639f0c7887e0ebc34d816a158bb07c9c72cefe1d4200980fa66f10200ff1ae1c",
    "derived": "/Users/rabii/.prime/agent/session-artifacts/01a18f18-3db0-73ba-a45a-f25faf5369ad/sub-575ea352/cod56-parallels-arm64-supplement/12-mechanism-result.json",
    "derived_bytes": 14625,
    "derived_sha256": "922962b1b6862a2f35ec6986b922362cc001fc464a96056977c30a846158da0c",
    "decoded_values_equal": true
  },
  {
    "source": "/Users/rabii/Documents/codebasegraph-cod56-vm-11023706-4804-4438-b044-e9768538a0c8/18-strict-mechanism-result.json",
    "raw_copy": "/Users/rabii/.prime/agent/session-artifacts/01a18f18-3db0-73ba-a45a-f25faf5369ad/sub-575ea352/cod56-parallels-arm64-supplement/original-raw/18-strict-mechanism-result.json",
    "raw_bytes": 17997,
    "raw_sha256": "6fdf2b79ac5ea662015e2ecf4dffb87e60b52502685344fa37de1d1d8f9feeae",
    "derived": "/Users/rabii/.prime/agent/session-artifacts/01a18f18-3db0-73ba-a45a-f25faf5369ad/sub-575ea352/cod56-parallels-arm64-supplement/18-strict-mechanism-result.json",
    "derived_bytes": 14769,
    "derived_sha256": "abf93b48b642c36b9f5542b4aecca32f4b87284721095246875d378f2f03f7f1",
    "decoded_values_equal": true
  }
]

## Harness equality
12-mechanism.ps1 staged and artifact bytes identical
18-strict-mechanism.ps1 staged and artifact bytes identical

## Unchanged prior identities
{
  "manifest.json": "4c2cc979f54d1c2625abbbae04d60f252c65cb3cfb1e0eec739bceea084e6bda",
  "report.md": "e5f64a9d3dac31a210793f20be3774fe8c50e37e507b4010b3d290e91e44f1d1"
}
