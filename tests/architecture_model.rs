use codebase_graph::architecture::*;
use serde_json::{json, Value};

fn document() -> ArchitectureDocument {
    serde_json::from_value(json!({"schema_version":"0.1","systems":[{"id":"s","name":"System","responsibilities":[{"id":"rs","statement":"Serve"}]}],"containers":[{"id":"c","system":"s","kind":"application","name":"Container","responsibilities":[{"id":"rc","statement":"Run"}]}],"components":[{"id":"p","container":"c","name":"Component","responsibilities":[{"id":"rp","statement":"Work"}]}],"relationships":[]})).unwrap()
}
#[test]
fn validates_complete_hierarchy_and_reports_errors() {
    let mut d = document();
    assert!(validate(&[d.clone()]).is_empty());
    d.containers[0].system = Id::new("missing").unwrap();
    d.components[0].responsibilities.clear();
    d.relationships = vec![serde_json::from_value(json!({"id":"edge","level":"component","from":"p","to":"c","description":"uses","contract_refs":[]})).unwrap()];
    let diagnostics = validate(&[d]);
    assert!(diagnostics
        .iter()
        .any(|x| x.subject == "c" && x.code == "invalid_parent"));
    assert!(diagnostics
        .iter()
        .any(|x| x.subject == "p" && x.code == "missing_responsibility"));
    assert!(diagnostics
        .iter()
        .any(|x| x.subject == "edge" && x.code == "invalid_endpoint"));
    let d = document();
    assert!(validate(&[d.clone(), d])
        .iter()
        .any(|x| x.code == "duplicate_id"));
}
#[test]
fn primitives_and_strict_shapes_reject_invalid_values() {
    for id in ["", "A", "a--b", "a/thing", "a_"] {
        assert!(Id::new(id).is_err());
    }
    for path in ["/a", "a/../b", "a//b", "a\\b", "https://x"] {
        assert!(RelativePath::new(path).is_err());
    }
    assert!(Digest::new(format!("sha256:{}", "a".repeat(64))).is_ok());
    assert!(Digest::new(format!("sha256:{}", "A".repeat(64))).is_err());
    assert!(GitObjectId::new("a".repeat(40)).is_ok());
    assert!(GitObjectId::new("a".repeat(64)).is_ok());
    assert!(GitObjectId::new("abc").is_err());
    assert!(ByteSpan::new(5, 5).is_err());
    assert!(serde_json::from_value::<ByteSpan>(json!({"start":9,"end":2})).is_err());
    let mut value = serde_json::to_value(document()).unwrap();
    value["schema_version"] = json!("0.2");
    assert!(serde_json::from_value::<ArchitectureDocument>(value).is_err());
    let mut value = serde_json::to_value(document()).unwrap();
    value["systems"][0]["source_selector"] = json!("fake");
    assert!(serde_json::from_value::<ArchitectureDocument>(value).is_err());
}
#[test]
fn ontology_is_additive_and_separate_from_legacy_schema() {
    let extension: Value = serde_json::from_str(SCHEMA).unwrap();
    let legacy: Value = serde_json::from_str(include_str!("../assets/graph_schema.json")).unwrap();
    assert_eq!(extension["version"], "0.1");
    let nodes = extension["node_types"].as_array().unwrap();
    let edges = extension["relation_types"].as_array().unwrap();
    assert_eq!(nodes.len(), 9);
    assert_eq!(edges.len(), 5);
    for entry in nodes.iter().chain(edges) {
        assert!(!legacy["node_types"]
            .as_array()
            .unwrap()
            .iter()
            .chain(legacy["relation_types"].as_array().unwrap())
            .any(|old| old["name"] == entry["name"]));
        for field in ["model_id", "snapshot_id", "state_kind", "schema_version"] {
            assert!(entry["fields"]
                .as_array()
                .unwrap()
                .iter()
                .any(|f| f["name"] == field && f["required"] == true));
        }
        assert!(!entry["fields"]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f["name"] == "line_start" || f["name"] == "path"));
    }
}
#[test]
fn state_and_identity_remain_distinct() {
    let states = [
        StateKind::AcceptedIntent,
        StateKind::CandidateIntent,
        StateKind::ObservedProjection,
        StateKind::PredictedProjection,
        StateKind::Evaluation,
    ];
    let encoded: std::collections::HashSet<_> = states
        .iter()
        .map(|s| serde_json::to_string(s).unwrap())
        .collect();
    assert_eq!(encoded.len(), 5);
    let row = RowIdentity {
        model_id: Id::new("m").unwrap(),
        snapshot_id: Id::new("accepted").unwrap(),
        logical_id: Id::new("x").unwrap(),
    };
    let mut candidate = row.clone();
    candidate.snapshot_id = Id::new("candidate").unwrap();
    assert_ne!(row, candidate);
}

