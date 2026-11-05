# COD-54 / PR163 — START acknowledgment timing approval

## Identity and preserved authority

- Work: `CBG-PLAT-01/S03`, [COD-54](https://linear.app/codebasegraph/issue/COD-54), [PR163](https://github.com/rabii-chaarani/codebaseGraph/pull/163); R03 / A05–A06 remain unchanged.
- Canonical specification revision0.10: [`docs/specs/CBG-PLAT-01/spec.md` at full approved SHA `1a9db7b558b60248888408daa6268a3b45f2f0df`](https://github.com/rabii-chaarani/codebaseGraph/blob/1a9db7b558b60248888408daa6268a3b45f2f0df/docs/specs/CBG-PLAT-01/spec.md).
- Original implementation approval: [`approvals/cod54-macos-openssl.md` at `71dc27e92857c4215bbbe583f104de7f341dc816`](https://github.com/rabii-chaarani/codebaseGraph/blob/71dc27e92857c4215bbbe583f104de7f341dc816/docs/specs/CBG-PLAT-01/approvals/cod54-macos-openssl.md).
- Current-macOS qualification amendment revision0.11: [`spec.md` at full approved SHA `8a410c7a22f5291f06a1cece5cd9e7c0be255e72`](https://github.com/rabii-chaarani/codebaseGraph/blob/8a410c7a22f5291f06a1cece5cd9e7c0be255e72/docs/specs/CBG-PLAT-01/spec.md); [D05a approval at `5c2e9a56d0fa87779fb31312562f4e53f902f0ac`](https://github.com/rabii-chaarani/codebaseGraph/blob/5c2e9a56d0fa87779fb31312562f4e53f902f0ac/docs/specs/CBG-PLAT-01/approvals/d05a-current-macos-only.md).
- Scryer approved repair/execution brief: `chg-1k9jre`, project `/Users/rabii/Projects/Repositories/codebaseGraph`; diagnosis parent `chg-mk8x5r`. Coordinator recorded the actual narrow user sign-off; no new model claims or topology. Backend owns only the lifecycle source/test file; Production owns this approval/evidence record and serialized Git delivery.
- Repair baseline: PR head `b603712e09a3e56651d0b5918af7e13a96d75bed`. Failed CI actually executed synthetic merge `4162dd67e1720f552bc3e2f3424192343d93d41d`, with parents main `7125900c740297be5c60821dbeb9811f32a5894a` and that PR head. Both source trees are `a7b5908da9a9f900a748746c24264e5c54930314`; head identity is not substituted for executed-merge provenance.

## Actual decision and exact authorization

Decision reference: **2026-10-30, this Prime Agent conversation**. The coordinator presented a selective START acknowledgment response deadline change from250ms to the existing bounded5-second `DEADLINE`, with no other control or containment change, versus leaving runtime unchanged and rerunning CI. The user replied **“approve the 5-second”**. The coordinator transmitted this exact approval to Production before this record was authored.

This authorizes a focused internal timing restoration in **`src/process_lifecycle/mod.rs` only**, including its focused regression tests. It is an explicit narrow exception to the original packaging-only scope/process-containment exclusion, not silent replacement of specification revisions0.10/0.11 or their immutable approvals. No new public requirement, acceptance criterion, platform contract, architecture claim or qualification is introduced.

### Approved behavior

- Increase only the response-frame read budget for the exact authenticated `START` command from250ms to the existing `DEADLINE` of5seconds.
- Preserve250ms connect and write deadlines,250ms non-START acknowledgment deadlines (including `PING`, `CANCEL` and recovery cancellation), and100ms inbound control-frame deadline.
- Send START once. Do not retry or replay a request whose effect may already have committed. Preserve nonce authentication, framing and bounded reads.
- Preserve worker gating, containment ownership/proof, lifecycle leases, cancellation, teardown and recovery policy. Failed START still uses existing cancellation and cleanup; delete request/workspace evidence only under the existing proven-empty rules, and retain it when emptiness is unknown.
- Maximum START response wait increases by4.75seconds. No broader control deadline increase, guard bypass, smoke weakening, workflow/config/dependency change or unrelated runtime change is approved.

## Evidence, planned checks and limits

Failed run [41013561222, job123951179128](https://github.com/rabii-chaarani/codebaseGraph/actions/runs/41013561222/job/123951179128) built, signed, validated and installed the archive before the installed CLI reported errno35 starting a guarded Ladybug phase. Source state and successful cleanup indicate START committed; response receive timeout is the strongest source/syscall inference, not captured CI syscall proof. The original acknowledgment delay remains unestablished. An isolated Darwin socket probe reproduced errno35 after250ms; a separate authenticated delayed-ACK probe committed START once, rejected a350ms acknowledgment under250ms, and accepted it under5seconds. These diagnostic probes are not product-test execution or qualification.

Before handoff, execute focused actual-product control tests: one authenticated START with acknowledgment delayed beyond250ms but within5seconds succeeds exactly once; non-START delayed acknowledgment retains its250ms bound; preserve relevant invalid nonce, closed/malformed/unterminated/trickle-frame and cancellation/empty-proof cases. Run affected native lifecycle checks and the unchanged final archive smoke, plus applicable formatting/lint/full-suite checks, independent verification and fresh final review. Record exact tested/reviewed source and any platform/environment limits separately. Planned or attached checks are not passing execution.

This approval record must be committed before dependent source implementation. Existing R03/A05–A06, current stable Apple Silicon macOS qualification, runtime integrity/failure requirements and maintainer aggregate closure ownership remain unchanged. No merge, release/publication, deployment, restart, passing-CI assertion or issue/milestone closure is authorized. The delivery endpoint remains an update to existing PR163 after its applicable verification/review gates.
