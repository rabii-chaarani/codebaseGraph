pub(crate) fn metadata_payload(source: &str) -> Result<serde_json::Value, String> {
    serde_json::from_str(source)
        .map_err(|error| format!("failed to parse embedded metadata: {error}"))
}

pub(crate) fn filter_architecture_group(
    payload: &mut serde_json::Value,
    group: &str,
) -> Result<(), String> {
    let groups = payload
        .get("groups")
        .and_then(serde_json::Value::as_array)
        .cloned()
        .unwrap_or_default();
    let selected: Vec<serde_json::Value> = groups
        .iter()
        .filter(|value| value.get("name").and_then(serde_json::Value::as_str) == Some(group))
        .cloned()
        .collect();
    if selected.is_empty() {
        let valid = groups
            .iter()
            .filter_map(|value| value.get("name").and_then(serde_json::Value::as_str))
            .collect::<Vec<_>>()
            .join(", ");
        return Err(format!(
            "Unknown architecture query group: {group}. Valid groups: {valid}"
        ));
    }
    if let Some(object) = payload.as_object_mut() {
        object.insert("groups".to_string(), serde_json::Value::Array(selected));
    }
    Ok(())
}
