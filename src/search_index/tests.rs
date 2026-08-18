use super::build::{build, SearchIndexBuildRequest};
use super::model::POSTINGS_SUFFIX;
use super::{search, sidecar_path, validate};
use serde_json::json;
use std::fs::{self, OpenOptions};
use std::io::{Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

#[test]
fn sidecar_build_and_search_use_deterministic_bm25_ties() {
    let fixture = Fixture::new("ranking");
    fixture.write_nodes(
        "function",
        &[
            node("fn:a", "alpha", "src/a.rs"),
            node("fn:b", "alpha", "src/b.rs"),
            node("fn:c", "unrelated", "src/c.rs"),
        ],
    );
    let built = build(fixture.request(4096)).unwrap();
    validate(&fixture.db_path, &built.metadata).unwrap();

    let hits = search(&fixture.db_path, &built.metadata, "alpha", "semantic", 10).unwrap();
    assert_eq!(
        hits.iter().map(|hit| hit.id.as_str()).collect::<Vec<_>>(),
        vec!["fn:a", "fn:b"]
    );
    assert_eq!(hits[0].score, hits[1].score);
    let top = search(&fixture.db_path, &built.metadata, "alpha", "semantic", 1).unwrap();
    assert_eq!(top[0].id, "fn:a");
}

#[test]
fn checksum_validation_rejects_corrupt_postings() {
    let fixture = Fixture::new("corruption");
    fixture.write_nodes("function", &[node("fn:a", "alpha", "src/a.rs")]);
    let built = build(fixture.request(4096)).unwrap();
    let postings_path = sidecar_path(&fixture.db_path, POSTINGS_SUFFIX);
    let mut postings = OpenOptions::new().write(true).open(postings_path).unwrap();
    postings.seek(SeekFrom::End(-1)).unwrap();
    postings.write_all(&[0xff]).unwrap();
    postings.flush().unwrap();

    let error = validate(&fixture.db_path, &built.metadata).unwrap_err();
    assert!(error.to_string().contains("checksum mismatch"));
}

#[test]
fn occurrence_sorting_stays_within_the_configured_chunk() {
    let fixture = Fixture::new("bounded");
    let nodes = (0..200)
        .map(|index| {
            node(
                &format!("fn:{index:04}"),
                &format!("token_{index:04} common"),
                &format!("src/{index:04}.rs"),
            )
        })
        .collect::<Vec<_>>();
    fixture.write_nodes("function", &nodes);
    let built = build(fixture.request(4096)).unwrap();

    assert!(built.spill_bytes > 0);
    assert!(built.high_water_bytes <= 4096);
    validate(&fixture.db_path, &built.metadata).unwrap();
}

#[test]
fn builder_consumes_all_staging_chunks_in_sequence() {
    let fixture = Fixture::new("staging-chunks");
    fixture.write_nodes("function", &[node("fn:a", "alpha", "src/a.rs")]);
    fixture.write_nodes_chunk("function", 1, &[node("fn:b", "bravo", "src/b.rs")]);
    let built = build(fixture.request(4096)).unwrap();

    let hits = search(&fixture.db_path, &built.metadata, "bravo", "semantic", 10).unwrap();
    assert_eq!(
        hits.iter().map(|hit| hit.id.as_str()).collect::<Vec<_>>(),
        vec!["fn:b"]
    );
}

#[test]
fn tokenizer_indexes_unicode_identifiers_case_insensitively() {
    let fixture = Fixture::new("unicode");
    fixture.write_nodes(
        "function",
        &[node("fn:unicode", "ÜberService", "src/unicode.rs")],
    );
    let built = build(fixture.request(4096)).unwrap();

    let hits = search(
        &fixture.db_path,
        &built.metadata,
        "überservice",
        "semantic",
        10,
    )
    .unwrap();
    assert_eq!(hits[0].id, "fn:unicode");
}

fn node(id: &str, label: &str, path: &str) -> serde_json::Value {
    json!({
        "id": id,
        "label": label,
        "path": path,
        "qualified_name": label,
        "summary": label,
        "text": label,
        "tree_sitter_node_type": "function_item"
    })
}

struct Fixture {
    root: PathBuf,
    staging_dir: PathBuf,
    db_path: PathBuf,
}

impl Fixture {
    fn new(name: &str) -> Self {
        let root = std::env::temp_dir().join(format!(
            "codebase-graph-search-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let staging_dir = root.join("staging");
        fs::create_dir_all(&staging_dir).unwrap();
        Self {
            db_path: root.join("graph.ldb"),
            staging_dir,
            root,
        }
    }

    fn write_nodes(&self, table: &str, nodes: &[serde_json::Value]) {
        fs::write(
            self.staging_dir.join(format!("{table}.json")),
            serde_json::to_vec(nodes).unwrap(),
        )
        .unwrap();
    }

    fn write_nodes_chunk(&self, table: &str, index: usize, nodes: &[serde_json::Value]) {
        fs::write(
            self.staging_dir.join(format!("{table}__{index:06}.json")),
            serde_json::to_vec(nodes).unwrap(),
        )
        .unwrap();
    }

    fn request(&self, chunk_bytes: usize) -> SearchIndexBuildRequest<'_> {
        SearchIndexBuildRequest {
            db_path: &self.db_path,
            staging_dir: &self.staging_dir,
            chunk_bytes,
        }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        if self.root.is_dir() {
            fs::remove_dir_all(&self.root).unwrap();
        }
    }
}

#[allow(dead_code)]
fn _assert_path(_: &Path) {}
