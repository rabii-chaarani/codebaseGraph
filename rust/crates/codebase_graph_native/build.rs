fn main() {
    if !cfg!(windows) && std::env::var_os("LBUG_SHARED").is_none() {
        println!("cargo:rustc-link-arg=-rdynamic");
    }

    #[cfg(feature = "python-extension")]
    pyo3_build_config::add_extension_module_link_args();
}
