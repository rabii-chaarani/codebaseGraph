fn main() {
    if !cfg!(windows) && std::env::var_os("LBUG_SHARED").is_none() {
        println!("cargo:rustc-link-arg=-rdynamic");
    }

    // lbug's prebuilt static library includes the HTTPFS extension, which
    // depends on OpenSSL without exporting its linker requirements.
    if cfg!(target_os = "linux") || cfg!(windows) {
        println!("cargo:rustc-link-lib=dylib=ssl");
        println!("cargo:rustc-link-lib=dylib=crypto");
    }
}
