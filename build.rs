fn main() {
    if !cfg!(windows) && std::env::var_os("LBUG_SHARED").is_none() {
        println!("cargo:rustc-link-arg=-rdynamic");
    }

    // lbug's prebuilt static library includes the HTTPFS extension, which
    // depends on OpenSSL without exporting its linker requirements.
    if cfg!(target_os = "linux") || cfg!(target_os = "macos") || cfg!(windows) {
        println!("cargo:rustc-link-lib=dylib=ssl");
        println!("cargo:rustc-link-lib=dylib=crypto");
    }

    if cfg!(target_os = "macos") {
        println!("cargo:rerun-if-env-changed=OPENSSL_LIB_DIR");
        let configured = std::env::var_os("OPENSSL_LIB_DIR").map(std::path::PathBuf::from);
        let library_dir = configured.into_iter().chain([
            std::path::PathBuf::from("/opt/homebrew/opt/openssl@3/lib"),
            std::path::PathBuf::from("/usr/local/opt/openssl@3/lib"),
        ]);
        if let Some(path) = library_dir
            .into_iter()
            .find(|path| path.join("libssl.dylib").is_file())
        {
            println!("cargo:rustc-link-search=native={}", path.display());
        }
    }
}