#[test]
fn cross_system_same_level_relationships_are_valid_and_parent_conflicts_reported() {
    let mut d = document();
    d.systems.push(serde_json::from_value(json!({"id":"s2","name":"Other","responsibilities":[{"id":"rs2","statement":"Serve elsewhere"}]})).unwrap());
    d.containers.push(serde_json::from_value(json!({"id":"c2","system":"s2","kind":"data_store","name":"Other container","responsibilities":[{"id":"rc2","statement":"Store"}]})).unwrap());
    d.relationships.push(serde_json::from_value(json!({"id":"r","level":"container","from":"c","to":"c2","description":"uses","contract_refs":[]})).unwrap());
    assert!(validate(&[d.clone()]).is_empty());
    let mut conflicting = d.containers[0].clone();
    conflicting.system = Id::new("s2").unwrap();
    d.containers.push(conflicting);
    assert!(validate(&[d])
        .iter()
        .any(|x| x.subject == "c" && x.code == "contradictory_parentage"));
}

#[test]
fn ontology_names_and_authored_fields_match_contract() {
    let schema: Value = serde_json::from_str(SCHEMA).unwrap();
    let nodes = schema["node_types"].as_array().unwrap();
    let relations = schema["relation_types"].as_array().unwrap();
    let names = |entries: &[Value]| {
        entries
            .iter()
            .map(|x| x["name"].as_str().unwrap().to_owned())
            .collect::<std::collections::HashSet<_>>()
    };
    assert_eq!(
        names(nodes),
        [
            "ArchitectureSystem",
            "ArchitectureContainer",
            "ArchitectureComponent",
            "ArchitectureRelationship",
            "ArchitectureConstraint",
            "ArchitectureScenario",
            "ArchitectureSnapshot",
            "ArchitectureMapping",
            "ArchitectureEvaluation"
        ]
        .into_iter()
        .map(str::to_owned)
        .collect()
    );
    assert_eq!(
        names(relations),
        [
            "ArchitectureContains",
            "ArchitectureRelates",
            "ArchitectureOwns",
            "ArchitectureDependsOn",
            "ArchitectureChecks"
        ]
        .into_iter()
        .map(str::to_owned)
        .collect()
    );
    let d = serde_json::to_value(document()).unwrap();
    for (kind, item) in [
        ("ArchitectureSystem", &d["systems"][0]),
        ("ArchitectureContainer", &d["containers"][0]),
        ("ArchitectureComponent", &d["components"][0]),
    ] {
        let def = nodes.iter().find(|n| n["name"] == kind).unwrap();
        let fields = def["fields"].as_array().unwrap();
        for name in item.as_object().unwrap().keys() {
            assert!(fields
                .iter()
                .any(|field| field["name"] == name.as_str() && field["required"] == true));
        }
    }
}

