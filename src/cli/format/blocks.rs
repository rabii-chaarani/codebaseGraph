pub(crate) fn serialize_error_block(payload: &serde_json::Value) -> String {
    let error = payload.get("error").unwrap_or(payload);
    format!(
        "error tool={} type={} message={}\n",
        block_value(value_str(error, "tool")),
        block_value(value_str(error, "type")),
        block_value(value_str(error, "message"))
    )
}

fn value_str<'a>(payload: &'a serde_json::Value, key: &str) -> &'a str {
    payload
        .get(key)
        .and_then(serde_json::Value::as_str)
        .unwrap_or("")
}

fn block_value(value: &str) -> String {
    if value.is_empty() {
        "\"\"".to_string()
    } else if value.chars().all(|character| {
        character.is_ascii_alphanumeric() || matches!(character, '_' | '-' | '.' | '/' | ':')
    }) {
        value.to_string()
    } else {
        serde_json::to_string(value).unwrap_or_else(|_| "\"\"".to_string())
    }
}
