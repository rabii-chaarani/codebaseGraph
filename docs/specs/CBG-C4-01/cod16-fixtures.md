# COD-16 illustrative contract fixtures

## Status and notation

These are concrete **planned fixture definitions**, not executed product tests. Work: `CBG-C4-01/F01/S02` / COD-16. They instantiate [supplement revision 0.2](cod16-contracts.md) and trace R01–R12/A01–A18 without inventing product implementation, actual pilot coverage, test reports or calibrated limits. Documentation validation may parse the examples and check their structure; that does not execute a compiler/evaluator or qualify an acceptance criterion.

Small graphs and evidence labels below are synthetic. `A`, `B`, `C`, `S`, `T`, `K`, `M`, `E` and `I` are fixture metavariables, not valid serialized IDs/digests or claims about repository symbols. Substitute complete valid IDs, digests, immutable revisions and SourceRefs when implementing the test harness. No placeholder can count as runtime evidence. Numerical graph values are hand-checkable test data, not execution caps or pilot budgets.

## Existing caller compatibility — F01–F03 / R12 / A15–A16

Compile these independent caller fixtures unchanged against the source baseline and eventual implementation. They are source snippets for later qualification; no Rust build is run for this documentation task. The old enum match must remain exhaustive without a wildcard over OperationRequest; the old literal must require no additional fields; the custom executor must require no architecture trait implementation.

```rust
use codebase_graph::api::{
    ApiError, OperationExecutor, OperationRequest, OperationResponse, OutputFormat,
    QueryRequest, RepoSelector,
};

fn old_exhaustive_match(request: &OperationRequest) -> &'static str {
    match request {
        OperationRequest::Health(_) => "health",
        OperationRequest::Search(_) => "search",
        OperationRequest::Context(_) => "context",
        OperationRequest::Query(_) => "query",
        OperationRequest::Materialize(_) => "materialize",
        OperationRequest::Plan(_) => "plan",
        OperationRequest::Catalog { .. } => "catalog",
        OperationRequest::Setup(_) => "setup",
        OperationRequest::Reinstall(_) => "reinstall",
        OperationRequest::Uninstall(_) => "uninstall",
        OperationRequest::InstallMcp(_) => "mcp-install",
        OperationRequest::Refresh(_) => "refresh",
    }
}

fn old_query_literal() -> QueryRequest {
    QueryRequest {
        repo: RepoSelector::default(),
        statement: "MATCH (n) RETURN n LIMIT 1".into(),
        parameters: serde_json::json!({}),
        limit: 1,
        output_format: OutputFormat::Typed,
    }
}

struct ExistingExecutor;
impl OperationExecutor for ExistingExecutor {
    fn execute(&self, request: &OperationRequest) -> Result<OperationResponse, ApiError> {
        Ok(OperationResponse::from_payload(
            request.operation_name(), request.output_format(), serde_json::json!({}),
        ))
    }
}
```

The eventual suite also compares existing public OperationDescriptor/response/error literals and all pre-existing CLI/MCP operation defaults. For each new operation, submit equivalent typed/CLI/MCP/CI inputs and compare logical records, simultaneous unknown/violation flags and error reasons; presentation may differ. The architecture family resolves through one catalog, while ExistingExecutor still implements only OperationExecutor.

## Synthetic shared graph data

The JSON block is fixture data, not an architecture wire request. Symbols are distinguished even when they share a module.

```json
{
  "fixture_id": "mixed-owner-projection",
  "containers": {"a": "runtime", "b": "runtime", "c": "other-runtime"},
  "symbols": {"s": {"module": "mixed", "owner": "a"}, "t": {"module": "mixed", "owner": "b"}, "u": {"module": "remote", "owner": "c"}},
  "qualified_edges": [["s", "t"], ["s", "t"], ["t", "u"]],
  "expected_component_edges": [["a", "b"], ["b", "c"]],
  "expected_container_edges": [["runtime", "other-runtime"]]
}
```

