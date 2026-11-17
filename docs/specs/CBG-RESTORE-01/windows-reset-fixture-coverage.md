# CBG-RESTORE-01 recovery-fixture diagnostic coverage

This is a routine verification-method extension of [windows-reset-verification-brief.md](windows-reset-verification-brief.md), under the unchanged approved restoration revision0.1/R05/R11. It supersedes only that brief's two-fixture opt-in scope. Public behavior, acceptance criteria, error/retry/deadline policy and all human approval records remain unchanged.

## Observed coverage gap

CI843/run44103319039 at9941bdbd99813c38dd77929acd6868d5637e27a3 failed Windows integration shard1/job134109289055: legacy_worker_pid_recovers_without_signalling_and_respects_writer_ownership, tests/refresh_recovery_process.rs465, graph_health id89 post-send Windows10054/nonretryable coordinator_transport_error. Seven tests passed and one failed. That fixture used start_daemon/mcp daemon serve with stderr discarded and no trace opt-in, so zero CGR_COORD_TRACE records give no cause information.

The same run's two previously traced cases passed without proving the earlier reset fixed. Two Mac test jobs never acquired runners according to GitHub check annotations; these are separately missing qualification. Both native artifacts were skipped and the required aggregate failed. No failed Windows test is treated as infrastructure or rerun until success.

## Bounded method and ownership

Before code, root assigns the same sole technical owner /root/restoration_implementation only:
- tests/refresh_recovery_process.rs
- tests/support/lifecycle_faults.rs

No production coordinator, protocol, workflow, configuration, manifest, dependency or new file ownership is granted. Preserve other work; no nested agents, Git/model/wiki/Linear/live mutations.

Enable the existing exact diagnostic flag and immediate bounded/schema-filtering stderr drain in the two central fixture-owned HTTP constructors: start_daemon (mcp daemon serve) and start_http_server_with_env (mcp http). This covers direct callers in this recovery test module and its lifecycle_faults submodule. Install/build/guardian/unrelated-PID children remain untraced; there is no global environment change.

Returned ChildGuard objects may hold optional drain state so existing panic unwinding kills/waits the same owned process, then reports only whitelisted bounded trace and nontrace_output_present. Preserve intentionally stopped-process trace tails where the lifecycle fixture already retains diagnostics for a later failure. Guard-level and aggregate reporting must not duplicate output. Capturing tail state must not retain a child/owner/native-lease object or change kill/wait ownership, extend process lifetime, or block on a reader while its writer is alive.

The existing bounds and privacy remain: fixed event kinds/scalar observations, 32-event ring,2048-byte one-shot producer,128-byte pending line,8192-byte retained tail, continuous drain/overlong discard, no raw tokens/payload/endpoints/environment/state/arbitrary errors. Ownership labels remain advisory/incomplete; no new PID/ownership assertion. Default-off and release omission are unchanged.

Do not change any existing success/error/storage/containment assertion, deadline, retry classification or number of submissions. No production behavior fix is authorized without an established causal path and a separate bounded assignment.

## Verification and handoff

Exercise the central legacy-daemon constructor with a small induced synthetic post-send read failure to prove the flag, bounded drain and safe panic diagnostics actually reach the retained output. This checks observation plumbing, not the Windows cause. Keep original fixture assertions and fail on the first unexpected result. Reexecute affected legacy-PID/writer-ownership and traced owner/guardian-loss cases, existing drain/privacy/no-replay checks, formatting and strict applicable Clippy. Freeze actual source/commands/binaries/runtime/JUnit and failures before target reuse.

Fresh independent Sol/xhigh focused review and independent Terra execution must verify coverage, privacy, lifetimes and unchanged behavior. Rebind later documentation/publication to exact tested inputs. The next unchanged-assertion Windows CI observation remains required; no qualified restoration/C4 handoff, merge, release or live activation is implied.
