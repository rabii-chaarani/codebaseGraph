#![cfg(unix)]

use super::{
    expected_package_files, set_executable, sha256_file, unique_temp_dir, write_internal_checksums,
    NativeTarget,
};
use std::collections::BTreeSet;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn installer_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../release/install/install.sh")
}

fn fixture() -> (PathBuf, PathBuf, PathBuf) {
    let root = unique_temp_dir("macos_installer")
        .unwrap()
        .join("fixture with spaces");
    let source = root.join("release payload");
    let destination = root.join("installed files");
    fs::create_dir_all(&source).unwrap();

    let (graph, wiki) = NativeTarget::MacosArm64.binary_names();
    for name in [graph, wiki] {
        let binary = source.join(name);
        fs::write(&binary, b"#!/bin/sh\nexit 0\n").unwrap();
        set_executable(&binary).unwrap();
    }
    for name in super::macos::PAYLOAD_FILES {
        fs::write(source.join(name), format!("fixture for {name}\n")).unwrap();
    }
    write_internal_checksums(&source, NativeTarget::MacosArm64).unwrap();
    (root, source, destination)
}

fn test_path(root: &Path) -> std::ffi::OsString {
    let bin = root.join("fake host tools");
    fs::create_dir_all(&bin).unwrap();
    let uname = bin.join("uname");
    fs::write(
        &uname,
        "#!/bin/sh\ncase \"$1\" in -s) echo Darwin ;; -m) echo arm64 ;; esac\n",
    )
    .unwrap();
    set_executable(&uname).unwrap();
    env::join_paths(std::iter::once(bin).chain(env::split_paths(&env::var_os("PATH").unwrap())))
        .unwrap()
}

fn invoke(root: &Path, source: &Path, destination: &Path, dry_run: bool) -> Output {
    let mut command = Command::new("bash");
    command
        .arg(installer_path())
        .arg("--source-dir")
        .arg(source)
        .arg("--bin-dir")
        .arg(destination)
        .env("PATH", test_path(root));
    if dry_run {
        command.arg("--dry-run");
    }
    command.output().unwrap()
}

fn installed_snapshot(destination: &Path) -> BTreeSet<(String, String)> {
    fs::read_dir(destination)
        .unwrap()
        .map(|entry| {
            let path = entry.unwrap().path();
            (
                path.file_name().unwrap().to_string_lossy().into_owned(),
                sha256_file(&path).unwrap(),
            )
        })
        .collect()
}

fn preserve_existing_files(destination: &Path) -> BTreeSet<(String, String)> {
    fs::create_dir_all(destination).unwrap();
    let (graph, wiki) = NativeTarget::MacosArm64.binary_names();
    fs::write(destination.join(graph), b"previous graph binary").unwrap();
    fs::write(destination.join(wiki), b"previous wiki binary").unwrap();
    for name in NativeTarget::MacosArm64.payload_files() {
        if name != graph && name != wiki {
            fs::write(destination.join(name), format!("previous {name}")).unwrap();
        }
    }
    fs::write(
        destination.join("existing sidecar"),
        b"previous unrelated sidecar",
    )
    .unwrap();
    installed_snapshot(destination)
}

fn run_fails_before_destination_changes(root: &Path, source: &Path, destination: &Path) {
    let before = preserve_existing_files(destination);
    let output = invoke(root, source, destination, false);
    assert!(
        !output.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(installed_snapshot(destination), before);
}

fn versioned_name(name: &str, replacement: &str) -> String {
    let version = name
        .strip_prefix("libcodebasegraph-ssl-")
        .and_then(|name| name.strip_suffix(".dylib"))
        .expect("SSL fixture filename has the canonical shape");
    name.replacen(version, replacement, 1)
}

#[test]
fn dry_run_validates_all_payloads_without_creating_destination() {
    let (root, source, destination) = fixture();
    let output = invoke(&root, &source, &destination, true);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!destination.exists());
    fs::remove_dir_all(root.parent().unwrap()).unwrap();
}

