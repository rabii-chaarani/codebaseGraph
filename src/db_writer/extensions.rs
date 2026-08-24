use crate::error::NativeError;
use std::io::Cursor;
use std::path::{Path, PathBuf};

// Both the prebuilt and source-built Ladybug 0.19.0 libraries resolve
// extensions from this ABI-compatible cache path. Seed it so packaged
// binaries never download extensions at runtime.
const LADYBUG_EXTENSION_CACHE_VERSIONS: [&str; 1] = ["0.19.0"];

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
        let missing: Vec<(PathBuf, PathBuf)> = LADYBUG_EXTENSION_CACHE_VERSIONS
            .iter()
            .filter_map(|cache_version| {
                let extension_dir = extension_dir(&home, cache_version, platform, extension);
                let extension_path = extension_dir.join(format!("lib{extension}.lbug_extension"));
                (!extension_path.exists()).then_some((extension_dir, extension_path))
            })
            .collect();
        if missing.is_empty() {
            continue;
        }
        let Some(bytes) = bundled_extension_bytes(extension)? else {
            continue;
        };
        for (extension_dir, extension_path) in missing {
            std::fs::create_dir_all(&extension_dir)?;
            std::fs::write(extension_path, &bytes)?;
        }
    }
    Ok(())
}

fn bundled_extension_bytes(extension: &str) -> Result<Option<Vec<u8>>, NativeError> {
    let Some(compressed) = bundled_extension_xz(extension) else {
        return Ok(None);
    };
    let mut bytes = Vec::new();
    lzma_rs::xz_decompress(&mut Cursor::new(compressed), &mut bytes).map_err(|error| {
        NativeError::Database(format!(
            "failed to decompress bundled LadyBug {extension} extension: {error}"
        ))
    })?;
    Ok(Some(bytes))
}

fn extension_dir(home: &Path, cache_version: &str, platform: &str, extension: &str) -> PathBuf {
    home.join(".lbdb")
        .join("extension")
        .join(cache_version)
        .join(platform)
        .join(extension)
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
fn bundled_extension_xz(extension: &str) -> Option<&'static [u8]> {
    match extension {
        "json" => Some(include_bytes!(
            "../../assets/ladybug-extensions/0.19.0/linux_amd64/json/libjson.lbug_extension.xz"
        )),
        "fts" => Some(include_bytes!(
            "../../assets/ladybug-extensions/0.19.0/linux_amd64/fts/libfts.lbug_extension.xz"
        )),
        _ => None,
    }
}

#[cfg(all(target_os = "linux", target_arch = "aarch64"))]
fn bundled_extension_xz(extension: &str) -> Option<&'static [u8]> {
    match extension {
        "json" => Some(include_bytes!(
            "../../assets/ladybug-extensions/0.19.0/linux_arm64/json/libjson.lbug_extension.xz"
        )),
        "fts" => Some(include_bytes!(
            "../../assets/ladybug-extensions/0.19.0/linux_arm64/fts/libfts.lbug_extension.xz"
        )),
        _ => None,
    }
}

#[cfg(all(target_os = "macos", target_arch = "x86_64"))]
fn bundled_extension_xz(extension: &str) -> Option<&'static [u8]> {
    match extension {
        "json" => Some(include_bytes!(
            "../../assets/ladybug-extensions/0.19.0/osx_amd64/json/libjson.lbug_extension.xz"
        )),
        "fts" => Some(include_bytes!(
            "../../assets/ladybug-extensions/0.19.0/osx_amd64/fts/libfts.lbug_extension.xz"
        )),
        _ => None,
    }
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn bundled_extension_xz(extension: &str) -> Option<&'static [u8]> {
    match extension {
        "json" => Some(include_bytes!(
            "../../assets/ladybug-extensions/0.19.0/osx_arm64/json/libjson.lbug_extension.xz"
        )),
        "fts" => Some(include_bytes!(
            "../../assets/ladybug-extensions/0.19.0/osx_arm64/fts/libfts.lbug_extension.xz"
        )),
        _ => None,
    }
}

#[cfg(all(
    target_os = "windows",
    target_arch = "x86_64",
    feature = "bundled-windows-extensions"
))]
fn bundled_extension_xz(extension: &str) -> Option<&'static [u8]> {
    match extension {
        "json" => Some(include_bytes!(
            "../../assets/ladybug-extensions/0.19.0/win_amd64/json/libjson.lbug_extension.xz"
        )),
        "fts" => Some(include_bytes!(
            "../../assets/ladybug-extensions/0.19.0/win_amd64/fts/libfts.lbug_extension.xz"
        )),
        _ => None,
    }
}

#[cfg(not(any(
    all(target_os = "linux", target_arch = "x86_64"),
    all(target_os = "linux", target_arch = "aarch64"),
    all(target_os = "macos", target_arch = "x86_64"),
    all(target_os = "macos", target_arch = "aarch64"),
    all(
        target_os = "windows",
        target_arch = "x86_64",
        feature = "bundled-windows-extensions"
    )
)))]
fn bundled_extension_xz(_extension: &str) -> Option<&'static [u8]> {
    None
}

#[cfg(test)]
mod tests {
    use super::{bundled_extension_bytes, extension_dir};
    use std::path::{Path, PathBuf};

    #[test]
    fn supports_prebuilt_and_source_built_ladybug_extension_cache_versions() {
        assert_eq!(super::LADYBUG_EXTENSION_CACHE_VERSIONS, ["0.19.0"]);
        assert_eq!(
            extension_dir(Path::new("cache"), "0.19.0", "osx_arm64", "json"),
            PathBuf::from("cache/.lbdb/extension/0.19.0/osx_arm64/json")
        );
    }

    #[test]
    fn bundled_extension_archives_decompress_before_cache_seed() {
        for extension in ["json", "fts"] {
            let Some(bytes) = bundled_extension_bytes(extension).unwrap() else {
                continue;
            };
            assert!(bytes.len() > 500_000, "{extension} extension is truncated");
        }
    }
}
