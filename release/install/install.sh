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
  --bin-dir     Destination directory for installed binaries and runtime libraries.
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
    printf '%s\n' "shasum"
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

case "$(uname -s):$(uname -m)" in
  Darwin:arm64|Darwin:aarch64) ;;
  *) echo 'Supported platforms: macOS ARM64 (install.sh) and Windows x86_64 (install.ps1).' >&2; exit 1 ;;
esac

source_dir="$(CDPATH='' cd -- "$source_dir" && pwd)"
checksums_path="${source_dir}/checksums.txt"
[[ -f "$checksums_path" && ! -L "$checksums_path" ]] || {
  echo "missing or unsafe checksums.txt in ${source_dir}" >&2
  exit 1
}

sha_cmd="$(resolve_sha_cmd)" || {
  echo "sha256 tool is required (sha256sum or shasum)." >&2
  exit 1
}

verify_payload() {
  local line digest name path hash_result actual_hash expected_version row_version
  local rows=0 graph_count=0 wiki_count=0 ssl_count=0 crypto_count=0 license_count=0
  local expected_version=""
  local graph_name="codebase-graph" wiki_name="k-wiki"
  local ssl_name="" crypto_name="" license_name=""

  while IFS= read -r line || [[ -n "$line" ]]; do
    [[ "$line" =~ ^([[:xdigit:]]{64})\ \ ([[:alnum:]_.-]+)$ ]] || {
      echo "malformed checksums.txt row" >&2
      return 1
    }
    digest="${BASH_REMATCH[1]}"
    name="${BASH_REMATCH[2]}"
    rows=$((rows + 1))

    if [[ "$name" == "$graph_name" ]]; then
      graph_count=$((graph_count + 1))
    elif [[ "$name" == "$wiki_name" ]]; then
      wiki_count=$((wiki_count + 1))
    elif [[ "$name" =~ ^libcodebasegraph-ssl-(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.dylib$ ]]; then
      ssl_count=$((ssl_count + 1))
      row_version="${BASH_REMATCH[1]}.${BASH_REMATCH[2]}.${BASH_REMATCH[3]}"
      ssl_name="$name"
    elif [[ "$name" =~ ^libcodebasegraph-crypto-(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.dylib$ ]]; then
      crypto_count=$((crypto_count + 1))
      row_version="${BASH_REMATCH[1]}.${BASH_REMATCH[2]}.${BASH_REMATCH[3]}"
      crypto_name="$name"
    elif [[ "$name" =~ ^openssl-(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)-LICENSE\.txt$ ]]; then
      license_count=$((license_count + 1))
      row_version="${BASH_REMATCH[1]}.${BASH_REMATCH[2]}.${BASH_REMATCH[3]}"
      license_name="$name"
    else
      echo "unexpected checksums.txt payload name: ${name}" >&2
      return 1
    fi

    if [[ "$name" != "$graph_name" && "$name" != "$wiki_name" ]]; then
      if [[ -z "$expected_version" ]]; then
        expected_version="$row_version"
      elif [[ "$row_version" != "$expected_version" ]]; then
        echo "OpenSSL payload filenames have different versions" >&2
        return 1
      fi
    fi

    path="${source_dir}/${name}"
    [[ -f "$path" && ! -L "$path" ]] || {
      echo "missing or unsafe payload file ${name} in ${source_dir}" >&2
      return 1
    }
    if [[ "$sha_cmd" == "sha256sum" ]]; then
      hash_result="$(sha256sum "$path")" || { echo "failed to hash ${name}" >&2; return 1; }
    else
      hash_result="$(shasum -a 256 "$path")" || { echo "failed to hash ${name}" >&2; return 1; }
    fi
    actual_hash="${hash_result%% *}"
    actual_hash="$(printf '%s' "$actual_hash" | tr '[:upper:]' '[:lower:]')"
    digest="$(printf '%s' "$digest" | tr '[:upper:]' '[:lower:]')"
    [[ "$actual_hash" == "$digest" ]] || {
      echo "SHA256 mismatch for ${name}" >&2
      return 1
    }
  done < "$checksums_path"

  [[ "$rows" -eq 5 && "$graph_count" -eq 1 && "$wiki_count" -eq 1 && \
    "$ssl_count" -eq 1 && "$crypto_count" -eq 1 && "$license_count" -eq 1 ]] || {
    echo "checksums.txt must contain exactly two binaries and one each SSL, crypto, and license payload" >&2
    return 1
  }
  printf '%s\n%s\n%s\n' "$ssl_name" "$crypto_name" "$license_name"
}

payload_names="$(verify_payload)" || exit 1
ssl_name="$(printf '%s\n' "$payload_names" | sed -n '1p')"
crypto_name="$(printf '%s\n' "$payload_names" | sed -n '2p')"
license_name="$(printf '%s\n' "$payload_names" | sed -n '3p')"

probe_binary() {
  local name="$1" probe_arg="$2" path="${source_dir}/$1"
  if ! env -i "PATH=/usr/bin:/bin:/usr/sbin:/sbin" "HOME=${HOME:-/tmp}" "$path" "$probe_arg" >/dev/null; then
    echo "binary probe failed for ${name} (${probe_arg})" >&2
    return 1
  fi
}

probe_binary "codebase-graph" "--help"
probe_binary "k-wiki" "--version"

if [[ "$dry_run" == "true" ]]; then
  printf 'validated %s, %s, and OpenSSL payloads from %s\n' "codebase-graph" "k-wiki" "$source_dir"
  exit 0
fi

install_file() {
  local name="$1" mode="$2"
  local source_path="${source_dir}/${name}"
  local target_path="${target_bin_dir}/${name}"
  local temp_path
  temp_path="$(mktemp "${target_bin_dir}/.${name}.tmp.XXXXXX")"
  if ! cp "$source_path" "$temp_path" || ! chmod "$mode" "$temp_path" || ! mv "$temp_path" "$target_path"; then
    rm -f -- "$temp_path"
    echo "failed to install ${name}" >&2
    return 1
  fi
}

mkdir -p "$target_bin_dir"
install_file "$ssl_name" 0644
install_file "$crypto_name" 0644
install_file "$license_name" 0644
install_file "codebase-graph" 0755
install_file "k-wiki" 0755

printf 'installed %s, %s, and OpenSSL payloads to %s\n' "codebase-graph" "k-wiki" "$target_bin_dir"
