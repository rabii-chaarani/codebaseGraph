set -euo pipefail
if [[ "$AUTOMATIC" == 'true' ]]; then
  [[ "$VERIFIED_PR" == "$RELEASE_PR" ]] || { echo 'Release PR identity changed.' >&2; exit 1; }
  jq -e --arg sha "$SOURCE_SHA" --arg tag "$RELEASE_TAG" '.source_sha == $sha and .tag_name == $tag' dist/metadata/release-metadata.json >/dev/null
fi
optional_get() {
  local response
  if response="$(gh api "$1" 2>"$RUNNER_TEMP/release-api-error")"; then
    printf '%s' "$response"
  elif jq -e '.status == "404"' <<<"$response" >/dev/null 2>&1; then
    printf 'null'
  else
    cat "$RUNNER_TEMP/release-api-error" >&2
    return 1
  fi
}
# Authenticated listing includes drafts that get-release-by-tag can omit.
releases="$(gh api --paginate --slurp "repos/$GITHUB_REPOSITORY/releases?per_page=100")"
matches="$(jq --arg tag "$RELEASE_TAG" '[.[][] | select(.tag_name == $tag)]' <<<"$releases")"
[[ "$(jq length <<<"$matches")" -le 1 ]] || { echo 'Ambiguous release tag.' >&2; exit 1; }
release="$(jq '.[0] // null' <<<"$matches")"
ref="$(optional_get "repos/$GITHUB_REPOSITORY/git/ref/tags/$RELEASE_TAG")"
if [[ "$ref" == 'null' ]]; then
  [[ "$AUTOMATIC" == 'true' && "$release" == 'null' ]] || { echo 'Existing release tag is missing.' >&2; exit 1; }
  gh api --method POST "repos/$GITHUB_REPOSITORY/git/refs" -f "ref=refs/tags/$RELEASE_TAG" -f "sha=$SOURCE_SHA" >/dev/null || {
    echo 'Cannot create the exact tag. Historical workflow changes may require RELEASE_PUBLISH_TOKEN with Contents and Workflows write permissions.' >&2; exit 1;
  }
  ref="$(gh api "repos/$GITHUB_REPOSITORY/git/ref/tags/$RELEASE_TAG")"
fi
tag_sha="$(jq -r .object.sha <<<"$ref")"
if [[ "$(jq -r .object.type <<<"$ref")" == 'tag' ]]; then
  tag_sha="$(gh api "repos/$GITHUB_REPOSITORY/git/tags/$tag_sha" --jq '.object.sha')"
fi
[[ "$tag_sha" == "$SOURCE_SHA" ]] || { echo 'Existing release tag points to a different commit.' >&2; exit 1; }
if [[ "$release" == 'null' ]]; then
  [[ "$AUTOMATIC" == 'true' ]] || { echo 'Existing-tag recovery requires an existing release.' >&2; exit 1; }
  jq '{tag_name, target_commitish: .source_sha, name: .tag_name, body: .notes,
       draft: true, prerelease: false}' dist/metadata/release-metadata.json > "$RUNNER_TEMP/release-request.json"
  release="$(gh api --method POST "repos/$GITHUB_REPOSITORY/releases" --input "$RUNNER_TEMP/release-request.json")"
