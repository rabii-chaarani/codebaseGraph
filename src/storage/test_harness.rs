#[path = "../error.rs"]
#[allow(dead_code)]
mod error;
#[path = "../process_lifecycle/mod.rs"]
#[allow(dead_code)]
mod process_lifecycle;
#[path = "../db_writer/rss.rs"]
#[allow(dead_code)]
mod process_rss;
mod db_writer {
    pub(crate) use crate::process_rss::sample_process_rss;
}
// This deliberately small protocol-only crate exercises storage foundations.
// Higher-layer maintenance tests run in the full library with ArtifactStore.
#[path = "."]
#[allow(dead_code, unused_imports)]
mod storage {
    #[path = "atomic.rs"]
    pub(crate) mod atomic;
    #[path = "containment.rs"]
    pub(crate) mod containment;
    #[path = "direct.rs"]
    pub(crate) mod direct;
    #[path = "inventory.rs"]
    pub(crate) mod inventory;
    #[path = "layout.rs"]
    pub(crate) mod layout;
    #[path = "locks.rs"]
    pub(crate) mod locks;
    #[path = "maintenance_admission.rs"]
    pub(crate) mod maintenance_admission;
    #[path = "maintenance_budget.rs"]
    pub(crate) mod maintenance_budget;
    #[path = "maintenance_controls.rs"]
    pub(crate) mod maintenance_controls;
    #[path = "maintenance_generation_gate.rs"]
    pub(crate) mod maintenance_generation_gate;
    #[path = "maintenance_json.rs"]
    pub(crate) mod maintenance_json;
    #[path = "maintenance_legacy_guard.rs"]
    pub(crate) mod maintenance_legacy_guard;
    #[path = "maintenance_origin.rs"]
    pub(crate) mod maintenance_origin;
    #[path = "maintenance_ownership.rs"]
    pub(crate) mod maintenance_ownership;
    #[path = "maintenance_projection.rs"]
    pub(crate) mod maintenance_projection;
    #[path = "maintenance_retirement.rs"]
    pub(crate) mod maintenance_retirement;
    #[path = "maintenance_workspace_gate.rs"]
    pub(crate) mod maintenance_workspace_gate;
    #[path = "managed.rs"]
    pub(crate) mod managed;
    #[path = "run_workspace.rs"]
    pub(crate) mod run_workspace;
}

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
use storage::locks::{
    try_open_locked, try_open_locked_in_existing_parent, CoordinatorLease, LockMode, RefreshLease,
    WorkerLease,
};
use storage::managed::{ActiveGeneration, ManagedStore, ManagedWriteSession};
use storage::run_workspace::{RunJournal, RunPhase, RunWorkspace};

fn scoped_run_fixture(
    runs_root: impl AsRef<std::path::Path>,
    base: Option<String>,
) -> RunWorkspace {
    let runs_root = runs_root.as_ref();
    let runs_root = if runs_root.is_absolute() {
        runs_root.to_owned()
    } else {
        std::env::current_dir().unwrap().join(runs_root)
    };
    let layout = ManagedLayout::new(runs_root.parent().unwrap());
    layout.ensure_roots().unwrap();
    drop(storage::locks::open_locked(layout.writer_lock_path(), LockMode::Exclusive).unwrap());
    let domain = storage::maintenance_ownership::ReferenceDomain::managed(&layout).unwrap();
    RunWorkspace::create_scoped(runs_root, &domain, base).unwrap()
}

static TEST_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[test]
fn existing_lock_probe_does_not_recreate_a_vanished_workspace() {
    let root = temp_dir("existing-lock-probe-does-not-recreate-workspace");
    let run_root = root.join("runs/run-vanished");
    let error =
        try_open_locked_in_existing_parent(run_root.join("lease.lock"), LockMode::Exclusive)
            .expect_err("a vanished workspace should report not found");

    assert!(matches!(
        error,
        error::NativeError::Io(ref error) if error.kind() == std::io::ErrorKind::NotFound
    ));
    assert!(!run_root.exists());

    fs::create_dir_all(&run_root).unwrap();
    let lease =
        try_open_locked_in_existing_parent(run_root.join("lease.lock"), LockMode::Exclusive)
            .unwrap()
            .expect("an existing workspace should receive a cleanup lease");
    assert!(run_root.join("lease.lock").is_file());
    drop(lease);
}

