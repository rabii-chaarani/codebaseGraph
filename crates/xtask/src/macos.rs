use flate2::read::GzDecoder;
use std::collections::BTreeSet;
use std::env;
use std::fs::{self, File};
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::process::Command;

const OPENSSL_VERSION: &str = "3.6.2";
const OPENSSL_SHA256: &str = "aaf51a1fe064384f811daeaeb4ec4dce7340ec8bd893027eee676af31e83a04f";
const OPENSSL_SOURCE_URL: &str =
    "https://github.com/openssl/openssl/releases/download/openssl-3.6.2/openssl-3.6.2.tar.gz";
const DEPLOYMENT_TARGET: &str = "13.3";
const SOURCE_TOP: &str = "openssl-3.6.2";
const SSL_SOURCE_NAME: &str = "libssl.3.dylib";
const CRYPTO_SOURCE_NAME: &str = "libcrypto.3.dylib";

pub const PAYLOAD_FILES: [&str; 3] = [
    "libcodebasegraph-ssl-3.6.2.dylib",
    "libcodebasegraph-crypto-3.6.2.dylib",
    "openssl-3.6.2-LICENSE.txt",
];

/// Fetch, verify, and build the approved OpenSSL release into a fresh private root.
pub fn prepare_openssl(root: &Path) -> Result<PathBuf, String> {
    ensure_fresh_root(root)?;
    let archive = root.join("openssl-3.6.2.tar.gz");
    let mut download = Command::new("curl");
    download.args([
        "--fail",
        "--location",
        "--silent",
        "--show-error",
        "--output",
    ]);
    download.arg(&archive).arg(OPENSSL_SOURCE_URL);
    run(&mut download, "OpenSSL source download")?;

    let actual_sha = super::sha256_file(&archive)
        .map_err(|error| format!("failed to hash downloaded OpenSSL source: {error}"))?;
    if actual_sha != OPENSSL_SHA256 {
        return Err(format!(
            "OpenSSL {OPENSSL_VERSION} source SHA-256 mismatch: expected {OPENSSL_SHA256}, got {actual_sha}"
        ));
    }

    let source = root.join("source");
    extract_verified_source(&archive, root, &source)?;
    let license = source.join("LICENSE.txt");
    let license_metadata = fs::symlink_metadata(&license)
        .map_err(|error| format!("verified OpenSSL source has no LICENSE.txt: {error}"))?;
    if !license_metadata.file_type().is_file() || license_metadata.len() == 0 {
        return Err("verified OpenSSL LICENSE.txt is missing or is not a regular file".into());
    }
    super::copy_file(&license, root.join("LICENSE.txt"))
        .map_err(|error| format!("failed to copy verified OpenSSL license: {error}"))?;

    let library_dir = root.join("build");
    let openssl_dir = root.join("openssl-config");
    let mut configure = Command::new("perl");
    configure.current_dir(&source).arg("Configure").args([
        "darwin64-arm64-cc",
        "shared",
        "no-tests",
        "no-apps",
        "no-module",
    ]);
    configure
        .arg(format!("--prefix={}", library_dir.display()))
        .arg(format!("--libdir={}", library_dir.display()))
        .arg(format!("--openssldir={}", openssl_dir.display()));
    clean_openssl_build_environment(&mut configure);
    configure
        .env("CC", "clang")
        .env("MACOSX_DEPLOYMENT_TARGET", DEPLOYMENT_TARGET);
    run(&mut configure, "OpenSSL arm64 configuration")?;

    let jobs = std::thread::available_parallelism()
        .map(usize::from)
        .unwrap_or(1);
    let mut build = Command::new("make");
    build
        .current_dir(&source)
        .arg(format!("-j{jobs}"))
        .arg("install_sw");
    clean_openssl_build_environment(&mut build);
    build.env("MACOSX_DEPLOYMENT_TARGET", DEPLOYMENT_TARGET);
    run(&mut build, "OpenSSL shared-library build and install")?;

    for name in [SSL_SOURCE_NAME, CRYPTO_SOURCE_NAME] {
        let path = library_dir.join(name);
        let metadata = fs::symlink_metadata(&path).map_err(|error| {
            format!("OpenSSL build did not produce {}: {error}", path.display())
        })?;
        if !metadata.file_type().is_file() || metadata.len() == 0 {
            return Err(format!(
                "OpenSSL build output is not a regular library: {}",
                path.display()
            ));
        }
    }
    for (link, target) in [
        ("libssl.dylib", SSL_SOURCE_NAME),
        ("libcrypto.dylib", CRYPTO_SOURCE_NAME),
    ] {
        let path = library_dir.join(link);
        let metadata = fs::symlink_metadata(&path).map_err(|error| {
            format!(
                "OpenSSL build did not produce linker symlink {}: {error}",
                path.display()
            )
        })?;
        if !metadata.file_type().is_symlink()
            || fs::read_link(&path).map_err(|error| error.to_string())? != Path::new(target)
        {
            return Err(format!(
                "OpenSSL linker symlink is invalid: {}",
                path.display()
            ));
        }
    }
    Ok(library_dir)
}