fi
jq -e --arg tag "$RELEASE_TAG" '.tag_name == $tag and .prerelease == false and (.draft | type == "boolean") and (.id | type == "number")' <<<"$release" >/dev/null
release_id="$(jq -r .id <<<"$release")"
assets=(dist/release-assets/codebase-graph-*.tar.gz dist/release-assets/codebase-graph-*.tar.gz.sha256)
verify_assets() {
  local listing asset name entry asset_id expected_size
  listing="$(gh api --paginate --slurp "repos/$GITHUB_REPOSITORY/releases/$release_id/assets?per_page=100")" || return 1
  for asset in "${assets[@]}"; do
    [[ -f "$asset" ]] || return 1
    name="$(basename "$asset")"
    expected_size="$(wc -c < "$asset" | tr -d ' ')"
    entry="$(jq --arg name "$name" --argjson size "$expected_size" '[.[][] | select(.name == $name)] | select(length == 1 and .[0].size == $size) | .[0]' <<<"$listing")"
    [[ -n "$entry" ]] || { echo "Missing or wrong-sized release asset: $name" >&2; return 1; }
    asset_id="$(jq -r .id <<<"$entry")"
    gh api -H 'Accept: application/octet-stream' "repos/$GITHUB_REPOSITORY/releases/assets/$asset_id" > "$RUNNER_TEMP/release-asset" || return 1
    cmp -s "$asset" "$RUNNER_TEMP/release-asset" || { echo "Release asset bytes differ: $name" >&2; return 1; }
  done
}
if [[ "$(jq -r .draft <<<"$release")" == 'false' ]]; then
  if [[ "$(jq -r .immutable <<<"$release")" == 'true' ]]; then
    verify_assets || { echo 'Immutable release is incomplete or mismatched; no mutation attempted.' >&2; exit 1; }
    exit 0
  fi
  # Preserve mutable published-release recovery, but skip matching assets.
  if verify_assets; then exit 0; fi
fi
[[ "$SOURCE_SHA" =~ ^[0-9a-f]{40}$ ]] || { echo 'Invalid source SHA.' >&2; exit 1; }
if [[ "$AUTOMATIC" == 'false' ]]; then
  current_ref="$(gh api "repos/$GITHUB_REPOSITORY/git/ref/tags/$RELEASE_TAG")"
  current_tag_sha="$(jq -r .object.sha <<<"$current_ref")"
  if [[ "$(jq -r .object.type <<<"$current_ref")" == 'tag' ]]; then
    current_tag_sha="$(gh api "repos/$GITHUB_REPOSITORY/git/tags/$current_tag_sha" --jq '.object.sha')"
  fi
  [[ "$current_tag_sha" =~ ^[0-9a-f]{40}$ && "$current_tag_sha" == "$SOURCE_SHA" ]] || { echo 'Existing release tag points to a different commit.' >&2; exit 1; }
fi
main_sha="$(gh api "repos/$GITHUB_REPOSITORY/git/ref/heads/main" --jq '.object.sha')"
[[ "$main_sha" =~ ^[0-9a-f]{40}$ ]] || { echo 'Invalid current main SHA.' >&2; exit 1; }
ancestry="$(gh api "repos/$GITHUB_REPOSITORY/compare/$SOURCE_SHA...$main_sha" --jq '.status')"
case "$ancestry" in
  identical|ahead) ;;
  *) echo 'Release source is not in current main history.' >&2; exit 1 ;;
esac
gh release upload "$RELEASE_TAG" "${assets[@]}" --clobber --repo "$GITHUB_REPOSITORY"
verify_assets || { echo 'Remote asset verification failed; release was not published.' >&2; exit 1; }
if [[ "$AUTOMATIC" == 'true' && "$(jq -r .draft <<<"$release")" == 'true' ]]; then
  # Decide latest at publication, not when the draft was first created.
  releases="$(gh api --paginate --slurp "repos/$GITHUB_REPOSITORY/releases?per_page=100")"
  latest="$(jq -r --arg tag "$RELEASE_TAG" \
    '($tag | ltrimstr("v") | split(".") | map(tonumber)) as $version |
     [.[][] | select(.draft == false and .prerelease == false)
      | .tag_name | select(test("^v[0-9]+\\.[0-9]+\\.[0-9]+$"))
      | ltrimstr("v") | split(".") | map(tonumber)] | all(. < $version)' <<<"$releases")"
  jq -n --arg latest "$latest" '{draft:false,make_latest:$latest}' > "$RUNNER_TEMP/release-publish.json"
  gh api --method PATCH "repos/$GITHUB_REPOSITORY/releases/$release_id" --input "$RUNNER_TEMP/release-publish.json" >/dev/null
  gh api "repos/$GITHUB_REPOSITORY/releases/$release_id" | jq -e --arg tag "$RELEASE_TAG" '.tag_name == $tag and .draft == false and .prerelease == false' >/dev/null
fi

