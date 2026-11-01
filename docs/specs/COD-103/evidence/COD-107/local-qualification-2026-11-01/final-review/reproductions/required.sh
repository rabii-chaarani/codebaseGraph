set -euo pipefail
if jq -e 'to_entries | any(.value.result != "success")' <<<"$RESULTS" >/dev/null; then
  echo "$RESULTS"
  exit 1
fi