#[test]
fn references_round_trip_but_do_not_qualify_bytes() {
    let commit = "a".repeat(40);
    let digest = format!("sha256:{}", "b".repeat(64));
    let artifact =
        json!({"repository":"repo", "commit":commit, "path":"src/lib.rs", "content_digest":digest});
    let source = json!({"repository":"repo", "commit":commit, "path":"src/lib.rs", "node_kind":"Symbol", "node_id":"revision-id", "symbol":"item", "byte_span":{"start":0,"end":4}, "file_digest":digest});
    let provenance = json!({"repository":"repo", "evaluated_commit":commit, "evaluated_tree":"c".repeat(40), "extractor_digest":digest, "build_context_digest":digest});
    assert_eq!(
        serde_json::to_value(serde_json::from_value::<ArtifactRef>(artifact.clone()).unwrap())
            .unwrap(),
        artifact
    );
    assert_eq!(
        serde_json::to_value(serde_json::from_value::<SourceRef>(source.clone()).unwrap()).unwrap(),
        source
    );
    assert_eq!(
        serde_json::to_value(
            serde_json::from_value::<SourceProvenance>(provenance.clone()).unwrap()
        )
        .unwrap(),
        provenance
    );
    for (field, bad) in [
        ("commit", json!("abc")),
        ("path", json!("../outside")),
        ("byte_span", json!({"start":4,"end":3})),
    ] {
        let mut invalid = source.clone();
        invalid[field] = bad;
        assert!(serde_json::from_value::<SourceRef>(invalid).is_err());
    }
}

#[test]
fn strict_shapes_and_global_architecture_ids() {
    let mut missing = serde_json::to_value(document()).unwrap();
    missing.as_object_mut().unwrap().remove("relationships");
    assert!(serde_json::from_value::<ArchitectureDocument>(missing).is_err());
    let mut bad = serde_json::to_value(document()).unwrap();
    bad["containers"][0]["kind"] = json!("unknown");
    assert!(serde_json::from_value::<ArchitectureDocument>(bad).is_err());
    let first = document();
    let mut second = document();
    second.systems[0].id = Id::new("rs").unwrap();
    assert!(validate(&[first, second])
        .iter()
        .any(|d| d.subject == "rs" && d.code == "duplicate_id"));
    let mut cross_kind = document();
    cross_kind.containers[0].system = Id::new("p").unwrap();
    cross_kind.components[0].container = Id::new("p").unwrap();
    let diagnostics = validate(&[cross_kind]);
    assert!(diagnostics
        .iter()
        .any(|d| d.subject == "c" && d.code == "invalid_parent"));
    assert!(diagnostics
        .iter()
        .any(|d| d.subject == "p" && d.code == "invalid_parent"));
}

#[test]
fn ontology_endpoint_contract_and_legacy_names_are_preserved() {
    let schema: Value = serde_json::from_str(SCHEMA).unwrap();
    let edges = schema["relation_types"].as_array().unwrap();
    let get = |name: &str| edges.iter().find(|e| e["name"] == name).unwrap();
    for (name, text) in [
        (
            "ArchitectureContains",
            "System to Container or Container to Component",
        ),
        (
            "ArchitectureRelates",
            "Container to Container or Component to Component",
        ),
        (
            "ArchitectureDependsOn",
            "Container to Container or Component to Component",
        ),
    ] {
        assert!(get(name)["constraints"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c.as_str().unwrap().contains(text)));
    }
    for (name, fields) in [
        ("ArchitectureRelates", vec!["relationship_id"]),
        (
            "ArchitectureDependsOn",
            vec!["source_witnesses", "domain", "coverage"],
        ),
        (
            "ArchitectureChecks",
            vec!["input_identity", "phase", "outcome", "provenance"],
        ),
    ] {
        for name_field in fields {
            assert!(get(name)["fields"]
                .as_array()
                .unwrap()
                .iter()
                .any(|f| f["name"] == name_field && f["required"] == true));
        }
    }
    let legacy: Value = serde_json::from_str(include_str!("../assets/graph_schema.json")).unwrap();
    assert!(legacy["node_types"]
        .as_array()
        .unwrap()
        .iter()
        .any(|x| x["name"] == "Component"));
    assert!(legacy["relation_types"]
        .as_array()
        .unwrap()
        .iter()
        .any(|x| x["name"] == "EvidencedBy"));
    let labels: Vec<_> = [
        StateKind::AcceptedIntent,
        StateKind::CandidateIntent,
        StateKind::ObservedProjection,
        StateKind::PredictedProjection,
        StateKind::Evaluation,
    ]
    .iter()
    .map(|s| serde_json::to_value(s).unwrap())
    .collect();
    assert_eq!(
        labels,
        vec![
            json!("accepted_intent"),
            json!("candidate_intent"),
            json!("observed_projection"),
            json!("predicted_projection"),
            json!("evaluation")
        ]
    );
}
