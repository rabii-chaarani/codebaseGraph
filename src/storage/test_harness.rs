#[path = "../error.rs"]
#[allow(dead_code)]
mod error;
#[path = "mod.rs"]
#[allow(dead_code, unused_imports)]
mod storage;

mod protocol {
    #[derive(Debug, Clone, Default)]
    pub struct GraphSummary {
        pub node_count: usize,
        pub edge_count: usize,
    }
}

use protocol::GraphSummary;
use serde_json::json;
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use storage::atomic::{
    write_json_atomically, write_json_atomically_with_fault, AtomicWriteFailure,
};
use storage::direct::{DirectPublishJournal, DirectPublishPhase, DirectStore};
use storage::layout::{managed_generation_id, DirectLayout, GenerationPaths, ManagedLayout};
use storage::locks::{try_open_locked, LockMode, RefreshLease, WorkerLease};
use storage::managed::{ActiveGeneration, ManagedStore, ManagedWriteSession};
use storage::run_workspace::{RunJournal, RunPhase, RunWorkspace};

static TEST_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[test]
fn managed_control_lock_paths_are_stable_and_role_scoped() {
    let root = temp_dir("managed_control_lock_paths");
    let layout = ManagedLayout::new(root.join("storage"));

    assert_eq!(
        layout.refresh_lock_path(),
        root.join("storage/refresh.lock")
    );
    assert_eq!(layout.worker_lock_path(), root.join("storage/worker.lock"));
    assert_ne!(layout.refresh_lock_path(), layout.worker_lock_path());
    assert_ne!(layout.refresh_lock_path(), layout.writer_lock_path());
}

#[test]
fn direct_control_lock_paths_are_stable_and_destination_scoped() {
    let root = temp_dir("direct_control_lock_paths");
    let first = DirectLayout::new(root.join("graph.ldb"), root.join("manifest.json"));
    let same = DirectLayout::new(root.join("graph.ldb"), root.join("manifest.json"));
    let other = DirectLayout::new(root.join("other.ldb"), root.join("manifest.json"));

    assert_eq!(first.refresh_lock_path(), same.refresh_lock_path());
    assert_eq!(first.worker_lock_path(), same.worker_lock_path());
    assert_eq!(first.refresh_lock_path().parent(), Some(root.as_path()));
    assert_eq!(first.worker_lock_path().parent(), Some(root.as_path()));
    assert_ne!(first.refresh_lock_path(), first.worker_lock_path());
    assert_ne!(first.refresh_lock_path(), first.writer_lock_path());
    assert_ne!(first.refresh_lock_path(), other.refresh_lock_path());
    assert_ne!(first.worker_lock_path(), other.worker_lock_path());
}

#[test]
fn control_leases_are_exclusive_nonblocking_and_transfer_after_release() {
    let root = temp_dir("control_lease_takeover");
    let layout = ManagedLayout::new(root.join("storage"));

    let refresh: RefreshLease = try_open_locked(layout.refresh_lock_path(), LockMode::Exclusive)
        .unwrap()
        .expect("first refresh owner should acquire the lease");
    assert!(
        try_open_locked(layout.refresh_lock_path(), LockMode::Exclusive)
            .unwrap()
            .is_none()
    );
    drop(refresh);
    assert!(
        try_open_locked(layout.refresh_lock_path(), LockMode::Exclusive)
            .unwrap()
            .is_some()
    );

    let worker: WorkerLease = try_open_locked(layout.worker_lock_path(), LockMode::Exclusive)
        .unwrap()
        .expect("first worker owner should acquire the lease");
    assert!(
        try_open_locked(layout.worker_lock_path(), LockMode::Exclusive)
            .unwrap()
            .is_none()
    );
    drop(worker);
    assert!(
        try_open_locked(layout.worker_lock_path(), LockMode::Exclusive)
            .unwrap()
            .is_some()
    );
}

