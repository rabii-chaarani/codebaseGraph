fn main() {
    if let Err(error) = codebase_graph_native::product_cli::run_from_env() {
        eprintln!("{error}");
        std::process::exit(codebase_graph_native::product_cli::error_exit_code(&error));
    }
}
