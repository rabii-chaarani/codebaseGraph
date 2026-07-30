fn main() {
    if let Err(error) = codebase_graph::run_from_env() {
        eprintln!("{error}");
        std::process::exit(codebase_graph::adapters::cli::error_exit_code(&error));
    }
}
