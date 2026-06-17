fn main() {
    if !cfg!(windows) && std::env::var_os("LBUG_SHARED").is_none() {
        println!("cargo:rustc-link-arg=-rdynamic");
    }

    emit_lbug_extension_exports();

    #[cfg(feature = "python-extension")]
    pyo3_build_config::add_extension_module_link_args();
}

fn emit_lbug_extension_exports() {
    if std::env::var_os("CARGO_FEATURE_PYTHON_EXTENSION").is_none()
        || std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("macos")
        || std::env::var_os("LBUG_SHARED").is_some()
    {
        return;
    }

    let Some(liblbug) = find_liblbug_archive() else {
        println!("cargo:warning=Could not find liblbug.a for macOS extension symbol exports");
        return;
    };
    let out_dir = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let export_list = out_dir.join("lbug-extension-exports.txt");
    let output = std::process::Command::new("nm")
        .arg("-gU")
        .arg(&liblbug)
        .output();
    let Ok(output) = output else {
        println!("cargo:warning=Could not run nm to export Ladybug symbols");
        return;
    };
    if !output.status.success() {
        println!(
            "cargo:warning=nm failed while reading {}",
            liblbug.display()
        );
        return;
    }

    let symbols = String::from_utf8_lossy(&output.stdout);
    let mut exported: Vec<&str> = symbols
        .lines()
        .filter_map(|line| line.split_whitespace().last())
        .filter(|symbol| is_lbug_extension_symbol(symbol))
        .collect();
    exported.sort_unstable();
    exported.dedup();
    if exported.is_empty() {
        println!(
            "cargo:warning=No Ladybug symbols found for extension exports in {}",
            liblbug.display()
        );
        return;
    }

    let mut contents = exported.join("\n");
    contents.push('\n');
    std::fs::write(&export_list, contents).expect("write Ladybug extension export list");
    println!("cargo:rerun-if-changed={}", liblbug.display());
    println!(
        "cargo:rustc-link-arg-cdylib=-Wl,-exported_symbols_list,{}",
        export_list.display()
    );
}

fn is_lbug_extension_symbol(symbol: &str) -> bool {
    symbol.starts_with("__Z") && symbol.contains("4lbug")
}

fn find_liblbug_archive() -> Option<std::path::PathBuf> {
    if let Some(path) = std::env::var_os("LBUG_PRECOMPILED_LIBRARY_DIR") {
        let candidate = std::path::PathBuf::from(path).join("liblbug.a");
        if candidate.exists() {
            return Some(candidate);
        }
    }

    let cache_key = lbug_prebuilt_cache_key();
    for registry_src in cargo_registry_src_dirs() {
        let Ok(entries) = std::fs::read_dir(registry_src) else {
            continue;
        };
        for entry in entries.flatten() {
            let file_name = entry.file_name();
            let Some(file_name) = file_name.to_str() else {
                continue;
            };
            if !file_name.starts_with("lbug-") {
                continue;
            }
            let candidate = entry
                .path()
                .join(".cache")
                .join("lbug-prebuilt")
                .join(&cache_key)
                .join("lib")
                .join("liblbug.a");
            if candidate.exists() {
                return Some(candidate);
            }
        }
    }
    None
}

fn lbug_prebuilt_cache_key() -> String {
    let source = if let Ok(run_id) = std::env::var("LBUG_PRECOMPILED_RUN_ID") {
        format!("run-{run_id}")
    } else if let Ok(version) = std::env::var("LBUG_VERSION") {
        format!("version-{version}")
    } else {
        "latest".to_string()
    };
    source
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' {
                c
            } else {
                '-'
            }
        })
        .collect()
}

fn cargo_registry_src_dirs() -> Vec<std::path::PathBuf> {
    let cargo_home = std::env::var_os("CARGO_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME")
                .map(std::path::PathBuf::from)
                .map(|home| home.join(".cargo"))
        });
    let Some(cargo_home) = cargo_home else {
        return Vec::new();
    };
    let registry_src = cargo_home.join("registry").join("src");
    let Ok(entries) = std::fs::read_dir(registry_src) else {
        return Vec::new();
    };
    entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect()
}
