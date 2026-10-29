# Preserved guardian startup failure: evidence and limits

Source snapshot: `47808ac34bcff79882abee4ab796c16c0621ed05`.
Failed tree: `C:\Users\rcha0053\pr158-qualification\smoke-repo` (not modified).

- Phase 0 and phase 1 reached READY and succeeded (exit 0).
- Phase 2 has proof and receipt but no READY. Its receipt is `code:1`, `cancelled:true`, `high_water_bytes:0`, `containment_empty:true`, `supervision_error:null`. The stderr leaf is empty.
- Phase 2 `proof.json` was created at **2026-10-28 11:47:26.871749 UTC** and last written at **12:10:51.336008 UTC**: about **1404.464 seconds** apart. The same interval was about 0.184 seconds and 0.158 seconds in phases 0 and 1. The proof-origin receipt in phase 2 was only created at 12:10:51.272592 UTC.
- Static flow: `src/process_lifecycle/windows.rs:319` publishes proof before `CreateProcessW`; the child is created with `CREATE_SUSPENDED` (line 363). `src/process_lifecycle/mod.rs:444-451` publishes READY after spawn and before the START command resumes the worker. Owner loss cancels the lifecycle (lines 449-462). Windows teardown uses `TerminateJobObject(..., 1)` (windows.rs:416).
- Therefore code 1 is the cancellation teardown status, **not evidence of native-loader or Ladybug worker startup failure**. No worker code is run before START.
- The timestamp gap spans lifecycle proof construction/publication. `write_private_json_recorded` creates the temporary file before acquiring output tracking/coordination and creating its origin receipt (`mod.rs:690-713`). Wall timestamps also jumped across recent tool calls, and process/host suspension is possible. A file timestamp gap does not prove active I/O time or filesystem flush latency. The artifacts do not identify the operation or source cause: disk flush, coordination wait, host/process suspension, or path-related effect are all unproven.
- All three phase proof paths have equal length (204 characters without the extended prefix). Successful phases 0/1 weaken a simple fixed path-limit explanation. A fresh short-path tree is useful as an independent success trial, but cannot by itself attribute this failure to path length.

No source changes, timeout/other constant changes, reruns, or cleanup were performed by this investigation. Detailed observed timestamps, receipts, and proof/receipt hashes are in `guardian-startup-investigation.json`. The existing failed tree remains preserved.
