def cod108_syntax_check: .number | select(type == "number" and . > 0 and floor == .); null
