//! Immutable release metadata. GitHub authorization and mutations stay in the workflow.
use serde::Serialize;
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

#[cfg(all(test, unix))]
mod workflow_tests;

#[derive(Debug, serde::Deserialize, serde::Serialize)]
struct CratePreflightReport {
    schema_version: u32,
    source_sha: String,
    version: String,
    tag_name: String,
    verification: String,
    crate_bytes: u64,
    crate_sha256: String,
}

pub(super) fn package_preflight_command(args: Vec<String>) -> Result<(), String> {
    package_preflight_with(&args, Path::new("curl"), Path::new("cargo"))
}

fn package_preflight_with(
    args: &[String],
    curl: &Path,
    cargo_executable: &Path,
) -> Result<(), String> {
    let options =
        super::parse_options(args, &["--source-dir", "--source-sha", "--tag", "--output"])?;
    let source_dir = Path::new(super::required_option(&options, "--source-dir")?);
    let source_sha = super::required_option(&options, "--source-sha")?;
    super::validate_commit_sha(source_sha)?;
    let tag = super::required_option(&options, "--tag")?;
    let version = super::ci::validate_release_tag(tag)?;
    let output_argument = Path::new(super::required_option(&options, "--output")?);
    let output_path = report_path_outside_source(source_dir, output_argument)?;

    super::ci::verify_checkout(source_dir, source_sha)?;
    let candidate = super::ci::validate_release_candidate(source_dir, Some(&version))?;
    if candidate.version != version {
        return Err("selected source version does not match the requested tag".to_string());
    }

    let registry = query_registry_version_with(curl, &version)?;
    let temp = super::unique_temp_dir("release_package_preflight")?;
    if !temp.is_absolute() {
        let _ = fs::remove_dir_all(&temp);
        return Err("fresh package target directory must be absolute".to_string());
    }
    let result = (|| {
        let target_dir = temp.join("target");
        let mut cargo = Command::new(cargo_executable);
        if registry {
            cargo.args(["package", "--locked"]);
        } else {
            cargo.args(["publish", "--dry-run", "--locked"]);
        }
        cargo
            .arg("--target-dir")
            .arg(&target_dir)
            .current_dir(source_dir);
        let output = cargo
            .output()
            .map_err(|error| format!("could not run Cargo package preflight: {error}"))?;
        if !output.status.success() {
            return Err(format!(
                "Cargo package preflight failed with status {}\nstdout:\n{}\nstderr:\n{}",
                output.status,
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            ));
        }
        let package_dir = target_dir.join("package");
        let archive_name = format!("codebase-graph-{version}.crate");
        let archive = package_dir.join(&archive_name);
        if !registry {
            let staged = package_dir.join("tmp-crate").join(&archive_name);
            fs::rename(&staged, &archive).map_err(|error| {
                format!(
                    "could not normalize verified Cargo archive {}: {error}; status {}\nstdout:\n{}\nstderr:\n{}",
                    staged.display(),
                    output.status,
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr)
                )
            })?;
        }
        let bytes = fs::metadata(&archive)
            .map_err(|error| {
                format!(
                    "expected Cargo archive {} is unavailable: {error}; status {}\nstdout:\n{}\nstderr:\n{}",
                    archive.display(),
                    output.status,
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr)
                )
            })?
            .len();
        super::validate_crate_size(bytes)?;
        let crate_sha256 = super::sha256_file(&archive)?;
        super::ci::verify_checkout(source_dir, source_sha)?;
        let report = CratePreflightReport {
            schema_version: 1,
            source_sha: source_sha.to_string(),
            version: version.clone(),
            tag_name: tag.to_string(),
            verification: if registry {
                "package-already-published"
            } else {
                "publish-dry-run"
            }
            .to_string(),
            crate_bytes: bytes,
            crate_sha256,
        };
        let json = serde_json::to_vec(&report)
            .map_err(|error| format!("could not serialize crate preflight report: {error}"))?;
        fs::write(output_path, json)
            .map_err(|error| format!("could not write crate preflight report: {error}"))?;
        println!("crate package preflight passed for {tag} ({bytes} bytes)");
        Ok(())
    })();
    let _ = fs::remove_dir_all(&temp);
    result
}