#[cfg(unix)]
#[test]
fn control_lease_rejects_a_symlinked_lock_file() {
    use std::os::unix::fs::symlink;

    let root = temp_dir("control_lease_symlink");
    let layout = ManagedLayout::new(root.join("storage"));
    fs::create_dir_all(layout.storage_root()).unwrap();
    let target = root.join("outside.lock");
    fs::write(&target, b"").unwrap();
    symlink(&target, layout.refresh_lock_path()).unwrap();

    let error = try_open_locked(layout.refresh_lock_path(), LockMode::Exclusive).unwrap_err();
    assert!(error.to_string().contains("lock path must be a real file"));
}

#[test]
fn atomic_write_failure_preserves_existing_file_and_cleans_temp() {
    let root = temp_dir("atomic_write_failure_preserves_existing_file_and_cleans_temp");
    let path = root.join("active.json");
    fs::write(&path, "{\"generation_id\":\"old\"}\n").unwrap();

    let error = write_json_atomically_with_fault(
        &path,
        &json!({"generation_id": "new"}),
        AtomicWriteFailure::AfterFileSync,
    )
    .unwrap_err();

    assert!(error.to_string().contains("injected atomic write failure"));
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        "{\"generation_id\":\"old\"}\n"
    );
    assert!(dir_entries_named(&root, ".active.json.tmp").is_empty());
}

#[test]
fn managed_begin_write_holds_writer_lock_and_marks_candidate_ready_explicitly() {
    let root =
        temp_dir("managed_begin_write_holds_writer_lock_and_marks_candidate_ready_explicitly");
    let store = open_managed_store(root.join("storage"));
    let mut session = store.begin_write().unwrap();

    assert!(
        try_open_locked(store.layout().writer_lock_path(), LockMode::Exclusive)
            .unwrap()
            .is_none()
    );
    assert_eq!(managed_session_journal(&session).phase, RunPhase::Staged);

    write_candidate_generation(
        &session.candidate.paths,
        b"db-one",
        json!({"files": ["one"]}),
        false,
    );
    session
        .mark_ready_with_stats(&GraphSummary::default())
        .unwrap();
    assert_eq!(
        managed_session_journal(&session).phase,
        RunPhase::CandidateReady
    );

    session.abort(Some("done".to_string())).unwrap();
}

#[test]
fn managed_writer_is_serialized_across_sessions() {
    let root = temp_dir("managed_writer_is_serialized_across_sessions");
    let store = open_managed_store(root.join("storage"));
    let session = store.begin_write().unwrap();

    assert!(
        try_open_locked(store.layout().writer_lock_path(), LockMode::Exclusive)
            .unwrap()
            .is_none()
    );

    drop(session);

    assert!(
        try_open_locked(store.layout().writer_lock_path(), LockMode::Exclusive)
            .unwrap()
            .is_some()
    );
}

#[test]
fn managed_cleanup_recovers_publishing_crash_before_generation_rename() {
    let root = temp_dir("managed_cleanup_recovers_publishing_crash_before_generation_rename");
    let store = open_managed_store(root.join("storage"));
    let first = publish_managed_generation(&store, b"db-one", json!({"files": ["one"]}));

    let (run_root, candidate_id) = orphan_publishing_run(
        &store,
        Some(first.clone()),
        b"db-two",
        json!({"files": ["two"]}),
        OrphanPublishStage::BeforeRename,
    );

    let report = store.cleanup().unwrap();
    assert_eq!(report.run_recovery.publishing_recovered, 1);
    let snapshot = store.open_read().unwrap();
    assert_eq!(snapshot.generation_id, candidate_id);
    assert_eq!(fs::read(snapshot.db_path).unwrap(), b"db-two");
    assert!(!run_root.exists());
}