/// Copy and relocate the two OpenSSL dylibs beside the already-staged binaries.
pub fn stage_runtime(library_dir: &Path, package: &Path) -> Result<(), String> {
    if !package.is_dir() {
        return Err(format!(
            "macOS package directory is missing: {}",
            package.display()
        ));
    }
    let (graph, wiki) = executable_paths(package)?;
    let source_license = library_dir
        .parent()
        .ok_or_else(|| "OpenSSL library directory has no parent for LICENSE.txt".to_string())?
        .join("LICENSE.txt");
    copy_regular_file(
        &library_dir.join(SSL_SOURCE_NAME),
        &package.join(PAYLOAD_FILES[0]),
    )?;
    copy_regular_file(
        &library_dir.join(CRYPTO_SOURCE_NAME),
        &package.join(PAYLOAD_FILES[1]),
    )?;
    copy_regular_file(&source_license, &package.join(PAYLOAD_FILES[2]))?;

    let ssl = package.join(PAYLOAD_FILES[0]);
    let crypto = package.join(PAYLOAD_FILES[1]);
    let graph_dependencies = original_openssl_dependencies(&graph, true)?;
    let wiki_dependencies = original_openssl_dependencies(&wiki, true)?;
    let ssl_dependencies = original_openssl_dependencies(&ssl, false)?;
    let crypto_dependencies = original_openssl_dependencies(&crypto, false)?;
    if !crypto_dependencies.is_empty() {
        return Err(format!(
            "OpenSSL crypto library has unexpected non-system dependencies: {crypto_dependencies:?}"
        ));
    }

    for (binary, dependencies) in [(&graph, graph_dependencies), (&wiki, wiki_dependencies)] {
        for (name, old_path) in dependencies {
            let staged_name = if name == SSL_SOURCE_NAME {
                PAYLOAD_FILES[0]
            } else {
                PAYLOAD_FILES[1]
            };
            change_dependency(
                binary,
                &old_path,
                &format!("@executable_path/{staged_name}"),
            )?;
        }
    }
    change_dependency(
        &ssl,
        &ssl_dependencies
            .get(CRYPTO_SOURCE_NAME)
            .ok_or_else(|| "OpenSSL ssl library does not load libcrypto.3.dylib".to_string())?
            .clone(),
        &format!("@loader_path/{}", PAYLOAD_FILES[1]),
    )?;
    set_dylib_id(&ssl, &format!("@loader_path/{}", PAYLOAD_FILES[0]))?;
    set_dylib_id(&crypto, &format!("@loader_path/{}", PAYLOAD_FILES[1]))?;

    for path in [&graph, &wiki, &ssl, &crypto] {
        for rpath in read_rpaths(path)? {
            let mut command = Command::new("install_name_tool");
            command.args(["-delete_rpath", &rpath]).arg(path);
            run(
                &mut command,
                &format!("remove LC_RPATH from {}", path.display()),
            )?;
        }
    }
    for path in [&crypto, &ssl, &graph, &wiki] {
        let mut command = Command::new("codesign");
        command.args(["--force", "--sign", "-"]).arg(path);
        run(&mut command, &format!("ad-hoc sign {}", path.display()))?;
    }
    validate_package(package)
}

