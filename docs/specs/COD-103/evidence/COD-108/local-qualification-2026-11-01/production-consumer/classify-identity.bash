set -euo pipefail
: "${GITHUB_EVENT_NAME:?}" "${GITHUB_REPOSITORY:?}" "${GITHUB_SHA:?}" "${GITHUB_REF:?}" "${GITHUB_EVENT_PATH:?}" "${GITHUB_OUTPUT:?}" "${RUNNER_TEMP:?}" "${REPOSITORY_ID:?}"
[[ "$GITHUB_REPOSITORY" =~ ^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$ ]]
mkdir -p "$RUNNER_TEMP/cod108"
event="$GITHUB_EVENT_PATH"
jq -se --arg repository "$GITHUB_REPOSITORY" --arg id "$REPOSITORY_ID" '
  length == 1 and (.[0] | type == "object" and .repository.full_name == $repository and
  (.repository.id | tostring) == $id)' "$event" >/dev/null
candidate=false
reason=ordinary-event
number=0
if [[ "$GITHUB_EVENT_NAME" == pull_request ]]; then
  read -r base head number < <(jq -er '
    [.pull_request.base.sha, .pull_request.head.sha, (.number | tostring)] |
    if all(.[]; type == "string") then @tsv else error("Incomplete PR event") end' "$event")
  [[ "$number" =~ ^[1-9][0-9]*$ ]]
  if gh api "repos/$GITHUB_REPOSITORY/pulls/$number" > "$RUNNER_TEMP/cod108/pr.json" &&
     gh api "repos/$GITHUB_REPOSITORY/git/ref/heads/main" > "$RUNNER_TEMP/cod108/main.json"; then
    if jq -se --slurpfile event "$event" --slurpfile main "$RUNNER_TEMP/cod108/main.json" \
      --arg repository "$GITHUB_REPOSITORY" --arg id "$REPOSITORY_ID" '
      def trusted:
        .state == "open" and .draft == false and
        .user.id == 41898282 and .user.login == "github-actions[bot]" and .user.type == "Bot" and
        .base.ref == "main" and .head.ref == "release-please--branches--main--components--codebase-graph" and
        .base.repo.full_name == $repository and .head.repo.full_name == $repository and
        (.base.repo.id | tostring) == $id and (.head.repo.id | tostring) == $id and
        (.labels | type == "array" and any(.[]; .name == "autorelease: pending"));
      length == 1 and (.[0] | ($event | length) == 1 and ($main | length) == 1 and
      ($event[0].action | IN("opened", "synchronize", "reopened", "edited", "labeled", "unlabeled")) and
      ($event[0].pull_request | trusted) and trusted and
      .number == $event[0].number and
      .base.sha == $event[0].pull_request.base.sha and .head.sha == $event[0].pull_request.head.sha and
      $main[0].ref == "refs/heads/main" and $main[0].object.type == "commit" and
      .base.sha == $main[0].object.sha)' "$RUNNER_TEMP/cod108/pr.json" >/dev/null; then
      candidate=true
      reason=eligibility-pending
    else
      reason=untrusted-or-stale-pr
    fi
  else
    reason=identity-acquisition-failed
  fi
elif [[ "$GITHUB_EVENT_NAME" == push && "$GITHUB_REF" == refs/heads/main ]]; then
  read -r base head < <(jq -er '[.before, .after] | if all(.[]; type == "string") then @tsv else error("Incomplete push") end' "$event")
  [[ "$head" == "$GITHUB_SHA" ]]
  reason=main-push
else
  echo 'Unsupported CI event' >&2
  exit 1
fi
[[ "$base" =~ ^[0-9a-f]{40}$ && "$head" =~ ^[0-9a-f]{40}$ && "$GITHUB_SHA" =~ ^[0-9a-f]{40}$ ]]
jq -cn --arg repository "$GITHUB_REPOSITORY" --arg repository_id "$REPOSITORY_ID" \
  --arg event_name "$GITHUB_EVENT_NAME" --arg run_sha "$GITHUB_SHA" \
  --arg base_sha "$base" --arg head_sha "$head" --arg reason_code "$reason" \
  --argjson pr_number "$number" \
  '{schema_version:1,mode:"full",reason_code:$reason_code,base_sha:$base_sha,head_sha:$head_sha,
    repository:$repository,repository_id:$repository_id,event_name:$event_name,run_sha:$run_sha,
    pr_number:$pr_number,positive_metadata:false}' > "$RUNNER_TEMP/cod108/decision.json"
printf 'candidate=%s\nbase_sha=%s\nhead_sha=%s\n' "$candidate" "$base" "$head" >> "$GITHUB_OUTPUT"
