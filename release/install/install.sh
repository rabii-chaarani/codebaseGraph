#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Install codebase-graph and k-wiki from an extracted release archive.

Usage:
  ./install.sh [--source-dir <directory>] [--bin-dir <directory>] [--dry-run]

Options:
  --source-dir  Extracted archive directory containing binaries and checksums.txt.
                Defaults to the directory containing this script.
  --bin-dir     Destination directory for installed binaries.
                Defaults to $HOME/.local/bin.
  --dry-run     Validate checksums and executable surfaces without replacing files.
EOF
}

resolve_sha_cmd() {
  if command -v sha256sum >/dev/null 2>&1; then
    printf '%s\n' "sha256sum"
    return 0
  fi
  if command -v shasum >/dev/null 2>&1; then
    printf '%s\n' "shasum -a 256"
    return 0
  fi
  return 1
}

script_dir="$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)"
source_dir="$script_dir"
target_bin_dir="${HOME}/.local/bin"
dry_run="false"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --source-dir)
      [[ $# -ge 2 ]] || { echo "missing value for --source-dir" >&2; exit 2; }
      source_dir="$2"
      shift 2
      ;;
    --bin-dir)
      [[ $# -ge 2 ]] || { echo "missing value for --bin-dir" >&2; exit 2; }
      target_bin_dir="$2"
      shift 2
      ;;
    --dry-run)
      dry_run="true"
      shift
      ;;
    -h|--help|help)
      usage
      exit 0
      ;;
    *)
      echo "unknown option: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

source_dir="$(CDPATH='' cd -- "$source_dir" && pwd)"
checksums_path="${source_dir}/checksums.txt"
[[ -f "$checksums_path" ]] || { echo "missing checksums.txt in ${source_dir}" >&2; exit 1; }

sha_cmd="$(resolve_sha_cmd)" || {
  echo "sha256 tool is required (sha256sum or shasum)." >&2
  exit 1
}

verify_binary() {
  local name="$1"
  local probe_arg="$2"
  local path="${source_dir}/${name}"
  [[ -f "$path" ]] || { echo "missing binary ${name} in ${source_dir}" >&2; exit 1; }
  (
    cd "$source_dir"
    grep -E "  ${name}\$" checksums.txt >/dev/null 2>&1 || {
      echo "checksums.txt does not contain ${name}" >&2
      exit 1
    }
    if [[ "$sha_cmd" == "sha256sum" ]]; then
      sha256sum --check --status --strict <(grep -E "  ${name}\$" checksums.txt)
    else
      shasum -a 256 --check --status <(grep -E "  ${name}\$" checksums.txt)
    fi
  )
  "$path" "$probe_arg" >/dev/null
}

install_binary() {
  local name="$1"
  local source_path="${source_dir}/${name}"
  local target_path="${target_bin_dir}/${name}"
  local temp_path
  temp_path="$(mktemp "${target_bin_dir}/.${name}.tmp.XXXXXX")"
  if ! cp "$source_path" "$temp_path" || ! chmod +x "$temp_path" || ! mv "$temp_path" "$target_path"; then
    rm -f -- "$temp_path"
    return 1
  fi
}

verify_binary "codebase-graph" "--help"
verify_binary "k-wiki" "--version"

if [[ "$dry_run" == "true" ]]; then
  printf 'validated %s and %s from %s\n' "codebase-graph" "k-wiki" "$source_dir"
  exit 0
fi

mkdir -p "$target_bin_dir"
install_binary "codebase-graph"
install_binary "k-wiki"

printf 'installed %s and %s to %s\n' "codebase-graph" "k-wiki" "$target_bin_dir"
