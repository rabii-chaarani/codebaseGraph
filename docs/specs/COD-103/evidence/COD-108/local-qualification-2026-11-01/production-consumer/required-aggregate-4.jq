def cod108_syntax_check($decision; $jobs): 
  "Required CI: \($decision.mode)\nBase: \($decision.base_sha)\nHead: \($decision.head_sha)\nReason: \($decision.reason_code)\nIntentional skips: \([$jobs | to_entries[] | select(.value.result == "skipped") | .key] | join(", "))"; null
