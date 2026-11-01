def cod108_syntax_check($decision; $event; $id; $main; $repository): 
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
     else true end)); null
