fn main() {
    if let Err(error) = codebase_graph::adapters::cli::run_from_env() {
        eprintln!("{error}");
        std::process::exit(codebase_graph::adapters::cli::error_exit_code(&error));
    }
}
