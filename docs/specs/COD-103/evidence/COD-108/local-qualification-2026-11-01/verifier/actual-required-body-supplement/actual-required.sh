set -euo pipefail
: "${RESULTS:?}" "${DECISION:?}" "${GITHUB_EVENT_NAME:?}" "${GITHUB_REPOSITORY:?}" "${GITHUB_SHA:?}" "${GITHUB_REF:?}" "${GITHUB_EVENT_PATH:?}" "${REPOSITORY_ID:?}" "${RUNNER_TEMP:?}" "${GITHUB_STEP_SUMMARY:?}"
[[ "$GITHUB_REPOSITORY" =~ ^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$ ]]
jq -en --argjson jobs "$RESULTS" --argjson decision "$DECISION" \
  --arg repository "$GITHUB_REPOSITORY" --arg repository_id "$REPOSITORY_ID" \
  --arg event "$GITHUB_EVENT_NAME" --arg run_sha "$GITHUB_SHA" '
  ($jobs | keys | sort) == (["classify","fmt","supply-chain","repository-policy","clippy","publish-dry-run","native","native_retention"] | sort) and
  ($decision | type == "object" and .schema_version == 1 and
    (.mode == "full" or .mode == "lightweight") and
    (.reason_code | type == "string" and test("^[a-z0-9-]{1,64}$")) and
    (.base_sha | type == "string" and test("^[0-9a-f]{40}$")) and
    (.head_sha | type == "string" and test("^[0-9a-f]{40}$")) and
    .repository == $repository and .repository_id == $repository_id and
    .event_name == $event and .run_sha == $run_sha and
    (.pr_number | type == "number" and . >= 0 and floor == .) and
    (.positive_metadata | type == "boolean")) and
  (["classify","fmt","supply-chain","repository-policy","native_retention"] | all(.[]; $jobs[.].result == "success")) and
  (if $decision.mode == "full" then
     $decision.positive_metadata == false and
     (["clippy","publish-dry-run","native"] | all(.[]; $jobs[.].result == "success"))
   else
     $decision.positive_metadata == true and $decision.reason_code == "metadata-only-version-bump" and
     $event == "pull_request" and
     (["clippy","publish-dry-run","native"] | all(.[]; $jobs[.].result == "skipped"))
   end)' >/dev/null
mkdir -p "$RUNNER_TEMP/cod108-required"
if [[ "$GITHUB_EVENT_NAME" == pull_request ]]; then
  number="$(jq -er '.number | select(type == "number" and . > 0 and floor == .)' "$GITHUB_EVENT_PATH")"
  [[ "$number" =~ ^[1-9][0-9]*$ ]]
  gh api "repos/$GITHUB_REPOSITORY/pulls/$number" > "$RUNNER_TEMP/cod108-required/pr.json"
  gh api "repos/$GITHUB_REPOSITORY/git/ref/heads/main" > "$RUNNER_TEMP/cod108-required/main.json"
  jq -se --argjson decision "$DECISION" --slurpfile event "$GITHUB_EVENT_PATH" \
    --slurpfile main "$RUNNER_TEMP/cod108-required/main.json" \
    --arg repository "$GITHUB_REPOSITORY" --arg id "$REPOSITORY_ID" '
    def trusted:
      .state == "open" and .draft == false and
      .user.id == 41898282 and .user.login == "github-actions[bot]" and .user.type == "Bot" and
      .base.ref == "main" and .head.ref == "release-please--branches--main--components--codebase-graph" and
      .base.repo.full_name == $repository and .head.repo.full_name == $repository and
      (.base.repo.id | tostring) == $id and (.head.repo.id | tostring) == $id and
      (.labels | type == "array" and any(.[]; .name == "autorelease: pending"));
    length == 1 and (.[0] | ($event | length) == 1 and ($main | length) == 1 and
    $event[0].repository.full_name == $repository and ($event[0].repository.id | tostring) == $id and
    ($event[0].action | IN("opened", "synchronize", "reopened", "edited", "labeled", "unlabeled")) and
    .state == "open" and .number == $event[0].number and .number == $decision.pr_number and
    .base.repo.full_name == $repository and (.base.repo.id | tostring) == $id and
    .base.repo.full_name == $event[0].pull_request.base.repo.full_name and
    .base.repo.id == $event[0].pull_request.base.repo.id and
    .base.ref == $event[0].pull_request.base.ref and
    .head.repo.full_name == $event[0].pull_request.head.repo.full_name and
    .head.repo.id == $event[0].pull_request.head.repo.id and
    (.head.repo.full_name | type == "string" and length > 0) and (.head.repo.id | type == "number" and . > 0) and
    .head.ref == $event[0].pull_request.head.ref and
    .base.sha == $event[0].pull_request.base.sha and .base.sha == $decision.base_sha and
    .head.sha == $event[0].pull_request.head.sha and .head.sha == $decision.head_sha and
    $main[0].ref == "refs/heads/main" and $main[0].object.type == "commit" and
    ($main[0].object.sha | type == "string" and test("^[0-9a-f]{40}$")) and
    (if $decision.mode == "lightweight" then
       trusted and ($event[0].pull_request | trusted) and .base.sha == $main[0].object.sha
     else true end))' "$RUNNER_TEMP/cod108-required/pr.json" >/dev/null
elif [[ "$GITHUB_EVENT_NAME" == push && "$GITHUB_REF" == refs/heads/main ]]; then
  jq -se --argjson decision "$DECISION" --arg repository "$GITHUB_REPOSITORY" --arg id "$REPOSITORY_ID" \
    --arg sha "$GITHUB_SHA" '
    length == 1 and (.[0] | .repository.full_name == $repository and (.repository.id | tostring) == $id and
    .ref == "refs/heads/main" and .before == $decision.base_sha and .after == $decision.head_sha and
    .after == $sha and $decision.pr_number == 0 and $decision.mode == "full" and
    $decision.positive_metadata == false)' "$GITHUB_EVENT_PATH" >/dev/null
else
  echo 'Unsupported required-check event' >&2
  exit 1
fi
jq -nr --argjson decision "$DECISION" --argjson jobs "$RESULTS" '
  "Required CI: \($decision.mode)\nBase: \($decision.base_sha)\nHead: \($decision.head_sha)\nReason: \($decision.reason_code)\nIntentional skips: \([$jobs | to_entries[] | select(.value.result == "skipped") | .key] | join(", "))"' >> "$GITHUB_STEP_SUMMARY"