#[test]
fn managed_cleanup_recovers_publishing_crash_after_generation_rename() {
    let root = temp_dir("managed_cleanup_recovers_publishing_crash_after_generation_rename");
    let store = open_managed_store(root.join("storage"));
    let first = publish_managed_generation(&store, b"db-one", json!({"files": ["one"]}));

    let (run_root, candidate_id) = orphan_publishing_run(
        &store,
        Some(first.clone()),
        b"db-two",
        json!({"files": ["two"]}),
        OrphanPublishStage::AfterRename,
    );

    let report = store.cleanup().unwrap();
    assert_eq!(report.run_recovery.publishing_recovered, 1);
    let snapshot = store.open_read().unwrap();
    assert_eq!(snapshot.generation_id, candidate_id);
    assert_eq!(fs::read(snapshot.db_path).unwrap(), b"db-two");
    assert!(
        fs::read_to_string(
            store
                .layout()
                .generation(&candidate_id)
                .unwrap()
                .ready_path()
        )
        .unwrap()
        .trim()
        .parse::<u64>()
        .unwrap_or_default()
            > 0
    );
    assert!(!run_root.exists());
}

#[test]
fn managed_cleanup_recovers_publishing_crash_after_active_pointer_write() {
    let root = temp_dir("managed_cleanup_recovers_publishing_crash_after_active_pointer_write");
    let store = open_managed_store(root.join("storage"));
    let first = publish_managed_generation(&store, b"db-one", json!({"files": ["one"]}));

    let (run_root, candidate_id) = orphan_publishing_run(
        &store,
        Some(first.clone()),
        b"db-two",
        json!({"files": ["two"]}),
        OrphanPublishStage::AfterActivePointer,
    );

    let report = store.cleanup().unwrap();
    assert_eq!(report.run_recovery.publishing_recovered, 1);
    let snapshot = store.open_read().unwrap();
    assert_eq!(snapshot.generation_id, candidate_id);
    assert!(!store
        .layout()
        .generation(&candidate_id)
        .unwrap()
        .retired_path()
        .exists());
    assert!(!store.layout().generation(&first).unwrap().root().exists());
    assert_eq!(report.retired_generations_pending, 0);
    assert!(!run_root.exists());
}

#[test]
fn managed_cleanup_recovers_published_crash_after_active_pointer_write() {
    let root = temp_dir("managed_cleanup_recovers_published_crash_after_active_pointer_write");
    let store = open_managed_store(root.join("storage"));
    let first = publish_managed_generation(&store, b"db-one", json!({"files": ["one"]}));

    let (run_root, candidate_id) = orphan_publishing_run(
        &store,
        Some(first.clone()),
        b"db-two",
        json!({"files": ["two"]}),
        OrphanPublishStage::AfterPublished,
    );

    let report = store.cleanup().unwrap();
    assert_eq!(report.run_recovery.publishing_recovered, 1);
    let snapshot = store.open_read().unwrap();
    assert_eq!(snapshot.generation_id, candidate_id);
    assert!(!run_root.exists());
    assert!(!store.layout().generation(&first).unwrap().root().exists());
    assert!(!store
        .layout()
        .generation(&candidate_id)
        .unwrap()
        .retired_path()
        .exists());
}

#[test]
fn managed_last_reader_drop_triggers_best_effort_retirement_cleanup() {
    let root = temp_dir("managed_last_reader_drop_triggers_best_effort_retirement_cleanup");
    let store = open_managed_store(root.join("storage"));
    let first = publish_managed_generation(&store, b"db-one", json!({"files": ["one"]}));
    let reader = store.open_read().unwrap();

    let second = publish_managed_generation(&store, b"db-two", json!({"files": ["two"]}));
    assert_ne!(first, second);
    assert!(store.layout().generation(&first).unwrap().root().exists());

    drop(reader);

    assert!(!store.layout().generation(&first).unwrap().root().exists());
}