/// Audit a final package on the matching native macOS ARM64 build host.
pub fn validate_package(package: &Path) -> Result<(), String> {
    if env::consts::OS != "macos" || env::consts::ARCH != "aarch64" {
        return Err(format!(
            "macOS package audit requires a native macOS/aarch64 host, current host is {}/{}",
            env::consts::OS,
            env::consts::ARCH
        ));
    }
    if !package.is_dir() {
        return Err(format!(
            "macOS package directory is missing: {}",
            package.display()
        ));
    }
    let (graph, wiki) = executable_paths(package)?;
    let ssl = package.join(PAYLOAD_FILES[0]);
    let crypto = package.join(PAYLOAD_FILES[1]);
    let license = package.join(PAYLOAD_FILES[2]);
    for path in [&ssl, &crypto, &license] {
        let metadata = fs::symlink_metadata(path).map_err(|error| {
            format!("missing macOS package payload {}: {error}", path.display())
        })?;
        if !metadata.file_type().is_file() || metadata.len() == 0 {
            return Err(format!("invalid macOS package payload: {}", path.display()));
        }
    }

    for path in [&crypto, &ssl, &graph, &wiki] {
        verify_signature(path)?;
    }
    audit_macho(&crypto, MachOKind::Crypto)?;
    audit_macho(&ssl, MachOKind::Ssl)?;
    audit_macho(&graph, MachOKind::Executable)?;
    audit_macho(&wiki, MachOKind::Executable)?;
    Ok(())
}

#[derive(Clone, Copy)]
enum MachOKind {
    Executable,
    Ssl,
    Crypto,
}

fn executable_paths(package: &Path) -> Result<(PathBuf, PathBuf), String> {
    let graph = package.join("codebase-graph");
    let wiki = package.join("k-wiki");
    for path in [&graph, &wiki] {
        let metadata = fs::symlink_metadata(path).map_err(|error| {
            format!(
                "missing staged macOS executable {}: {error}",
                path.display()
            )
        })?;
        if !metadata.file_type().is_file() || metadata.len() == 0 {
            return Err(format!(
                "staged macOS executable is invalid: {}",
                path.display()
            ));
        }
    }
    Ok((graph, wiki))
}

fn audit_macho(path: &Path, kind: MachOKind) -> Result<(), String> {
    let archs = output(
        Command::new("lipo").arg("-archs").arg(path),
        "inspect Mach-O architecture",
    )?;
    let archs: Vec<&str> = archs.split_whitespace().collect();
    if archs != ["arm64"] {
        return Err(format!(
            "{} must contain only arm64, found {archs:?}",
            path.display()
        ));
    }
    let load_commands = output(
        Command::new("otool").arg("-l").arg(path),
        "inspect Mach-O load commands",
    )?;
    let minimum = parse_macos_minimum(&load_commands)?;
    if minimum > (13, 3, 0) {
        return Err(format!(
            "{} has macOS minimum {}.{}.{} above 13.3",
            path.display(),
            minimum.0,
            minimum.1,
            minimum.2
        ));
    }
    let rpaths = parse_rpaths(&load_commands)?;
    if !rpaths.is_empty() {
        return Err(format!(
            "{} retains LC_RPATH entries: {rpaths:?}",
            path.display()
        ));
    }

    let dependencies = read_dependencies(path, matches!(kind, MachOKind::Ssl | MachOKind::Crypto))?;
    let expected = match kind {
        MachOKind::Executable => vec![
            format!("@executable_path/{}", PAYLOAD_FILES[0]),
            format!("@executable_path/{}", PAYLOAD_FILES[1]),
        ],
        MachOKind::Ssl => vec![format!("@loader_path/{}", PAYLOAD_FILES[1])],
        MachOKind::Crypto => Vec::new(),
    };
    let actual: Vec<String> = dependencies
        .into_iter()
        .filter(|dependency| !is_apple_system_library(dependency))
        .collect();
    let actual_set: BTreeSet<&str> = actual.iter().map(String::as_str).collect();
    let expected_set: BTreeSet<&str> = expected.iter().map(String::as_str).collect();
    if actual.len() != actual_set.len() || actual_set != expected_set {
        return Err(format!(
            "{} has unexpected non-system dependencies: expected {expected:?}, got {actual:?}",
            path.display()
        ));
    }
    if matches!(kind, MachOKind::Ssl | MachOKind::Crypto) {
        let id = read_dylib_id(path)?;
        let expected_id = if matches!(kind, MachOKind::Ssl) {
            format!("@loader_path/{}", PAYLOAD_FILES[0])
        } else {
            format!("@loader_path/{}", PAYLOAD_FILES[1])
        };
        if id != expected_id {
            return Err(format!(
                "{} has unexpected dylib ID: expected {expected_id}, got {id}",
                path.display()
            ));
        }
    }
    Ok(())
}

