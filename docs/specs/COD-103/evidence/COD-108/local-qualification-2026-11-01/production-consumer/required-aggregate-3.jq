def cod108_syntax_check($decision; $id; $repository; $sha): 
    length == 1 and (.[0] | .repository.full_name == $repository and (.repository.id | tostring) == $id and
    .ref == "refs/heads/main" and .before == $decision.base_sha and .after == $decision.head_sha and
    .after == $sha and $decision.pr_number == 0 and $decision.mode == "full" and
    $decision.positive_metadata == false); null
