# PR164 Unix PATH repair — implementation report

- Baseline: `afcb83feb98c7a9cffeb2a942dd6f30b6ed23271`
- Written source: `/private/tmp/codebasegraph-pr164-ci-clean/src/client_process.rs` only
- SHA-256 of final source: `dae066b97109bd2aad214a43ab6c62836bc4952d3d0263b3f39940f3da231db3`
- Change: Unix PATH candidates are first made absolute, then accepted only if they are regular files and `libc::access(X_OK)` succeeds on the exact absolute path returned for pinning. Native path bytes are passed through `OsStrExt` and `CString`; the unsafe call has a bounded safety comment. Explicit-path logic is unchanged. Windows discovery remains untouched. Other non-Unix/non-Windows discovery retains its previous file-only behavior.
- Unix exact-filename test fixture now receives execute mode `0755`.
- Added tests: `unix_path_skips_non_executable_candidates_and_runs_selected_script_with_exact_arguments`, `unix_path_returns_none_for_all_non_executable_candidates_but_keeps_explicit_path_behavior`, and `unix_path_prefers_first_executable_candidate_and_pins_relative_directories`.
- Baseline RED was executed by the coordinator after PATH test fixtures were corrected to absolute entries for the permission cases; all three test cases exposed the reported failures (first non-executable candidate selected, all-nonexec not rejected, and relative PATH not pinned from the supplied current directory). See coordinator evidence `regression-red.log`.
- Check executed: `rustfmt --check src/client_process.rs` — passed after formatting.
- Not run by this worker: Cargo tests, Clippy, CI, or Git writes. Coordinator owns GREEN and broader verification.

## Diff reviewed

```diff
diff --git a/src/client_process.rs b/src/client_process.rs
index 483df6e..0a77306 100644
--- a/src/client_process.rs
+++ b/src/client_process.rs
@@ -123,16 +123,32 @@ fn discover_non_windows(
     }

     for directory in path_entries {
-        let candidate = directory.join(requested);
-        if candidate.is_file() {
-            return Ok(Some(ClientExecutable {
-                path: absolute_path(&candidate, current_dir)?,
-            }));
+        let candidate = absolute_path(&directory.join(requested), current_dir)?;
+        if candidate.is_file() && is_executable(&candidate) {
+            return Ok(Some(ClientExecutable { path: candidate }));
         }
     }
     Ok(None)
 }

+#[cfg(unix)]
+fn is_executable(path: &Path) -> bool {
+    use std::ffi::CString;
+    use std::os::unix::ffi::OsStrExt;
+
+    let Ok(path) = CString::new(path.as_os_str().as_bytes()) else {
+        return false;
+    };
+
+    // SAFETY: CString provides a valid, NUL-terminated native path for access.
+    unsafe { libc::access(path.as_ptr(), libc::X_OK) == 0 }
+}
+
+#[cfg(all(not(unix), not(windows)))]
+fn is_executable(_path: &Path) -> bool {
+    true
+}
+
 #[cfg(windows)]
 fn discover_windows(
     program: &str,
@@ -324,6 +340,111 @@ mod tests {
         fs::File::create(path).expect("create fixture file");
     }

+    #[cfg(unix)]
+    fn write_unix_script(path: &Path, marker: &str, mode: u32) {
+        use std::os::unix::fs::PermissionsExt;
+
+        if let Some(parent) = path.parent() {
+            fs::create_dir_all(parent).expect("create fixture parent");
+        }
+        fs::write(
+            path,
+            format!("#!/bin/sh\nprintf '%s\\n' '{marker}'\nprintf '%s\\n' \"$@\"\n"),
+        )
+        .expect("write script fixture");
+        fs::set_permissions(path, fs::Permissions::from_mode(mode))
+            .expect("set script fixture permissions");
+    }
+
+    #[cfg(unix)]
+    #[test]
+    fn unix_path_skips_non_executable_candidates_and_runs_selected_script_with_exact_arguments() {
+        let temp = TestDirectory::new();
+        let blocked = temp.path().join("blocked/client");
+        let runnable = temp.path().join("runnable/client");
+        write_unix_script(&blocked, "blocked", 0o644);
+        write_unix_script(&runnable, "runnable", 0o755);
+        let current_dir = temp.path();
+        let path_entries = [
+            blocked
+                .parent()
+                .expect("blocked PATH directory")
+                .to_path_buf(),
+            runnable
+                .parent()
+                .expect("runnable PATH directory")
+                .to_path_buf(),
+        ];
+
+        let selected = ClientExecutable::discover_in_path("client", &path_entries, current_dir)
+            .expect("discovery")
+            .expect("later executable candidate");
+        assert_eq!(selected.path, runnable);
+
+        let args = vec!["with spaces".to_string(), "plain".to_string()];
+        let output = selected
+            .command(&args)
+            .expect("construct invocation")
+            .output()
+            .expect("spawn selected executable");
+        assert!(output.status.success());
+        assert_eq!(output.stdout, b"runnable\nwith spaces\nplain\n");
+    }
+
+    #[cfg(unix)]
+    #[test]
+    fn unix_path_returns_none_for_all_non_executable_candidates_but_keeps_explicit_path_behavior() {
+        let temp = TestDirectory::new();
+        let blocked = temp.path().join("blocked/client");
+        let also_blocked = temp.path().join("also-blocked/client");
+        write_unix_script(&blocked, "blocked", 0o644);
+        write_unix_script(&also_blocked, "also-blocked", 0o644);
+        let current_dir = temp.path();
+        let path_entries = [
+            blocked
+                .parent()
+                .expect("blocked PATH directory")
+                .to_path_buf(),
+            also_blocked
+                .parent()
+                .expect("second PATH directory")
+                .to_path_buf(),
+        ];
+
+        assert!(
+            ClientExecutable::discover_in_path("client", &path_entries, current_dir)
+                .expect("PATH discovery")
+                .is_none()
+        );
+
+        let explicit = ClientExecutable::discover_in_path(
+            blocked.to_str().expect("UTF-8 fixture path"),
+            &[],
+            current_dir,
+        )
+        .expect("explicit-path discovery")
+        .expect("explicit non-executable file remains selected");
+        assert_eq!(explicit.path, blocked);
+    }
+
+    #[cfg(unix)]
+    #[test]
+    fn unix_path_prefers_first_executable_candidate_and_pins_relative_directories() {
+        let temp = TestDirectory::new();
+        let first = temp.path().join("first/client");
+        let second = temp.path().join("second/client");
+        write_unix_script(&first, "first", 0o755);
+        write_unix_script(&second, "second", 0o755);
+        let current_dir = temp.path();
+        let path_entries = [PathBuf::from("first"), PathBuf::from("second")];
+
+        let selected = ClientExecutable::discover_in_path("client", &path_entries, current_dir)
+            .expect("discovery")
+            .expect("executable candidate");
+        assert_eq!(selected.path, first);
+        assert!(selected.path.is_absolute());
+    }
+
     #[cfg(not(windows))]
     #[test]
     fn cod55_non_windows_discovery_keeps_exact_filename_semantics() {
@@ -340,6 +461,12 @@ mod tests {
                 .is_none()
         );
         touch(&exact);
+        #[cfg(unix)]
+        {
+            use std::os::unix::fs::PermissionsExt;
+            fs::set_permissions(&exact, fs::Permissions::from_mode(0o755))
+                .expect("make exact fixture executable");
+        }
         let selected = ClientExecutable::discover_in_path("client", &path_entries, &current_dir)
             .expect("exact discovery")
             .expect("exact filename selected");
```