#[test]
fn managed_metadata_sizes_include_db_sidecars_and_allocated_bytes() {
    let root = temp_dir("managed_metadata_sizes_include_db_sidecars_and_allocated_bytes");
    let store = open_managed_store(root.join("storage"));
    let mut session = store.begin_write().unwrap();
    write_candidate_generation(
        &session.candidate.paths,
        b"db-one",
        json!({"files": ["one"]}),
        false,
    );
    fs::write(
        format!("{}.wal", session.candidate.paths.db_path().display()),
        b"wal-one",
    )
    .unwrap();
    fs::write(
        format!("{}.tmp", session.candidate.paths.db_path().display()),
        b"tmp-one",
    )
    .unwrap();

    session
        .mark_ready_with_stats(&GraphSummary::default())
        .unwrap();
    let metadata: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(session.candidate.paths.metadata_path()).unwrap())
            .unwrap();
    let logical = metadata["logical_size_bytes"].as_u64().unwrap();
    let physical = metadata["physical_size_bytes"].as_u64().unwrap();
    let expected_logical = b"db-one".len() as u64
        + serde_json::to_vec_pretty(&json!({"files": ["one"]}))
            .unwrap()
            .len() as u64
        + b"wal-one".len() as u64
        + b"tmp-one".len() as u64;

    assert!(logical >= expected_logical);
    assert!(physical >= logical);
    session.abort(Some("done".to_string())).unwrap();
}

#[test]
fn direct_write_session_holds_writer_lock_and_drop_cleans_unpublished_candidates() {
    let root =
        temp_dir("direct_write_session_holds_writer_lock_and_drop_cleans_unpublished_candidates");
    let layout = DirectLayout::new(
        root.join("db/graph.ldb"),
        root.join("manifest/manifest.json"),
    );
    let store = DirectStore::new(layout.clone()).unwrap();
    let session = store.begin_write().unwrap();

    assert!(
        try_open_locked(layout.writer_lock_path(), LockMode::Exclusive)
            .unwrap()
            .is_none()
    );
    fs::write(session.db_candidate_path(), b"db-one").unwrap();
    fs::write(session.manifest_candidate_path(), "{\"version\":1}\n").unwrap();
    fs::write(
        format!("{}.wal", session.db_candidate_path().display()),
        b"wal",
    )
    .unwrap();

    drop(session);

    assert!(!layout.db_candidate_path().exists());
    assert!(!layout.manifest_candidate_path().exists());
    assert!(!PathBuf::from(format!("{}.wal", layout.db_candidate_path().display())).exists());
}

#[test]
fn direct_store_recovers_interrupted_publication_with_sidecars_and_split_parents() {
    let root =
        temp_dir("direct_store_recovers_interrupted_publication_with_sidecars_and_split_parents");
    let layout = DirectLayout::new(
        root.join("db/graph.ldb"),
        root.join("manifest/manifest.json"),
    );
    let store = DirectStore::new(layout.clone()).unwrap();

    fs::create_dir_all(layout.db_path().parent().unwrap()).unwrap();
    fs::create_dir_all(layout.manifest_path().parent().unwrap()).unwrap();
    fs::write(layout.db_candidate_path(), b"db-v2").unwrap();
    fs::write(layout.manifest_candidate_path(), "{\"version\":2}\n").unwrap();
    fs::write(
        format!("{}.wal", layout.db_candidate_path().display()),
        b"wal-v2",
    )
    .unwrap();
    write_json_atomically(
        &layout.journal_path(),
        &DirectPublishJournal {
            phase: DirectPublishPhase::Prepared,
            db_path: layout.db_path().to_path_buf(),
            manifest_path: layout.manifest_path().to_path_buf(),
            db_candidate_path: layout.db_candidate_path(),
            manifest_candidate_path: layout.manifest_candidate_path(),
            db_sha256: sha256_hex(b"db-v2"),
            manifest_sha256: sha256_hex(b"{\"version\":2}\n"),
            sidecar_sha256: BTreeMap::from([("wal".to_string(), sha256_hex(b"wal-v2"))]),
        },
    )
    .unwrap();

    let _read_lease = store.begin_read().unwrap();
    assert_eq!(fs::read(layout.db_path()).unwrap(), b"db-v2");
    assert_eq!(
        fs::read_to_string(layout.manifest_path()).unwrap(),
        "{\"version\":2}\n"
    );
    assert_eq!(
        fs::read(format!("{}.wal", layout.db_path().display())).unwrap(),
        b"wal-v2"
    );
}

