set -euo pipefail
: "${RUNNER_TEMP:?}" "${GITHUB_OUTPUT:?}" "${GITHUB_STEP_SUMMARY:?}" "${GITHUB_WORKSPACE:?}" "${CANDIDATE:?}"
decision="$RUNNER_TEMP/cod108/decision.json"
if [[ "$CANDIDATE" == true ]]; then
  base="$(jq -er '.base_sha' "$decision")"
  head="$(jq -er '.head_sha' "$decision")"
  reason=classifier-unavailable
  # Only protected B supplies executable code, manifests, lockfile and build context.
  if [[ "$TRUSTED_BASE_OUTCOME" == success && "$OBJECT_STORE_OUTCOME" == success &&
        -f "$GITHUB_WORKSPACE/trusted-base/crates/xtask/src/ci.rs" ]]; then
    if git -C "$GITHUB_WORKSPACE/git-objects" -c protocol.file.allow=never fetch --no-tags --no-recurse-submodules \
         "https://github.com/$GITHUB_REPOSITORY.git" "$head" &&
       (cd "$GITHUB_WORKSPACE/trusted-base" &&
        CARGO_TARGET_DIR="$RUNNER_TEMP/cod108/trusted-target" cargo run -p xtask --locked -- \
          ci-mode --repo "$GITHUB_WORKSPACE/git-objects" --base-sha "$base" --head-sha "$head") \
         > "$RUNNER_TEMP/cod108/classifier.json"; then
      if jq -se --arg base "$base" --arg head "$head" '
        length == 1 and (.[0] |
          type == "object" and .schema_version == 1 and
          (.mode == "full" or .mode == "lightweight") and
          (.reason_code | type == "string" and test("^[a-z0-9-]{1,64}$")) and
          .base_sha == $base and .head_sha == $head and
          (.mode != "lightweight" or .reason_code == "metadata-only-version-bump"))' \
        "$RUNNER_TEMP/cod108/classifier.json" >/dev/null; then
        jq --slurpfile classified "$RUNNER_TEMP/cod108/classifier.json" '
          .mode = $classified[0].mode | .reason_code = $classified[0].reason_code |
          .positive_metadata = (.mode == "lightweight")' "$decision" > "$decision.next"
        mv "$decision.next" "$decision"
        reason=
      else
        reason=classifier-invalid-output
      fi
    else
      reason=classifier-acquisition-failed
    fi
  fi
  if [[ -n "$reason" ]]; then
    jq --arg reason "$reason" '.mode="full" | .positive_metadata=false | .reason_code=$reason' \
      "$decision" > "$decision.next"
    mv "$decision.next" "$decision"
  fi
fi
jq -e '.schema_version == 1 and (.mode == "full" or .mode == "lightweight")' "$decision" >/dev/null
printf 'mode=%s\ndecision=%s\n' "$(jq -r '.mode' "$decision")" "$(jq -c . "$decision")" >> "$GITHUB_OUTPUT"
jq -r '"CI mode: \(.mode)\nBase: \(.base_sha)\nHead: \(.head_sha)\nReason: \(.reason_code)"' "$decision" >> "$GITHUB_STEP_SUMMARY"
