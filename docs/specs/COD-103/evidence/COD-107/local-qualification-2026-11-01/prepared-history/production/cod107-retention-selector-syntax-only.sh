set -euo pipefail
: "${GITHUB_EVENT_NAME:?}" "${GITHUB_REF:?}" "${GITHUB_REPOSITORY:?}" "${GITHUB_EVENT_PATH:?}" "${GITHUB_OUTPUT:?}"
retain="$(jq -r \
  --arg event "$GITHUB_EVENT_NAME" \
  --arg ref "$GITHUB_REF" \
  --arg repository "$GITHUB_REPOSITORY" '
  if type != "object" then error("Expected an event object")
  elif $event == "push" then $ref == "refs/heads/main"
  elif $event == "pull_request" then
    ((.action == "opened" or .action == "synchronize" or
      .action == "reopened" or .action == "edited") and
     .pull_request.head.repo.full_name == $repository and
     .pull_request.base.repo.full_name == $repository and
     .pull_request.head.ref == "cod103-stack/cod-107" and
     (.pull_request.base.ref == "cod103-stack/cod-56" or
      .pull_request.base.ref == "main"))
  else false
  end' "$GITHUB_EVENT_PATH")"
case "$retain" in
  true|false) printf 'retain=%s\n' "$retain" >> "$GITHUB_OUTPUT" ;;
  *) echo 'Invalid native retention result' >&2; exit 1 ;;
esac