#[test]
fn direct_store_recovers_database_manifest_and_committed_journal_phases() {
    for phase in [
        DirectPublishPhase::DatabasePromoted,
        DirectPublishPhase::ManifestPromoted,
        DirectPublishPhase::Committed,
    ] {
        let root = temp_dir(&format!("direct-recovery-{phase:?}"));
        let layout = DirectLayout::new(root.join("graph.ldb"), root.join("manifest.json"));
        let store = DirectStore::new(layout.clone()).unwrap();
        let db_v2 = b"db-v2";
        let manifest_v2 = b"{\"version\":2}\n";
        fs::write(layout.db_path(), db_v2).unwrap();
        if phase == DirectPublishPhase::DatabasePromoted {
            fs::write(layout.manifest_path(), "{\"version\":1}\n").unwrap();
            fs::write(layout.manifest_candidate_path(), manifest_v2).unwrap();
        } else {
            fs::write(layout.manifest_path(), manifest_v2).unwrap();
        }
        write_json_atomically(
            &layout.journal_path(),
            &DirectPublishJournal {
                phase,
                db_path: layout.db_path().to_path_buf(),
                manifest_path: layout.manifest_path().to_path_buf(),
                db_candidate_path: layout.db_candidate_path(),
                manifest_candidate_path: layout.manifest_candidate_path(),
                db_sha256: sha256_hex(db_v2),
                manifest_sha256: sha256_hex(manifest_v2),
                sidecar_sha256: BTreeMap::new(),
            },
        )
        .unwrap();

        let _read_lease = store.begin_read().unwrap();
        assert_eq!(fs::read(layout.db_path()).unwrap(), db_v2);
        assert_eq!(fs::read(layout.manifest_path()).unwrap(), manifest_v2);
        assert!(!layout.journal_path().exists());
    }
}

#[test]
fn direct_store_rejects_forged_journal_candidate_paths() {
    let root = temp_dir("direct-store-rejects-forged-journal-candidate-paths");
    let layout = DirectLayout::new(root.join("graph.ldb"), root.join("manifest.json"));
    let store = DirectStore::new(layout.clone()).unwrap();
    let forged_db = root.join("forged.ldb");
    fs::write(&forged_db, b"forged").unwrap();
    fs::write(layout.manifest_candidate_path(), b"{}\n").unwrap();
    write_json_atomically(
        &layout.journal_path(),
        &DirectPublishJournal {
            phase: DirectPublishPhase::Prepared,
            db_path: layout.db_path().to_path_buf(),
            manifest_path: layout.manifest_path().to_path_buf(),
            db_candidate_path: forged_db.clone(),
            manifest_candidate_path: layout.manifest_candidate_path(),
            db_sha256: sha256_hex(b"forged"),
            manifest_sha256: sha256_hex(b"{}\n"),
            sidecar_sha256: BTreeMap::new(),
        },
    )
    .unwrap();

    let error = store.begin_read().unwrap_err();
    assert!(error.to_string().contains("unexpected candidate paths"));
    assert!(forged_db.exists());
    assert!(!layout.db_path().exists());
}

#[test]
fn managed_store_rejects_untrusted_generation_ids() {
    let root = temp_dir("managed-store-rejects-untrusted-generation-ids");
    let store = open_managed_store(root.join("storage"));
    write_json_atomically(
        &store.layout().active_pointer_path(),
        &ActiveGeneration {
            schema_version: 2,
            generation_id: "../../outside".to_string(),
            published_at: String::new(),
            activated_at_ms: 0,
        },
    )
    .unwrap();

    let error = store.resolve_active_read().unwrap_err();
    assert!(error.to_string().contains("invalid managed generation id"));
}

