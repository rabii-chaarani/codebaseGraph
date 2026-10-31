# CBG-PLAT-01 S08 / COD-59 epoch observer fixture

## Scope and assumptions

- Worked only on `tests/support/storage_maintenance/process.rs` in `/tmp/codebasegraph-pr153-ci-only-source`.
- Parent-supplied baseline: `862a8f88d485d5bd257ebd9456c194156fd4ee94`, branch `fix/pr153-ci-maintenance`.
- Parent-supplied approved spec SHA: `1a9db7b558b60248888408daa6268a3b45f2f0df`.
- Did not invoke Git, Cargo/builds, remote services, Linear, model or wiki tools.
- Assumed CI-only approval permits regression tests local to this fixture. Parent explicitly approved no helper extraction.

## Change

The existing `CoveredEpochObservation::observe` retry guard now accepts either `ErrorKind::WouldBlock` or a present raw OS error equal to `fs2::lock_contended_error().raw_os_error()`. This exactly follows the classifier in `src/storage/locks.rs::is_lock_contended` without changing product code.

The existing five-second deadline, ten-millisecond retry delay, terminal error propagation, epoch checksum/covered checks, coordination creation header checks, publication binding and receipt/payload checks, and same-birth/advanced-epoch assertions are unchanged.

Added two fixture regressions:

1. `fixture::process::tests::covered_epoch_observer_retries_actual_contention_until_release` (`cfg(windows)`): owns an exclusive lock and checks an independently opened handle returns the real fs2 raw contention error; handshakes with the observer worker, requires no observer result while the lock remains held, releases the owner, then requires the observer reaches the unchanged `actual epoch envelope invalid` check on deliberately invalid fixture-owned epoch bytes. No native graph is fabricated.
2. `fixture::process::tests::covered_epoch_observer_propagates_missing_serial_lock`: requires actual missing-lock open error propagation rather than fabricated success or epoch observation.

New test-only bounds are two seconds for channel handshake and post-release receipt and 100 milliseconds for the held-lock result receive. These are not fixture/product lock deadlines. There are no sleeps or elapsed-time assertions in the regressions.

## Validation and recommended narrow commands

The worker did static source/diff inspection only, and left formatting, Cargo and execution to the parent as instructed. Suggested commands (from project root, use parent's existing target/environment):

```text
cargo fmt --all -- --check
cargo test --test storage_maintenance fixture::process::tests::covered_epoch_observer_propagates_missing_serial_lock -- --exact
cargo test --test storage_maintenance isolated_worker_first_creation_and_followup_keep_same_covered_publication_birth -- --exact
# Native Windows only:
cargo test --test storage_maintenance fixture::process::tests::covered_epoch_observer_retries_actual_contention_until_release -- --exact
```

## Limits

- Native Windows compilation and execution are pending parent/CI validation.
- The cross-platform missing-lock case checks real observer input/error propagation, but does not force a noncontention failure from inside `try_lock_shared`; injecting that would require helper extraction or OS manipulation beyond this narrow change. The unmatched error branch is unchanged.
- The Windows worker handshake proves real independent-handle contention immediately before calling the uninstrumented observer. It cannot prove the scheduler has entered the internal retry loop before the parent's bounded receive; eliminating that black-box scheduling limit would require instrumentation/helper extraction explicitly declined for this task.

## Worker diff

```diff
--- a/tests/support/storage_maintenance/process.rs
+++ b/tests/support/storage_maintenance/process.rs
@@ -734,7 +734,10 @@
             match fs2::FileExt::try_lock_shared(&serial) {
                 Ok(()) => break,
                 Err(error)
-                    if error.kind() == std::io::ErrorKind::WouldBlock
+                    if (error.kind() == std::io::ErrorKind::WouldBlock
+                        || (error.raw_os_error().is_some()
+                            && error.raw_os_error()
+                                == fs2::lock_contended_error().raw_os_error()))
                         && Instant::now() < lock_deadline =>
                 {
                     thread::sleep(Duration::from_millis(10))
@@ -851,3 +854,71 @@
         );
     }
 }
+
+#[cfg(test)]
+mod tests {
+    use super::*;
+
+    #[cfg(windows)]
+    #[test]
+    fn covered_epoch_observer_retries_actual_contention_until_release() {
+        use std::sync::mpsc::{self, RecvTimeoutError};
+
+        let root = FixtureRoot::new("epoch-observer-contention").unwrap();
+        let artifacts = root.path().join("artifacts");
+        fs::create_dir(&artifacts).unwrap();
+        let lock_path = artifacts.join(".reference-epoch.lock");
+        let owner = fs::OpenOptions::new()
+            .read(true)
+            .write(true)
+            .create_new(true)
+            .open(&lock_path)
+            .unwrap();
+        fs2::FileExt::try_lock_exclusive(&owner).unwrap();
+        // After release, the real observer must reach its unchanged integrity check.
+        fs::write(artifacts.join(".reference-epoch"), b"invalid fixture epoch").unwrap();
+        let health = serde_json::json!({ "storage_root": root.path() });
+        let (contended_tx, contended_rx) = mpsc::channel();
+        let (result_tx, result_rx) = mpsc::channel();
+
+        thread::scope(|scope| {
+            let observer = scope.spawn(|| {
+                let probe = fs::File::open(&lock_path).unwrap();
+                let error = fs2::FileExt::try_lock_shared(&probe).unwrap_err();
+                assert!(error.raw_os_error().is_some());
+                assert_eq!(
+                    error.raw_os_error(),
+                    fs2::lock_contended_error().raw_os_error()
+                );
+                drop(probe);
+                contended_tx.send(()).unwrap();
+                result_tx
+                    .send(CoveredEpochObservation::observe(&root, &health).map(|_| ()))
+                    .unwrap();
+            });
+            contended_rx.recv_timeout(Duration::from_secs(2)).unwrap();
+            let held_result = result_rx.recv_timeout(Duration::from_millis(100));
+            fs2::FileExt::unlock(&owner).unwrap();
+            let released_result = result_rx.recv_timeout(Duration::from_secs(2));
+            observer.join().unwrap();
+            assert_eq!(held_result, Err(RecvTimeoutError::Timeout));
+            assert_eq!(
+                released_result.unwrap().unwrap_err(),
+                "actual epoch envelope invalid"
+            );
+        });
+    }
+
+    #[test]
+    fn covered_epoch_observer_propagates_missing_serial_lock() {
+        let root = FixtureRoot::new("epoch-observer-missing-lock").unwrap();
+        let artifacts = root.path().join("artifacts");
+        fs::create_dir(&artifacts).unwrap();
+        let expected = fs::File::open(artifacts.join(".reference-epoch.lock")).unwrap_err();
+        let health = serde_json::json!({ "storage_root": root.path() });
+        let error = CoveredEpochObservation::observe(&root, &health)
+            .err()
+            .expect("missing serial lock must fail");
+        assert_eq!(error, expected.to_string());
+    }
+}
```
