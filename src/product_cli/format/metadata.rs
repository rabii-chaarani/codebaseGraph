use crate::product_cli::graph::MetadataOutputOptions;
use std::io::Write;

pub(in crate::product_cli) fn metadata_payload(source: &str) -> Result<serde_json::Value, String> {
    serde_json::from_str(source)
        .map_err(|error| format!("failed to parse embedded metadata: {error}"))
}

pub(in crate::product_cli) fn write_metadata_output<W: Write>(
    stdout: &mut W,
    payload: &serde_json::Value,
    options: &MetadataOutputOptions,
    block_serializer: fn(&serde_json::Value) -> String,
) -> Result<(), String> {
    let text = if options.format == "json" {
        if options.pretty {
            serde_json::to_string_pretty(payload).map_err(|error| error.to_string())?
        } else {
            serde_json::to_string(payload).map_err(|error| error.to_string())?
        }
    } else {
        block_serializer(payload)
    };
    writeln!(stdout, "{text}").map_err(|error| error.to_string())
}

pub(in crate::product_cli) fn filter_architecture_group(
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
