use flate2::read::GzDecoder;
use flate2::{Compression, GzBuilder};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use yaml_serde::Value as YamlValue;

const CONFIRMATIONS: &[&str] = &["release-environment", "private-vulnerability-reporting"];
const NATIVE_TARGETS: [NativeTarget; 4] = [
    NativeTarget::LinuxX86_64,
    NativeTarget::MacosArm64,
    NativeTarget::MacosX86_64,
    NativeTarget::WindowsX86_64,
];

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum NativeTarget {
    LinuxX86_64,
    MacosArm64,
    MacosX86_64,
    WindowsX86_64,
}

impl NativeTarget {
    fn parse(value: &str) -> Result<Self, String> {
        match value {
            "linux-x86_64" => Ok(Self::LinuxX86_64),
            "macos-arm64" => Ok(Self::MacosArm64),
            "macos-x86_64" => Ok(Self::MacosX86_64),
            "windows-x86_64" => Ok(Self::WindowsX86_64),
            other => Err(format!("unsupported native target: {other}")),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::LinuxX86_64 => "linux-x86_64",
            Self::MacosArm64 => "macos-arm64",
            Self::MacosX86_64 => "macos-x86_64",
            Self::WindowsX86_64 => "windows-x86_64",
        }
    }

    fn binary_names(self) -> (&'static str, &'static str) {
        match self {
            Self::WindowsX86_64 => ("codebase-graph.exe", "k-wiki.exe"),
            _ => ("codebase-graph", "k-wiki"),
        }
    }

    fn host(self) -> (&'static str, &'static str) {
        match self {
            Self::LinuxX86_64 => ("linux", "x86_64"),
            Self::MacosArm64 => ("macos", "aarch64"),
            Self::MacosX86_64 => ("macos", "x86_64"),
            Self::WindowsX86_64 => ("windows", "x86_64"),
        }
    }

    fn archive_name(self, version: &str) -> String {
        format!("codebase-graph-{version}-{}.tar.gz", self.as_str())
    }

    fn uses_windows_extensions(self) -> bool {
        self == Self::WindowsX86_64
    }
}

#[derive(Debug, Deserialize, Serialize)]
struct NativeProvenance {
    schema_version: u32,
    commit_sha: String,
    version: String,
    target: String,
    archive: String,
    archive_sha256: String,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let mut args = env::args().skip(1);
    match args.next().as_deref() {
        Some("release-gate") => release_gate(args.collect()),
        Some("check-workflows") => check_workflows_command(),
        Some("native-test") => native_test(args.collect()),
        Some("native-artifact") => native_artifact(args.collect()),
        Some("validate-native-artifacts") => validate_native_artifacts(args.collect()),
        Some("smoke-artifact") => {
            let executable = args
                .next()
                .ok_or_else(|| "smoke-artifact requires a binary path".to_string())?;
            smoke_artifact(Path::new(&executable))
        }
        Some("smoke-wiki-artifact") => {
            let executable = args
                .next()
                .ok_or_else(|| "smoke-wiki-artifact requires a binary path".to_string())?;
            smoke_wiki_artifact(Path::new(&executable))
        }
        Some("verify-release-version") => {
            let tag = args
                .next()
                .ok_or_else(|| "verify-release-version requires a vX.Y.Z tag".to_string())?;
            verify_release_version(&tag)
        }
        Some(command) => Err(format!("unknown xtask command: {command}")),
        None => Err(
            "usage: cargo run -p xtask -- <release-gate|check-workflows|native-test|native-artifact|validate-native-artifacts|smoke-artifact|smoke-wiki-artifact|verify-release-version>"
                .to_string(),
        ),
    }
}

fn native_test(args: Vec<String>) -> Result<(), String> {
    let options = parse_options(&args, &["--target"])?;
    let target = NativeTarget::parse(required_option(&options, "--target")?)?;
    ensure_native_host(target)?;
    let command_args = native_test_command(target);
    let mut command = Command::new("cargo");
    command.args(command_args);
    run_command(&mut command, "native test")
}

fn native_test_command(target: NativeTarget) -> Vec<&'static str> {
    let mut args = vec!["test", "--workspace", "--locked"];
    if target.uses_windows_extensions() {
        args.extend([
            "--release",
            "--features",
            "codebase-graph/bundled-windows-extensions,k-wiki/bundled-windows-extensions",
        ]);
    }
    args
}

fn native_artifact(args: Vec<String>) -> Result<(), String> {
    let options = parse_options(&args, &["--target", "--commit-sha", "--output"])?;
    let target = NativeTarget::parse(required_option(&options, "--target")?)?;
    let commit_sha = required_option(&options, "--commit-sha")?;
    validate_commit_sha(commit_sha)?;
    let output = Path::new(required_option(&options, "--output")?);
    ensure_native_host(target)?;

    let version = release_version()?;
    build_native_binaries(target)?;
    fs::create_dir_all(output).map_err(|error| {
        format!(
            "failed to create artifact output {}: {error}",
            output.display()
        )
    })?;

    let staging = unique_temp_dir(&format!("native_artifact_{}", target.as_str()))?;
    let result = build_and_smoke_native_archive(target, &version, commit_sha, output, &staging);
    let _ = fs::remove_dir_all(&staging);
    result
}

fn validate_native_artifacts(args: Vec<String>) -> Result<(), String> {
    let options = parse_options(&args, &["--input", "--tag", "--commit-sha"])?;
    let input = Path::new(required_option(&options, "--input")?);
    let tag = required_option(&options, "--tag")?;
    let commit_sha = required_option(&options, "--commit-sha")?;
    validate_commit_sha(commit_sha)?;
    let version = release_version_from_tag(tag)?;
    validate_native_artifact_set(input, &version, commit_sha)
}

fn parse_options(args: &[String], allowed: &[&str]) -> Result<BTreeMap<String, String>, String> {
    let allowed: BTreeSet<&str> = allowed.iter().copied().collect();
    let mut options = BTreeMap::new();
    let mut index = 0;
    while index < args.len() {
        let flag = args[index].as_str();
        if !allowed.contains(flag) {
            return Err(format!("unknown option: {flag}"));
        }
        index += 1;
        let value = args
            .get(index)
            .ok_or_else(|| format!("{flag} requires a value"))?;
        if options.insert(flag.to_string(), value.clone()).is_some() {
            return Err(format!("duplicate option: {flag}"));
        }
        index += 1;
    }
    Ok(options)
}

fn required_option<'a>(
    options: &'a BTreeMap<String, String>,
    flag: &str,
) -> Result<&'a str, String> {
    options
        .get(flag)
        .map(String::as_str)
        .ok_or_else(|| format!("missing required option: {flag}"))
}

fn validate_commit_sha(value: &str) -> Result<(), String> {
    if value.len() == 40 && value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        Ok(())
    } else {
        Err(format!(
            "commit SHA must be 40 hexadecimal characters, got {value:?}"
        ))
    }
}

fn ensure_native_host(target: NativeTarget) -> Result<(), String> {
    let (expected_os, expected_arch) = target.host();
    if env::consts::OS == expected_os && env::consts::ARCH == expected_arch {
        Ok(())
    } else {
        Err(format!(
            "target {} requires {expected_os}/{expected_arch}, current host is {}/{}",
            target.as_str(),
            env::consts::OS,
            env::consts::ARCH
        ))
    }
}

fn release_version() -> Result<String, String> {
    let root = cargo_version(Path::new("Cargo.toml"))?;
    let wiki = cargo_version(Path::new("crates/k-wiki/Cargo.toml"))?;
    let dependency = dependency_version(Path::new("crates/k-wiki/Cargo.toml"), "codebase-graph")?;
    if root != wiki || root != dependency {
        return Err(format!(
            "release versions are not aligned: root={root}, k-wiki={wiki}, dependency={dependency}"
        ));
    }
    Ok(root)
}

fn release_version_from_tag(tag: &str) -> Result<String, String> {
    let version = tag
        .strip_prefix('v')
        .ok_or_else(|| format!("release tag must match vX.Y.Z, got {tag:?}"))?;
    let parts: Vec<&str> = version.split('.').collect();
    if parts.len() != 3
        || parts
            .iter()
            .any(|part| part.is_empty() || !part.bytes().all(|byte| byte.is_ascii_digit()))
    {
        return Err(format!("release tag must match vX.Y.Z, got {tag:?}"));
    }
    Ok(version.to_string())
}

fn build_native_binaries(target: NativeTarget) -> Result<(), String> {
    let mut graph = Command::new("cargo");
    graph.args(["build", "--locked", "--release", "--bin", "codebase-graph"]);
    if target.uses_windows_extensions() {
        graph.args(["--features", "bundled-windows-extensions"]);
    }
    run_command(&mut graph, "codebase-graph release build")?;

    let mut wiki = Command::new("cargo");
    wiki.args([
        "build",
        "--locked",
        "--release",
        "-p",
        "k-wiki",
        "--bin",
        "k-wiki",
    ]);
    if target.uses_windows_extensions() {
        wiki.args(["--features", "bundled-windows-extensions"]);
    }
    run_command(&mut wiki, "k-wiki release build")
}

