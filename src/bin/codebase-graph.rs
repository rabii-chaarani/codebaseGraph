fn main() {
    if let Err(error) = codebase_graph::cli::run_from_env() {
        eprintln!("{error}");
        std::process::exit(codebase_graph::cli::error_exit_code(&error));
    }
}