| Fixture | Given / mutation | Expected observable result | Trace |
| --- | --- | --- | --- |
| F04 unique owner | S's export/path predicates establish only A; every competing rule refuted with relevant coverage | A qualifies automatically; no human mapping gate | R03/A04 |
| F05 ambiguity | Both A and B match S | No accepted owner edge; mandatory dependent checks UNKNOWN | R03/A04 |
| F06 unsupported anchor | Only an unverifiable linked-contract anchor/prose rationale supports S→A | unsupported_evidence and UNKNOWN; no source match manufactured | R03/A04 |
| F07 invalid correction | Existing S→B; unchanged owner-relevant inputs; submit S→A with only lower metric | Preserve pinned B; correction unqualified; no metric PASS from reassignment | R03/A04–05/APR-01 |
| F08 valid correction | Prior decisive B predicate independently falsified; unique current A predicates established; lineage qualified | Correction may qualify automatically; retain old/new evidence and classification delta | R03/A04–05 |
| F09 mixed module | Use JSON graph; duplicate S→T witnesses | Two component pairs, one container pair as listed; no module-wide owner shortcut | R04/A05/A08 |
| F10 internalised edge | Candidate source unchanged; reassign T from B to A | S→T internalised only by classification; retain old-owner counterfactual A→B; cannot call this source dependency removal | R03–04/A05/APR-01 |
| F11 wrong target | A last-name match supplies concrete target; caller scope independently binds another symbol | Reject advisory target as qualification; unresolved mandatory projection UNKNOWN | R04/A06 |
| F12 omitted source unit | Submitted scanned inventory omits one required reachable unit or unsupported site | Validator-derived inventory mismatch; incomplete relevant checks UNKNOWN | R04/A06 |
| F13 changed cfg | Same source bytes but different target/features/cfg changes binding availability | Domain/input identity changes; no old completeness/result reuse | R04/R09/A06/A12 |
| F14 ambiguous move | Two candidate definitions share a syntax/name fingerprint for one prior symbol | Lineage UNKNOWN; no positional/name shortcut | R03/A04–05 |
| F15 qualified transfer | Unique structural move plus qualified parent/binding evidence and separately qualifying new owner | Record transfer and source/classification deltas; lineage alone never qualifies owner | R03/A04–05 |

## Compilation, policy and outcomes

| Fixture | Given / mutation | Expected observable result | Trace |
| --- | --- | --- | --- |
| F16 closure escape | Root lists `../outside.yaml`, an absolute path, symlink, duplicate path or remote include | path_not_confined or precise duplicate diagnostic; accepted/source state unchanged | R01–02/A01–02 |
| F17 invalid YAML | Duplicate key/ID, unknown field, invalid parent or second YAML document | Exact file/span diagnostic; no partial publication or silent field/edge drop | R01/A01 |
| F18 contradictory rule | Require A→B but no explicit corresponding allow pair | contradictory_policy at compilation; permission cannot be inferred | R05/A07 |
| F19 permission only | Allow A→B, complete observations contain no A→B, no require | No missing-required-edge violation is invented | R05/A07 |
| F20 mixed outcomes | Proven unpermitted A→B and unknown B→C evidence | has_violation=true and has_unknown=true, eligible=false; CLI3; both records retained | R05/R12/A06/A15 |
| F21 phase separation | Design predicts conforming future source but no actual mappings/test execution exists | Label predictions; no implementation/integration promotion | R02/A03 |

## Scenarios and contract equality

```json
{
  "fixture_id": "bypass-with-required-connectivity",
  "baseline_edges": [["caller", "read-service"], ["read-service", "store"]],
  "bypass_edges": [["caller", "read-service"], ["read-service", "store"], ["caller", "store"]],
  "disconnected_edges": [],
  "obligations": [
    {"kind": "reachable", "id": "connected", "from": "caller", "to": "store", "level": "component", "projection": "P"},
    {"kind": "no_bypass", "id": "mediated", "from": "caller", "to": "store", "via": "read-service", "level": "component", "projection": "P"}
  ]
}
```

`P` is a synthetic projection reference; the harness substitutes the qualified pinned projection. Both graphs need complete relevant coverage to conclude absence.

| Fixture | Given / mutation | Expected observable result | Trace |
| --- | --- | --- | --- |
| F22 mediated path | Baseline graph above; both explicit obligations | Reach exists; removing read-service removes caller→store reach; both PASS | R08/A11 |
| F23 bypass | bypass_edges above | reachable PASS, no_bypass VIOLATION; comparison cannot reward bypass | R08/A11/APR-05 |
| F24 disconnected | disconnected_edges above with complete coverage | no_bypass may individually PASS, reachable VIOLATION; scenario not eligible | R08/A11 |
| F25 missing pair | no_bypass present without explicit paired reachable | Compilation diagnostic; no hidden obligation injection | R08/A11 |
| F26 weakened candidate | Baseline/candidate transforms differ; candidate drops shared capability, parameter or obligation | Compile/obligation failure or UNKNOWN as applicable; cannot win by lower reach | R08/A11/APR-05 |
| F27 inapplicable/unknown | Missing target or incomplete precondition binding | Mandatory scenario UNKNOWN; no skip-PASS | R08/A11 |
| F28 wrong type/level | Container endpoint substituted in component obligation or string used for ElementId parameter | Compilation diagnostic; no coercion | R08/A11 |
| F29 contract reorder | Relationship refs [K1,K2]; expected [K2,K1] with identical pinned ref fields | Equal canonical set | R08/A11 |
| F30 contract set changed | Relationship refs [K1,K2]; expected [K1], [K1,K2,K3] or changed commit/digest for K2 | VIOLATION for exact-set mismatch when contents qualified | R08/A11 |
| F31 duplicate/unavailable | Duplicate K1 in either ref array; separately, unavailable required K2 contents | Duplicate is compilation diagnostic; unavailable contents UNKNOWN | R01/R08/A01/A11 |
| F32 empty contract set | Expected [] | PASS only when relationship's complete actual contract set is empty | R08/A11 |