fn build_and_smoke_native_archive(
    target: NativeTarget,
    version: &str,
    commit_sha: &str,
    output: &Path,
    staging: &Path,
) -> Result<(), String> {
    let package = staging.join("package");
    let extracted = staging.join("extracted");
    let install_bin = staging.join("install-bin");
    fs::create_dir_all(&package).map_err(|error| error.to_string())?;

    let (graph_name, wiki_name) = target.binary_names();
    copy_file(
        Path::new("target/release").join(graph_name),
        package.join(graph_name),
    )?;
    copy_file(
        Path::new("target/release").join(wiki_name),
        package.join(wiki_name),
    )?;
    copy_file("release/install/install.sh", package.join("install.sh"))?;
    copy_file("release/install/install.ps1", package.join("install.ps1"))?;
    set_executable(&package.join(graph_name))?;
    set_executable(&package.join(wiki_name))?;
    set_executable(&package.join("install.sh"))?;

    let checksums = format!(
        "{}  {graph_name}\n{}  {wiki_name}\n",
        sha256_file(&package.join(graph_name))?,
        sha256_file(&package.join(wiki_name))?
    );
    fs::write(package.join("checksums.txt"), checksums).map_err(|error| error.to_string())?;

    let archive_name = target.archive_name(version);
    let archive_path = output.join(&archive_name);
    create_native_archive(&package, &archive_path, target)?;
    let archive_sha256 = sha256_file(&archive_path)?;
    fs::write(
        output.join(format!("{archive_name}.sha256")),
        format!("{archive_sha256}  {archive_name}\n"),
    )
    .map_err(|error| error.to_string())?;
    let provenance = NativeProvenance {
        schema_version: 1,
        commit_sha: commit_sha.to_string(),
        version: version.to_string(),
        target: target.as_str().to_string(),
        archive: archive_name.clone(),
        archive_sha256,
    };
    fs::write(
        output.join("provenance.json"),
        serde_json::to_vec_pretty(&provenance).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;

    extract_and_validate_native_archive(&archive_path, target, &extracted)?;
    dry_run_installer(target, &extracted, &install_bin)?;
    smoke_artifact(&extracted.join(graph_name))?;
    smoke_wiki_artifact(&extracted.join(wiki_name))?;
    println!("{}", archive_path.display());
    Ok(())
}

fn copy_file(source: impl AsRef<Path>, destination: impl AsRef<Path>) -> Result<(), String> {
    let source = source.as_ref();
    let destination = destination.as_ref();
    fs::copy(source, destination).map_err(|error| {
        format!(
            "failed to copy {} to {}: {error}",
            source.display(),
            destination.display()
        )
    })?;
    Ok(())
}

#[cfg(unix)]
fn set_executable(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    let mut permissions = fs::metadata(path)
        .map_err(|error| error.to_string())?
        .permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions).map_err(|error| error.to_string())
}

#[cfg(not(unix))]
fn set_executable(_path: &Path) -> Result<(), String> {
    Ok(())
}

fn create_native_archive(
    package: &Path,
    archive_path: &Path,
    target: NativeTarget,
) -> Result<(), String> {
    let file = File::create(archive_path)
        .map_err(|error| format!("failed to create {}: {error}", archive_path.display()))?;
    let encoder = GzBuilder::new()
        .mtime(0)
        .write(file, Compression::default());
    let mut archive = tar::Builder::new(encoder);
    let (graph_name, wiki_name) = target.binary_names();
    for (name, mode) in [
        (graph_name, 0o755),
        (wiki_name, 0o755),
        ("checksums.txt", 0o644),
        ("install.sh", 0o755),
        ("install.ps1", 0o644),
    ] {
        append_deterministic_file(&mut archive, &package.join(name), name, mode)?;
    }
    let encoder = archive
        .into_inner()
        .map_err(|error| format!("failed to finish tar archive: {error}"))?;
    encoder
        .finish()
        .map_err(|error| format!("failed to finish gzip archive: {error}"))?;
    Ok(())
}

fn append_deterministic_file<W: Write>(
    archive: &mut tar::Builder<W>,
    source: &Path,
    name: &str,
    mode: u32,
) -> Result<(), String> {
    let mut file = File::open(source)
        .map_err(|error| format!("failed to open {}: {error}", source.display()))?;
    let size = file.metadata().map_err(|error| error.to_string())?.len();
    let mut header = tar::Header::new_gnu();
    header.set_size(size);
    header.set_mode(mode);
    header.set_uid(0);
    header.set_gid(0);
    header.set_mtime(0);
    header.set_cksum();
    archive
        .append_data(&mut header, name, &mut file)
        .map_err(|error| format!("failed to append {name}: {error}"))
}

fn extract_and_validate_native_archive(
    archive_path: &Path,
    target: NativeTarget,
    destination: &Path,
) -> Result<(), String> {
    fs::create_dir_all(destination).map_err(|error| error.to_string())?;
    let file = File::open(archive_path)
        .map_err(|error| format!("failed to open {}: {error}", archive_path.display()))?;
    let mut archive = tar::Archive::new(GzDecoder::new(file));
    let expected = expected_package_files(target);
    let mut actual = BTreeSet::new();
    for entry in archive.entries().map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        if !entry.header().entry_type().is_file() {
            return Err("native archive contains a non-file entry".to_string());
        }
        let path = entry.path().map_err(|error| error.to_string())?;
        let text = path
            .to_str()
            .ok_or_else(|| "native archive contains a non-UTF-8 path".to_string())?;
        if !expected.contains(text) || !actual.insert(text.to_string()) {
            return Err(format!(
                "unexpected or duplicate native archive entry: {text}"
            ));
        }
    }
    if actual != expected {
        return Err(format!(
            "native archive entries do not match contract: expected {expected:?}, got {actual:?}"
        ));
    }

    let file = File::open(archive_path).map_err(|error| error.to_string())?;
    tar::Archive::new(GzDecoder::new(file))
        .unpack(destination)
        .map_err(|error| format!("failed to extract {}: {error}", archive_path.display()))?;
    for executable in [target.binary_names().0, target.binary_names().1] {
        set_executable(&destination.join(executable))?;
    }
    set_executable(&destination.join("install.sh"))?;
    validate_internal_checksums(destination, target)
}

fn expected_package_files(target: NativeTarget) -> BTreeSet<String> {
    let (graph_name, wiki_name) = target.binary_names();
    [
        graph_name,
        wiki_name,
        "checksums.txt",
        "install.sh",
        "install.ps1",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}

fn validate_internal_checksums(directory: &Path, target: NativeTarget) -> Result<(), String> {
    let text = fs::read_to_string(directory.join("checksums.txt"))
        .map_err(|error| format!("failed to read checksums.txt: {error}"))?;
    let mut checksums = BTreeMap::new();
    for line in text.lines() {
        let mut parts = line.split_whitespace();
        let digest = parts.next().unwrap_or_default();
        let name = parts.next().unwrap_or_default();
        if parts.next().is_some()
            || digest.len() != 64
            || !digest.bytes().all(|byte| byte.is_ascii_hexdigit())
            || checksums
                .insert(name.to_string(), digest.to_ascii_lowercase())
                .is_some()
        {
            return Err(format!("invalid checksums.txt line: {line:?}"));
        }
    }
    let (graph_name, wiki_name) = target.binary_names();
    let expected_names: BTreeSet<String> = [graph_name, wiki_name]
        .into_iter()
        .map(str::to_string)
        .collect();
    if checksums.keys().cloned().collect::<BTreeSet<_>>() != expected_names {
        return Err("checksums.txt does not contain exactly both packaged binaries".to_string());
    }
    for (name, expected) in checksums {
        let actual = sha256_file(&directory.join(&name))?;
        if actual != expected {
            return Err(format!("SHA256 mismatch for {name}"));
        }
    }
    Ok(())
}

fn dry_run_installer(
    target: NativeTarget,
    extracted: &Path,
    install_bin: &Path,
) -> Result<(), String> {
    let mut command = if target == NativeTarget::WindowsX86_64 {
        let mut command = Command::new("pwsh");
        command
            .arg("-File")
            .arg(extracted.join("install.ps1"))
            .arg("-SourceDir")
            .arg(extracted)
            .arg("-BinDir")
            .arg(install_bin)
            .arg("-DryRun");
        command
    } else {
        let mut command = Command::new("bash");
        command
            .arg(extracted.join("install.sh"))
            .arg("--source-dir")
            .arg(extracted)
            .arg("--bin-dir")
            .arg(install_bin)
            .arg("--dry-run");
        command
    };
    run_command(&mut command, "packaged installer dry-run")
}

fn sha256_file(path: &Path) -> Result<String, String> {
    let mut file =
        File::open(path).map_err(|error| format!("failed to open {}: {error}", path.display()))?;
    let mut digest = Sha256::new();
    std::io::copy(&mut file, &mut digest).map_err(|error| error.to_string())?;
    Ok(format!("{:x}", digest.finalize()))
}

fn validate_native_artifact_set(
    input: &Path,
    version: &str,
    commit_sha: &str,
) -> Result<(), String> {
    if !input.is_dir() {
        return Err(format!(
            "artifact input is not a directory: {}",
            input.display()
        ));
    }
    let files = all_files_under(input);
    let provenance_files: Vec<PathBuf> = files
        .iter()
        .filter(|path| {
            path.file_name()
                .is_some_and(|name| name == "provenance.json")
        })
        .cloned()
        .collect();
    if provenance_files.len() != NATIVE_TARGETS.len() {
        return Err(format!(
            "expected {} provenance files, found {}",
            NATIVE_TARGETS.len(),
            provenance_files.len()
        ));
    }

    let mut validated_targets = BTreeSet::new();
    let mut expected_files = BTreeSet::new();
    for provenance_path in provenance_files {
        let provenance: NativeProvenance =
            serde_json::from_slice(&fs::read(&provenance_path).map_err(|error| error.to_string())?)
                .map_err(|error| format!("invalid {}: {error}", provenance_path.display()))?;
        if provenance.schema_version != 1 {
            return Err(format!(
                "unsupported provenance schema: {}",
                provenance.schema_version
            ));
        }
        if provenance.commit_sha != commit_sha || provenance.version != version {
            return Err(format!(
                "mixed release provenance in {}: expected {commit_sha}/{version}, got {}/{}",
                provenance_path.display(),
                provenance.commit_sha,
                provenance.version
            ));
        }
        let target = NativeTarget::parse(&provenance.target)?;
        if !validated_targets.insert(target) {
            return Err(format!("duplicate provenance target: {}", target.as_str()));
        }
        let expected_archive = target.archive_name(version);
        if provenance.archive != expected_archive {
            return Err(format!(
                "provenance archive mismatch for {}: expected {expected_archive}, got {}",
                target.as_str(),
                provenance.archive
            ));
        }
        let parent = provenance_path
            .parent()
            .ok_or_else(|| "provenance file has no parent directory".to_string())?;
        let archive_path = parent.join(&expected_archive);
        let sidecar_path = parent.join(format!("{expected_archive}.sha256"));
        let actual_sha = sha256_file(&archive_path)?;
        if provenance.archive_sha256 != actual_sha {
            return Err(format!(
                "archive SHA256 does not match provenance for {expected_archive}"
            ));
        }
        let expected_sidecar = format!("{actual_sha}  {expected_archive}");
        let sidecar = fs::read_to_string(&sidecar_path)
            .map_err(|error| format!("failed to read {}: {error}", sidecar_path.display()))?;
        if sidecar.trim_end() != expected_sidecar {
            return Err(format!("invalid SHA256 sidecar for {expected_archive}"));
        }
        let extracted = unique_temp_dir(&format!("validate_{}", target.as_str()))?;
        let validation = extract_and_validate_native_archive(&archive_path, target, &extracted);
        let _ = fs::remove_dir_all(&extracted);
        validation?;
        expected_files.extend([provenance_path, archive_path, sidecar_path]);
    }

    let all_targets: BTreeSet<NativeTarget> = NATIVE_TARGETS.into_iter().collect();
    if validated_targets != all_targets {
        return Err(format!(
            "native artifact targets are incomplete: expected {all_targets:?}, got {validated_targets:?}"
        ));
    }
    let actual_files: BTreeSet<PathBuf> = files.into_iter().collect();
    if actual_files != expected_files {
        return Err("artifact input contains unexpected or missing files".to_string());
    }
    println!("validated {} native artifact targets", NATIVE_TARGETS.len());
    Ok(())
}

fn all_files_under(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let Ok(entries) = fs::read_dir(root) else {
        return files;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            files.extend(all_files_under(&path));
        } else {
            files.push(path);
        }
    }
    files
}

