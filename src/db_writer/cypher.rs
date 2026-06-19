pub(super) fn quote_identifier(value: &str) -> String {
    format!("`{}`", value.replace('`', "``"))
}

fn cypher_string(value: &str) -> String {
    value.replace('\\', "\\\\").replace('\'', "\\'")
}

pub(super) fn cypher_string_list(values: &[String]) -> String {
    values
        .iter()
        .map(|value| format!("'{}'", cypher_string(value)))
        .collect::<Vec<_>>()
        .join(", ")
}
