# PR163 runtime CI diagnosis (in progress)

## Identity
- PR head: b603712e09a3e56651d0b5918af7e13a96d75bed.
- Actual CI checkout/build commit: 4162dd67e1720f552bc3e2f3424192343d93d41d (synthetic merge of base7125900 and PRheadb603712).
- `git diff --stat b603712 4162dd6`: empty; source tree identical.
- Isolated clean detached worktree: /private/tmp/codebasegraph-pr163-ci-repair-20261030.
- No product/config/dependency edits or test/build execution yet.

## Failure evidence
Run41013561222 job123951179128. Archive validation and install succeeded at07:54:39–40. At07:55:33 installed CLI failed with `failed to start guarded Ladybug phase: Resource temporarily unavailable (os error35)`.
Cleanup: cancelled=true, containment_empty=true, signal9, high_water_bytes5474560, supervision_error=None.

## Source trace at exact source tree
- `src/db_writer/phase.rs:248–260`: prepare succeeds; only child.start() failure emits exact message.
- `src/process_lifecycle/mod.rs:266–267`: PreparedChild::start only calls control(START); no spawn/thread call exists on parent branch.
- `src/process_lifecycle/mod.rs:549–575`: control connects loopback with250ms deadline, sets250ms IO timeouts, sends framed START, awaits framed OK via read_socket_frame.
- `src/process_lifecycle/mod.rs:587–610`: read_socket_frame sets remaining socket read timeout and directly stringifies IO errors; macOS receive timeout may be errno35.
- `src/process_lifecycle/mod.rs:470–480`: guardian starts already-prepared worker and immediately attempts OK response; write result ignored.
- `src/process_lifecycle/mod.rs:510–520`: high_water>0 assigned ONLY after guardian started=true, proving accepted START in this incident.
- `src/process_lifecycle/unix.rs:136–141`: child.start writes one S byte into already-spawned child's stdin; no fork.
- Guardian later accepted CANCEL, no supervision error, cleanly tore down running worker.

## Current conclusion and limits
Failure is app lifecycle loopback control, not archive build/sign/install/OpenSSL. START demonstrably committed despite parent IO error. The log cannot identify exact failed IO syscall because control erases context. Timeout awaiting ACK is strongest hypothesis, not proven; connect/write syscall EAGAIN remains possible before a later/late START handling observation. Do not replay START, blindly raise deadlines, weaken containment or remove smoke.

## Discovery limitations
Root graph semantic search found phase.rs, xtask/main.rs and rss.rs directly. Lifecycle files absent as direct File nodes; graph-returned durable docs reference their full paths (indirect discovery authorized by coordinator). Explicit phase runtime context fetched. Graph/model older-root orientation is not target-SHA evidence; source trace uses actual fetched target tree.

## Next bounded check
Confirm Darwin timeout errno and identify available exact-tree binaries/build cache before standard isolated app reproduction. Source edits remain pending coordinator approval/role assignment.

## Executed native Darwin socket check
`rustc --edition=2021 darwin_socket_timeout.rs -o darwin_socket_timeout; ./darwin_socket_timeout` on Darwin25.6 arm64, Rust1.96. A normal loopback server reads one control byte and delays response400ms; client waits250ms. Result: display=Resource temporarily unavailable (os error 35); kind=WouldBlock; errno=Some(35); elapsed_ms=250
This proves the exact observed errno/message can be an ordinary socket receive timeout after a server received the command. It does not prove the original CI delay or authorize a timeout change.