fn run_command(command: &mut Command, description: &str) -> Result<(), String> {
    let status = command
        .status()
        .map_err(|error| format!("failed to run {description}: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{description} failed with status {status}"))
    }
}

fn release_gate(args: Vec<String>) -> Result<(), String> {
    let mut production = false;
    let mut require_conda = false;
    let mut confirmations = BTreeSet::new();
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--production" => production = true,
            "--require-conda" => require_conda = true,
            "--confirm" => {
                index += 1;
                let flag = args
                    .get(index)
                    .ok_or_else(|| "--confirm requires a value".to_string())?;
                confirmations.insert(flag.clone());
            }
            other => return Err(format!("unknown release-gate option: {other}")),
        }
        index += 1;
    }

    let mut issues = Vec::new();
    check_security_policy(&mut issues);
    check_rust_only_files(&mut issues);
    check_cargo_metadata(&mut issues);
    check_release_version_sync(&mut issues);
    check_release_please_config(&mut issues);
    check_workflows(&mut issues);
    check_no_legacy_surfaces(&mut issues);
    if require_conda {
        check_conda_recipe(&mut issues);
    }
    if production {
        for flag in CONFIRMATIONS {
            if !confirmations.contains(*flag) {
                issues.push(format!("FAIL: external-confirmation-missing: production release requires --confirm {flag}."));
            }
        }
    }

    if issues.is_empty() {
        println!("release gate passed");
        Ok(())
    } else {
        for issue in &issues {
            eprintln!("{issue}");
        }
        Err("release gate failed".to_string())
    }
}

fn check_workflows_command() -> Result<(), String> {
    let mut issues = Vec::new();
    check_workflows(&mut issues);
    if issues.is_empty() {
        println!("workflow policy passed");
        Ok(())
    } else {
        for issue in &issues {
            eprintln!("{issue}");
        }
        Err("workflow policy failed".to_string())
    }
}

fn check_security_policy(issues: &mut Vec<String>) {
    let path = Path::new("SECURITY.md");
    let Ok(text) = fs::read_to_string(path) else {
        issues.push("FAIL: security-policy-missing: SECURITY.md is required.".to_string());
        return;
    };
    for required in ["Reporting a Vulnerability", "graph_query", "--allow-remote"] {
        if !text.contains(required) {
            issues.push(format!(
                "FAIL: security-policy-incomplete: SECURITY.md must mention {required:?}."
            ));
        }
    }
}

fn check_rust_only_files(issues: &mut Vec<String>) {
    for forbidden in ["pyproject.toml", "requirements-dev.txt"] {
        if Path::new(forbidden).exists() {
            issues.push(format!(
                "FAIL: python-tooling-present: {forbidden} must not exist."
            ));
        }
    }
    for directory in ["scripts", "src/codebase_graph"] {
        if Path::new(directory).exists() {
            issues.push(format!(
                "FAIL: python-surface-present: {directory} must not exist."
            ));
        }
    }
    for path in files_under(Path::new(".")) {
        let relative = path.strip_prefix(".").unwrap_or(&path);
        let text_path = relative.to_string_lossy();
        if text_path.contains("/target/")
            || text_path.contains("/.git/")
            || text_path.contains("/.codebaseGraph/")
            || text_path.contains("/.kwiki/")
        {
            continue;
        }
        if path.extension().is_some_and(|extension| extension == "py") {
            issues.push(format!(
                "FAIL: python-file-present: {} must not be maintained source.",
                text_path
            ));
        }
    }
}

fn check_cargo_metadata(issues: &mut Vec<String>) {
    let Ok(cargo) = fs::read_to_string("Cargo.toml") else {
        issues.push("FAIL: cargo-missing: root Cargo.toml is required.".to_string());
        return;
    };
    for required in [
        r#"name = "codebase-graph""#,
        r#"name = "codebase_graph""#,
        r#"name = "codebase-graph""#,
        r#"license = "MIT""#,
        r#"repository = "https://github.com/rabii-chaarani/codebaseGraph""#,
        r#"readme = "README.md""#,
    ] {
        if !cargo.contains(required) {
            issues.push(format!(
                "FAIL: cargo-metadata-incomplete: Cargo.toml must contain {required}."
            ));
        }
    }
    let wiki_manifest = Path::new("crates/k-wiki/Cargo.toml");
    match fs::read_to_string(wiki_manifest) {
        Ok(wiki) => {
            for required in [
                r#"name = "k-wiki""#,
                r#"name = "k_wiki""#,
                r#"name = "k-wiki""#,
                r#"license = "MIT""#,
            ] {
                if !wiki.contains(required) {
                    issues.push(format!(
                        "FAIL: cargo-metadata-incomplete: {} must contain {required}.",
                        wiki_manifest.display()
                    ));
                }
            }
        }
        Err(_) => issues.push(format!(
            "FAIL: cargo-missing: {} is required.",
            wiki_manifest.display()
        )),
    }
}

fn check_release_version_sync(issues: &mut Vec<String>) {
    let root = match cargo_version(Path::new("Cargo.toml")) {
        Ok(version) => version,
        Err(error) => {
            issues.push(format!(
                "FAIL: cargo-version-missing: could not read root version: {error}."
            ));
            return;
        }
    };
    let wiki = match cargo_version(Path::new("crates/k-wiki/Cargo.toml")) {
        Ok(version) => version,
        Err(error) => {
            issues.push(format!(
                "FAIL: cargo-version-missing: could not read k-wiki version: {error}."
            ));
            return;
        }
    };
    if root != wiki {
        issues.push(format!(
            "FAIL: release-version-divergence: root Cargo.toml version {root} does not match crates/k-wiki/Cargo.toml version {wiki}."
        ));
    }
    match dependency_version(Path::new("crates/k-wiki/Cargo.toml"), "codebase-graph") {
        Ok(version) if version != root => issues.push(format!(
            "FAIL: release-version-divergence: crates/k-wiki/Cargo.toml depends on codebase-graph {version} but root Cargo.toml is {root}."
        )),
        Ok(_) => {}
        Err(error) => issues.push(format!(
            "FAIL: cargo-version-missing: could not read crates/k-wiki dependency version: {error}."
        )),
    }
}

fn check_release_please_config(issues: &mut Vec<String>) {
    let Ok(text) = fs::read_to_string("release-please-config.json") else {
        issues.push(
            "FAIL: release-please-config-missing: release-please-config.json is required."
                .to_string(),
        );
        return;
    };
    let Ok(parsed) = serde_json::from_str::<Value>(&text) else {
        issues.push(
            "FAIL: release-please-config-invalid: release-please-config.json must be valid JSON."
                .to_string(),
        );
        return;
    };
    let Some(extra_files) = parsed
        .get("packages")
        .and_then(|packages| packages.get("."))
        .and_then(|root| root.get("extra-files"))
        .and_then(Value::as_array)
    else {
        issues.push(
            "FAIL: release-please-config-incomplete: root package must declare extra-files for crates/k-wiki/Cargo.toml."
                .to_string(),
        );
        return;
    };
    let required = [
        ("crates/k-wiki/Cargo.toml", "$.package.version"),
        (
            "crates/k-wiki/Cargo.toml",
            "$.dependencies['codebase-graph'].version",
        ),
    ];
    for (path, jsonpath) in required {
        let present = extra_files.iter().any(|entry| {
            entry.get("type").and_then(Value::as_str) == Some("toml")
                && entry.get("path").and_then(Value::as_str) == Some(path)
                && entry.get("jsonpath").and_then(Value::as_str) == Some(jsonpath)
        });
        if !present {
            issues.push(format!(
                "FAIL: release-please-config-incomplete: root package extra-files must include {path} with {jsonpath}."
            ));
        }
    }
}

fn check_workflows(issues: &mut Vec<String>) {
    let paths = [
        ".github/workflows/ci.yml",
        ".github/workflows/native.yml",
        ".github/workflows/release.yml",
    ];
    let mut parsed = BTreeMap::new();
    for workflow in paths {
        let Ok(text) = fs::read_to_string(workflow) else {
            issues.push(format!("FAIL: workflow-missing: {workflow} is required."));
            continue;
        };
        let workflow_forbidden = [
            concat!("actions/setup-", "python"),
            concat!("python", " "),
            concat!("p", "ip", " "),
            concat!("py", "test"),
            concat!("ru", "ff"),
            concat!("p", "ip", "-audit"),
            "scripts/",
        ];
        for forbidden in workflow_forbidden {
            if text.contains(forbidden) {
                issues.push(format!(
                    "FAIL: workflow-python-tooling-present: {workflow} contains {forbidden}."
                ));
            }
        }
        match yaml_serde::from_str::<YamlValue>(&text) {
            Ok(value) => {
                parsed.insert(workflow, value);
            }
            Err(error) => issues.push(format!(
                "FAIL: workflow-yaml-invalid: {workflow} is not valid YAML: {error}."
            )),
        }
    }
    if let (Some(ci), Some(native), Some(release)) = (
        parsed.get(".github/workflows/ci.yml"),
        parsed.get(".github/workflows/native.yml"),
        parsed.get(".github/workflows/release.yml"),
    ) {
        check_workflow_policy(ci, native, release, issues);
    }
}

