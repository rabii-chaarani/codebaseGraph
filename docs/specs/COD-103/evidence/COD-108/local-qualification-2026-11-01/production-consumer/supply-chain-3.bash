time_features="$(cargo tree -e features -i time --locked)"
if grep -q 'time feature "parsing"' <<<"$time_features"; then
  echo "time parsing is enabled; RUSTSEC-2026-0009 must not be ignored" >&2
  exit 1
fi
cargo audit --ignore RUSTSEC-2026-0009
