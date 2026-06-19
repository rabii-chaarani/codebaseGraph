use crate::error::NativeError;
use std::path::PathBuf;

pub fn preseed_ladybug_extensions(include_fts: bool) -> Result<(), NativeError> {
    let home = ladybug_home_dir()?;
    let Some(platform) = ladybug_platform() else {
        return Ok(());
    };
    let mut extensions = vec!["json"];
    if include_fts {
        extensions.push("fts");
    }
    for extension in extensions {
        let Some(bytes) = bundled_extension_bytes(extension) else {
            continue;
        };
        let extension_dir = home
            .join(".lbdb")
            .join("extension")
            .join("0.17.0")
            .join(platform)
            .join(extension);
        let extension_path = extension_dir.join(format!("lib{extension}.lbug_extension"));
        if extension_path.exists() {
            continue;
        }
        std::fs::create_dir_all(&extension_dir)?;
        std::fs::write(extension_path, bytes)?;
    }
    Ok(())
}

fn ladybug_home_dir() -> Result<PathBuf, NativeError> {
    let variable = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    std::env::var_os(variable)
        .map(PathBuf::from)
        .filter(|path| !path.as_os_str().is_empty())
        .ok_or_else(|| {
            NativeError::Database(format!(
                "LadyBug extension cache cannot be seeded because {variable} is not set"
            ))
        })
}

fn ladybug_platform() -> Option<&'static str> {
    if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        Some("linux_amd64")
    } else if cfg!(all(target_os = "linux", target_arch = "aarch64")) {
        Some("linux_arm64")
    } else if cfg!(all(target_os = "macos", target_arch = "x86_64")) {
        Some("osx_amd64")
    } else if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        Some("osx_arm64")
    } else if cfg!(all(target_os = "windows", target_arch = "x86_64")) {
        Some("win_amd64")
    } else {
        None
    }
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn bundled_extension_bytes(extension: &str) -> Option<&'static [u8]> {
    match extension {
        "json" => Some(include_bytes!(
            "../../assets/ladybug-extensions/0.17.0/linux_amd64/json/libjson.lbug_extension"
        )),
        "fts" => Some(include_bytes!(
            "../../assets/ladybug-extensions/0.17.0/linux_amd64/fts/libfts.lbug_extension"
        )),
        _ => None,
    }
}

#[cfg(all(target_os = "linux", target_arch = "aarch64"))]
fn bundled_extension_bytes(extension: &str) -> Option<&'static [u8]> {
    match extension {
        "json" => Some(include_bytes!(
            "../../assets/ladybug-extensions/0.17.0/linux_arm64/json/libjson.lbug_extension"
        )),
        "fts" => Some(include_bytes!(
            "../../assets/ladybug-extensions/0.17.0/linux_arm64/fts/libfts.lbug_extension"
        )),
        _ => None,
    }
}

#[cfg(all(target_os = "macos", target_arch = "x86_64"))]
fn bundled_extension_bytes(extension: &str) -> Option<&'static [u8]> {
    match extension {
        "json" => Some(include_bytes!(
            "../../assets/ladybug-extensions/0.17.0/osx_amd64/json/libjson.lbug_extension"
        )),
        "fts" => Some(include_bytes!(
            "../../assets/ladybug-extensions/0.17.0/osx_amd64/fts/libfts.lbug_extension"
        )),
        _ => None,
    }
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn bundled_extension_bytes(extension: &str) -> Option<&'static [u8]> {
    match extension {
        "json" => Some(include_bytes!(
            "../../assets/ladybug-extensions/0.17.0/osx_arm64/json/libjson.lbug_extension"
        )),
        "fts" => Some(include_bytes!(
            "../../assets/ladybug-extensions/0.17.0/osx_arm64/fts/libfts.lbug_extension"
        )),
        _ => None,
    }
}

#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn bundled_extension_bytes(extension: &str) -> Option<&'static [u8]> {
    match extension {
        "json" => Some(include_bytes!(
            "../../assets/ladybug-extensions/0.17.0/win_amd64/json/libjson.lbug_extension"
        )),
        "fts" => Some(include_bytes!(
            "../../assets/ladybug-extensions/0.17.0/win_amd64/fts/libfts.lbug_extension"
        )),
        _ => None,
    }
}

#[cfg(not(any(
    all(target_os = "linux", target_arch = "x86_64"),
    all(target_os = "linux", target_arch = "aarch64"),
    all(target_os = "macos", target_arch = "x86_64"),
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "windows", target_arch = "x86_64")
)))]
fn bundled_extension_bytes(_extension: &str) -> Option<&'static [u8]> {
    None
}
