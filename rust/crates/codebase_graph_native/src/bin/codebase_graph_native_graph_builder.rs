fn main() {
    if let Err(error) = codebase_graph_native::legacy_cli::run_cli() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