## Metrics, reuse and state transitions

| Fixture | Given / mutation | Expected observable result | Trace |
| --- | --- | --- | --- |
| F33 metric graph | A→B, B→C, C→B; all origins, complete coverage | cyclic_nodes=2 (B,C); max_direct_fanout=1; max_transitive_reach=2; preserve SCC/origin witnesses | R06/A08 |
| F34 nonlocal reach | Baseline A→B; add B→C with A's direct edge unchanged | A reach increases 1→2; invalidate old A reach result | R06/R09/A09 |
| F35 aggregate limitation | Fanouts A=10,B=0 become A=8,B=10 | Aggregate maximum remains10; show both origin deltas, no per-origin guarantee claimed | R06/A08–09 |
| F36 stale ratchet | Both candidates8 and9 evaluated against accepted10; accept8 first | Candidate9 conflicts with old expected state; recompare against8 and reject deterioration | R02/R07/A02/A10/APR-03 |
| F37 interacting proposals | One candidate adds A→B; another adds B→A | Combined integration graph has a cycle; independent acyclic heads cannot authorize it | R02/R06/A09/APR-03 |
| F38 report-only commit | New Git commit changes only fixed table; mapping facts identical | Preserve original evaluation commit/ID only after content/build equivalence and trusted regeneration; fresh authorization still required | R09/R11/A12/A14 |
| F39 changed mapping in table | Source fingerprint excludes table, but owner/rationale/citation/provenance changed | MappingFactsDigest changes; no dependent result reuse | R09/A12/APR-02 |
| F40 forged cells | Input IDs unchanged; alter PASS/value, omit mandatory row or substitute execution ref | Trusted enumeration/regeneration detects mismatch; no acceptance | R09/R11/A12/A14/APR-02 |
| F41 stale authorization | Deterministic evidence unchanged but review dismissed, waiver expires or actual integration differs | Fresh final check blocks; old green/table is not authority | R10/A13/A18/APR-04 |
| F42 missing receipt | Atomic transition committed, connection lost before receipt | Discover current accepted identity; do not undo or blindly replay accepted transition | R02/R09/A02/A17 |

## Resources and cancellation

Resource policy values are test parameters supplied explicitly by the harness; none below prescribes a default number. Let a configured limit be L and the matching demand be L+1.

| Fixture | Given / mutation | Expected observable result | Trace |
| --- | --- | --- | --- |
| F43 missing policy | Omit required policy field or provide invalid nonpositive value | architecture_configuration_required, CLI2 before evaluation; no silent defaults | R12/A15 |
| F44 caller override | Shared policy L; caller attempts higher limit, separately lower limit | Higher override rejected; lower becomes effective; record full effective values/digest identically across transports | R09/R12/A12/A15 |
| F45 resource exhaustion | Demand L+1 before identity, separately during established evaluation | Pre-evaluation rejection CLI2; unfinished established mandatory check UNKNOWN/CLI4; retain valid input-bound completed records and accepted state | R02/R09/R12/A02/A12/A15 |
| F46 cancellation/deadline | Caller timeout while work remains executing | Caller gets failure/UNKNOWN as appropriate; real permit stays occupied until actual completion/cleanup | R02/R12/A02/A16 |
| F47 ambiguous dispatch | Connection lost after request may have dispatched, including a read | No automatic replay; explicit recovery inspects state; proved pre-dispatch failure is a separate safe-retry case | R02/R12/A02/A16 |
| F48 resource policy changed | Same source but any effective policy value differs | Input identity changes; do not reuse a previous incomplete/pass result as current | R09/R12/A12/A15 |

## Qualification boundary

The eventual owning suites must bind executed results to the implemented revision/environment and the linked acceptance criteria. These definitions do not replace tests, Q1 pilot evidence, COD-18 hosted-path demonstrations or COD-19 implementation approval. Synthetic container graphs cannot establish actual pilot coverage; scenario instances remain COD-40; numerical calibration remains D17. No product suite is claimed passing here.
