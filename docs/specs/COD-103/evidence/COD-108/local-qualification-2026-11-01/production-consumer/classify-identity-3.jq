def cod108_syntax_check: [.before, .after] | if all(.[]; type == "string") then @tsv else error("Incomplete push") end; null
