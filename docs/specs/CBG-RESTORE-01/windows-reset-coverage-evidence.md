# CBG-RESTORE-01 central fixture capture checkpoint

This follows [windows-reset-verification-evidence.md](windows-reset-verification-evidence.md) under the pre-code [fixture coverage brief](windows-reset-fixture-coverage.md). Core revision 0.1 and CI/security approvals are unchanged. The change supplies diagnostic coverage; no Windows reset cause or behavior fix is claimed.

## Observation requiring wider capture

At published 9941bdbd99813c38dd77929acd6868d5637e27a3, CI843/run44103319039 failed legacy_worker_pid_recovers_without_signalling_and_respects_writer_ownership in Windows integration shard1/job134109289055, tests/refresh_recovery_process.rs465. A post-send graph_health id 89 returned nonretryable coordinator_transport_error/Windows 10054. Seven tests passed and one failed; no CGR trace was present because this fixture used mcp daemon serve with stderr=null and no opt-in.

Exact failure log SHA256:1e1f35ed4949704bd926f23db2c5e7ce5ca30183ab3492d25dda2a774d7718c7. Artifact 14339860956 SHA256:707c5b8d36e01d3780cb8fa3f5c3b158660aa5deba65ee5d557b7a5ebac884b9. The earlier traced failure cohort passed, which does not resolve either historical failure.

CI843 ended with 19 successful jobs, two unexecuted Mac jobs, one Windows test failure, both artifact jobs skipped and failed required aggregation. Mac job annotations independently state that hosted ARM64 runners were not acquired after multiple attempts; those jobs ran no step. That infrastructure cause applies only to the Mac cancellations, not to Windows resets. No source/policy change or failed-test rerun was used to erase these outcomes.

## Reviewed fixture-only change

Source **085075e82ea8fae32e080bbbb1e52ac02cdf14a3**, tree **ca937af2ceb0307eb724a59d1da85f460554dd9d**, changes only tests/refresh_recovery_process.rs and tests/support/lifecycle_faults.rs after method brief 6f5000d. Production coordinator, transport, configuration, workflow, dependency and public inputs retain their reviewed hashes.

Both central fixture-owned HTTP constructors immediately enable the existing exact trace flag and attach the bounded/schema-filtering drain. ChildGuard preserves its original kill/wait ownership and reports safe diagnostics only during panic unwinding. Lifecycle fixtures transfer reporting ownership after successful construction, retaining intentionally stopped-process tails for later failures. Early panics remain guard-owned and reporting is not duplicated. Non-HTTP build/guardian/unrelated-PID children remain plain/untraced.

Existing assertions, deadlines and request-submission counts are unchanged. The new central-daemon synthetic test induces a post-send close in a controlled fake coordinator, verifies one submission and safe retained panic diagnostics, and unwinds the owned daemon before releasing its lock. It tests observation plumbing, not the real Windows cause. Production trace bounds, privacy/default-off/release gating and best-effort limitations remain those already reviewed.

Fresh independent default/Sol/xhigh/fork-none adversarial review passed this two-file change with no confirmed P0–P3. Graph scope remains bounded to recorded project receipts and exact reviewed source. The reviewer ran no tests or mutations.

## Independent execution

Fresh default/Terra/medium/fork-none verification independently rebuilt the relevant root/test inputs with Rust 1.98.1, verified native019/OpenSSL, all features and the existing four-thread test profile.

- Entire current refresh_recovery_process binary: **26 passed**, one ignored subprocess helper, 229.005s, no LEAK observed. JUnit SHA256 **c27fd44c87a4f8916bb8301bcdc0f26accebbe85d0a08123b17e47274bf870ed**.
- Focused coordinator library: **21 passed**, 451 filtered rather than ignored, covering no-replay/nonretryable ambiguity, deadlines and bounded/private/default-off trace behavior. JUnit SHA256 **e3ddeea57cf5d9396d0cbe7320fcc830473f06e9b342d4c59317b9f18bf609ea**.
- Formatting and strict workspace/all-target/all-feature Clippy passed.
- Actual executables/non-system dependencies were frozen before later builds. The rebuilt debug CLI hash 43254e021ae808b6e6921c8fc4fbdb512763b66fc9dc3f1c7ffaac2f54428fc6 differs from the earlier CLI; different build-source paths were recorded, not claimed byte-identical.

The private archive's implementation/independent-final/085-verification-report.json contains exact source/commands/native/profile/output identities; 085-executables.json and 085-coordinator-executables.json retain their respective 88/89-file inventories. The prior independent 885-test workspace remains evidence for unchanged inputs; current affected-module execution is stated separately, not relabelled as a new full-workspace run. Final-head native workspace/artefact checks remain mandatory.

The owner's final nine-case run passed without LEAK, JUnit f635243a6172556c3e2088e134206eb50b7639787124d043e19d7dd85b96fba3. An earlier owner seven-case run reported LEAK on a pure schema-filter test that spawns no process and uses no constructor/drain. Its retained output identifies no surviving process/handle. The precise cause remains unexplained; later clean owner/independent runs do not establish a cause or suppress that evidence. An initial sandbox loopback denial is likewise retained rather than counted as behavior qualification.

## Remaining gate

Publish and rebind the exact reviewed/tested source, then obtain an unchanged-assertion Windows observation and all required native/artifact/audit/policy/package results. Preserve the first failure and static trace; do not replay failed requests, weaken assertions, infer a cause from absent best-effort output, or substitute old-head greens. The Windows 10054 cause remains unknown. Restoration/C4 handoff, merge, release and live-store/client changes remain outside the results recorded here.