#[cfg(unix)]
#[test]
fn real_install_places_verified_runtime_license_and_binaries_in_spaced_directory() {
    use std::os::unix::fs::PermissionsExt;

    let (root, source, destination) = fixture();
    let output = invoke(&root, &source, &destination, false);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let expected: BTreeSet<String> = NativeTarget::MacosArm64
        .payload_files()
        .into_iter()
        .map(str::to_string)
        .collect();
    let actual: BTreeSet<String> = fs::read_dir(&destination)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(actual, expected);
    for name in NativeTarget::MacosArm64.payload_files() {
        assert_eq!(
            sha256_file(&source.join(name)).unwrap(),
            sha256_file(&destination.join(name)).unwrap()
        );
        let mode = fs::metadata(destination.join(name))
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        let expected_mode = if NativeTarget::MacosArm64.binary_names().0 == name
            || NativeTarget::MacosArm64.binary_names().1 == name
        {
            0o755
        } else {
            0o644
        };
        assert_eq!(mode, expected_mode, "mode for {name}");
    }
    assert_eq!(expected_package_files(NativeTarget::MacosArm64).len(), 8);
    fs::remove_dir_all(root.parent().unwrap()).unwrap();
}

#[cfg(unix)]
#[test]
fn missing_corrupt_and_symlink_payloads_preserve_existing_destination_files() {
    use std::os::unix::fs::symlink;

    let (root, source, destination) = fixture();
    for name in NativeTarget::MacosArm64.payload_files() {
        let selected_path = source.join(name);
        let saved = fs::read(&selected_path).unwrap();

        fs::remove_file(&selected_path).unwrap();
        run_fails_before_destination_changes(&root, &source, &destination);
        fs::write(&selected_path, &saved).unwrap();

        fs::write(&selected_path, b"corrupted payload bytes").unwrap();
        run_fails_before_destination_changes(&root, &source, &destination);
        fs::write(&selected_path, &saved).unwrap();
    }

    let selected_path = source.join(super::macos::PAYLOAD_FILES[0]);
    let saved = fs::read(&selected_path).unwrap();
    fs::remove_file(&selected_path).unwrap();
    let external = root.join("outside runtime");
    fs::write(&external, &saved).unwrap();
    symlink(&external, &selected_path).unwrap();
    run_fails_before_destination_changes(&root, &source, &destination);

    fs::remove_dir_all(root.parent().unwrap()).unwrap();
}

#[cfg(unix)]
#[test]
fn invalid_inventory_rows_and_mixed_versions_preserve_destination() {
    let (root, source, destination) = fixture();
    let checksum_path = source.join("checksums.txt");
    let valid = fs::read_to_string(&checksum_path).unwrap();
    let rows: Vec<&str> = valid.lines().collect();
    let duplicate = format!("{}\n{}\n", rows[0], rows[0]);
    let malformed = format!(
        "not-a-digest  {}\n",
        NativeTarget::MacosArm64.binary_names().0
    );
    let traversal = format!("{}  ../payload\n", "0".repeat(64));
    let unknown = format!("{}\n{}", valid.trim_end(), traversal);
    let missing = rows[1..].join("\n");

    for manifest in [duplicate, malformed, unknown, missing] {
        fs::write(&checksum_path, manifest).unwrap();
        run_fails_before_destination_changes(&root, &source, &destination);
    }

    let original_name = super::macos::PAYLOAD_FILES[0];
    let alternate_name = versioned_name(original_name, "9.8.7");
    fs::rename(source.join(original_name), source.join(&alternate_name)).unwrap();
    let mut mixed_rows = vec![format!(
        "{}  {alternate_name}",
        sha256_file(&source.join(&alternate_name)).unwrap()
    )];
    for name in NativeTarget::MacosArm64
        .payload_files()
        .into_iter()
        .filter(|name| *name != original_name)
    {
        mixed_rows.push(format!(
            "{}  {name}",
            sha256_file(&source.join(name)).unwrap()
        ));
    }
    fs::write(&checksum_path, format!("{}\n", mixed_rows.join("\n"))).unwrap();
    run_fails_before_destination_changes(&root, &source, &destination);

    fs::remove_dir_all(root.parent().unwrap()).unwrap();
}

#[cfg(unix)]
#[test]
fn nonzero_binary_probe_preserves_existing_destination_files() {
    let (root, source, destination) = fixture();
    let graph = source.join(NativeTarget::MacosArm64.binary_names().0);
    fs::write(&graph, b"#!/bin/sh\nexit 17\n").unwrap();
    set_executable(&graph).unwrap();
    write_internal_checksums(&source, NativeTarget::MacosArm64).unwrap();
    run_fails_before_destination_changes(&root, &source, &destination);
    fs::remove_dir_all(root.parent().unwrap()).unwrap();
}