#[test]
fn managed_control_lock_paths_are_stable_and_role_scoped() {
    let root = temp_dir("managed_control_lock_paths");
    let layout = ManagedLayout::new(root.join("storage"));

    assert_eq!(
        layout.refresh_lock_path(),
        root.join("storage/refresh.lock")
    );
    assert_eq!(layout.worker_lock_path(), root.join("storage/worker.lock"));
    assert_eq!(layout.worker_state_path(), root.join("storage/worker.json"));
    assert_eq!(
        layout.coordinator_lock_path(),
        root.join("storage/coordinator.lock")
    );
    assert_eq!(
        layout.coordinator_state_path(),
        root.join("storage/coordinator.json")
    );
    assert_ne!(layout.refresh_lock_path(), layout.worker_lock_path());
    assert_ne!(layout.worker_lock_path(), layout.coordinator_lock_path());
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
    assert_eq!(first.worker_state_path(), same.worker_state_path());
    assert_eq!(
        first.worker_workspace_root_path(),
        same.worker_workspace_root_path()
    );
    assert_eq!(first.coordinator_lock_path(), same.coordinator_lock_path());
    assert_eq!(
        first.coordinator_state_path(),
        same.coordinator_state_path()
    );
    assert_eq!(first.refresh_lock_path().parent(), Some(root.as_path()));
    assert_eq!(first.worker_lock_path().parent(), Some(root.as_path()));
    assert_ne!(first.refresh_lock_path(), first.worker_lock_path());
    assert_ne!(first.worker_lock_path(), first.coordinator_lock_path());
    assert_ne!(first.refresh_lock_path(), first.writer_lock_path());
    assert_ne!(first.refresh_lock_path(), other.refresh_lock_path());
    assert_ne!(first.worker_lock_path(), other.worker_lock_path());
    assert_ne!(first.worker_state_path(), other.worker_state_path());
    assert_ne!(
        first.worker_workspace_root_path(),
        other.worker_workspace_root_path()
    );
    assert_ne!(first.coordinator_lock_path(), other.coordinator_lock_path());
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

    let coordinator: CoordinatorLease =
        try_open_locked(layout.coordinator_lock_path(), LockMode::Exclusive)
            .unwrap()
            .expect("first coordinator owner should acquire the lease");
    assert!(
        try_open_locked(layout.coordinator_lock_path(), LockMode::Exclusive)
            .unwrap()
            .is_none()
    );
    drop(coordinator);
    assert!(
        try_open_locked(layout.coordinator_lock_path(), LockMode::Exclusive)
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
    let first_metadata: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(store.layout().generation(&first).unwrap().metadata_path()).unwrap(),
    )
    .unwrap();
    let first_published_at = first_metadata["published_at_ms"]
        .as_u64()
        .expect("published generation should carry a timestamp");
    assert!(first_published_at > 0);
    assert_eq!(reader.published_at_ms, first_published_at);

    let second = publish_managed_generation(&store, b"db-two", json!({"files": ["two"]}));
    assert_ne!(first, second);
    assert!(store.layout().generation(&first).unwrap().root().exists());

    let active_reader = store.open_read().unwrap();
    assert_eq!(active_reader.generation_id, second);
    assert_eq!(
        active_reader.published_at_ms,
        serde_json::from_str::<serde_json::Value>(
            &fs::read_to_string(store.layout().generation(&second).unwrap().metadata_path(),)
                .unwrap(),
        )
        .unwrap()["published_at_ms"]
            .as_u64()
            .unwrap()
    );
    assert_eq!(reader.published_at_ms, first_published_at);
    drop(active_reader);

    drop(reader);

    // Ordinary reader destruction releases ownership only. The bounded
    // controller owns automatic retirement; this historical compatibility
    // fixture invokes the explicit strict adapter to verify the old sweep.
    assert!(store.layout().generation(&first).unwrap().root().exists());
    let cleanup = store.cleanup().unwrap();
    assert!(cleanup.retired_generations_deleted > 0);
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

fn committed_direct_read_registration_case(corrupt: bool, read_only_parent: bool) {
    use storage::inventory::{default_direct, MaintenanceTarget};
    use storage::locks::{open_locked, path_identity};
    use storage::maintenance_ownership::{
        claim_maintenance, ArtifactUseLease, DatabaseUseLease, MaintenanceAdmission,
    };
    let root = fs::canonicalize(temp_dir("direct-read-registration")).unwrap();
    let state = root.join(".codebaseGraph");
    fs::create_dir(&state).unwrap();
    fs::write(state.join("config.json"), br#"{"schema_version":2}"#).unwrap();
    let mut target = MaintenanceTarget {
        repository_root: root.clone(),
        state_root: state.clone(),
        storage_root: None,
        direct_pair: None,
        config_path: None,
    };
    let pair = default_direct(&target);
    let layout = DirectLayout::new(&pair.database, &pair.manifest);
    target.direct_pair = Some(pair);
    fs::write(layout.db_path(), b"committed graph").unwrap();
    fs::write(layout.manifest_path(), b"{}").unwrap();
    drop(open_locked(layout.writer_lock_path(), LockMode::Exclusive).unwrap());
    for resource in [layout.db_path(), layout.manifest_path()] {
        drop(DatabaseUseLease::acquire_shared(resource).unwrap());
    }
    // A modern artifact root alone still cannot prove destination-wide
    // migration. Reading must not create that missing proof or registration.
    drop(ArtifactUseLease::acquire_shared(&layout.artifact_root_path()).unwrap());
    let registration = layout.registration_path();
    let expected_registration = if corrupt {
        fs::write(&registration, b"interrupted registration").unwrap();
        Some((
            path_identity(&registration).unwrap(),
            fs::read(&registration).unwrap(),
        ))
    } else {
        None
    };
    let database_identity = path_identity(layout.db_path()).unwrap();
    let manifest_identity = path_identity(layout.manifest_path()).unwrap();
    let permissions = fs::metadata(&state).unwrap().permissions();
    #[cfg(unix)]
    if read_only_parent {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&state, fs::Permissions::from_mode(0o555)).unwrap();
    }
    #[cfg(not(unix))]
    assert!(!read_only_parent, "Unix directory permission fixture only");
    let store = DirectStore::new(layout.clone()).unwrap();
    let reader = store.begin_read();
    // Restore fixture permissions even if admission unexpectedly fails.
    fs::set_permissions(&state, permissions).unwrap();
    let reader = reader.expect("committed read must not produce/decode registration");
    assert!(
        try_open_locked(layout.writer_lock_path(), LockMode::Exclusive)
            .unwrap()
            .is_none()
    );
    for resource in [layout.db_path(), layout.manifest_path()] {
        assert!(DatabaseUseLease::try_exclusive(resource).unwrap().is_none());
    }
    drop(reader);
    assert_eq!(path_identity(layout.db_path()).unwrap(), database_identity);
    assert_eq!(
        path_identity(layout.manifest_path()).unwrap(),
        manifest_identity
    );
    assert_eq!(fs::read(layout.db_path()).unwrap(), b"committed graph");
    assert_eq!(fs::read(layout.manifest_path()).unwrap(), b"{}");
    if let Some((identity, bytes)) = &expected_registration {
        assert_eq!(path_identity(&registration).unwrap(), *identity);
        assert_eq!(fs::read(&registration).unwrap(), *bytes);
        assert!(
            store.begin_write().is_err(),
            "producer registration remains strict"
        );
        fs::write(layout.journal_path(), b"required replay remains strict").unwrap();
        let journal_identity = path_identity(&layout.journal_path()).unwrap();
        assert!(store.begin_read().is_err());
        assert_eq!(
            path_identity(&layout.journal_path()).unwrap(),
            journal_identity
        );
        assert_eq!(fs::read(&registration).unwrap(), *bytes);
        fs::remove_file(layout.journal_path()).unwrap();
    } else {
        assert!(
            !registration.exists(),
            "read must leave missing evidence missing"
        );
    }
    match claim_maintenance(&target) {
        Ok(MaintenanceAdmission::Acquired(lease)) => {
            assert!(
                !lease.cohort_verified().unwrap(),
                "read cannot certify legacy quiescence"
            );
        }
        Ok(MaintenanceAdmission::Unknown(_)) | Ok(MaintenanceAdmission::Deferred(_)) => (),
        Err(error) => panic!("unexpected maintenance observation error: {error}"),
    }
    assert_eq!(path_identity(layout.db_path()).unwrap(), database_identity);
    assert_eq!(
        path_identity(layout.manifest_path()).unwrap(),
        manifest_identity
    );
    if let Some((identity, bytes)) = expected_registration {
        assert_eq!(path_identity(&registration).unwrap(), identity);
        assert_eq!(fs::read(&registration).unwrap(), bytes);
    } else {
        assert!(!registration.exists());
    }
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn direct_committed_read_preserves_missing_registration() {
    committed_direct_read_registration_case(false, false);
}

#[test]
fn direct_committed_read_preserves_corrupt_registration() {
    committed_direct_read_registration_case(true, false);
}

#[test]
#[cfg(unix)]
fn direct_committed_read_accepts_readonly_parent_with_existing_guards() {
    committed_direct_read_registration_case(false, true);
}

#[test]
fn direct_committed_registered_pair_reads_after_publication() {
    use storage::locks::path_identity;
    let root = temp_dir("direct-registered-read");
    let layout = DirectLayout::new(root.join("graph.ldb"), root.join("manifest.json"));
    let store = DirectStore::new(layout.clone()).unwrap();
    let mut session = store.begin_write().unwrap();
    fs::write(session.db_candidate_path(), b"published storage bundle").unwrap();
    fs::write(session.manifest_candidate_path(), b"{}").unwrap();
    session.publish().unwrap();
    session.finish();
    assert!(!layout.journal_path().exists());
    let registration = layout.registration_path();
    let registration_identity = path_identity(&registration).unwrap();
    let registration_bytes = fs::read(&registration).unwrap();
    let database_identity = path_identity(layout.db_path()).unwrap();
    let manifest_identity = path_identity(layout.manifest_path()).unwrap();
    let reader = store.begin_read().unwrap();
    assert_eq!(
        fs::read(layout.db_path()).unwrap(),
        b"published storage bundle"
    );
    assert_eq!(fs::read(layout.manifest_path()).unwrap(), b"{}");
    assert_eq!(path_identity(layout.db_path()).unwrap(), database_identity);
    assert_eq!(
        path_identity(layout.manifest_path()).unwrap(),
        manifest_identity
    );
    assert_eq!(path_identity(&registration).unwrap(), registration_identity);
    assert_eq!(fs::read(&registration).unwrap(), registration_bytes);
    assert!(
        try_open_locked(layout.writer_lock_path(), LockMode::Exclusive)
            .unwrap()
            .is_none()
    );
    drop(reader);
    fs::remove_dir_all(root).unwrap();
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
    fs::write(
        format!(
            "{}.search.lexicon.bin",
            layout.db_candidate_path().display()
        ),
        b"lexicon-v2",
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
            sidecar_sha256: BTreeMap::from([
                ("wal".to_string(), sha256_hex(b"wal-v2")),
                ("search.lexicon.bin".to_string(), sha256_hex(b"lexicon-v2")),
            ]),
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
    assert_eq!(
        fs::read(format!("{}.search.lexicon.bin", layout.db_path().display())).unwrap(),
        b"lexicon-v2"
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

fn direct_sidecar(path: &std::path::Path, suffix: &str) -> PathBuf {
    PathBuf::from(format!("{}.{suffix}", path.display()))
}

#[test]
fn direct_recovery_replays_every_rename_and_checkpoint() {
    use storage::direct::faults::{arm, Point};
    use storage::layout::DIRECT_DB_SIDECAR_SUFFIXES;

    for replacement in [false, true] {
        // All sidecars exercise each rename; a sparse bundle also removes obsolete ones.
        for sparse in [false, true] {
            let suffixes: Vec<_> = DIRECT_DB_SIDECAR_SUFFIXES
                .iter()
                .copied()
                .filter(|suffix| !sparse || suffix.starts_with("search."))
                .collect();
            let mut boundaries = vec![Point::BeforeCheckpoint(DirectPublishPhase::Prepared)];
            // Relative paths are rebound to each isolated fixture below.
            for path in std::iter::once(PathBuf::from("db/g.ldb"))
                .chain(
                    suffixes
                        .iter()
                        .map(|suffix| direct_sidecar(std::path::Path::new("db/g.ldb"), suffix)),
                )
                .chain(std::iter::once(PathBuf::from("manifest/m.json")))
            {
                if replacement {
                    boundaries.push(Point::AfterShadowRename(path.clone()));
                }
                boundaries.push(Point::AfterCandidateRename(path));
            }
            for phase in [
                DirectPublishPhase::DatabasePromoted,
                DirectPublishPhase::ManifestPromoted,
                DirectPublishPhase::Committed,
            ] {
                boundaries.push(Point::BeforeCheckpoint(phase));
            }
            for boundary in boundaries {
                let root = temp_dir("dr");
                let layout = DirectLayout::new(root.join("db/g.ldb"), root.join("manifest/m.json"));
                let store = DirectStore::new(layout.clone()).unwrap();
                let source = root.join("source.rs");
                fs::write(&source, b"source stays untouched").unwrap();
                let mut session = store.begin_write().unwrap();
                if replacement {
                    fs::write(layout.db_path(), b"db-v1").unwrap();
                    fs::write(layout.manifest_path(), b"manifest-v1").unwrap();
                    for suffix in DIRECT_DB_SIDECAR_SUFFIXES {
                        fs::write(direct_sidecar(layout.db_path(), suffix), b"sidecar-v1").unwrap();
                    }
                }
                fs::write(session.db_candidate_path(), b"db-v2").unwrap();
                fs::write(session.manifest_candidate_path(), b"manifest-v2").unwrap();
                for suffix in &suffixes {
                    fs::write(
                        direct_sidecar(&session.db_candidate_path(), suffix),
                        format!("v2-{suffix}"),
                    )
                    .unwrap();
                }
                let point = match boundary.clone() {
                    Point::AfterShadowRename(path) => Point::AfterShadowRename(root.join(path)),
                    Point::AfterCandidateRename(path) => {
                        Point::AfterCandidateRename(root.join(path))
                    }
                    point => point,
                };
                let fault = arm(point);
                let error = session.publish().unwrap_err();
                assert!(
                    error.to_string().contains("injected Direct"),
                    "{boundary:?}: {error}"
                );
                drop(fault);
                drop(session);
                if boundary == Point::BeforeCheckpoint(DirectPublishPhase::Prepared) {
                    for _ in 0..3 {
                        drop(store.begin_read().unwrap());
                    }
                    assert_eq!(layout.db_path().exists(), replacement);
                    assert_eq!(layout.manifest_path().exists(), replacement);
                    if replacement {
                        assert_eq!(fs::read(layout.db_path()).unwrap(), b"db-v1");
                        assert_eq!(fs::read(layout.manifest_path()).unwrap(), b"manifest-v1");
                    }
                } else {
                    // Reinterrupt recovery twice, proving valid promoted sidecars survive replay.
                    for _ in 0..2 {
                        let fault = arm(Point::BeforeCheckpoint(DirectPublishPhase::Committed));
                        assert!(store
                            .begin_read()
                            .unwrap_err()
                            .to_string()
                            .contains("injected Direct"));
                        drop(fault);
                        assert!(layout.journal_path().exists());
                    }
                    for _ in 0..3 {
                        drop(store.begin_read().unwrap());
                        assert_eq!(
                            sha256_hex(&fs::read(layout.db_path()).unwrap()),
                            sha256_hex(b"db-v2")
                        );
                        assert_eq!(
                            sha256_hex(&fs::read(layout.manifest_path()).unwrap()),
                            sha256_hex(b"manifest-v2")
                        );
                        for suffix in DIRECT_DB_SIDECAR_SUFFIXES {
                            let path = direct_sidecar(layout.db_path(), suffix);
                            if suffixes.contains(suffix) {
                                assert_eq!(
                                    sha256_hex(&fs::read(path).unwrap()),
                                    sha256_hex(format!("v2-{suffix}").as_bytes())
                                );
                            } else {
                                assert!(!path.exists());
                            }
                        }
                    }
                }
                assert!(!layout.journal_path().exists());
                assert!(!layout.db_candidate_path().exists());
                assert!(!layout.manifest_candidate_path().exists());
                for suffix in DIRECT_DB_SIDECAR_SUFFIXES {
                    assert!(!direct_sidecar(&layout.db_candidate_path(), suffix).exists());
                }
                assert_eq!(fs::read(source).unwrap(), b"source stays untouched");
                fs::remove_dir_all(root).unwrap();
            }
        }
    }
}

#[test]
fn direct_prepared_recovery_preserves_expected_sidecars_on_validation_failure() {
    use storage::direct::faults::{arm, Point};
    for damage in [
        "missing",
        "corrupt",
        "directory",
        #[cfg(unix)]
        "symlink",
    ] {
        let root = temp_dir("drbad");
        let layout = DirectLayout::new(root.join("g.ldb"), root.join("m.json"));
        let store = DirectStore::new(layout.clone()).unwrap();
        let mut session = store.begin_write().unwrap();
        fs::write(session.db_candidate_path(), b"db-v2").unwrap();
        fs::write(session.manifest_candidate_path(), b"manifest-v2").unwrap();
        for suffix in ["search.lexicon.bin", "search.postings.bin"] {
            fs::write(direct_sidecar(&session.db_candidate_path(), suffix), suffix).unwrap();
        }
        let fault = arm(Point::BeforeCheckpoint(
            DirectPublishPhase::DatabasePromoted,
        ));
        assert!(session.publish().is_err());
        drop(fault);
        drop(session);
        let damaged = direct_sidecar(layout.db_path(), "search.postings.bin");
        let protected = root.join("protected-source.rs");
        fs::write(&protected, b"search.postings.bin").unwrap();
        let expected_error = match damage {
            "missing" => {
                fs::remove_file(&damaged).unwrap();
                "publish source does not exist"
            }
            "directory" => {
                fs::remove_file(&damaged).unwrap();
                fs::create_dir(&damaged).unwrap();
                "not a file"
            }
            #[cfg(unix)]
            "symlink" => {
                fs::remove_file(&damaged).unwrap();
                std::os::unix::fs::symlink(&protected, &damaged).unwrap();
                "symlinked file"
            }
            _ => {
                fs::write(&damaged, b"corrupt").unwrap();
                "checksum mismatch"
            }
        };
        // A stray unjournaled candidate has no replay/deletion ownership. Keep
        // its exact inode/bytes outside the committed bundle across recovery.
        let stray = direct_sidecar(&layout.db_candidate_path(), "wal");
        fs::write(&stray, b"stray").unwrap();
        let stray_identity = storage::locks::path_identity(&stray).unwrap();
        for _ in 0..3 {
            let error = store.begin_read().unwrap_err();
            assert!(
                error.to_string().contains(expected_error),
                "{damage}: {error}"
            );
            assert_eq!(fs::read(&protected).unwrap(), b"search.postings.bin");
            assert_eq!(
                fs::read(direct_sidecar(layout.db_path(), "search.lexicon.bin")).unwrap(),
                b"search.lexicon.bin"
            );
            assert!(!direct_sidecar(layout.db_path(), "wal").exists());
            assert_eq!(fs::read(&stray).unwrap(), b"stray");
            assert_eq!(
                storage::locks::path_identity(&stray).unwrap(),
                stray_identity
            );
            assert!(layout.journal_path().exists());
            assert!(!layout.manifest_path().exists());
        }
        if damage == "directory" {
            fs::remove_dir(&damaged).unwrap();
        } else if damage != "missing" {
            fs::remove_file(&damaged).unwrap();
        }
        fs::write(damaged, b"search.postings.bin").unwrap();
        drop(store.begin_read().unwrap());
        assert!(!layout.journal_path().exists());
        assert_eq!(fs::read(&stray).unwrap(), b"stray");
        assert_eq!(
            storage::locks::path_identity(&stray).unwrap(),
            stray_identity
        );
        assert!(!direct_sidecar(layout.db_path(), "wal").exists());
        assert!(store.candidate_cleanup_pending());
        fs::remove_dir_all(root).unwrap();
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
fn managed_publish_reconciles_an_error_after_active_pointer_commit() {
    let root = temp_dir("managed_publish_reconciles_post_commit_error");
    let store = open_managed_store(root.join("storage"));
    let first_generation = publish_managed_generation(&store, b"db-first", json!({"files": []}));
    let first_paths = store.layout().generation(&first_generation).unwrap();
    fs::create_dir(first_paths.retired_path()).unwrap();

    let mut session = store.begin_write().unwrap();
    let expected_generation = session.candidate.generation_id.clone();
    write_candidate_generation(
        &session.candidate.paths,
        b"db-second",
        json!({"files": []}),
        false,
    );
    session
        .mark_ready_with_stats(&GraphSummary::default())
        .unwrap();

    let published_generation = session
        .publish_with_stats(&GraphSummary::default())
        .unwrap();
    assert_eq!(published_generation, expected_generation);
    assert_eq!(
        store
            .read_active_generation()
            .unwrap()
            .unwrap()
            .generation_id,
        expected_generation
    );

    session.finish();
    let cleanup = store.cleanup().unwrap();
    assert_eq!(cleanup.run_recovery.skipped_locked, 0);
    assert!(fs::read_dir(store.layout().runs_root())
        .unwrap()
        .next()
        .is_none());
}

#[test]
fn managed_cleanup_reports_and_later_removes_locked_runs() {
    let root = temp_dir("managed-cleanup-reports-and-later-removes-locked-runs");
    let store = open_managed_store(root.join("storage"));
    let workspace = scoped_run_fixture(store.layout().runs_root(), None);

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
        let workspace = scoped_run_fixture(root.join("runs"), None);
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
    let workspace = scoped_run_fixture(store.layout().runs_root(), base_generation_id.clone());
    let run_root = workspace.root().to_owned();
    let run_id = name_from_run_root(&run_root);
    let candidate_id = managed_generation_id();
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

    assert_eq!(workspace.abandon_for_test(), run_root);
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

fn unknown_phase_registry(root: &std::path::Path) -> PathBuf {
    use storage::containment::{BuildDescriptorV2, ContainmentProof};
    let lifecycle = root.join("phase-test/.lifecycle-test");
    fs::create_dir_all(&lifecycle).unwrap();
    fs::write(lifecycle.join("lease"), b"").unwrap();
    let descriptor = BuildDescriptorV2 {
        version: 2,
        build_id: "a".repeat(64),
        lifecycle_dir: lifecycle.clone(),
        endpoint: String::new(),
        nonce: "b".repeat(64),
        worker_pid: std::process::id(),
        guardian_pid: 0,
        proof: ContainmentProof::Pending,
    };
    write_json_atomically(&lifecycle.join("proof.json"), &descriptor).unwrap();
    lifecycle
}
fn prove_phase_empty(lifecycle: &std::path::Path) {
    use storage::containment::ExitOutcome;
    write_json_atomically(
        &lifecycle.join("receipt.json"),
        &ExitOutcome {
            success: false,
            code: None,
            signal: None,
            high_water_bytes: 0,
            budget_reason: None,
            cancelled: true,
            containment_empty: true,
            supervision_error: None,
        },
    )
    .unwrap();
}
#[test]
fn phase_registry_probe_never_recreates_missing_roots_or_locks() {
    let root = temp_dir("phase_probe");
    let missing = root.join("missing");
    assert!(storage::containment::probe_registry(&missing).unwrap());
    assert!(!missing.exists());
    let lifecycle = root.join("phase/.lifecycle-incomplete");
    fs::create_dir_all(&lifecycle).unwrap();
    fs::write(lifecycle.join("proof.json"), b"{}").unwrap();
    assert!(!storage::containment::probe_registry(&root).unwrap());
    assert!(!lifecycle.join("lease").exists());
}
#[test]
fn managed_phase_unknown_preserves_candidate_and_active_read_until_empty() {
    let root = temp_dir("managed_phase_barrier");
    let store = open_managed_store(root.join("s"));
    let base = publish_managed_generation(&store, b"base", json!({}));
    let (run, candidate) = orphan_publishing_run(
        &store,
        Some(base.clone()),
        b"candidate",
        json!({}),
        OrphanPublishStage::BeforeRename,
    );
    let lifecycle = unknown_phase_registry(&store.layout().phase_workspace_root());
    assert!(store
        .begin_write()
        .unwrap_err()
        .to_string()
        .contains("containment_unknown"));
    let blocked = store.cleanup().unwrap();
    assert_eq!(blocked.run_recovery.publishing_recovered, 0);
    assert_eq!(blocked.run_recovery.skipped_locked, 1);
    assert!(run.exists());
    assert_eq!(store.open_read().unwrap().generation_id, base);
    assert!(lifecycle.join("proof.json").exists());
    prove_phase_empty(&lifecycle);
    assert_eq!(
        store.cleanup().unwrap().run_recovery.publishing_recovered,
        1
    );
    assert_eq!(store.open_read().unwrap().generation_id, candidate);
    assert!(!run.exists());
    drop(store.begin_write().unwrap());
}
#[test]
fn managed_abort_and_drop_retain_runs_while_phase_unknown() {
    let root = temp_dir("managed_abort_barrier");
    let store = open_managed_store(root.join("s"));
    let base = publish_managed_generation(&store, b"base", json!({}));
    let session = store.begin_write().unwrap();
    let candidate = session.candidate.paths.root().to_path_buf();
    let lifecycle = unknown_phase_registry(&session.phase_workspace_root());
    assert!(session
        .abort(Some("failure".into()))
        .unwrap_err()
        .to_string()
        .contains("containment_unknown"));
    assert!(candidate.exists());
    let pending = store.cleanup().unwrap();
    assert_eq!(pending.run_recovery.deleted, 0);
    assert_eq!(pending.run_recovery.skipped_locked, 1);
    assert_eq!(store.open_read().unwrap().generation_id, base);
    assert!(candidate.exists());
    prove_phase_empty(&lifecycle);
    let cleaned = store.cleanup().unwrap();
    assert_eq!(cleaned.run_recovery.deleted, 1);
    assert_eq!(cleaned.run_recovery.skipped_locked, 0);
    assert!(!candidate.exists());
    let session = store.begin_write().unwrap();
    let candidate = session.candidate.paths.root().to_path_buf();
    fs::remove_dir_all(store.layout().phase_workspace_root()).unwrap();
    let lifecycle = unknown_phase_registry(&session.phase_workspace_root());
    drop(session);
    assert!(candidate.exists());
    prove_phase_empty(&lifecycle);
    store.cleanup().unwrap();
    assert!(!candidate.exists());
}
#[test]
fn direct_phase_unknown_preserves_base_and_blocks_publish_replay_and_cleanup() {
    let root = temp_dir("direct_phase_barrier");
    let layout = DirectLayout::new(root.join("graph.ldb"), root.join("manifest.json"));
    let store = DirectStore::new(layout.clone()).unwrap();
    fs::write(layout.db_path(), b"base").unwrap();
    fs::write(layout.manifest_path(), b"{}").unwrap();
    let mut session = store.begin_write().unwrap();
    fs::write(session.db_candidate_path(), b"candidate").unwrap();
    fs::write(session.manifest_candidate_path(), b"{}").unwrap();
    let lifecycle = unknown_phase_registry(&session.phase_workspace_root());
    assert!(session
        .publish()
        .unwrap_err()
        .to_string()
        .contains("containment_unknown"));
    drop(session);
    assert!(layout.db_candidate_path().exists());
    drop(store.begin_read().unwrap());
    assert_eq!(fs::read(layout.db_path()).unwrap(), b"base");
    assert!(store
        .begin_write()
        .unwrap_err()
        .to_string()
        .contains("containment_unknown"));
    write_json_atomically(
        &layout.journal_path(),
        &DirectPublishJournal {
            phase: DirectPublishPhase::Prepared,
            db_path: layout.db_path().into(),
            db_candidate_path: layout.db_candidate_path(),
            manifest_path: layout.manifest_path().into(),
            manifest_candidate_path: layout.manifest_candidate_path(),
            db_sha256: sha256_hex(b"candidate"),
            manifest_sha256: sha256_hex(b"{}"),
            sidecar_sha256: BTreeMap::new(),
        },
    )
    .unwrap();
    assert!(store
        .begin_read()
        .unwrap_err()
        .to_string()
        .contains("containment_unknown"));
    assert!(layout.db_candidate_path().exists());
    prove_phase_empty(&lifecycle);
    drop(store.begin_read().unwrap());
    assert_eq!(fs::read(layout.db_path()).unwrap(), b"candidate");
    assert!(!layout.db_candidate_path().exists());
}

#[test]
fn persisted_run_barrier_survives_drop_and_blocks_generic_orphan_collection() {
    let root = temp_dir("run_phase_barrier");
    let runs = root.join("runs");
    let registry = root.join("phases");
    let mut run = scoped_run_fixture(&runs, None);
    run.set_cleanup_barrier(registry.clone()).unwrap();
    let run_root = run.root().to_path_buf();
    fs::write(run.candidate_root().join("owned"), b"candidate").unwrap();
    let lifecycle = unknown_phase_registry(&registry);
    drop(run);
    assert!(run_root.join("candidate/owned").exists());
    assert_eq!(
        RunWorkspace::cleanup_orphans(&runs).unwrap().skipped_locked,
        1
    );
    assert!(run_root.exists());
    prove_phase_empty(&lifecycle);
    assert_eq!(RunWorkspace::cleanup_orphans(&runs).unwrap().deleted, 1);
    assert!(!run_root.exists());
}

#[test]
fn empty_prelease_startup_is_retired_and_does_not_block_next_writer() {
    let root = temp_dir("prelease_retire");
    let store = open_managed_store(root.join("s"));
    let phase = store.layout().phase_workspace_root().join("phase-orphan");
    let lifecycle = phase.join(".lifecycle-empty");
    fs::create_dir_all(&lifecycle).unwrap();
    assert!(storage::containment::probe_registry(&store.layout().phase_workspace_root()).unwrap());
    assert!(lifecycle.join("retired").is_file());
    assert!(
        fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(lifecycle.join("lease"))
            .is_err(),
        "late preparer must fail before it can publish launch metadata"
    );
    drop(store.begin_write().unwrap());
    assert!(
        !phase.exists(),
        "later writer collects the safely retired orphan"
    );
}
#[test]
fn missing_lease_with_any_launch_evidence_remains_unknown() {
    let root = temp_dir("prelease_unknown");
    for name in ["launch.json", "proof.json", "ready.json", "receipt.json"] {
        let lifecycle = root.join(format!("phase-{name}/.lifecycle-proof"));
        fs::create_dir_all(&lifecycle).unwrap();
        fs::write(lifecycle.join(name), b"{}").unwrap();
        assert!(!storage::containment::probe_lifecycle(&lifecycle).unwrap());
        assert!(!lifecycle.join("lease").exists());
        assert!(!lifecycle.join("retired").exists());
    }
}
#[test]
fn phase_registry_collection_retains_unknown_siblings_then_reclaims_empty_orphans() {
    let root = temp_dir("registry_collect");
    let registry = root.join("phases");
    let known_parent = registry.join("phase-known");
    let known = unknown_phase_registry(&known_parent);
    prove_phase_empty(&known);
    fs::write(known_parent.join("request.json"), b"{}").unwrap();
    fs::write(known_parent.join("stderr"), b"diagnostics").unwrap();
    let unknown_parent = registry.join("phase-unknown");
    let unknown = unknown_phase_registry(&unknown_parent);
    assert!(storage::containment::collect_registry(&registry)
        .unwrap_err()
        .contains("containment_unknown"));
    assert!(known_parent.exists());
    assert!(unknown_parent.exists());
    prove_phase_empty(&unknown);
    storage::containment::collect_registry(&registry).unwrap();
    assert!(!known_parent.exists());
    assert!(!unknown_parent.exists());
}

#[test]
fn relative_registry_probe_matches_absolute_private_descriptor() {
    let relative = PathBuf::from("target").join(format!("registry-{}", managed_generation_id()));
    let absolute = std::env::current_dir().unwrap().join(&relative);
    let lifecycle = unknown_phase_registry(&absolute);
    prove_phase_empty(&lifecycle);
    assert!(storage::containment::probe_registry(&relative).unwrap());
    assert!(lifecycle.join("retired").exists());
    fs::remove_dir_all(absolute).unwrap();
}

#[test]
fn owned_writer_cleanup_reports_readers_and_collects_after_their_release() {
    let root = temp_dir("owned_writer_gc");
    let store = open_managed_store(root.join("s"));
    let base = publish_managed_generation(&store, b"base", json!({}));
    let reader = store.open_read().unwrap();
    let mut session = store.begin_write().unwrap();
    write_candidate_generation(&session.candidate.paths, b"next", json!({}), false);
    session
        .publish_with_stats(&GraphSummary::default())
        .unwrap();
    let observed = store.cleanup().unwrap();
    assert_eq!(
        observed.retired_generations_pending, 1,
        "contended cleanup still reports the retained retirement marker"
    );
    assert_eq!(
        session
            .recover_and_gc()
            .unwrap()
            .retired_generations_pending,
        1
    );
    assert!(store.layout().generation(&base).unwrap().root().exists());
    drop(reader);
    assert_eq!(
        session
            .recover_and_gc()
            .unwrap()
            .retired_generations_pending,
        0
    );
    assert!(!store.layout().generation(&base).unwrap().root().exists());
    session.finish();
    assert_eq!(
        fs::read_dir(store.layout().generations_root())
            .unwrap()
            .count(),
        1
    );
}

#[test]
fn persisted_relative_barrier_survives_cleanup_from_a_different_process_cwd() {
    use fs2::FileExt;
    let root = temp_dir("cwd_barrier");
    let other = root.join("other");
    fs::create_dir(&other).unwrap();
    let runs = root.join("runs");
    let lifecycle = root.join("phases/phase-owned/.lifecycle-unready");
    fs::create_dir_all(&lifecycle).unwrap();
    let lease = fs::OpenOptions::new()
        .create_new(true)
        .read(true)
        .write(true)
        .open(lifecycle.join("lease"))
        .unwrap();
    lease.lock_exclusive().unwrap();
    run_relative_barrier_fixture(&root, &runs, "create", true);
    let run_root = fs::read_dir(&runs).unwrap().next().unwrap().unwrap().path();
    assert!(run_root.join("candidate/owned").is_file());
    assert!(!other.join("phases").exists());
    run_relative_barrier_fixture(&other, &runs, "cleanup", true);
    assert!(run_root.exists());
    drop(lease);
    run_relative_barrier_fixture(&other, &runs, "cleanup", false);
    assert!(!run_root.exists());
    assert!(!other.join("phases").exists());
}
fn run_relative_barrier_fixture(
    cwd: &std::path::Path,
    runs: &std::path::Path,
    mode: &str,
    retained: bool,
) {
    use std::{
        process::{Command, Stdio},
        thread,
        time::{Duration, Instant},
    };
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--ignored",
            "--exact",
            "relative_barrier_subprocess_fixture",
            "--nocapture",
        ])
        .current_dir(cwd)
        .env("CBG_BARRIER_FIXTURE_RUNS", runs)
        .env("CBG_BARRIER_FIXTURE_MODE", mode)
        .env(
            "CBG_BARRIER_FIXTURE_RETAINED",
            if retained { "yes" } else { "no" },
        )
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if child.try_wait().unwrap().is_some() {
            let output = child.wait_with_output().unwrap();
            assert!(
                output.status.success(),
                "barrier fixture {mode} failed: {}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            return;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let output = child.wait_with_output().unwrap();
            panic!(
                "barrier fixture {mode} timed out: {}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
        thread::sleep(Duration::from_millis(10));
    }
}
#[test]
#[ignore = "subprocess fixture for cwd-independent run cleanup"]
fn relative_barrier_subprocess_fixture() {
    let Some(runs) = std::env::var_os("CBG_BARRIER_FIXTURE_RUNS") else {
        return;
    };
    let runs = PathBuf::from(runs);
    match std::env::var("CBG_BARRIER_FIXTURE_MODE").unwrap().as_str() {
        "create" => {
            let mut run = scoped_run_fixture(&runs, None);
            run.set_cleanup_barrier(PathBuf::from("phases")).unwrap();
            let root = run.root().to_path_buf();
            fs::write(run.candidate_root().join("owned"), b"candidate").unwrap();
            drop(run);
            assert!(root.join("candidate/owned").is_file());
        }
        "cleanup" => {
            let retained = std::env::var("CBG_BARRIER_FIXTURE_RETAINED").unwrap() == "yes";
            let report = RunWorkspace::cleanup_orphans(&runs).unwrap();
            assert_eq!(report.skipped_locked, usize::from(retained));
            assert_eq!(report.deleted, usize::from(!retained));
        }
        mode => panic!("unknown barrier fixture mode: {mode}"),
    }
}

#[cfg(unix)]
#[test]
fn private_lifecycle_receipt_accepts_ancestor_alias_but_rejects_foreign_or_symlink_roots() {
    use std::os::unix::fs::symlink;
    use storage::containment::{probe_lifecycle, BuildDescriptorV2};
    let root = temp_dir("lifecycle_alias");
    let physical = root.join("physical");
    let lifecycle = unknown_phase_registry(&physical);
    prove_phase_empty(&lifecycle);
    let alias = root.join("alias");
    symlink(&physical, &alias).unwrap();
    let alias_lifecycle = alias.join("phase-test/.lifecycle-test");
    assert!(!fs::symlink_metadata(&alias_lifecycle)
        .unwrap()
        .file_type()
        .is_symlink());
    assert!(probe_lifecycle(&alias_lifecycle).unwrap());
    assert!(lifecycle.join("retired").is_file());
    let foreign = unknown_phase_registry(&root.join("foreign"));
    prove_phase_empty(&foreign);
    let mut descriptor: BuildDescriptorV2 =
        serde_json::from_slice(&fs::read(lifecycle.join("proof.json")).unwrap()).unwrap();
    write_json_atomically(&foreign.join("proof.json"), &descriptor).unwrap();
    assert!(!probe_lifecycle(&foreign).unwrap());
    let leaf_alias = root.join(".lifecycle-symlink");
    symlink(&lifecycle, &leaf_alias).unwrap();
    assert!(!probe_lifecycle(&leaf_alias).unwrap());
    descriptor.lifecycle_dir = leaf_alias;
    write_json_atomically(&lifecycle.join("proof.json"), &descriptor).unwrap();
    assert!(!probe_lifecycle(&lifecycle).unwrap());
    descriptor.lifecycle_dir = root.join("missing");
    write_json_atomically(&lifecycle.join("proof.json"), &descriptor).unwrap();
    assert!(!probe_lifecycle(&lifecycle).unwrap());
}

#[test]
fn final_containment_verification_requires_prior_retirement_without_writing() {
    use fs2::FileExt;
    let root = temp_dir("containment_final_readonly");
    let lifecycle = unknown_phase_registry(&root);
    prove_phase_empty(&lifecycle);
    let marker = lifecycle.join("retired");
    assert!(!marker.exists());
    assert!(storage::containment::ensure_retired_registry_empty(&root).is_err());
    assert!(!marker.exists(), "final verification created a marker");
    storage::containment::ensure_registry_empty(&root).unwrap();
    let bytes = fs::read(&marker).unwrap();
    storage::containment::ensure_retired_registry_empty(&root).unwrap();
    assert_eq!(fs::read(&marker).unwrap(), bytes);
    let lease = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(lifecycle.join("lease"))
        .unwrap();
    lease.try_lock_exclusive().unwrap();
    assert!(storage::containment::ensure_retired_registry_empty(&root).is_err());
    assert_eq!(fs::read(&marker).unwrap(), bytes);
}
