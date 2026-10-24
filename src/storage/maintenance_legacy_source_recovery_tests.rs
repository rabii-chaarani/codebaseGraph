//! Real mailbox/dispatcher regression for a durable source intent cut.
use super::super::maintenance_budget::MemoryBox;
use super::super::maintenance_scheduler::{AttemptOutcome, TargetBinding};
use super::*;
use std::cell::RefCell;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

#[test]
fn dc_legacy_staging_owned_intent_recovers_without_another_confirmation() {
    recover_after_cut(2);
}
#[test]
fn dc_legacy_staging_owned_slot_recovers_without_another_confirmation() {
    recover_after_cut(3);
}
#[test]
fn dc_legacy_staging_owned_commit_recovers_without_another_confirmation() {
    recover_after_cut(4);
}
fn recover_after_cut(allowance: usize) {
    let root = std::env::temp_dir().join(format!(
        "legacy-source-intent-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let state = root.join(".codebaseGraph");
    std::fs::create_dir_all(state.join("storage/artifacts")).unwrap();
    std::fs::create_dir(state.join("native-staging")).unwrap();
    std::fs::create_dir(state.join("artifacts")).unwrap();
    std::fs::write(state.join("config.json"), br#"{"schema_version":3}"#).unwrap();
    let root = std::fs::canonicalize(root).unwrap();
    let target = MaintenanceTarget {
        repository_root: root.clone(),
        state_root: root.join(".codebaseGraph"),
        storage_root: None,
        direct_pair: None,
        config_path: None,
    };
    let request_target = target.clone();
    struct Work {
        scope: MemoryBox<PreparedLegacyStagingScope>,
        owner: Option<LegacySourceOwner>,
        memory: CleanupMemory,
        cut: bool,
        recovering: bool,
        done: bool,
    }
    let verified = Arc::new(AtomicBool::new(false));
    let observed = verified.clone();
    let mut retained_binding = None;
    let snapshot = super::super::maintenance_scheduler::with_claimed_legacy_confirmation(
        Some(&request_target),
        Arc::new(crate::coordinator::ExecutionDispatcher::default()),
        move |memory, request, parked: Option<MemoryBox<RefCell<Work>>>| {
            let resumed = parked.is_some();
            let work = if let Some(work) = parked {
                work
            } else {
                let binding = retained_binding.get_or_insert_with(|| {
                    MemoryBox::new(
                        super::super::maintenance_admission::prepare_target_binding(
                            &target, memory,
                        )
                        .unwrap(),
                        memory,
                    )
                    .unwrap()
                });
                let scope = prepare_legacy_staging_scope(
                    &target,
                    binding,
                    &target.state_root.join("native-staging"),
                    memory,
                )
                .unwrap();
                MemoryBox::new(
                    RefCell::new(Work {
                        scope: MemoryBox::new(scope, memory).unwrap(),
                        owner: None,
                        memory: memory.clone(),
                        cut: false,
                        recovering: false,
                        done: false,
                    }),
                    memory,
                )
                .unwrap()
            };
            let binding;
            {
                let mut work = work.borrow_mut();
                let Work {
                    scope,
                    owner,
                    memory,
                    cut,
                    recovering,
                    done,
                } = &mut *work;
                binding = TargetBinding(scope.binding.digest());
                if *cut {
                    if owner.as_ref().unwrap().closed_version().phase() == LegacyPhase::Active {
                        assert!(
                            super::super::maintenance_legacy_guard::before_staging_producer(
                                &scope.staging
                            )
                            .unwrap()
                            .is_some()
                        );
                        observed.store(true, Ordering::Release);
                        *done = true;
                    } else {
                        let state = owner.as_ref().unwrap().state_pin.try_clone().unwrap();
                        let mut wire = [0; codec::INTENT_BYTES];
                        locks::read_fixed_range(
                            &state,
                            SELECTOR + codec::COMMIT_BYTES as u64,
                            &mut wire,
                        )
                        .unwrap();
                        assert!(codec::decode_intent(
                            &wire,
                            &owner.as_ref().unwrap().binding.nonce
                        )
                        .is_ok());

                        *recovering = scope.refresh_publication_recovery(memory).unwrap();
                        assert!(*recovering, "durable own intent must be recognized");
                    }
                } else if let Some(owner) = owner.as_ref() {
                    scope
                        .prepare_transition(owner, LegacyPhase::Active, memory)
                        .unwrap();
                } else if resumed {
                    scope.refresh_enrollment_preparation(memory).unwrap();
                } else if request.is_some() {
                    scope.prepare_enrollment_metadata(request, memory).unwrap();
                }
            }
            Ok((binding, work))
        },
        move |work, confirmation, grant| {
            let mut work = work.borrow_mut();
            let Work {
                scope,
                owner,
                memory,
                cut,
                recovering,
                done,
            } = &mut *work;
            if *done {
                return Ok(AttemptOutcome::Complete);
            }
            if *recovering {
                assert!(
                    confirmation.is_none(),
                    "retry must use accepted job, not a new TRUE"
                );
                scope
                    .recover_publication(owner.as_mut(), grant, memory)
                    .unwrap();
                return Ok(AttemptOutcome::Pending);
            }
            if let Some(owner) = owner.as_mut() {
                assert!(confirmation.is_none());
                // One entry opens the actual exclusive staging gate; one fsync
                // makes each real publication write durable. Exhaustion cuts
                // after the selected intent, slot, or commit sync.
                grant
                    .take_entries(grant.remaining_entries() - allowance)
                    .unwrap();
                let result = abort_legacy_staging_preparation(scope, owner, grant, memory);
                assert!(matches!(
                    result,
                    Err(FixedControlError::Grant(
                        super::super::maintenance_budget::GrantExhausted::Entries
                    ))
                ));
                *cut = true;
                return Ok(AttemptOutcome::Pending);
            }
            match with_legacy_staging_enrollment(scope, confirmation, grant, memory, &mut |_, _| {
                Ok(())
            }) {
                Ok((_, actual)) => *owner = Some(actual),
                Err(FixedControlError::Invalid(Problem::Busy) | FixedControlError::Grant(_))
                    if scope.enrollment_pending() => {}
                Err(error) => panic!("enrollment: {error:?}"),
            }
            Ok(AttemptOutcome::Pending)
        },
    );
    assert!(verified.load(Ordering::Acquire), "{snapshot:?}");
    assert_eq!(
        snapshot.last_attempt.unwrap().outcome,
        AttemptOutcome::Complete
    );
    std::fs::remove_dir_all(root).unwrap();
}
