fn main() {
    if let Err(error) = codebase_graph::product_cli::run_from_env() {
        eprintln!("{error}");
        std::process::exit(codebase_graph::product_cli::error_exit_code(&error));
    }
}