#[test]
fn managed_publish_rejects_a_stale_base_generation() {
    let root = temp_dir("managed-publish-rejects-a-stale-base-generation");
    let store = open_managed_store(root.join("storage"));
    let mut session = store.begin_write().unwrap();
    write_candidate_generation(
        &session.candidate.paths,
        b"db-stale",
        json!({"files": []}),
        false,
    );
    session
        .mark_ready_with_stats(&GraphSummary::default())
        .unwrap();
    write_json_atomically(
        &store.layout().active_pointer_path(),
        &ActiveGeneration {
            schema_version: 2,
            generation_id: "concurrent-generation".to_string(),
            published_at: String::new(),
            activated_at_ms: 0,
        },
    )
    .unwrap();

    let error = session
        .publish_with_stats(&GraphSummary::default())
        .unwrap_err();
    assert!(error.to_string().contains("stale generation base"));
    assert!(fs::read_dir(store.layout().runs_root())
        .unwrap()
        .next()
        .is_none());
}

#[test]
fn managed_cleanup_reports_and_later_removes_locked_runs() {
    let root = temp_dir("managed-cleanup-reports-and-later-removes-locked-runs");
    let store = open_managed_store(root.join("storage"));
    let workspace = RunWorkspace::create(store.layout().runs_root(), None).unwrap();

    let cleanup = store.cleanup().unwrap();
    assert_eq!(cleanup.run_recovery.skipped_locked, 1);
    assert!(workspace.root().exists());

    workspace.finish().unwrap();
    let cleanup = store.cleanup().unwrap();
    assert_eq!(cleanup.run_recovery.skipped_locked, 0);
    assert!(fs::read_dir(store.layout().runs_root())
        .unwrap()
        .next()
        .is_none());
}

#[test]
fn run_workspace_cleanup_rejects_symlink_content() {
    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;

        let root = temp_dir("run_workspace_cleanup_rejects_symlink_content");
        let workspace = RunWorkspace::create(root.join("runs"), None).unwrap();
        let outside = root.join("outside.txt");
        fs::write(&outside, "outside").unwrap();
        symlink(&outside, workspace.root().join("bad-link")).unwrap();

        let error = workspace.finish().unwrap_err();
        assert!(error
            .to_string()
            .contains("refusing to remove symlinked path"));
    }
}

fn publish_managed_generation(
    store: &ManagedStore,
    db_bytes: &[u8],
    manifest: serde_json::Value,
) -> String {
    let mut session = store.begin_write().unwrap();
    write_candidate_generation(&session.candidate.paths, db_bytes, manifest, false);
    session
        .mark_ready_with_stats(&GraphSummary::default())
        .unwrap();
    session
        .publish_with_stats(&GraphSummary::default())
        .unwrap()
}

enum OrphanPublishStage {
    BeforeRename,
    AfterRename,
    AfterActivePointer,
    AfterPublished,
}