#[cfg(test)]
fn workflow_yaml_error(text: &str) -> Option<String> {
    yaml_serde::from_str::<YamlValue>(text)
        .err()
        .map(|error| error.to_string())
}

fn check_workflow_policy(
    ci: &YamlValue,
    native: &YamlValue,
    release: &YamlValue,
    issues: &mut Vec<String>,
) {
    for event in ["pull_request", "push"] {
        let branches = yaml_path(ci, &["on", event, "branches"])
            .and_then(yaml_string_set)
            .unwrap_or_default();
        if branches != BTreeSet::from(["main".to_string()]) {
            issues.push(format!(
                "FAIL: workflow-trigger-invalid: CI {event} branches must contain only main."
            ));
        }
    }
    if yaml_path(ci, &["jobs", "native", "uses"]).and_then(YamlValue::as_str)
        != Some("./.github/workflows/native.yml")
    {
        issues.push(
            "FAIL: workflow-native-reuse-missing: CI native job must call native.yml.".to_string(),
        );
    }
    let required_needs = yaml_path(ci, &["jobs", "required", "needs"])
        .and_then(yaml_string_set)
        .unwrap_or_default();
    let expected_needs: BTreeSet<String> =
        ["fmt", "clippy", "supply-chain", "publish-dry-run", "native"]
            .into_iter()
            .map(str::to_string)
            .collect();
    if required_needs != expected_needs {
        issues.push(
            "FAIL: workflow-required-incomplete: required job must depend on every mandatory CI job."
                .to_string(),
        );
    }
    if yaml_path(ci, &["jobs", "required", "if"]).and_then(YamlValue::as_str)
        != Some("${{ always() }}")
    {
        issues.push(
            "FAIL: workflow-required-condition: required job must use if: always().".to_string(),
        );
    }

    for input in [
        "target",
        "source-sha",
        "run-tests",
        "upload-artifact",
        "artifact-retention-days",
    ] {
        if yaml_path(native, &["on", "workflow_call", "inputs", input]).is_none() {
            issues.push(format!(
                "FAIL: workflow-native-input-missing: native workflow input {input} is required."
            ));
        }
    }
    if yaml_path(native, &["jobs", "native", "permissions", "contents"]).and_then(YamlValue::as_str)
        != Some("read")
    {
        issues.push(
            "FAIL: workflow-native-permissions: native workflow must use contents: read."
                .to_string(),
        );
    }

    if yaml_path(release, &["on", "push"]).is_some() {
        issues.push(
            "FAIL: release-direct-push-trigger: release must wait for the completed CI workflow."
                .to_string(),
        );
    }
    for (field, expected) in [
        ("workflows", BTreeSet::from(["CI".to_string()])),
        ("types", BTreeSet::from(["completed".to_string()])),
        ("branches", BTreeSet::from(["main".to_string()])),
    ] {
        let actual = yaml_path(release, &["on", "workflow_run", field])
            .and_then(yaml_string_set)
            .unwrap_or_default();
        if actual != expected {
            issues.push(format!(
                "FAIL: release-workflow-run-trigger: release workflow_run {field} is invalid."
            ));
        }
    }
    if yaml_path(release, &["on", "workflow_dispatch"]).is_none() {
        issues.push(
            "FAIL: release-manual-trigger-missing: release must retain workflow_dispatch recovery."
                .to_string(),
        );
    }
    if yaml_path(release, &["concurrency", "group"]).and_then(YamlValue::as_str)
        != Some(
            "release-${{ github.event_name == 'workflow_run' && 'main' || inputs.publish-existing-tag }}",
        )
        || yaml_path(release, &["concurrency", "cancel-in-progress"]).and_then(YamlValue::as_bool)
            != Some(false)
    {
        issues.push(
            "FAIL: release-concurrency-invalid: automatic releases must serialize as release-main without cancellation."
                .to_string(),
        );
    }

    let release_please =
        yaml_path(release, &["jobs", "release-please"]).unwrap_or(&YamlValue::Null);
    for marker in [
        "github.event_name == 'workflow_run'",
        "github.event.workflow_run.event == 'push'",
        "github.event.workflow_run.head_branch == 'main'",
        "github.event.workflow_run.conclusion == 'success'",
    ] {
        if !yaml_path(release_please, &["if"])
            .is_some_and(|condition| yaml_contains_string(condition, marker))
        {
            issues.push(format!(
                "FAIL: release-success-guard-missing: release-please must require {marker}."
            ));
        }
    }
    let trigger_step = yaml_step_by_id(release_please, "trigger").unwrap_or(&YamlValue::Null);
    for (field, expected) in [
        ("CI_RUN_ID", "${{ github.event.workflow_run.id }}"),
        ("CI_HEAD_SHA", "${{ github.event.workflow_run.head_sha }}"),
    ] {
        if yaml_path(trigger_step, &["env", field]).and_then(YamlValue::as_str) != Some(expected) {
            issues.push(format!(
                "FAIL: release-trigger-binding-missing: trigger step {field} must bind {expected}."
            ));
        }
    }
    for marker in ["git/ref/heads/main", "current-tip"] {
        if !yaml_path(trigger_step, &["run"]).is_some_and(|run| yaml_contains_string(run, marker)) {
            issues.push(format!(
                "FAIL: release-trigger-binding-missing: trigger step must contain {marker}."
            ));
        }
    }
    let release_action = yaml_step_by_id(release_please, "release").unwrap_or(&YamlValue::Null);
    if yaml_path(release_action, &["uses"]).and_then(YamlValue::as_str)
        != Some("googleapis/release-please-action@45996ed1f6d02564a971a2fa1b5860e934307cf7")
    {
        issues.push(
            "FAIL: release-please-action-missing: release job must use the pinned release-please action."
                .to_string(),
        );
    }
    let post_release_step = yaml_step_by_name(
        release_please,
        "Recheck current main tip after release-please",
    )
    .unwrap_or(&YamlValue::Null);
    if yaml_path(post_release_step, &["env", "RELEASE_SHA"]).and_then(YamlValue::as_str)
        != Some("${{ steps.release.outputs.sha }}")
        || !yaml_path(post_release_step, &["run"]).is_some_and(|run| {
            yaml_contains_string(run, "main_sha") && yaml_contains_string(run, "CI_HEAD_SHA")
        })
    {
        issues.push(
            "FAIL: release-post-action-freshness-missing: release-please must recheck the current main tip and release SHA."
                .to_string(),
        );
    }
    if [
        release_please,
        yaml_path(release, &["jobs", "release-target"]).unwrap_or(&YamlValue::Null),
        yaml_path(release, &["jobs", "ci-gate"]).unwrap_or(&YamlValue::Null),
    ]
    .iter()
    .any(|value| yaml_contains_string(value, "github.sha"))
    {
        issues.push(
            "FAIL: release-github-sha-forbidden: workflow_run releases must use the triggering CI head SHA."
                .to_string(),
        );
    }

    let release_target =
        yaml_path(release, &["jobs", "release-target"]).unwrap_or(&YamlValue::Null);
    let resolve_step = yaml_step_by_id(release_target, "resolve").unwrap_or(&YamlValue::Null);
    for (field, expected) in [
        (
            "RELEASE_CI_RUN_ID",
            "${{ needs.release-please.outputs.ci-run-id }}",
        ),
        (
            "RELEASE_CI_HEAD_SHA",
            "${{ needs.release-please.outputs.ci-head-sha }}",
        ),
    ] {
        if yaml_path(resolve_step, &["env", field]).and_then(YamlValue::as_str) != Some(expected) {
            issues.push(format!(
                "FAIL: release-target-binding-missing: resolve step {field} must bind {expected}."
            ));
        }
    }
    for marker in ["tag_sha", "source_sha"] {
        if !yaml_path(resolve_step, &["run"]).is_some_and(|run| yaml_contains_string(run, marker)) {
            issues.push(format!(
                "FAIL: release-target-binding-missing: resolve step must preserve {marker}."
            ));
        }
    }

    if yaml_path(release, &["jobs", "ci-gate", "permissions", "actions"])
        .and_then(YamlValue::as_str)
        != Some("read")
        || yaml_path(release, &["jobs", "ci-gate", "outputs", "ci-run-id"]).is_none()
    {
        issues.push(
            "FAIL: release-exact-sha-gate-missing: release must resolve an exact-SHA CI run with actions: read."
                .to_string(),
        );
    }
    let ci_gate = yaml_path(release, &["jobs", "ci-gate"]).unwrap_or(&YamlValue::Null);
    let ci_gate_step = yaml_step_by_id(ci_gate, "wait").unwrap_or(&YamlValue::Null);
    for (field, expected) in [
        ("AUTOMATIC", "${{ needs.release-target.outputs.automatic }}"),
        (
            "REQUESTED_CI_RUN_ID",
            "${{ needs.release-target.outputs.ci-run-id }}",
        ),
    ] {
        if yaml_path(ci_gate_step, &["env", field]).and_then(YamlValue::as_str) != Some(expected) {
            issues.push(format!(
                "FAIL: release-trigger-run-validation-missing: CI gate {field} must bind {expected}."
            ));
        }
    }
    for marker in [
        "if [[ \"$AUTOMATIC\" == 'true' ]]",
        "actions/runs/$REQUESTED_CI_RUN_ID",
        ".github/workflows/ci.yml",
        ".event == \"push\"",
        ".head_branch == \"main\"",
        ".status == \"completed\"",
        ".conclusion == \"success\"",
    ] {
        if !yaml_path(ci_gate_step, &["run"]).is_some_and(|run| yaml_contains_string(run, marker)) {
            issues.push(format!(
                "FAIL: release-trigger-run-validation-missing: exact triggering CI validation must contain {marker}."
            ));
        }
    }
    let select_artifacts =
        yaml_path(release, &["jobs", "select-artifacts"]).unwrap_or(&YamlValue::Null);
    let select_step = yaml_step_by_id(select_artifacts, "select").unwrap_or(&YamlValue::Null);
    if yaml_path(select_step, &["env", "AUTOMATIC"]).and_then(YamlValue::as_str)
        != Some("${{ needs.release-target.outputs.automatic }}")
        || !yaml_path(select_step, &["run"]).is_some_and(|run| {
            yaml_contains_string(run, "\"$AUTOMATIC\" == 'false'")
                && yaml_contains_string(run, "rebuild-if-missing")
        })
    {
        issues.push(
            "FAIL: release-automatic-rebuild: all-target rebuild recovery must remain manual-only."
                .to_string(),
        );
    }
    if yaml_path(release, &["jobs", "rebuild-artifacts", "uses"]).and_then(YamlValue::as_str)
        != Some("./.github/workflows/native.yml")
    {
        issues.push(
            "FAIL: release-recovery-reuse-missing: recovery must call native.yml.".to_string(),
        );
    }
    let Some(jobs) = yaml_path(release, &["jobs"]).and_then(YamlValue::as_mapping) else {
        issues.push("FAIL: release-jobs-missing: release workflow jobs are required.".to_string());
        return;
    };
    let publishers: Vec<&YamlValue> = jobs
        .values()
        .filter(|job| yaml_contains_string(job, "gh release upload"))
        .collect();
    if publishers.len() != 1
        || !yaml_contains_string(
            yaml_path(release, &["jobs", "publish-release-assets"]).unwrap_or(&YamlValue::Null),
            "gh release upload",
        )
    {
        issues.push(
            "FAIL: release-publisher-count: exactly one publish-release-assets job may upload release assets."
                .to_string(),
        );
    }
    if yaml_path(
        release,
        &["jobs", "publish-release-assets", "permissions", "contents"],
    )
    .and_then(YamlValue::as_str)
        != Some("write")
        || yaml_path(
            release,
            &["jobs", "publish-release-assets", "environment", "name"],
        )
        .and_then(YamlValue::as_str)
            != Some("cargo")
    {
        issues.push(
            "FAIL: release-publisher-permissions: the single publisher must use contents: write in cargo."
                .to_string(),
        );
    }
    if !yaml_contains_string(
        yaml_path(release, &["jobs", "publish-crate"]).unwrap_or(&YamlValue::Null),
        "cargo publish --dry-run --locked",
    ) || !yaml_contains_string(
        yaml_path(release, &["jobs", "publish-crate"]).unwrap_or(&YamlValue::Null),
        "cargo publish --locked",
    ) {
        issues.push(
            "FAIL: release-publish-gate-missing: crate publication must retain dry-run and publish steps."
                .to_string(),
        );
    }
}