fn report_path_outside_source(source_dir: &Path, output: &Path) -> Result<PathBuf, String> {
    let parent = output
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let parent = std::fs::canonicalize(parent)
        .map_err(|error| format!("could not resolve preflight report directory: {error}"))?;
    let file_name = output
        .file_name()
        .ok_or_else(|| "preflight report path must name a file".to_string())?;
    let resolved = parent.join(file_name);
    let source = std::fs::canonicalize(source_dir)
        .map_err(|error| format!("could not resolve selected source directory: {error}"))?;
    if resolved == source || resolved.starts_with(&source) {
        return Err("preflight report must be outside selected source".to_string());
    }
    if std::fs::symlink_metadata(&resolved).is_ok() {
        return Err("preflight output path must not already exist".to_string());
    }
    Ok(resolved)
}

fn query_registry_version_with(curl: &Path, version: &str) -> Result<bool, String> {
    let url = format!("https://crates.io/api/v1/crates/codebase-graph/{version}");
    let user_agent = format!(
        "codebaseGraph-release-workflow ({}/{})",
        std::env::var("GITHUB_SERVER_URL").unwrap_or_else(|_| "https://github.com".to_string()),
        std::env::var("GITHUB_REPOSITORY")
            .unwrap_or_else(|_| "rabii-chaarani/codebaseGraph".to_string())
    );
    let output = Command::new(curl)
        .args([
            "--silent",
            "--show-error",
            "--connect-timeout",
            "10",
            "--max-time",
            "30",
            "--user-agent",
            &user_agent,
            "--output",
            "-",
            "--write-out",
            "\n%{http_code}",
            &url,
        ])
        .output()
        .map_err(|error| format!("could not query crates.io for {version}: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "crates.io query failed for {version} with status {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let split = output
        .stdout
        .iter()
        .rposition(|byte| *byte == b'\n')
        .ok_or_else(|| "crates.io response omitted its HTTP status".to_string())?;
    let body = &output.stdout[..split];
    let status = std::str::from_utf8(&output.stdout[split + 1..])
        .map_err(|_| "crates.io HTTP status is invalid".to_string())?;
    match status {
        "404" => Ok(false),
        "200" => {
            let value: Value = serde_json::from_slice(body)
                .map_err(|error| format!("crates.io response is malformed: {error}"))?;
            if value["version"]["num"].as_str() != Some(version)
                || value["version"]["crate"].as_str() != Some("codebase-graph")
            {
                return Err("crates.io returned a mismatched crate version".to_string());
            }
            Ok(true)
        }
        other => Err(format!("crates.io returned unexpected HTTP status {other}")),
    }
}

#[derive(Debug, Serialize)]
struct ReleaseMetadata {
    source_sha: String,
    version: String,
    tag_name: String,
    notes: String,
}

pub(super) fn metadata_command(args: Vec<String>) -> Result<(), String> {
    let options = super::parse_options(&args, &["--source-sha", "--source-dir"])?;
    let sha = super::required_option(&options, "--source-sha")?;
    super::validate_commit_sha(sha)?;
    let root = Path::new(super::required_option(&options, "--source-dir")?);
    let head = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(root)
        .output()
        .map_err(|e| e.to_string())?;
    if !head.status.success() || String::from_utf8_lossy(&head.stdout).trim() != sha {
        return Err("release metadata checkout does not match the verified CI SHA".into());
    }
    let metadata = read_metadata(root, sha)?;
    println!(
        "{}",
        serde_json::to_string(&metadata).map_err(|e| e.to_string())?
    );
    Ok(())
}

fn read_metadata(root: &Path, sha: &str) -> Result<ReleaseMetadata, String> {
    super::ci::verify_checkout(root, sha)?;
    let candidate = super::ci::validate_release_candidate(root, None)?;
    let tag_name = format!("v{}", candidate.version);
    Ok(ReleaseMetadata {
        source_sha: sha.into(),
        version: candidate.version,
        tag_name,
        notes: candidate.notes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[cfg(unix)]
    fn fake_executable(path: &Path, body: &str) {
        use std::os::unix::fs::PermissionsExt;
        fs::write(path, body).unwrap();
        let mut permissions = fs::metadata(path).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(path, permissions).unwrap();
    }

    #[cfg(unix)]
    fn preflight_args(root: &Path, sha: &str, output: &Path) -> Vec<String> {
        [
            "--source-dir".to_string(),
            root.to_string_lossy().into_owned(),
            "--source-sha".to_string(),
            sha.to_string(),
            "--tag".to_string(),
            "v4.0.5".to_string(),
            "--output".to_string(),
            output.to_string_lossy().into_owned(),
        ]
        .to_vec()
    }

    #[cfg(unix)]
    #[test]
    fn package_preflight_uses_exact_command_cwd_archive_and_writes_bound_report() {
        let fixture = CandidateFixture::new("4.0.5");
        let commands = super::super::unique_temp_dir("cod110_fake_commands").unwrap();
        let log = commands.join("calls.log");
        let curl = commands.join("curl");
        let cargo = commands.join("cargo");
        fake_executable(
            &curl,
            &format!(
                "#!/bin/sh\nprintf 'curl %s\\n' \"$*\" >> {}\nprintf '\\n404'",
                log.display()
            ),
        );
        fake_executable(&cargo, &format!(
            "#!/bin/sh\nprintf 'cargo cwd=%s\\n' \"$PWD\" >> {}\nprintf 'cargo argv=%s\\n' \"$*\" >> {}\ntarget=\"\"\nwhile [ $# -gt 0 ]; do if [ \"$1\" = --target-dir ]; then shift; target=\"$1\"; fi; shift; done\nmkdir -p \"$target/package/tmp-crate\"\nprintf 'crate fixture' > \"$target/package/tmp-crate/codebase-graph-4.0.5.crate\"\n",
            log.display(),
            log.display()
        ));
        let report = commands.join("report.json");
        let args = preflight_args(&fixture.root, &fixture.sha, &report);
        let result = package_preflight_with(&args, &curl, &cargo);
        assert!(result.is_ok(), "{result:?}");
        let calls = fs::read_to_string(&log).unwrap();
        assert!(
            calls.contains("https://crates.io/api/v1/crates/codebase-graph/4.0.5"),
            "{calls}"
        );
        let cargo_call = calls
            .lines()
            .find(|line| line.starts_with("cargo cwd="))
            .expect("Cargo command was invoked");
        assert!(
            cargo_call.contains(fixture.root.to_str().unwrap()),
            "{cargo_call}"
        );
        let argv = calls
            .lines()
            .find(|line| line.starts_with("cargo argv="))
            .unwrap();
        assert!(
            argv.contains("publish --dry-run --locked --target-dir"),
            "{argv}"
        );
        assert!(argv.contains("/release_package_preflight_"), "{argv}");
        assert!(
            calls.contains("--connect-timeout 10 --max-time 30"),
            "{calls}"
        );
        assert!(calls.contains("codebaseGraph-release-workflow"), "{calls}");
        let target = argv.split("--target-dir ").nth(1).unwrap();
        let target = PathBuf::from(target);
        assert!(target.is_absolute(), "{target:?}");
        let owned_temp = target.parent().unwrap();
        assert!(!owned_temp.exists(), "owned Cargo target was not cleaned");
        let report: CratePreflightReport =
            serde_json::from_slice(&fs::read(&report).unwrap()).unwrap();
        assert_eq!(report.schema_version, 1);
        assert_eq!(report.source_sha, fixture.sha);
        assert_eq!(report.version, "4.0.5");
        assert_eq!(report.tag_name, "v4.0.5");
        assert_eq!(report.verification, "publish-dry-run");
        assert_eq!(report.crate_bytes, 13);
        let archive = commands.join("archive-copy.crate");
        fs::write(&archive, b"crate fixture").unwrap();
        assert_eq!(
            report.crate_sha256,
            super::super::sha256_file(&archive).unwrap()
        );
        fs::remove_dir_all(commands).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn package_preflight_accepts_exact_registry_size_limit() {
        let fixture = CandidateFixture::new("4.0.5");
        let commands = super::super::unique_temp_dir("cod110_preflight_size_boundary").unwrap();
        let curl = commands.join("curl");
        let cargo = commands.join("cargo");
        fake_executable(&curl, "#!/bin/sh\nprintf '\n404'");
        fake_executable(&cargo, "#!/bin/sh\ntarget=\"\"\nwhile [ $# -gt 0 ]; do if [ \"$1\" = --target-dir ]; then shift; target=\"$1\"; fi; shift; done\nmkdir -p \"$target/package/tmp-crate\"\nhead -c 10485760 /dev/zero > \"$target/package/tmp-crate/codebase-graph-4.0.5.crate\"\n");
        let report = commands.join("report.json");
        let result = package_preflight_with(
            &preflight_args(&fixture.root, &fixture.sha, &report),
            &curl,
            &cargo,
        );
        assert!(result.is_ok(), "{result:?}");
        let report: CratePreflightReport =
            serde_json::from_slice(&fs::read(&report).unwrap()).unwrap();
        assert_eq!(report.crate_bytes, 10_485_760);
        fs::remove_dir_all(commands).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn package_preflight_rejects_registry_errors_cargo_failure_missing_archive_and_oversize() {
        let cases = [
            ("curl", "#!/bin/sh\nprintf '\n503'", "registry"),
            ("curl", "#!/bin/sh\nprintf 'not-json\n200'", "malformed"),
            (
                "curl",
                r###"#!/bin/sh
printf '{"version":{"crate":"codebase-graph","num":"4.0.4"}}\n200'"###,
                "mismatched-200",
            ),
            ("curl", "#!/bin/sh\nexit 6", "curl-transport"),
            ("curl", "#!/bin/sh\nprintf '\n404'", "cargo-failure"),
            ("curl", "#!/bin/sh\nprintf '\n404'", "missing-archive"),
            ("curl", "#!/bin/sh\nprintf '\n404'", "wrong-final"),
            ("curl", "#!/bin/sh\nprintf '\n404'", "wrong-name"),
            ("curl", "#!/bin/sh\nprintf '\n404'", "oversize"),
            ("curl", "#!/bin/sh\nprintf '\n404'", "source-mutated"),
        ];
        for (_, curl_body, kind) in cases {
            let fixture = CandidateFixture::new("4.0.5");
            let commands = super::super::unique_temp_dir("cod110_preflight_failure").unwrap();
            let curl = commands.join("curl");
            let cargo = commands.join("cargo");
            fake_executable(&curl, curl_body);
            let cargo_body = match kind {
                "cargo-failure" => "#!/bin/sh\necho fake-cargo-failed >&2\nexit 7\n".to_string(),
                "missing-archive" => "#!/bin/sh\ntarget=\"\"\nwhile [ $# -gt 0 ]; do if [ \"$1\" = --target-dir ]; then shift; target=\"$1\"; fi; shift; done\nmkdir -p \"$target/package/tmp-crate\"\n".to_string(),
                "wrong-final" | "wrong-name" => {
                    let relative = if kind == "wrong-final" {
                        "package/codebase-graph-4.0.5.crate"
                    } else {
                        "package/tmp-crate/another-4.0.5.crate"
                    };
                    format!("#!/bin/sh\ntarget=\"\"\nwhile [ $# -gt 0 ]; do if [ \"$1\" = --target-dir ]; then shift; target=\"$1\"; fi; shift; done\nmkdir -p \"$target/package/tmp-crate\"\nprintf 'unrelated archive' > \"$target/{relative}\"\necho successful-cargo-output\necho successful-cargo-diagnostic >&2\n")
                }
                "oversize" => "#!/bin/sh\ntarget=\"\"\nwhile [ $# -gt 0 ]; do if [ \"$1\" = --target-dir ]; then shift; target=\"$1\"; fi; shift; done\nmkdir -p \"$target/package/tmp-crate\"\nhead -c 10485761 /dev/zero > \"$target/package/tmp-crate/codebase-graph-4.0.5.crate\"\n".to_string(),
                "source-mutated" => format!("#!/bin/sh\ntarget=\"\"\nwhile [ $# -gt 0 ]; do if [ \"$1\" = --target-dir ]; then shift; target=\"$1\"; fi; shift; done\nmkdir -p \"$target/package/tmp-crate\"\nprintf 'crate fixture' > \"$target/package/tmp-crate/codebase-graph-4.0.5.crate\"\nprintf 'tampered\n' >> \"{}\"\n", fixture.root.join("Cargo.toml").display()),
                _ => "#!/bin/sh\nexit 7\n".to_string(),
            };
            fake_executable(&cargo, &cargo_body);
            let report = commands.join("report.json");
            let args = preflight_args(&fixture.root, &fixture.sha, &report);
            let result = package_preflight_with(&args, &curl, &cargo);
            assert!(result.is_err(), "{kind}: {result:?}");
            if matches!(kind, "wrong-final" | "wrong-name") {
                let error = result.unwrap_err();
                assert!(error.contains("successful-cargo-output"), "{error}");
                assert!(error.contains("successful-cargo-diagnostic"), "{error}");
            }
            assert!(!report.exists(), "{kind}");
            fs::remove_dir_all(commands).unwrap();
        }
    }

    #[cfg(unix)]
    #[test]
    fn package_preflight_rejects_report_inside_selected_source_before_external_commands() {
        let fixture = CandidateFixture::new("4.0.5");
        let commands = super::super::unique_temp_dir("cod110_preflight_outside").unwrap();
        let curl = commands.join("curl");
        let cargo = commands.join("cargo");
        fake_executable(&curl, "#!/bin/sh\nexit 99\n");
        fake_executable(&cargo, "#!/bin/sh\nexit 99\n");
        let report = fixture.root.join("preflight.json");
        let args = preflight_args(&fixture.root, &fixture.sha, &report);
        assert!(package_preflight_with(&args, &curl, &cargo)
            .unwrap_err()
            .contains("outside selected source"));
        assert!(!report.exists());
        let existing_report = commands.join("existing.json");
        fs::write(&existing_report, b"existing").unwrap();
        let args = preflight_args(&fixture.root, &fixture.sha, &existing_report);
        assert!(package_preflight_with(&args, &curl, &cargo)
            .unwrap_err()
            .contains("must not already exist"));
        assert_eq!(fs::read(existing_report).unwrap(), b"existing");
        fs::remove_dir_all(commands).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn package_preflight_present_version_uses_verified_cargo_package() {
        let fixture = CandidateFixture::new("4.0.5");
        let commands = super::super::unique_temp_dir("cod110_preflight_present").unwrap();
        let log = commands.join("calls.log");
        let curl = commands.join("curl");
        let cargo = commands.join("cargo");
        fake_executable(&curl, "#!/bin/sh\nprintf '{\"version\":{\"crate\":\"codebase-graph\",\"num\":\"4.0.5\"}}\n200'");
        fake_executable(&cargo, &format!("#!/bin/sh\nprintf '%s\\n' \"$*\" >> {}\ntarget=\"\"\nwhile [ $# -gt 0 ]; do if [ \"$1\" = --target-dir ]; then shift; target=\"$1\"; fi; shift; done\nmkdir -p \"$target/package\"\nprintf 'crate fixture' > \"$target/package/codebase-graph-4.0.5.crate\"\n",log.display()));
        let report = commands.join("report.json");
        let result = package_preflight_with(
            &preflight_args(&fixture.root, &fixture.sha, &report),
            &curl,
            &cargo,
        );
        assert!(result.is_ok(), "{result:?}");
        let calls = fs::read_to_string(&log).unwrap();
        assert!(calls.contains("package --locked"), "{calls}");
        assert!(!calls.contains("--no-verify"), "{calls}");
        let report: CratePreflightReport =
            serde_json::from_slice(&fs::read(&report).unwrap()).unwrap();
        assert_eq!(report.verification, "package-already-published");
        fs::remove_dir_all(commands).unwrap();
    }

    #[test]
    fn notes_only_include_the_exact_release() {
        let text = "# Changelog\n## [2.1.0](url)\nnew\n## [2.0.0](url) (date)\n\n### Fixes\nverified\n## 1.0.0\nold\n";
        let notes = super::super::ci::release_notes(text, "2.0.0").unwrap();
        assert!(notes.contains("verified"));
        assert!(!notes.contains("new"));
        assert!(!notes.contains("old"));
        assert!(super::super::ci::release_notes(text, "2.0").is_err());
    }

    #[test]
    fn notes_reject_missing_duplicate_and_empty_sections() {
        for text in [
            "## 2.0.1\nother",
            "## 2.0.0\none\n## 2.0.0\ntwo",
            "## 2.0.0\n\n",
        ] {
            assert!(
                super::super::ci::release_notes(text, "2.0.0").is_err(),
                "{text}"
            );
        }
    }

    struct CandidateFixture {
        root: PathBuf,
        sha: String,
    }

    impl CandidateFixture {
        fn new(version: &str) -> Self {
            let root = super::super::unique_temp_dir("cod110_candidate").unwrap();
            let wiki = root.join("crates/k-wiki");
            let xtask = root.join("crates/xtask");
            fs::create_dir_all(&wiki).unwrap();
            fs::create_dir_all(&xtask).unwrap();
            let files = [
                (".release-please-manifest.json", format!("{{\n  \".\": \"{version}\"\n}}\n")),
                ("Cargo.toml", format!("[package]\nname = \"codebase-graph\"\nversion = \"{version}\"\nedition = \"2021\"\n\n[workspace]\nmembers = [\".\", \"crates/k-wiki\", \"crates/xtask\"]\n")),
                ("crates/k-wiki/Cargo.toml", format!("[package]\nname = \"k-wiki\"\nversion = \"{version}\"\nedition = \"2021\"\n\n[dependencies]\ncodebase-graph = {{ version = \"{version}\", path = \"../..\" }}\n")),
                ("crates/xtask/Cargo.toml", format!("[package]\nname = \"xtask\"\nversion = \"{version}\"\nedition = \"2021\"\npublish = false\n")),
                ("Cargo.lock", format!("# lock\nversion = 4\n\n[[package]]\nname = \"codebase-graph\"\nversion = \"{version}\"\ndependencies = [\"k-wiki\"]\n\n[[package]]\nname = \"k-wiki\"\nversion = \"{version}\"\ndependencies = [\"codebase-graph\"]\n\n[[package]]\nname = \"xtask\"\nversion = \"{version}\"\ndependencies = [\"serde\"]\n")),
                ("CHANGELOG.md", format!("# Changelog\n\n## [{version}](https://github.com/rabii-chaarani/codebaseGraph/compare/v1.0.0...v{version}) (2026-11-01)\n\nVerified release notes.\n")),
            ];
            for (relative, contents) in files {
                let path = root.join(relative);
                if let Some(parent) = path.parent() {
                    fs::create_dir_all(parent).unwrap();
                }
                fs::write(path, contents).unwrap();
            }
            for args in [
                vec!["init", "--quiet", "-b", "main"],
                vec!["add", "--all"],
                vec![
                    "-c",
                    "user.name=COD-110 tests",
                    "-c",
                    "user.email=cod110@example.invalid",
                    "commit",
                    "--quiet",
                    "-m",
                    "candidate",
                ],
            ] {
                let output = Command::new("git")
                    .arg("-C")
                    .arg(&root)
                    .args(args)
                    .output()
                    .unwrap();
                assert!(
                    output.status.success(),
                    "{}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }
            let output = Command::new("git")
                .args(["-C", root.to_str().unwrap(), "rev-parse", "HEAD"])
                .output()
                .unwrap();
            assert!(output.status.success());
            Self {
                root,
                sha: String::from_utf8(output.stdout).unwrap().trim().to_string(),
            }
        }
    }

    impl Drop for CandidateFixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn candidate_metadata_accepts_current_real_generated_six_file_layout() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let candidate = super::super::ci::validate_release_candidate(&root, None).unwrap();
        assert_eq!(
            candidate.version,
            super::super::cargo_version(&root.join("Cargo.toml")).unwrap()
        );
        assert!(!candidate.notes.trim().is_empty());
        assert_eq!(
            super::super::ci::validate_release_candidate(&root, Some(&candidate.version)).unwrap(),
            candidate
        );
    }

    #[test]
    fn candidate_metadata_accepts_crlf_and_mixed_six_file_layouts_without_writing() {
        fn with_endings(bytes: &[u8], mixed: bool) -> Vec<u8> {
            let mut output = Vec::with_capacity(bytes.len() * 2);
            let mut line = 0;
            for byte in bytes {
                if *byte == b'\n' {
                    if !mixed || line % 2 == 0 {
                        output.push(b'\r');
                    }
                    output.push(*byte);
                    line += 1;
                } else {
                    output.push(*byte);
                }
            }
            output
        }

        let fixture = CandidateFixture::new("4.0.5");
        let paths = [
            ".release-please-manifest.json",
            "Cargo.toml",
            "Cargo.lock",
            "CHANGELOG.md",
            "crates/k-wiki/Cargo.toml",
            "crates/xtask/Cargo.toml",
        ];
        let originals: Vec<(&str, Vec<u8>)> = paths
            .into_iter()
            .map(|path| (path, fs::read(fixture.root.join(path)).unwrap()))
            .collect();
        let expected = super::super::ci::validate_release_candidate(&fixture.root, None).unwrap();

        for mixed in [false, true] {
            for (path, bytes) in &originals {
                fs::write(fixture.root.join(*path), with_endings(bytes, mixed)).unwrap();
            }
            let before: Vec<Vec<u8>> = originals
                .iter()
                .map(|(path, _)| fs::read(fixture.root.join(*path)).unwrap())
                .collect();
            let candidate =
                super::super::ci::validate_release_candidate(&fixture.root, Some("4.0.5")).unwrap();
            assert_eq!(candidate, expected);
            let after: Vec<Vec<u8>> = originals
                .iter()
                .map(|(path, _)| fs::read(fixture.root.join(*path)).unwrap())
                .collect();
            assert_eq!(after, before);
        }
    }

    #[test]
    fn candidate_metadata_keeps_rejecting_bare_cr_ambiguous_and_mismatched_inputs() {
        let fixture = CandidateFixture::new("4.0.5");
        let cargo = fixture.root.join("Cargo.toml");
        let contents = fs::read_to_string(&cargo).unwrap();
        fs::write(
            &cargo,
            contents.replacen("edition = \"2021\"", "edition = \"2021\"\r # bare CR", 1),
        )
        .unwrap();
        assert!(super::super::ci::validate_release_candidate(&fixture.root, None).is_err());

        let fixture = CandidateFixture::new("4.0.5");
        let cargo = fixture.root.join("Cargo.toml");
        let contents = fs::read_to_string(&cargo).unwrap();
        fs::write(
            &cargo,
            contents.replacen(
                "version = \"4.0.5\"\n",
                "version = \"4.0.5\"\nversion = \"4.0.5\"\n",
                1,
            ),
        )
        .unwrap();
        assert!(super::super::ci::validate_release_candidate(&fixture.root, None).is_err());

        let fixture = CandidateFixture::new("4.0.5");
        fs::write(
            fixture.root.join(".release-please-manifest.json"),
            b"{\n  \".\": \"4.0.5\",\n  \".\": \"4.0.5\"\n}\n",
        )
        .unwrap();
        assert!(super::super::ci::validate_release_candidate(&fixture.root, None).is_err());

        let fixture = CandidateFixture::new("4.0.5");
        fs::write(
            fixture.root.join(".release-please-manifest.json"),
            b"{\n  \".\": \"4.0.6\"\n}\n",
        )
        .unwrap();
        assert!(super::super::ci::validate_release_candidate(&fixture.root, None).is_err());
    }

    #[test]
    fn candidate_metadata_requires_complete_consistent_generated_contract() {
        let fixture = CandidateFixture::new("4.0.5");
        let candidate = super::super::ci::validate_release_candidate(&fixture.root, None).unwrap();
        assert_eq!(candidate.version, "4.0.5");
        assert!(candidate.notes.contains("Verified release notes"));
        assert!(super::super::ci::validate_release_candidate(&fixture.root, Some("4.0.5")).is_ok());
        assert!(
            super::super::ci::validate_release_candidate(&fixture.root, Some("4.0.6")).is_err()
        );
    }

    #[test]
    fn candidate_metadata_rejects_leading_zero_in_all_candidate_entrypoints() {
        let fixture = CandidateFixture::new("01.2.3");
        assert!(super::super::ci::validate_release_candidate(&fixture.root, None).is_err());
        assert!(
            super::super::ci::validate_release_candidate(&fixture.root, Some("01.2.3")).is_err()
        );
        assert!(super::super::ci::validate_release_tag("v01.2.3").is_err());
        assert!(super::super::verify_release_version("v01.2.3").is_err());
    }

    #[test]
    fn candidate_metadata_does_not_require_metadata_transition_or_changelog_history_rewrite() {
        let fixture = CandidateFixture::new("4.0.5");
        assert!(super::super::ci::validate_release_candidate(&fixture.root, Some("4.0.5")).is_ok());
    }

    #[test]
    fn candidate_metadata_rejects_missing_or_mismatched_generated_identities() {
        for (relative, contents) in [
            ("crates/xtask/Cargo.toml", "[package]\nname = \"xtask\"\nversion = \"4.0.6\"\nedition = \"2021\"\n"),
            ("crates/k-wiki/Cargo.toml", "[package]\nname = \"k-wiki\"\nversion = \"4.0.5\"\nedition = \"2021\"\n\n[dependencies]\ncodebase-graph = { version = \"4.0.6\", path = \"../..\" }\n"),
            ("Cargo.lock", "# lock\nversion = 4\n"),
            ("CHANGELOG.md", "# Changelog\n\n## [4.0.6](https://github.com/rabii-chaarani/codebaseGraph/compare/v1.0.0...v4.0.6) (2026-11-01)\n\nOnly a different version.\n"),
        ] {
            let fixture=CandidateFixture::new("4.0.5");
            fs::write(fixture.root.join(relative),contents).unwrap();
            assert!(super::super::ci::validate_release_candidate(&fixture.root,None).is_err(),"{relative}");
        }
    }

    #[test]
    fn candidate_metadata_rejects_duplicate_lock_identity_and_missing_changelog_version() {
        let fixture = CandidateFixture::new("4.0.5");
        let lock_path = fixture.root.join("Cargo.lock");
        let lock = fs::read_to_string(&lock_path).unwrap();
        let duplicate = format!("{lock}\n[[package]]\nname = \"xtask\"\nversion = \"4.0.5\"\n");
        fs::write(&lock_path, duplicate).unwrap();
        assert!(super::super::ci::validate_release_candidate(&fixture.root, None).is_err());

        let fixture = CandidateFixture::new("4.0.5");
        fs::write(fixture.root.join("CHANGELOG.md"), "# Changelog\n\n## [4.0.4](https://github.com/rabii-chaarani/codebaseGraph/compare/v1.0.0...v4.0.4) (2026-11-01)\n\nold\n").unwrap();
        assert!(super::super::ci::validate_release_candidate(&fixture.root, None).is_err());
    }

    #[test]
    fn candidate_metadata_rejects_missing_duplicate_empty_and_mismatched_inputs() {
        for (relative, contents) in [
            (".release-please-manifest.json", "{}\n"),
            (
                "Cargo.toml",
                "[package]\nname = \"codebase-graph\"\nedition = \"2021\"\n",
            ),
            (
                "crates/k-wiki/Cargo.toml",
                "[package]\nname = \"k-wiki\"\nversion = \"4.0.5\"\nedition = \"2021\"\n",
            ),
            (
                "crates/xtask/Cargo.toml",
                "[package]\nname = \"wrong\"\nversion = \"4.0.5\"\nedition = \"2021\"\n",
            ),
            ("CHANGELOG.md", "# Changelog\n\n"),
        ] {
            let fixture = CandidateFixture::new("4.0.5");
            fs::write(fixture.root.join(relative), contents).unwrap();
            assert!(
                super::super::ci::validate_release_candidate(&fixture.root, None).is_err(),
                "{relative}"
            );
        }

        let fixture = CandidateFixture::new("4.0.5");
        let manifest = fixture.root.join(".release-please-manifest.json");
        fs::write(manifest, "{\n  \".\": \"4.0.5\",\n  \".\": \"4.0.5\"\n}\n").unwrap();
        assert!(super::super::ci::validate_release_candidate(&fixture.root, None).is_err());

        let fixture = CandidateFixture::new("4.0.5");
        let wiki = fixture.root.join("crates/k-wiki/Cargo.toml");
        let mut contents = fs::read_to_string(&wiki).unwrap();
        contents.push_str("\n[package]\nname = \"k-wiki\"\nversion = \"4.0.5\"\n");
        fs::write(wiki, contents).unwrap();
        assert!(super::super::ci::validate_release_candidate(&fixture.root, None).is_err());
    }

    #[test]
    fn candidate_metadata_requires_checksumless_source_less_workspace_lock_records() {
        for field in [
            "source = \"registry+https://example.invalid\"\n",
            "checksum = \"aaaaaaaa\"\n",
        ] {
            let fixture = CandidateFixture::new("4.0.5");
            let path = fixture.root.join("Cargo.lock");
            let contents = fs::read_to_string(&path).unwrap();
            let changed = contents.replacen(
                "version = \"4.0.5\"\n",
                &format!("version = \"4.0.5\"\n{field}"),
                1,
            );
            fs::write(path, changed).unwrap();
            assert!(super::super::ci::validate_release_candidate(&fixture.root, None).is_err());
        }
    }

    #[test]
    fn metadata_rejects_inconsistent_versions() {
        let fixture = CandidateFixture::new("4.0.5");
        let metadata = super::read_metadata(&fixture.root, &fixture.sha).unwrap();
        assert_eq!(metadata.version, "4.0.5");
        assert_eq!(metadata.tag_name, "v4.0.5");
        assert_eq!(metadata.source_sha, fixture.sha);
        assert!(metadata.notes.contains("Verified release notes"));
    }
}