fn original_openssl_dependencies(
    path: &Path,
    require_both: bool,
) -> Result<std::collections::BTreeMap<&'static str, String>, String> {
    let mut openssl = std::collections::BTreeMap::new();
    for dependency in read_dependencies(path, !require_both)? {
        if is_apple_system_library(&dependency) {
            continue;
        }
        let basename = Path::new(&dependency)
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| {
                format!(
                    "invalid non-system dependency {dependency:?} in {}",
                    path.display()
                )
            })?;
        let expected_name = match basename {
            SSL_SOURCE_NAME => SSL_SOURCE_NAME,
            CRYPTO_SOURCE_NAME => CRYPTO_SOURCE_NAME,
            _ => {
                return Err(format!(
                    "{} has unexpected non-system dependency {dependency:?}",
                    path.display()
                ));
            }
        };
        if openssl.insert(expected_name, dependency.clone()).is_some() {
            return Err(format!(
                "{} loads {expected_name} more than once",
                path.display()
            ));
        }
    }
    if require_both
        && (openssl.len() != 2
            || !openssl.contains_key(SSL_SOURCE_NAME)
            || !openssl.contains_key(CRYPTO_SOURCE_NAME))
    {
        return Err(format!(
            "{} must load exactly one libssl.3.dylib and libcrypto.3.dylib, found {openssl:?}",
            path.display()
        ));
    }
    if !require_both
        && path
            .file_name()
            .is_some_and(|name| name == PAYLOAD_FILES[0])
        && (openssl.len() != 1 || !openssl.contains_key(CRYPTO_SOURCE_NAME))
    {
        return Err(format!(
            "{} must load exactly libcrypto.3.dylib and no other non-system dependency, found {openssl:?}",
            path.display()
        ));
    }
    if !require_both
        && path
            .file_name()
            .is_some_and(|name| name == PAYLOAD_FILES[1])
        && !openssl.is_empty()
    {
        return Err(format!(
            "{} has unexpected OpenSSL dependencies: {openssl:?}",
            path.display()
        ));
    }
    Ok(openssl)
}

fn change_dependency(path: &Path, old: &str, new: &str) -> Result<(), String> {
    let mut command = Command::new("install_name_tool");
    command.args(["-change", old, new]).arg(path);
    run(
        &mut command,
        &format!("rewrite dependency in {}", path.display()),
    )
}

fn set_dylib_id(path: &Path, id: &str) -> Result<(), String> {
    let mut command = Command::new("install_name_tool");
    command.args(["-id", id]).arg(path);
    run(
        &mut command,
        &format!("set dylib ID for {}", path.display()),
    )
}

fn read_dependencies(path: &Path, has_dylib_id: bool) -> Result<Vec<String>, String> {
    let dylib_id = if has_dylib_id {
        Some(read_dylib_id(path)?)
    } else {
        None
    };
    let text = output(
        Command::new("otool").arg("-L").arg(path),
        "inspect Mach-O dependencies",
    )?;
    parse_dependencies(&text, dylib_id.as_deref())
}

fn parse_dependencies(text: &str, dylib_id: Option<&str>) -> Result<Vec<String>, String> {
    let mut lines = text.lines();
    let header = lines
        .next()
        .ok_or_else(|| "otool -L returned no header".to_string())?;
    if header.trim().is_empty() {
        return Err("otool -L returned an empty header".into());
    }
    let mut dependencies = Vec::new();
    for line in lines {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let dependency = line
            .split_once(" (compatibility version ")
            .map_or(line, |(path, _)| path)
            .trim();
        if dependency.is_empty() {
            return Err("otool -L returned an empty dependency".into());
        }
        if dylib_id.is_some_and(|id| dependency == id) {
            continue;
        }
        dependencies.push(dependency.to_string());
    }
    if let Some(dylib_id) = dylib_id {
        let self_entries = text
            .lines()
            .skip(1)
            .filter(|line| {
                line.trim()
                    .split_once(" (compatibility version ")
                    .map_or(line.trim(), |(path, _)| path.trim())
                    == dylib_id
            })
            .count();
        if self_entries != 1 {
            return Err(format!(
                "otool -L must report exactly one matching LC_ID_DYLIB {dylib_id:?}, found {self_entries}"
            ));
        }
    }
    Ok(dependencies)
}