fn yaml_path<'a>(value: &'a YamlValue, path: &[&str]) -> Option<&'a YamlValue> {
    let mut current = value;
    for key in path {
        current = current
            .as_mapping()?
            .get(YamlValue::String((*key).to_string()))?;
    }
    Some(current)
}

fn yaml_string_set(value: &YamlValue) -> Option<BTreeSet<String>> {
    value.as_sequence().map(|items| {
        items
            .iter()
            .filter_map(YamlValue::as_str)
            .map(str::to_string)
            .collect()
    })
}

fn yaml_step_by_id<'a>(job: &'a YamlValue, id: &str) -> Option<&'a YamlValue> {
    yaml_path(job, &["steps"])?
        .as_sequence()?
        .iter()
        .find(|step| yaml_path(step, &["id"]).and_then(YamlValue::as_str) == Some(id))
}

fn yaml_step_by_name<'a>(job: &'a YamlValue, name: &str) -> Option<&'a YamlValue> {
    yaml_path(job, &["steps"])?
        .as_sequence()?
        .iter()
        .find(|step| yaml_path(step, &["name"]).and_then(YamlValue::as_str) == Some(name))
}

fn yaml_contains_string(value: &YamlValue, needle: &str) -> bool {
    match value {
        YamlValue::String(text) => text.contains(needle),
        YamlValue::Sequence(items) => items.iter().any(|item| yaml_contains_string(item, needle)),
        YamlValue::Mapping(mapping) => mapping.iter().any(|(key, value)| {
            yaml_contains_string(key, needle) || yaml_contains_string(value, needle)
        }),
        YamlValue::Tagged(tagged) => yaml_contains_string(&tagged.value, needle),
        _ => false,
    }
}

fn check_no_legacy_surfaces(issues: &mut Vec<String>) {
    let old_crate_name = ["codebase", "graph", "native"].join("_");
    let old_builder = format!("{old_crate_name}_graph_builder");
    let old_workspace_crate = ["rust", "crates", &old_crate_name, "Cargo.toml"].join("/");
    for forbidden in [
        concat!("src/", "legacy", "_cli.rs"),
        "src/ffi.rs",
        &format!("src/bin/{old_builder}.rs"),
        concat!("rust", "/Cargo.toml"),
        &old_workspace_crate,
    ] {
        if Path::new(forbidden).exists() {
            issues.push(format!(
                "FAIL: legacy-surface-present: {forbidden} must not exist."
            ));
        }
    }
    for path in [
        "Cargo.toml",
        "src/lib.rs",
        "src/adapters/cli/mod.rs",
        "src/ladybug_writer.rs",
    ] {
        let text = fs::read_to_string(path).unwrap_or_default();
        let forbidden_tokens = [
            concat!("py", "o3"),
            concat!("python", "-extension"),
            concat!("cdy", "lib"),
            concat!("legacy", "-protocol"),
            concat!("legacy", "_cli"),
            old_builder.as_str(),
        ];
        for forbidden in forbidden_tokens {
            if text.contains(forbidden) {
                issues.push(format!(
                    "FAIL: legacy-token-present: {path} contains {forbidden}."
                ));
            }
        }
    }
}

fn check_conda_recipe(issues: &mut Vec<String>) {
    let Ok(recipe) = fs::read_to_string("conda-forge/recipe/meta.yaml") else {
        issues.push(
            "FAIL: conda-recipe-missing: conda-forge/recipe/meta.yaml is required.".to_string(),
        );
        return;
    };
    for placeholder in [
        "PUT_RELEASE_VERSION_HERE",
        "PUT_RELEASE_ARCHIVE_SHA256_HERE",
        "PUT_SPDX_LICENSE_ID_HERE",
    ] {
        if recipe.contains(placeholder) {
            issues.push(format!(
                "FAIL: conda-placeholder: conda recipe still contains {placeholder}."
            ));
        }
    }
    if recipe.contains(concat!("rust", "/Cargo.toml")) {
        issues.push(
            "FAIL: conda-stale-path: conda recipe must build from root Cargo.toml.".to_string(),
        );
    }
}

fn smoke_artifact(executable: &Path) -> Result<(), String> {
    if !executable.exists() {
        return Err(format!("binary does not exist: {}", executable.display()));
    }
    let temp = unique_temp_dir("codebase_graph_smoke")?;
    fs::create_dir_all(temp.join("sample")).map_err(|error| error.to_string())?;
    fs::write(
        temp.join("sample/service.py"),
        "def helper():\n    return 1\n",
    )
    .map_err(|error| error.to_string())?;

    run_checked(executable, ["--help"])?;
    let schema = run_capture(executable, ["schema", "--json"])?;
    serde_json::from_str::<Value>(&schema)
        .map_err(|error| format!("schema did not emit JSON: {error}"))?;
    run_checked(
        executable,
        [
            "install",
            "--repo-root",
            temp.join("sample").to_str().ok_or("invalid temp path")?,
            "--mcp-client",
            "none",
            "--instructions-target",
            "skip",
            "--dry-run",
            "--json",
        ],
    )?;
    run_checked(
        executable,
        [
            "install",
            "--repo-root",
            temp.join("sample").to_str().ok_or("invalid temp path")?,
            "--mcp-client",
            "none",
            "--instructions-target",
            "skip",
            "--json",
        ],
    )?;
    run_checked(
        executable,
        [
            "check-health",
            "--repo-root",
            temp.join("sample").to_str().ok_or("invalid temp path")?,
            "--json",
        ],
    )?;
    run_checked(
        executable,
        [
            "codebase-search",
            "helper",
            "--repo-root",
            temp.join("sample").to_str().ok_or("invalid temp path")?,
            "--no-refresh",
        ],
    )?;
    smoke_mcp_stdio(executable, &temp.join("sample"))?;
    Ok(())
}

fn smoke_wiki_artifact(executable: &Path) -> Result<(), String> {
    if !executable.exists() {
        return Err(format!("binary does not exist: {}", executable.display()));
    }
    let executable = executable.canonicalize().map_err(|error| {
        format!(
            "failed to resolve wiki binary {}: {error}",
            executable.display()
        )
    })?;
    let temp = unique_temp_dir("k_wiki_smoke")?;
    let bundle = temp.join("bundle");
    let concepts = bundle.join("concepts");
    let site = temp.join("site");
    fs::create_dir_all(&concepts).map_err(|error| error.to_string())?;
    fs::write(
        bundle.join("index.md"),
        "---\nokf_version: \"0.1\"\ntitle: Smoke Bundle\n---\n# Smoke Bundle\n",
    )
    .map_err(|error| error.to_string())?;
    fs::write(
        concepts.join("welcome.md"),
        "---\ntype: guide\ntitle: Welcome\ntags: [smoke]\n---\n# Welcome\n\nPackaged wiki smoke.\n",
    )
    .map_err(|error| error.to_string())?;
    let repo_root = temp.join("repo");
    let knowledge_root = repo_root.join("knowledge");
    fs::create_dir_all(&knowledge_root).map_err(|error| error.to_string())?;
    fs::write(
        knowledge_root.join("index.md"),
        "---\nokf_version: \"0.1\"\ntitle: Repo Knowledge\n---\n# Repo Knowledge\n",
    )
    .map_err(|error| error.to_string())?;

    let bundle_text = bundle.to_str().ok_or("invalid bundle path")?;
    let site_text = site.to_str().ok_or("invalid site path")?;
    assert_version_surface(&executable, Path::new("crates/k-wiki/Cargo.toml"))?;
    run_checked_in(&executable, ["--help"], &temp)?;
    run_checked_in(&executable, ["validate", bundle_text, "--json"], &temp)?;
    run_checked_in(
        &executable,
        ["build", bundle_text, "--out", site_text],
        &temp,
    )?;
    assert_wiki_codex_project_install(&executable, &repo_root, &temp)?;
    if !site.join("index.html").is_file() {
        return Err("wiki artifact smoke did not generate index.html".to_string());
    }
    smoke_wiki_http(&executable, &bundle, &temp)?;
    smoke_wiki_mcp_stdio(&executable, &bundle, &temp)?;
    Ok(())
}

