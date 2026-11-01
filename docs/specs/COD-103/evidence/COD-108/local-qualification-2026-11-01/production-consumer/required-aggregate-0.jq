def cod108_syntax_check($decision; $event; $jobs; $repository; $repository_id; $run_sha): 
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
   end); null
