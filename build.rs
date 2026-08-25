fn main() {
    build_tree_sitter_wat();

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

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos")
        && std::env::var("CARGO_CFG_TARGET_ARCH").as_deref() == Ok("x86_64")
    {
        link_macos_x86_compiler_runtime();
    }
}

fn build_tree_sitter_wat() {
    let source_dir = std::path::Path::new("vendor/tree-sitter-wat/src");
    let parser = source_dir.join("parser.c");
    let scanner = source_dir.join("scanner.c");

    let mut build = cc::Build::new();
    build
        .std("c11")
        .include(source_dir)
        .flag_if_supported("-Wno-unused-parameter")
        .file(&parser)
        .file(&scanner);
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        build.flag("-utf-8");
    }
    build.compile("tree-sitter-wat");

    println!("cargo:rerun-if-changed={}", parser.display());
    println!("cargo:rerun-if-changed={}", scanner.display());
    println!(
        "cargo:rerun-if-changed={}",
        source_dir.join("tree_sitter/parser.h").display()
    );
}

fn link_macos_x86_compiler_runtime() {
    println!("cargo:rerun-if-env-changed=DEVELOPER_DIR");
    let output = std::process::Command::new("xcrun")
        .args(["clang", "--print-resource-dir"])
        .output()
        .expect("macOS x86_64 builds require xcrun and clang");
    assert!(
        output.status.success(),
        "xcrun clang --print-resource-dir failed with {}",
        output.status
    );

    let resource_dir =
        String::from_utf8(output.stdout).expect("clang resource directory must be valid UTF-8");
    let library_dir = std::path::Path::new(resource_dir.trim())
        .join("lib")
        .join("darwin");
    let runtime = library_dir.join("libclang_rt.osx.a");
    assert!(
        runtime.is_file(),
        "macOS compiler runtime not found at {}",
        runtime.display()
    );
    println!("cargo:rustc-link-search=native={}", library_dir.display());
    println!("cargo:rustc-link-lib=static=clang_rt.osx");
}
