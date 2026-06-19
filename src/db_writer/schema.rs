pub(super) fn schema_statements(include_fts: bool, provided: Vec<String>) -> Vec<String> {
    if !provided.is_empty() {
        return provided;
    }
    let mut statements = vec!["INSTALL json".to_string(), "LOAD json".to_string()];
    if include_fts {
        statements.extend(["INSTALL fts".to_string(), "LOAD fts".to_string()]);
    }
    statements
}
