def cod108_syntax_check: 
    [.pull_request.base.sha, .pull_request.head.sha, (.number | tostring)] |
    if all(.[]; type == "string") then @tsv else error("Incomplete PR event") end; null