fn read_dylib_id(path: &Path) -> Result<String, String> {
    let text = output(
        Command::new("otool").arg("-D").arg(path),
        "inspect dylib ID",
    )?;
    let ids: Vec<&str> = text
        .lines()
        .skip(1)
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    if ids.len() != 1 {
        return Err(format!(
            "{} must have exactly one dylib ID, got {ids:?}",
            path.display()
        ));
    }
    Ok(ids[0].to_string())
}

fn read_rpaths(path: &Path) -> Result<Vec<String>, String> {
    let text = output(
        Command::new("otool").arg("-l").arg(path),
        "inspect Mach-O rpaths",
    )?;
    parse_rpaths(&text)
}

fn parse_rpaths(text: &str) -> Result<Vec<String>, String> {
    let lines: Vec<&str> = text.lines().collect();
    let mut rpaths = Vec::new();
    let mut index = 0;
    while index < lines.len() {
        if lines[index].trim() != "cmd LC_RPATH" {
            index += 1;
            continue;
        }
        index += 1;
        let mut found = None;
        while index < lines.len() && !lines[index].trim_start().starts_with("cmd ") {
            if let Some(rest) = lines[index].trim().strip_prefix("path ") {
                found = Some(
                    rest.split_once(" (offset ")
                        .map_or(rest, |(path, _)| path)
                        .to_string(),
                );
                break;
            }
            index += 1;
        }
        let path = found.ok_or_else(|| "LC_RPATH has no path entry".to_string())?;
        rpaths.push(path);
    }
    Ok(rpaths)
}

fn parse_macos_minimum(text: &str) -> Result<(u32, u32, u32), String> {
    let lines: Vec<&str> = text.lines().collect();
    let mut versions = Vec::new();
    let mut index = 0;
    while index < lines.len() {
        let command = lines[index].trim();
        if command == "cmd LC_VERSION_MIN_MACOSX" {
            index += 1;
            while index < lines.len() && !lines[index].trim_start().starts_with("cmd ") {
                if let Some(value) = lines[index].trim().strip_prefix("version ") {
                    versions.push(parse_version(value)?);
                    break;
                }
                index += 1;
            }
        } else if command == "cmd LC_BUILD_VERSION" {
            index += 1;
            let mut platform = None;
            let mut minimum = None;
            while index < lines.len() && !lines[index].trim_start().starts_with("cmd ") {
                let line = lines[index].trim();
                if let Some(value) = line.strip_prefix("platform ") {
                    platform = Some(value);
                } else if let Some(value) = line.strip_prefix("minos ") {
                    minimum = Some(value);
                }
                index += 1;
            }
            if matches!(platform, Some("MACOS" | "1")) {
                versions.push(parse_version(minimum.ok_or_else(|| {
                    "LC_BUILD_VERSION for macOS has no minos value".to_string()
                })?)?);
            }
            continue;
        }
        index += 1;
    }
    if versions.len() != 1 {
        return Err(format!(
            "expected one macOS minimum-version load command, found {versions:?}"
        ));
    }
    Ok(versions[0])
}

fn parse_version(value: &str) -> Result<(u32, u32, u32), String> {
    let parts: Vec<&str> = value.split('.').collect();
    if !(2..=3).contains(&parts.len())
        || parts
            .iter()
            .any(|part| part.is_empty() || !part.bytes().all(|byte| byte.is_ascii_digit()))
    {
        return Err(format!("invalid macOS minimum version {value:?}"));
    }
    let major = parts[0]
        .parse()
        .map_err(|_| format!("invalid macOS minimum version {value:?}"))?;
    let minor = parts[1]
        .parse()
        .map_err(|_| format!("invalid macOS minimum version {value:?}"))?;
    let patch = if parts.len() == 3 {
        parts[2]
            .parse()
            .map_err(|_| format!("invalid macOS minimum version {value:?}"))?
    } else {
        0
    };
    Ok((major, minor, patch))
}

