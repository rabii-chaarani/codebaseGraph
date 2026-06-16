fn main() {
    if let Err(error) = codebase_graph_native::legacy::run_cli() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
