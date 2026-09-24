//! Immutable release metadata. GitHub authorization and mutations stay in the workflow.
use serde::Serialize;
use serde_json::Value;
use std::{fs, path::Path, process::Command};

#[cfg(all(test, unix))]
mod workflow_tests;

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
    let version = super::cargo_version(&root.join("Cargo.toml"))?;
    let tag = format!("v{version}");
    super::release_version_from_tag(&tag)?;
    let wiki_path = root.join("crates/k-wiki/Cargo.toml");
    let wiki = super::cargo_version(&wiki_path)?;
    let dependency = super::dependency_version(&wiki_path, "codebase-graph")?;
    let manifest: Value = serde_json::from_str(
        &fs::read_to_string(root.join(".release-please-manifest.json"))
            .map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    if wiki != version || dependency != version || manifest["."].as_str() != Some(&version) {
        return Err("root, wiki, dependency, and release manifest versions must agree".into());
    }
    let changelog = fs::read_to_string(root.join("CHANGELOG.md")).map_err(|e| e.to_string())?;
    Ok(ReleaseMetadata {
        source_sha: sha.into(),
        notes: release_notes(&changelog, &version)?,
        tag_name: tag,
        version,
    })
}

fn release_notes(changelog: &str, version: &str) -> Result<String, String> {
    let linked = format!("## [{version}](");
    let plain = format!("## {version}");
    let mut matches = 0;
    let mut collecting = false;
    let mut notes = Vec::new();
    for line in changelog.lines() {
        if line.starts_with("## ") {
            let selected = line.starts_with(&linked)
                || line == plain
                || line.starts_with(&format!("{plain} "));
            collecting = selected;
            matches += usize::from(selected);
        }
        if collecting {
            notes.push(line);
        }
    }
    if matches != 1 || notes.iter().skip(1).all(|line| line.trim().is_empty()) {
        return Err(format!(
            "expected one nonempty changelog section for {version}"
        ));
    }
    Ok(notes.join("\n").trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notes_only_include_the_exact_release() {
        let text = "# Changelog\n## [2.1.0](url)\nnew\n## [2.0.0](url) (date)\n\n### Fixes\nverified\n## 1.0.0\nold\n";
        let notes = release_notes(text, "2.0.0").unwrap();
        assert!(notes.contains("verified"));
        assert!(!notes.contains("new"));
        assert!(!notes.contains("old"));
        assert!(release_notes(text, "2.0").is_err());
    }

    #[test]
    fn notes_reject_missing_duplicate_and_empty_sections() {
        for text in [
            "## 2.0.1\nother",
            "## 2.0.0\none\n## 2.0.0\ntwo",
            "## 2.0.0\n\n",
        ] {
            assert!(release_notes(text, "2.0.0").is_err(), "{text}");
        }
    }

    #[test]
    fn metadata_rejects_inconsistent_versions() {
        let root = super::super::unique_temp_dir("release_metadata").unwrap();
        fs::create_dir_all(root.join("crates/k-wiki")).unwrap();
        fs::write(root.join("Cargo.toml"), "version = \"2.0.0\"\n").unwrap();
        fs::write(
            root.join("crates/k-wiki/Cargo.toml"),
            "version = \"2.0.0\"\ncodebase-graph = { version = \"2.0.0\" }\n",
        )
        .unwrap();
        fs::write(
            root.join(".release-please-manifest.json"),
            r#"{".":"2.0.1"}"#,
        )
        .unwrap();
        assert!(read_metadata(&root, &"a".repeat(40))
            .unwrap_err()
            .contains("versions must agree"));
        fs::write(
            root.join(".release-please-manifest.json"),
            r#"{".":"2.0.0"}"#,
        )
        .unwrap();
        fs::write(root.join("CHANGELOG.md"), "## 2.0.0\nverified\n").unwrap();
        assert_eq!(
            read_metadata(&root, &"a".repeat(40)).unwrap().tag_name,
            "v2.0.0"
        );
        fs::remove_dir_all(root).unwrap();
    }
}