fn orphan_publishing_run(
    store: &ManagedStore,
    base_generation_id: Option<String>,
    db_bytes: &[u8],
    manifest: serde_json::Value,
    stage: OrphanPublishStage,
) -> (PathBuf, String) {
    let run_id = managed_generation_id();
    let candidate_id = managed_generation_id();
    let run_root = store.layout().runs_root().join(format!("run-{run_id}"));
    let candidate_root = run_root
        .join("candidate")
        .join(format!("gen-{candidate_id}"));
    fs::create_dir_all(&candidate_root).unwrap();

    let candidate_paths = GenerationPaths::new(candidate_root.clone(), candidate_id.clone());
    write_candidate_generation(&candidate_paths, db_bytes, manifest, true);
    write_json_atomically(
        &run_root.join("journal.json"),
        &RunJournal {
            run_id,
            phase: RunPhase::Publishing,
            base_generation_id: base_generation_id.clone(),
            candidate_generation_id: Some(candidate_id.clone()),
            active_generation_id: base_generation_id.clone(),
            last_error: None,
        },
    )
    .unwrap();
    fs::write(run_root.join("lease.lock"), b"").unwrap();

    match stage {
        OrphanPublishStage::BeforeRename => {}
        OrphanPublishStage::AfterRename
        | OrphanPublishStage::AfterActivePointer
        | OrphanPublishStage::AfterPublished => {
            let published = store.layout().generation(&candidate_id).unwrap();
            if let Some(parent) = published.root().parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::rename(candidate_paths.root(), published.root()).unwrap();
            if matches!(
                stage,
                OrphanPublishStage::AfterActivePointer | OrphanPublishStage::AfterPublished
            ) {
                write_json_atomically(
                    &store.layout().active_pointer_path(),
                    &ActiveGeneration {
                        schema_version: 2,
                        generation_id: candidate_id.clone(),
                        published_at: "unix:2".to_string(),
                        activated_at_ms: 2,
                    },
                )
                .unwrap();
            }
            if matches!(stage, OrphanPublishStage::AfterPublished) {
                write_json_atomically(
                    &run_root.join("journal.json"),
                    &RunJournal {
                        run_id: name_from_run_root(&run_root),
                        phase: RunPhase::Published,
                        base_generation_id: base_generation_id.clone(),
                        candidate_generation_id: Some(candidate_id.clone()),
                        active_generation_id: Some(candidate_id.clone()),
                        last_error: None,
                    },
                )
                .unwrap();
            }
        }
    }

    (run_root, candidate_id)
}

fn write_candidate_generation(
    paths: &GenerationPaths,
    db_bytes: &[u8],
    manifest: serde_json::Value,
    write_ready: bool,
) {
    paths.ensure_root().unwrap();
    fs::write(paths.db_path(), db_bytes).unwrap();
    fs::write(
        paths.manifest_path(),
        serde_json::to_vec_pretty(&manifest).unwrap(),
    )
    .unwrap();
    write_json_atomically(
        &paths.metadata_path(),
        &json!({
            "schema_version": 2,
            "generation_id": paths.generation_id(),
            "created_at_ms": 0,
            "published_at_ms": 0,
            "base_generation_id": null,
            "logical_size_bytes": 0,
            "physical_size_bytes": 0,
            "node_count": 0,
            "edge_count": 0
        }),
    )
    .unwrap();
    if write_ready {
        fs::write(paths.ready_path(), b"ready\n").unwrap();
    }
}

fn managed_session_journal(session: &ManagedWriteSession) -> RunJournal {
    let run_root = session
        .candidate
        .paths
        .root()
        .parent()
        .and_then(|path| path.parent())
        .expect("candidate generation should live in a run workspace");
    serde_json::from_str(&fs::read_to_string(run_root.join("journal.json")).unwrap()).unwrap()
}

fn name_from_run_root(run_root: &std::path::Path) -> String {
    run_root
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap()
        .trim_start_matches("run-")
        .to_string()
}

fn dir_entries_named(root: &PathBuf, prefix: &str) -> Vec<String> {
    fs::read_dir(root)
        .unwrap()
        .filter_map(|entry| {
            let name = entry.unwrap().file_name().to_string_lossy().into_owned();
            name.contains(prefix).then_some(name)
        })
        .collect()
}

fn temp_dir(name: &str) -> PathBuf {
    let seq = TEST_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!(
        "codebase_graph_storage_{name}_{}_{}",
        std::process::id(),
        seq
    ));
    if root.exists() {
        fs::remove_dir_all(&root).unwrap();
    }
    fs::create_dir_all(&root).unwrap();
    root
}

fn open_managed_store(storage_root: PathBuf) -> ManagedStore {
    let store = ManagedStore::new(ManagedLayout::new(storage_root));
    store.ensure_layout().unwrap();
    store
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};

    let mut digest = Sha256::new();
    digest.update(bytes);
    digest
        .finalize()
        .as_slice()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