fn is_apple_system_library(path: &str) -> bool {
    path.starts_with("/System/Library/") || path.starts_with("/usr/lib/")
}

fn verify_signature(path: &Path) -> Result<(), String> {
    let mut command = Command::new("codesign");
    command.args(["--verify", "--strict"]).arg(path);
    run(
        &mut command,
        &format!("strict code-signature verification for {}", path.display()),
    )
}

fn output(command: &mut Command, description: &str) -> Result<String, String> {
    let result = command
        .output()
        .map_err(|error| format!("failed to run {description}: {error}"))?;
    if !result.status.success() {
        return Err(format!(
            "{description} failed with status {}: {}",
            result.status,
            String::from_utf8_lossy(&result.stderr).trim()
        ));
    }
    String::from_utf8(result.stdout)
        .map_err(|error| format!("{description} returned non-UTF-8 output: {error}"))
}

fn run(command: &mut Command, description: &str) -> Result<(), String> {
    let status = command
        .status()
        .map_err(|error| format!("failed to run {description}: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{description} failed with status {status}"))
    }
}

fn ensure_fresh_root(root: &Path) -> Result<(), String> {
    match fs::symlink_metadata(root) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
            return Err(format!(
                "OpenSSL staging root is not a real directory: {}",
                root.display()
            ));
        }
        Ok(_) => {
            if fs::read_dir(root)
                .map_err(|error| format!("failed to inspect OpenSSL staging root: {error}"))?
                .next()
                .is_some()
            {
                return Err(format!(
                    "OpenSSL staging root is not fresh and empty: {}",
                    root.display()
                ));
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir_all(root).map_err(|error| {
                format!(
                    "failed to create OpenSSL staging root {}: {error}",
                    root.display()
                )
            })?;
        }
        Err(error) => {
            return Err(format!(
                "failed to inspect OpenSSL staging root {}: {error}",
                root.display()
            ));
        }
    }
    Ok(())
}

fn extract_verified_source(archive_path: &Path, root: &Path, source: &Path) -> Result<(), String> {
    let file = File::open(archive_path)
        .map_err(|error| format!("failed to open verified OpenSSL archive: {error}"))?;
    let mut archive = tar::Archive::new(GzDecoder::new(file));
    let mut seen = BTreeSet::new();
    let mut root_directory = false;
    let mut license_file = false;
    for entry in archive
        .entries()
        .map_err(|error| format!("failed to read OpenSSL source archive: {error}"))?
    {
        let mut entry =
            entry.map_err(|error| format!("invalid OpenSSL source archive entry: {error}"))?;
        if entry.header().entry_type().is_pax_global_extensions() {
            validate_pax_global_metadata(&mut entry)?;
            continue;
        }
        let path = entry
            .path()
            .map_err(|error| format!("invalid OpenSSL archive path: {error}"))?
            .into_owned();
        let components: Vec<_> = path.components().collect();
        if path.is_absolute()
            || components.is_empty()
            || components[0] != Component::Normal(Path::new(SOURCE_TOP).as_os_str())
            || components
                .iter()
                .any(|component| !matches!(component, Component::Normal(_)))
        {
            return Err(format!(
                "unsafe path in verified OpenSSL archive: {}",
                path.display()
            ));
        }
        if !seen.insert(path.clone()) {
            return Err(format!(
                "duplicate path in verified OpenSSL archive: {}",
                path.display()
            ));
        }
        let entry_type = entry.header().entry_type();
        if !entry_type.is_file() && !entry_type.is_dir() {
            return Err(format!(
                "unsupported entry type in verified OpenSSL archive: {}",
                path.display()
            ));
        }
        if path == Path::new(SOURCE_TOP) && entry_type.is_dir() {
            root_directory = true;
        }
        if path == Path::new(SOURCE_TOP).join("LICENSE.txt") && entry_type.is_file() {
            license_file = true;
        }
    }
    if !root_directory || !license_file {
        return Err(
            "verified OpenSSL archive is missing its source directory or LICENSE.txt".into(),
        );
    }
    let file = File::open(archive_path)
        .map_err(|error| format!("failed to reopen OpenSSL archive: {error}"))?;
    let mut archive = tar::Archive::new(GzDecoder::new(file));
    for entry in archive
        .entries()
        .map_err(|error| format!("failed to read verified OpenSSL source entries: {error}"))?
    {
        let mut entry =
            entry.map_err(|error| format!("invalid verified OpenSSL source entry: {error}"))?;
        if entry.header().entry_type().is_pax_global_extensions() {
            validate_pax_global_metadata(&mut entry)?;
            continue;
        }
        if !entry
            .unpack_in(root)
            .map_err(|error| format!("failed to extract verified OpenSSL source entry: {error}"))?
        {
            return Err("verified OpenSSL source entry was rejected during extraction".into());
        }
    }
    fs::rename(root.join(SOURCE_TOP), source).map_err(|error| {
        format!(
            "failed to place OpenSSL source at {}: {error}",
            source.display()
        )
    })
}