fn smoke_wiki_http(executable: &Path, bundle: &Path, current_dir: &Path) -> Result<(), String> {
    let mut child = Command::new(executable)
        .args(["serve"])
        .arg(bundle)
        .args(["--host", "127.0.0.1", "--port", "0"])
        .current_dir(current_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("failed to spawn wiki preview: {error}"))?;

    let result = (|| {
        let stderr = child.stderr.take().ok_or("missing wiki preview stderr")?;
        let mut reader = BufReader::new(stderr);
        let mut line = String::new();
        reader
            .read_line(&mut line)
            .map_err(|error| format!("failed to read wiki preview address: {error}"))?;
        let address = line
            .trim()
            .strip_prefix("Knowledge Wiki preview: http://")
            .ok_or_else(|| format!("wiki preview did not report its address: {line:?}"))?;

        for path in ["/healthz", "/", "/assets/wiki.css"] {
            let response = wiki_http_get(address, path)?;
            if !response.starts_with("HTTP/1.1 200") {
                return Err(format!(
                    "wiki preview returned a non-success response for {path}: {}",
                    response.lines().next().unwrap_or_default()
                ));
            }
        }
        Ok(())
    })();

    let _ = child.kill();
    let _ = child.wait();
    result
}

fn wiki_http_get(address: &str, path: &str) -> Result<String, String> {
    let mut stream = TcpStream::connect(address)
        .map_err(|error| format!("failed to connect to wiki preview: {error}"))?;
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .map_err(|error| error.to_string())?;
    stream
        .set_write_timeout(Some(Duration::from_secs(5)))
        .map_err(|error| error.to_string())?;
    write!(
        stream,
        "GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n"
    )
    .map_err(|error| error.to_string())?;
    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .map_err(|error| error.to_string())?;
    Ok(response)
}

fn smoke_wiki_mcp_stdio(
    executable: &Path,
    bundle: &Path,
    current_dir: &Path,
) -> Result<(), String> {
    let mut child = Command::new(executable)
        .arg("mcp")
        .arg(bundle)
        .current_dir(current_dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("failed to spawn wiki MCP server: {error}"))?;
    {
        let mut stdin = child.stdin.take().ok_or("missing wiki MCP stdin")?;
        writeln!(
            stdin,
            "{}",
            json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize",
                "params": {"protocolVersion": "2025-11-25"}
            })
        )
        .map_err(|error| error.to_string())?;
        writeln!(
            stdin,
            "{}",
            json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {}})
        )
        .map_err(|error| error.to_string())?;
    }

    let output = child
        .wait_with_output()
        .map_err(|error| error.to_string())?;
    if !output.status.success() {
        return Err(format!(
            "wiki MCP smoke failed with status {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let stdout = String::from_utf8(output.stdout).map_err(|error| error.to_string())?;
    if !stdout.contains(r#""name":"Knowledge Wiki""#)
        || !stdout.contains(r#""name":"wiki_get_concept""#)
        || !stdout.contains(r#""name":"wiki_populate_page""#)
        || !stdout.contains(r#""name":"wiki_validate""#)
        || !stdout.contains(r#""name":"wiki_check_links""#)
        || !stdout.contains(r#""name":"wiki_build""#)
    {
        return Err("wiki MCP smoke did not advertise the Knowledge Wiki schema".to_string());
    }
    Ok(())
}

fn assert_version_surface(executable: &Path, manifest: &Path) -> Result<(), String> {
    let expected = cargo_version(manifest)?;
    let output = run_capture_in(executable, ["--version"], Path::new("."))?;
    let stdout = String::from_utf8(output.stdout).map_err(|error| error.to_string())?;
    let actual = stdout.trim();
    let expected_line = format!("k-wiki {expected}");
    if actual != expected_line {
        return Err(format!(
            "unexpected version output: expected {expected_line:?}, got {actual:?}"
        ));
    }
    Ok(())
}

fn assert_wiki_codex_project_install(
    executable: &Path,
    repo_root: &Path,
    current_dir: &Path,
) -> Result<(), String> {
    let home = current_dir.join("isolated-home");
    let codex_home = home.join(".codex");
    let empty_path = current_dir.join("empty-path");
    let legacy_repo = current_dir.join("StructuralFactory");
    let legacy_bundle = legacy_repo.join("knowledge");
    fs::create_dir_all(&codex_home).map_err(|error| error.to_string())?;
    fs::create_dir_all(&empty_path).map_err(|error| error.to_string())?;
    fs::create_dir_all(&legacy_bundle).map_err(|error| error.to_string())?;
    fs::write(
        legacy_bundle.join("index.md"),
        "---\nokf_version: \"0.1\"\ntitle: Legacy Knowledge\n---\n# Legacy Knowledge\n",
    )
    .map_err(|error| error.to_string())?;
    let legacy_bundle = legacy_bundle
        .canonicalize()
        .map_err(|error| format!("failed to canonicalize legacy bundle: {error}"))?;
    fs::write(
        codex_home.join("config.toml"),
        format!(
            "model = \"preserved\"\n\n[mcp_servers.k_wiki]\ncommand = \"legacy-k-wiki\"\nargs = [\"mcp\", {}]\nstartup_timeout_sec = 60\n",
            serde_json::to_string(legacy_bundle.to_string_lossy().as_ref())
                .map_err(|error| error.to_string())?,
        ),
    )
    .map_err(|error| error.to_string())?;
    let home_text = home.to_string_lossy().to_string();
    let codex_home_text = codex_home.to_string_lossy().to_string();
    let empty_path_text = empty_path.to_string_lossy().to_string();
    let executable_text = executable.to_string_lossy().to_string();
    let environment = [
        ("HOME", home_text.as_str()),
        ("CODEX_HOME", codex_home_text.as_str()),
        ("PATH", empty_path_text.as_str()),
        ("K_WIKI_SERVER_COMMAND", executable_text.as_str()),
    ];
    let output = run_capture_in_with_env(
        executable,
        [
            "mcp",
            "install",
            "--client",
            "codex",
            "--scope",
            "project",
            "--repo-root",
            repo_root.to_str().ok_or("invalid repo root path")?,
            "--dry-run",
            "--verify",
        ],
        current_dir,
        &environment,
    )?;
    let stdout = String::from_utf8(output.stdout).map_err(|error| error.to_string())?;
    let payload: Value = serde_json::from_str(stdout.trim()).map_err(|error| {
        format!("dry-run MCP install did not return valid JSON: {error}; stdout: {stdout}")
    })?;
    let method = payload
        .get("method")
        .and_then(Value::as_str)
        .ok_or("dry-run MCP install did not report method")?;
    if method != "file_adapter" {
        return Err(format!(
            "dry-run MCP install expected method=file_adapter, got {method}"
        ));
    }
    let locality = payload
        .get("target_locality")
        .and_then(Value::as_str)
        .ok_or("dry-run MCP install did not report target_locality")?;
    if locality != "repository_local" {
        return Err(format!(
            "dry-run MCP install expected repository_local locality, got {locality}"
        ));
    }
    let expected_path = repo_root
        .canonicalize()
        .map_err(|error| format!("failed to canonicalize repo root for dry-run smoke: {error}"))?
        .join(".codex/config.toml");
    let reported_path = payload
        .get("path")
        .and_then(Value::as_str)
        .ok_or("dry-run MCP install did not report path")?;
    if Path::new(reported_path) != expected_path {
        return Err(format!(
            "dry-run MCP install expected path {}, got {}",
            expected_path.display(),
            reported_path
        ));
    }
    if payload
        .pointer("/verification/reason")
        .and_then(Value::as_str)
        != Some("dry_run")
    {
        return Err("dry-run MCP install did not report verification as skipped".to_string());
    }

    let install_args = [
        "mcp",
        "install",
        "--client",
        "codex",
        "--scope",
        "project",
        "--repo-root",
        repo_root.to_str().ok_or("invalid repo root path")?,
        "--verify",
    ];
    let installed = run_capture_in_with_env(executable, install_args, current_dir, &environment)?;
    let installed_stdout =
        String::from_utf8(installed.stdout).map_err(|error| error.to_string())?;
    let installed: Value = serde_json::from_str(installed_stdout.trim()).map_err(|error| {
        format!("MCP install did not return valid JSON: {error}; stdout: {installed_stdout}")
    })?;
    if !matches!(
        installed.get("action").and_then(Value::as_str),
        Some("created" | "updated")
    ) || installed
        .pointer("/verification/ok")
        .and_then(Value::as_bool)
        != Some(true)
        || installed
            .pointer("/legacy_cleanup/action")
            .and_then(Value::as_str)
            != Some("renamed")
    {
        return Err(format!(
            "packaged MCP install did not update, verify, and preserve the legacy entry: {installed}"
        ));
    }
    let preserved_as = installed
        .pointer("/legacy_cleanup/preserved_as")
        .and_then(Value::as_str)
        .ok_or("packaged MCP install did not report the preserved legacy name")?;
    if !preserved_as.starts_with("k_wiki_structuralfactory_") {
        return Err(format!("unexpected preserved legacy name: {preserved_as}"));
    }
    let local_config = fs::read_to_string(&expected_path).map_err(|error| error.to_string())?;
    let expected_command = format!(
        "command = {}",
        serde_json::to_string(&executable_text).map_err(|error| error.to_string())?
    );
    if !local_config.contains("[mcp_servers.k_wiki]")
        || !local_config
            .lines()
            .any(|line| line.trim() == expected_command)
    {
        return Err("packaged MCP install did not write the repository-local registration".into());
    }
    let shared_config =
        fs::read_to_string(codex_home.join("config.toml")).map_err(|error| error.to_string())?;
    if !shared_config.contains("model = \"preserved\"")
        || shared_config.contains("[mcp_servers.k_wiki]")
        || !shared_config.contains(&format!("[mcp_servers.{preserved_as}]"))
        || !shared_config.contains("command = \"legacy-k-wiki\"")
    {
        return Err("packaged MCP install did not preserve the shared legacy registration".into());
    }

    let repeated = run_capture_in_with_env(executable, install_args, current_dir, &environment)?;
    let repeated_stdout = String::from_utf8(repeated.stdout).map_err(|error| error.to_string())?;
    let repeated: Value = serde_json::from_str(repeated_stdout.trim()).map_err(|error| {
        format!("repeat MCP install did not return valid JSON: {error}; stdout: {repeated_stdout}")
    })?;
    if repeated.get("action").and_then(Value::as_str) != Some("unchanged")
        || repeated
            .pointer("/verification/ok")
            .and_then(Value::as_bool)
            != Some(true)
    {
        return Err(format!(
            "packaged MCP install was not idempotent: {repeated}"
        ));
    }
    Ok(())
}

fn smoke_mcp_stdio(executable: &Path, repo_root: &Path) -> Result<(), String> {
    let mut child = Command::new(executable)
        .args(["mcp", "start", "--repo-root"])
        .arg(repo_root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("failed to spawn MCP server: {error}"))?;
    {
        let mut stdin = child.stdin.take().ok_or("missing MCP stdin")?;
        let initialize = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2025-11-25",
                "capabilities": {},
                "clientInfo": {"name": "xtask-smoke", "version": "0"}
            }
        });
        writeln!(stdin, "{}", initialize).map_err(|error| error.to_string())?;
        let tools = json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {}});
        writeln!(stdin, "{}", tools).map_err(|error| error.to_string())?;
    }
    let mut output = String::new();
    if let Some(mut stdout) = child.stdout.take() {
        stdout.read_to_string(&mut output).ok();
    }
    let status = child.wait().map_err(|error| error.to_string())?;
    if !status.success() && output.is_empty() {
        return Err(format!("MCP stdio smoke failed with status {status}"));
    }
    if !output.contains("graph_search") {
        return Err("MCP stdio smoke did not list graph_search".to_string());
    }
    Ok(())
}

fn verify_release_version(tag: &str) -> Result<(), String> {
    let expected = tag
        .strip_prefix('v')
        .ok_or_else(|| format!("release tag must match vX.Y.Z, got {tag:?}"))?;
    if expected.split('.').count() != 3
        || !expected
            .chars()
            .all(|item| item.is_ascii_digit() || item == '.')
    {
        return Err(format!("release tag must match vX.Y.Z, got {tag:?}"));
    }
    let actual = cargo_version(Path::new("Cargo.toml"))?;
    if actual != expected {
        return Err(format!(
            "Cargo package version {actual} does not match release tag {tag}"
        ));
    }
    let wiki = cargo_version(Path::new("crates/k-wiki/Cargo.toml"))?;
    if wiki != expected {
        return Err(format!(
            "Knowledge Wiki package version {wiki} does not match release tag {tag}"
        ));
    }
    let dependency = dependency_version(Path::new("crates/k-wiki/Cargo.toml"), "codebase-graph")?;
    if dependency != expected {
        return Err(format!(
            "Knowledge Wiki depends on codebase-graph {dependency} but release tag is {tag}"
        ));
    }
    println!("{actual}");
    Ok(())
}

fn cargo_version(manifest: &Path) -> Result<String, String> {
    let cargo = fs::read_to_string(manifest).map_err(|error| error.to_string())?;
    for line in cargo.lines() {
        let line = line.trim();
        if let Some(value) = line.strip_prefix("version = ") {
            return Ok(value.trim_matches('"').to_string());
        }
    }
    Err(format!(
        "{} does not contain package version",
        manifest.display()
    ))
}

fn dependency_version(manifest: &Path, dependency: &str) -> Result<String, String> {
    let cargo = fs::read_to_string(manifest).map_err(|error| error.to_string())?;
    for line in cargo.lines() {
        let line = line.trim();
        if line.starts_with(&format!("{dependency} =")) {
            if let Some(version_index) = line.find("version = ") {
                let value = &line[(version_index + "version = ".len())..];
                if let Some(first_quote) = value.find('"') {
                    let rest = &value[(first_quote + 1)..];
                    if let Some(end_quote) = rest.find('"') {
                        return Ok(rest[..end_quote].to_string());
                    }
                }
            }
        }
    }
    Err(format!(
        "{} does not contain dependency version for {}",
        manifest.display(),
        dependency
    ))
}

fn run_checked<'a, I>(executable: &Path, args: I) -> Result<(), String>
where
    I: IntoIterator<Item = &'a str>,
{
    let output = Command::new(executable)
        .args(args)
        .output()
        .map_err(|error| error.to_string())?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "command failed with status {}: {}\nstdout:\n{}\nstderr:\n{}",
            output.status,
            executable.display(),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ))
    }
}

