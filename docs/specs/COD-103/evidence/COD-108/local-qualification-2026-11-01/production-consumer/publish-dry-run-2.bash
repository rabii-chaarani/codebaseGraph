set -euo pipefail
cargo package --locked --no-verify
crate_path="$(find target/package -maxdepth 1 -type f -name 'codebase-graph-*.crate' -print -quit)"
test -n "$crate_path"
cargo run -p xtask -- verify-crate-size "$crate_path"