fn validate_pax_global_metadata<R: Read>(entry: &mut tar::Entry<R>) -> Result<(), String> {
    let path = entry
        .path()
        .map_err(|error| format!("invalid PAX global header path: {error}"))?;
    if path != Path::new("pax_global_header") {
        return Err(format!(
            "unexpected PAX global header path: {}",
            path.display()
        ));
    }
    let mut extensions = entry
        .pax_extensions()
        .map_err(|error| format!("invalid PAX global metadata: {error}"))?
        .ok_or_else(|| "PAX global header has no metadata".to_string())?;
    let mut count = 0;
    for extension in &mut extensions {
        let extension =
            extension.map_err(|error| format!("invalid PAX global metadata: {error}"))?;
        let key = extension
            .key()
            .map_err(|error| format!("PAX global metadata key is not UTF-8: {error}"))?;
        let value = extension
            .value()
            .map_err(|error| format!("PAX global metadata value is not UTF-8: {error}"))?;
        if key != "comment"
            || value.len() != 40
            || !value.bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            return Err(format!(
                "unexpected OpenSSL source PAX metadata: {key}={value:?}"
            ));
        }
        count += 1;
    }
    if count != 1 {
        return Err(format!(
            "expected one OpenSSL source PAX comment, found {count}"
        ));
    }
    Ok(())
}

fn copy_regular_file(source: &Path, destination: &Path) -> Result<(), String> {
    let metadata = fs::symlink_metadata(source).map_err(|error| {
        format!(
            "required OpenSSL payload is missing at {}: {error}",
            source.display()
        )
    })?;
    if !metadata.file_type().is_file() || metadata.len() == 0 {
        return Err(format!(
            "OpenSSL payload is not a nonempty regular file: {}",
            source.display()
        ));
    }
    super::copy_file(source, destination)
        .map_err(|error| format!("failed to stage OpenSSL payload: {error}"))
}