## Final ordering and mechanism assessment
At exact executed source tree4162dd6 (identical to headb603712):
1. phase.rs `run_guarded_phase` calls `PreparedChild::prepare`.
2. prepare creates launch/lease, spawns guardian, waits up to15s for ready.json, then returns. It does not send PING.
3. Guardian setup takes startup lease, spawns owner monitor, binds loopback listener, spawns gated worker and waits inheritance ACK, writes ready.json, drops startup lock, enters control loop.
4. Parent immediately calls start()->control(START). NO PING occurs between prepare and START on this path. PING exists only as a supported guardian command in this module; there is no internal caller. Claims of prepare->PING->START ordering are false for this exact tree.
5. Each guardian loop iteration checks owner, accepts at most one connection, normalizes accepted socket blocking, reads frame up to100ms, authenticates nonce, starts gated worker if needed, writes OK (write result ignored), checks worker exit, samples RSS only if started, sleeps25ms.
6. Before initial START, started=false, so RSS cannot delay initial ACK. The ACK write precedes RSS. After accepted START (including when client misses ACK), subsequent commands may encounter prior RSS work plus25ms sleep. PING itself has no work before ACK other than common parse/auth; after PING the common loop samples RSS only if started.

No inherent deterministic >250ms delay before first START ACK is proven. Already-normalized accepted socket rules out prior nonblocking-socket regression. child.start is a one-byte pipe write into prepared child; no spawn or resource allocation. An ordinary scheduler stall can exceed250ms, but original CI cause is not known. No behavior-preserving mechanism change is supported by available evidence. Removing RSS/moving ACK cannot fix an initial ACK already before RSS; retries would replay known-committed START and are rejected.

## Bounded policy decision
Recommended option if user authorizes tolerance change: only exact START response deadline changes250ms to existing DEADLINE5s; connect/write250ms, PING/CANCEL250ms, inbound frame100ms, FIFO/lease/proof/teardown behavior unchanged. This increases maximum failed-START wait by4.75s and does not alter recovery probes. No replay. This is explicit timing policy, not proof that CI suffered a scheduling stall. Broad5s control is not recommended (would slow PING/CANCEL/recovery). Alternative: preserve all timing and rerun exact head once as an infrastructure transient, while retaining unresolved cause; that risks recurrence and is not a fix.

## Executed bounded evidence
Standalone authenticated framed-loopback check: nonce+START validated, exactly one commit, ACK delayed350ms. Current250ms receive budget returns errno35 at251ms; selective5s returnsOK at355ms. Sources/output in authenticated_delayed_ack.rs/.txt. This is a faithful isolated IO-mechanism check, not an execution of product control(), and does not independently qualify product.

## Final state and knowledge assessment
Isolated worktree remains git-clean and detached atb603712. No product source/test/config/dependency changes, no build/install, no process exhaustion, no CI retry. Git setup/fetch was serialized. No memory write warranted: result remains diagnosis with unresolved originating delay; reusable errno behavior alone does not justify repository memory.

## Coordinator decision handoff
Option A (recommended, requires explicit runtime scope expansion): exact START ACK read budget250ms->existingDEADLINE5s. Connect timeout250ms, socket write timeout250ms, initial socket read setting250ms (overridden by framed read remaining budget), PING/CANCEL ACK250ms, inbound control frame100ms unchanged. No replay/retry. Failed START still invokes terminate(CANCEL phase-start-failed), retains proof unless cleanup establishes containment_empty, and removes request/workspace only under existing proven-empty rules. Maximum failed START acknowledgement wait grows4.75s; recovery cancellation timing unchanged.
Option B: no runtime change; authorize one exact-head CI rerun, retaining unresolved timing failure and recurrence risk. This is not a code repair or passing evidence until executed.
Tests for option A: actual product control() with authenticated one-START server and delayedACK>250ms/<5s succeeds once; non-START delayedACK retains250ms failure; unterminated/trickle boundedness, nonce rejection, closed response and cancellation/empty-proof preservation remain covered; native focused lifecycle tests plus unchanged final archive smoke. No test should replay START or weaken guard/smoke.
Inference strength: fresh per-guardian nonce and only START can set started; started+RSS means guardian parsed a full authenticated START frame. A failed connect cannot have sent it. write_all normally returnsOk only after the full newline-framed request was written. Thus response receive timeout is strongest complete source/syscall trace; because errors have no syscall labels, it remains an inference rather than captured CI syscall evidence.
Stop: diagnosis complete, await explicit assignment. No further source/runtime action.