fn run_capture_in<'a, I>(
    executable: &Path,
    args: I,
    current_dir: &Path,
) -> Result<std::process::Output, String>
where
    I: IntoIterator<Item = &'a str>,
{
    Command::new(executable)
        .args(args)
        .current_dir(current_dir)
        .output()
        .map_err(|error| error.to_string())
        .and_then(|output| {
            if output.status.success() {
                Ok(output)
            } else {
                Err(format!(
                    "command failed with status {}: {}\nstdout:\n{}\nstderr:\n{}",
                    output.status,
                    executable.display(),
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr)
                ))
            }
        })
}

fn run_capture_in_with_env<'a, I>(
    executable: &Path,
    args: I,
    current_dir: &Path,
    env_pairs: &[(&str, &str)],
) -> Result<std::process::Output, String>
where
    I: IntoIterator<Item = &'a str>,
{
    let mut command = Command::new(executable);
    command.args(args).current_dir(current_dir);
    for (key, value) in env_pairs {
        command.env(key, value);
    }
    command
        .output()
        .map_err(|error| error.to_string())
        .and_then(|output| {
            if output.status.success() {
                Ok(output)
            } else {
                Err(format!(
                    "command failed with status {}: {}\nstdout:\n{}\nstderr:\n{}",
                    output.status,
                    executable.display(),
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr)
                ))
            }
        })
}

fn run_checked_in<'a, I>(executable: &Path, args: I, current_dir: &Path) -> Result<(), String>
where
    I: IntoIterator<Item = &'a str>,
{
    let output = Command::new(executable)
        .args(args)
        .current_dir(current_dir)
        .output()
        .map_err(|error| error.to_string())?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "command failed with status {}: {}\nstdout:\n{}\nstderr:\n{}",
            output.status,
            executable.display(),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ))
    }
}