fn clean_openssl_build_environment(command: &mut Command) {
    for key in [
        "OPENSSL_CONF",
        "OPENSSL_CONF_INCLUDE",
        "OPENSSL_MODULES",
        "OPENSSL_ENGINES",
        "OPENSSL_INIT_SETTINGS",
        "CFLAGS",
        "CPPFLAGS",
        "LDFLAGS",
        "CC",
        "CXX",
        "AR",
        "RANLIB",
        "CROSS_COMPILE",
        "MAKEFLAGS",
        "MFLAGS",
    ] {
        command.env_remove(key);
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_dependencies, parse_macos_minimum, parse_rpaths};

    #[test]
    fn source_archive_accepts_comment_metadata_without_extracting_it() {
        let root = super::super::unique_temp_dir("openssl_pax_source").unwrap();
        let archive_path = root.join("source.tar.gz");
        let file = std::fs::File::create(&archive_path).unwrap();
        let encoder = flate2::write::GzEncoder::new(file, flate2::Compression::default());
        let mut archive = tar::Builder::new(encoder);
        let metadata = b"52 comment=fe686e15d84334b284f883118ed92f64b409b3aa\n";
        let mut header = tar::Header::new_gnu();
        header.set_entry_type(tar::EntryType::new(b'g'));
        header.set_size(metadata.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        archive
            .append_data(&mut header, "pax_global_header", &metadata[..])
            .unwrap();
        let mut directory = tar::Header::new_gnu();
        directory.set_entry_type(tar::EntryType::Directory);
        directory.set_size(0);
        directory.set_mode(0o755);
        directory.set_cksum();
        archive
            .append_data(&mut directory, super::SOURCE_TOP, std::io::empty())
            .unwrap();
        let mut license = tar::Header::new_gnu();
        license.set_size(7);
        license.set_mode(0o644);
        license.set_cksum();
        archive
            .append_data(
                &mut license,
                format!("{}/LICENSE.txt", super::SOURCE_TOP),
                &b"license"[..],
            )
            .unwrap();
        archive.into_inner().unwrap().finish().unwrap();
        let destination = root.join("extracted");
        std::fs::create_dir(&destination).unwrap();
        super::extract_verified_source(&archive_path, &destination, &destination.join("source"))
            .unwrap();
        assert_eq!(
            std::fs::read(destination.join("source/LICENSE.txt")).unwrap(),
            b"license"
        );
        assert!(!destination.join("pax_global_header").exists());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn dylib_dependency_parser_filters_only_matching_install_id() {
        let text = "/package/libcodebasegraph-ssl-3.6.2.dylib:\n\t/producer/libssl.3.dylib (compatibility version 3.0.0, current version 3.0.0)\n\t@loader_path/libcodebasegraph-crypto-3.6.2.dylib (compatibility version 3.0.0, current version 3.0.0)\n";
        assert_eq!(
            parse_dependencies(text, Some("/producer/libssl.3.dylib")).unwrap(),
            ["@loader_path/libcodebasegraph-crypto-3.6.2.dylib"]
        );
        assert!(parse_dependencies(text, Some("/wrong/libssl.dylib")).is_err());
        assert_eq!(
            parse_dependencies(text, None).unwrap(),
            [
                "/producer/libssl.3.dylib",
                "@loader_path/libcodebasegraph-crypto-3.6.2.dylib"
            ]
        );
    }

    #[test]
    fn dependency_parser_preserves_paths_with_spaces() {
        let text = "/tmp/package with spaces/lib.dylib:\n\t@loader_path/libcodebasegraph-crypto-3.6.2.dylib (compatibility version 3.0.0, current version 3.0.0)\n\t/usr/lib/libSystem.B.dylib (compatibility version 1.0.0, current version 1356.0.0)\n";
        assert_eq!(
            parse_dependencies(text, None).unwrap(),
            [
                "@loader_path/libcodebasegraph-crypto-3.6.2.dylib",
                "/usr/lib/libSystem.B.dylib"
            ]
        );
    }

    #[test]
    fn minimum_version_parser_reads_macos_build_and_legacy_commands() {
        let build = "Load command 1\n      cmd LC_BUILD_VERSION\n  cmdsize 32\n platform MACOS\n    minos 13.3\n      sdk 26.5\n";
        let legacy = "Load command 1\n      cmd LC_VERSION_MIN_MACOSX\n  cmdsize 16\n  version 10.13\n      sdk 14.0\n";
        assert_eq!(parse_macos_minimum(build).unwrap(), (13, 3, 0));
        assert_eq!(parse_macos_minimum(legacy).unwrap(), (10, 13, 0));
        assert!(parse_macos_minimum("cmd LC_BUILD_VERSION\n platform IOS\n minos 13.0\n").is_err());
        assert!(
            parse_macos_minimum("cmd LC_BUILD_VERSION\n platform MACOS\n minos invalid\n").is_err()
        );
    }

    #[test]
    fn rpath_parser_reads_and_rejects_malformed_entries() {
        let text = "Load command 1\n      cmd LC_RPATH\n  cmdsize 32\n      path @loader_path/../lib (offset 12)\n";
        assert_eq!(parse_rpaths(text).unwrap(), ["@loader_path/../lib"]);
        assert!(parse_rpaths("cmd LC_RPATH\n cmdsize 8\n").is_err());
        assert!(parse_rpaths("cmd LC_LOAD_DYLIB\n").unwrap().is_empty());
    }
}
