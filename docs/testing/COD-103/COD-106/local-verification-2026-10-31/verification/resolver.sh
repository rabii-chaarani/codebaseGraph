set -euo pipefail
tag=''
source_sha="$VERIFIED_SHA"
ci_run_id="$CI_RUN_ID"
should_publish=false
automatic=false
artifact_source=promote
dry_run=false
[[ "$GITHUB_EVENT_NAME" != 'workflow_dispatch' ]] || dry_run="$MANUAL_DRY_RUN"
if [[ -n "$MANUAL_TAG" ]]; then
  tag="$MANUAL_TAG"
  [[ "$tag" =~ ^v[0-9]+\.[0-9]+\.[0-9]+$ ]] || { echo 'Invalid release tag.' >&2; exit 1; }
  ref="$(gh api "repos/$GITHUB_REPOSITORY/git/ref/tags/$tag")"
  source_sha="$(jq -r .object.sha <<<"$ref")"
  if [[ "$(jq -r .object.type <<<"$ref")" == 'tag' ]]; then
    source_sha="$(gh api "repos/$GITHUB_REPOSITORY/git/tags/$source_sha" --jq '.object.sha')"
  fi
  [[ "$source_sha" =~ ^[0-9a-f]{40}$ ]] || { echo 'Invalid source SHA.' >&2; exit 1; }
  main_sha="$(gh api "repos/$GITHUB_REPOSITORY/git/ref/heads/main" --jq '.object.sha')"
  [[ "$main_sha" =~ ^[0-9a-f]{40}$ ]] || { echo 'Invalid current main SHA.' >&2; exit 1; }
  ancestry="$(gh api "repos/$GITHUB_REPOSITORY/compare/$source_sha...$main_sha" --jq '.status')"
  case "$ancestry" in
    identical|ahead) ;;
    *) echo 'Release source is not in current main history.' >&2; exit 1 ;;
  esac
  ci_run_id=''
  should_publish=true
  artifact_source="$MANUAL_SOURCE"
elif [[ -n "$RELEASE_PR" ]]; then
  manifest="$(gh api "repos/$GITHUB_REPOSITORY/contents/.release-please-manifest.json?ref=$source_sha" --jq .content | base64 --decode)"
  version="$(jq -er '.["."]' <<<"$manifest")"
  tag="v$version"
  [[ "$tag" =~ ^v[0-9]+\.[0-9]+\.[0-9]+$ ]] || { echo 'Invalid release manifest version.' >&2; exit 1; }
  automatic=true
  should_publish=true
fi
[[ "$source_sha" =~ ^[0-9a-f]{40}$ ]] || { echo 'Invalid source SHA.' >&2; exit 1; }
publish_assets=false
[[ "$should_publish" != 'true' || "$dry_run" == 'true' ]] || publish_assets=true
{
  echo "should-publish=$should_publish"
  echo "automatic=$automatic"
  echo "tag-name=$tag"
  echo "version=${tag#v}"
  echo "source-sha=$source_sha"
  echo "publish_assets=$publish_assets"
  echo "artifact-source=$artifact_source"
  echo "ci-run-id=$ci_run_id"
  echo "dry-run=$dry_run"
} >> "$GITHUB_OUTPUT"
echo "Source: $source_sha; CI: $ci_run_id; release PR: ${RELEASE_PR:-none}; tag: ${tag:-none}; dry run: $dry_run" >> "$GITHUB_STEP_SUMMARY"