fn run_capture<'a, I>(executable: &Path, args: I) -> Result<String, String>
where
    I: IntoIterator<Item = &'a str>,
{
    let output = Command::new(executable)
        .args(args)
        .output()
        .map_err(|error| error.to_string())?;
    if !output.status.success() {
        return Err(format!(
            "command failed with status {}: {}\nstdout:\n{}\nstderr:\n{}",
            output.status,
            executable.display(),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    String::from_utf8(output.stdout).map_err(|error| error.to_string())
}

fn unique_temp_dir(prefix: &str) -> Result<PathBuf, String> {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_nanos();
    let path = env::temp_dir().join(format!("{prefix}_{nanos}"));
    fs::create_dir_all(&path).map_err(|error| error.to_string())?;
    Ok(path)
}

fn files_under(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let Ok(entries) = fs::read_dir(root) else {
        return files;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = path
            .file_name()
            .and_then(|item| item.to_str())
            .unwrap_or_default();
        if name == ".git" || name == "target" || name == ".codebaseGraph" || name == ".kwiki" {
            continue;
        }
        if path.is_dir() {
            files.extend(files_under(&path));
        } else {
            files.push(path);
        }
    }
    files
}

#[cfg(test)]
mod tests {
    use super::{
        check_workflow_policy, create_native_archive, extract_and_validate_native_archive,
        native_test_command, release_version_from_tag, sha256_file, validate_commit_sha,
        validate_native_artifact_set, workflow_yaml_error, NativeProvenance, NativeTarget,
        YamlValue, NATIVE_TARGETS,
    };
    use std::fs;
    use std::path::Path;

    #[test]
    fn workflow_yaml_validation_rejects_unindented_heredoc_body() {
        let invalid =
            "jobs:\n  build:\n    steps:\n      - run: |\n          cat <<'EOF'\nbody\nEOF\n";
        let valid = "jobs:\n  build:\n    steps:\n      - run: |\n          cat <<'EOF'\n          body\n          EOF\n";

        assert!(workflow_yaml_error(invalid).is_some());
        assert!(workflow_yaml_error(valid).is_none());
    }

    #[test]
    fn native_target_contract_maps_names_and_binaries() {
        assert_eq!(
            NativeTarget::parse("linux-x86_64").unwrap().binary_names(),
            ("codebase-graph", "k-wiki")
        );
        assert_eq!(
            NativeTarget::parse("windows-x86_64")
                .unwrap()
                .binary_names(),
            ("codebase-graph.exe", "k-wiki.exe")
        );
        assert!(NativeTarget::parse("linux-arm64").is_err());
    }

    #[test]
    fn windows_native_tests_forward_bundled_extensions() {
        let windows = native_test_command(NativeTarget::WindowsX86_64);
        assert!(windows.contains(&"--release"));
        assert!(windows.contains(
            &"codebase-graph/bundled-windows-extensions,k-wiki/bundled-windows-extensions"
        ));

        let linux = native_test_command(NativeTarget::LinuxX86_64);
        assert!(!linux.contains(&"--features"));
        assert!(!linux.contains(&"--release"));
    }

    #[test]
    fn release_identity_validation_is_strict() {
        assert_eq!(release_version_from_tag("v1.2.3").unwrap(), "1.2.3");
        assert!(release_version_from_tag("1.2.3").is_err());
        assert!(release_version_from_tag("v1.2").is_err());
        assert!(validate_commit_sha("0123456789abcdef0123456789abcdef01234567").is_ok());
        assert!(validate_commit_sha("abc").is_err());
    }

    #[test]
    fn archive_contract_rejects_unexpected_entries() {
        let temp = super::unique_temp_dir("xtask_archive_contract").unwrap();
        let package = temp.join("package");
        fs::create_dir_all(&package).unwrap();
        for name in [
            "codebase-graph",
            "k-wiki",
            "checksums.txt",
            "install.sh",
            "install.ps1",
        ] {
            fs::write(package.join(name), name).unwrap();
        }
        let archive = temp.join("artifact.tar.gz");
        create_native_archive(&package, &archive, NativeTarget::LinuxX86_64).unwrap();
        let extracted = temp.join("extracted");
        let error =
            extract_and_validate_native_archive(&archive, NativeTarget::LinuxX86_64, &extracted)
                .unwrap_err();
        assert!(error.contains("checksums.txt") || error.contains("SHA256"));
        fs::remove_dir_all(temp).unwrap();
    }

    #[test]
    fn complete_artifact_set_rejects_mixed_provenance() {
        let temp = super::unique_temp_dir("xtask_artifact_set").unwrap();
        let version = "1.2.3";
        let commit_sha = "0123456789abcdef0123456789abcdef01234567";
        for target in NATIVE_TARGETS {
            write_test_artifact(&temp, target, version, commit_sha);
        }
        validate_native_artifact_set(&temp, version, commit_sha).unwrap();

        let provenance_path = temp.join("native-linux-x86_64/provenance.json");
        let mut provenance: NativeProvenance =
            serde_json::from_slice(&fs::read(&provenance_path).unwrap()).unwrap();
        provenance.commit_sha = "ffffffffffffffffffffffffffffffffffffffff".to_string();
        fs::write(
            &provenance_path,
            serde_json::to_vec_pretty(&provenance).unwrap(),
        )
        .unwrap();
        let error = validate_native_artifact_set(&temp, version, commit_sha).unwrap_err();
        assert!(error.contains("mixed release provenance"));
        fs::remove_dir_all(temp).unwrap();
    }

    fn write_test_artifact(root: &Path, target: NativeTarget, version: &str, commit_sha: &str) {
        let artifact_dir = root.join(format!("native-{}", target.as_str()));
        let package = artifact_dir.join("package");
        fs::create_dir_all(&package).unwrap();
        let (graph_name, wiki_name) = target.binary_names();
        fs::write(package.join(graph_name), b"graph").unwrap();
        fs::write(package.join(wiki_name), b"wiki").unwrap();
        fs::write(package.join("install.sh"), b"installer").unwrap();
        fs::write(package.join("install.ps1"), b"installer").unwrap();
        fs::write(
            package.join("checksums.txt"),
            format!(
                "{}  {graph_name}\n{}  {wiki_name}\n",
                sha256_file(&package.join(graph_name)).unwrap(),
                sha256_file(&package.join(wiki_name)).unwrap()
            ),
        )
        .unwrap();
        let archive_name = target.archive_name(version);
        let archive_path = artifact_dir.join(&archive_name);
        create_native_archive(&package, &archive_path, target).unwrap();
        fs::remove_dir_all(&package).unwrap();
        let archive_sha256 = sha256_file(&archive_path).unwrap();
        fs::write(
            artifact_dir.join(format!("{archive_name}.sha256")),
            format!("{archive_sha256}  {archive_name}\n"),
        )
        .unwrap();
        let provenance = NativeProvenance {
            schema_version: 1,
            commit_sha: commit_sha.to_string(),
            version: version.to_string(),
            target: target.as_str().to_string(),
            archive: archive_name,
            archive_sha256,
        };
        fs::write(
            artifact_dir.join("provenance.json"),
            serde_json::to_vec_pretty(&provenance).unwrap(),
        )
        .unwrap();
    }

    fn valid_ci_workflow() -> YamlValue {
        yaml_serde::from_str(
            "on:\n  pull_request: {branches: [main]}\n  push: {branches: [main]}\njobs:\n  native: {uses: './.github/workflows/native.yml'}\n  required:\n    if: '${{ always() }}'\n    needs: [fmt, clippy, supply-chain, publish-dry-run, native]\n",
        )
        .unwrap()
    }

    fn valid_native_workflow() -> YamlValue {
        yaml_serde::from_str(
            "on:\n  workflow_call:\n    inputs:\n      target: {}\n      source-sha: {}\n      run-tests: {}\n      upload-artifact: {}\n      artifact-retention-days: {}\njobs:\n  native:\n    permissions: {contents: read}\n",
        )
        .unwrap()
    }

    fn valid_release_workflow_text() -> String {
        r#"on:
  workflow_run:
    workflows: [CI]
    types: [completed]
    branches: [main]
  workflow_dispatch: {}
concurrency:
  group: release-${{ github.event_name == 'workflow_run' && 'main' || inputs.publish-existing-tag }}
  cancel-in-progress: false
jobs:
  release-please:
    if: ${{ github.event_name == 'workflow_run' && github.event.workflow_run.event == 'push' && github.event.workflow_run.head_branch == 'main' && github.event.workflow_run.conclusion == 'success' }}
    outputs:
      ci-run-id: ${{ steps.trigger.outputs.ci-run-id }}
      ci-head-sha: ${{ steps.trigger.outputs.ci-head-sha }}
    steps:
      - id: trigger
        env:
          CI_RUN_ID: ${{ github.event.workflow_run.id }}
          CI_HEAD_SHA: ${{ github.event.workflow_run.head_sha }}
        run: |
          main_sha="$(gh api "repos/$GITHUB_REPOSITORY/git/ref/heads/main" --jq '.object.sha')"
          echo 'current-tip=true'
      - id: release
        uses: googleapis/release-please-action@45996ed1f6d02564a971a2fa1b5860e934307cf7
      - name: Recheck current main tip after release-please
        env:
          RELEASE_SHA: ${{ steps.release.outputs.sha }}
        run: |
          main_sha=current
          test "$main_sha" = "$CI_HEAD_SHA"
  release-target:
    outputs:
      ci-run-id: ${{ steps.resolve.outputs.ci-run-id }}
    steps:
      - id: resolve
        env:
          RELEASE_CI_RUN_ID: ${{ needs.release-please.outputs.ci-run-id }}
          RELEASE_CI_HEAD_SHA: ${{ needs.release-please.outputs.ci-head-sha }}
        run: |
          tag_sha=tag
          source_sha=source
  ci-gate:
    permissions: {actions: read}
    outputs: {ci-run-id: x}
    steps:
      - id: wait
        env:
          REQUESTED_CI_RUN_ID: ${{ needs.release-target.outputs.ci-run-id }}
          AUTOMATIC: ${{ needs.release-target.outputs.automatic }}
        run: |
          if [[ "$AUTOMATIC" == 'true' ]]; then
            gh api "repos/$GITHUB_REPOSITORY/actions/runs/$REQUESTED_CI_RUN_ID"
            jq '.path == ".github/workflows/ci.yml" and .event == "push" and .head_branch == "main" and .status == "completed" and .conclusion == "success"'
          fi
  select-artifacts:
    steps:
      - id: select
        env:
          AUTOMATIC: ${{ needs.release-target.outputs.automatic }}
        run: |
          if [[ "$AUTOMATIC" == 'false' && "$ARTIFACT_SOURCE" == 'rebuild-if-missing' ]]; then
            echo rebuild
          fi
  rebuild-artifacts: {uses: './.github/workflows/native.yml'}
  publish-release-assets:
    permissions: {contents: write}
    environment: {name: cargo}
    steps: [{run: 'gh release upload'}]
  publish-crate:
    steps:
      - {run: 'cargo publish --dry-run --locked'}
      - {run: 'cargo publish --locked'}
"#
        .to_string()
    }

    fn workflow_policy_issues(release_text: &str) -> Vec<String> {
        let release = yaml_serde::from_str(release_text).unwrap();
        let mut issues = Vec::new();
        check_workflow_policy(
            &valid_ci_workflow(),
            &valid_native_workflow(),
            &release,
            &mut issues,
        );
        issues
    }

    #[test]
    fn workflow_policy_requires_ci_completion_release_trigger() {
        let issues = workflow_policy_issues(&valid_release_workflow_text());
        assert!(issues.is_empty(), "{issues:?}");
    }

    #[test]
    fn workflow_policy_rejects_direct_release_push_trigger() {
        let broken = valid_release_workflow_text().replace(
            "  workflow_dispatch: {}",
            "  push: {branches: [main]}\n  workflow_dispatch: {}",
        );
        let issues = workflow_policy_issues(&broken);
        assert!(
            issues
                .iter()
                .any(|issue| issue.contains("release-direct-push-trigger")),
            "{issues:?}"
        );
    }

    #[test]
    fn workflow_policy_rejects_missing_ci_success_guard() {
        let broken = valid_release_workflow_text().replace(
            "github.event.workflow_run.conclusion == 'success'",
            "github.event.workflow_run.conclusion != 'cancelled'",
        );
        let issues = workflow_policy_issues(&broken);
        assert!(
            issues
                .iter()
                .any(|issue| issue.contains("release-success-guard-missing")),
            "{issues:?}"
        );
    }

    #[test]
    fn workflow_policy_rejects_github_sha_for_release_identity() {
        let broken = valid_release_workflow_text().replace(
            "${{ github.event.workflow_run.head_sha }}",
            "${{ github.sha }}",
        );
        let issues = workflow_policy_issues(&broken);
        assert!(
            issues
                .iter()
                .any(|issue| issue.contains("release-github-sha-forbidden")),
            "{issues:?}"
        );
    }

    #[test]
    fn workflow_policy_rejects_missing_triggering_run_binding() {
        let broken = valid_release_workflow_text().replace(
            "${{ github.event.workflow_run.id }}",
            "${{ github.run_id }}",
        );
        let issues = workflow_policy_issues(&broken);
        assert!(
            issues
                .iter()
                .any(|issue| issue.contains("release-trigger-binding-missing")),
            "{issues:?}"
        );
    }

    #[test]
    fn workflow_policy_rejects_missing_post_action_freshness_check() {
        let broken = valid_release_workflow_text().replace(
            "Recheck current main tip after release-please",
            "Do something unrelated",
        );
        let issues = workflow_policy_issues(&broken);
        assert!(
            issues
                .iter()
                .any(|issue| issue.contains("release-post-action-freshness-missing")),
            "{issues:?}"
        );
    }

    #[test]
    fn workflow_policy_rejects_automatic_rebuild_recovery() {
        let broken = valid_release_workflow_text()
            .replace("\"$AUTOMATIC\" == 'false'", "\"$AUTOMATIC\" == 'true'");
        let issues = workflow_policy_issues(&broken);
        assert!(
            issues
                .iter()
                .any(|issue| issue.contains("release-automatic-rebuild")),
            "{issues:?}"
        );
    }
}
