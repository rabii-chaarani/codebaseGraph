def cod108_syntax_check($event; $id; $main; $repository): 
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
      .base.sha == $main[0].object.sha); null
