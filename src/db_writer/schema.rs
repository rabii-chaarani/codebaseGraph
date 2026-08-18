#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct DatabaseStatementPhases<'a> {
    pub(super) pre_copy: Vec<&'a str>,
    pub(super) post_copy: Vec<&'a str>,
}

pub(super) fn statement_phases<'a>(
    include_fts: bool,
    provided: &'a [String],
) -> DatabaseStatementPhases<'a> {
    let mut phases = DatabaseStatementPhases::default();
    if provided.is_empty() {
        phases.pre_copy.extend(["INSTALL json", "LOAD json"]);
        if include_fts {
            phases.pre_copy.extend(["INSTALL fts", "LOAD fts"]);
        }
        return phases;
    }

    phases.pre_copy.reserve(provided.len());
    for statement in provided {
        if !include_fts && is_fts_statement(statement) {
            continue;
        }
        if is_post_copy_statement(statement) {
            phases.post_copy.push(statement);
        } else {
            phases.pre_copy.push(statement);
        }
    }
    phases
}

fn is_fts_statement(statement: &str) -> bool {
    let statement = statement.trim().trim_end_matches(';').trim_end();
    statement.eq_ignore_ascii_case("INSTALL fts")
        || statement.eq_ignore_ascii_case("LOAD fts")
        || is_post_copy_statement(statement)
}

fn is_post_copy_statement(statement: &str) -> bool {
    statement
        .trim_start()
        .get(.."CALL CREATE_FTS_INDEX".len())
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("CALL CREATE_FTS_INDEX"))
}

#[cfg(test)]
mod tests {
    use super::{statement_phases, DatabaseStatementPhases};

    #[test]
    fn classifies_fts_indexes_after_copy_without_reordering_each_phase() {
        let statements = vec![
            "INSTALL json".to_string(),
            "  call create_fts_index('File', 'files', ['label'])".to_string(),
            "LOAD json".to_string(),
            "CREATE NODE TABLE `File` (`id` STRING PRIMARY KEY)".to_string(),
            "CALL CREATE_FTS_INDEX('Symbol', 'symbols', ['label'])".to_string(),
        ];

        assert_eq!(
            statement_phases(true, &statements),
            DatabaseStatementPhases {
                pre_copy: vec![
                    "INSTALL json",
                    "LOAD json",
                    "CREATE NODE TABLE `File` (`id` STRING PRIMARY KEY)",
                ],
                post_copy: vec![
                    "  call create_fts_index('File', 'files', ['label'])",
                    "CALL CREATE_FTS_INDEX('Symbol', 'symbols', ['label'])",
                ],
            }
        );
    }

    #[test]
    fn extension_setup_is_always_pre_copy() {
        assert_eq!(
            statement_phases(true, &[]),
            DatabaseStatementPhases {
                pre_copy: vec!["INSTALL json", "LOAD json", "INSTALL fts", "LOAD fts"],
                post_copy: Vec::new(),
            }
        );
    }

    #[test]
    fn disabled_fts_filters_provided_extension_and_index_statements() {
        let statements = vec![
            "INSTALL json".to_string(),
            " INSTALL fts; ".to_string(),
            "LOAD fts;".to_string(),
            "CREATE NODE TABLE `File` (`id` STRING PRIMARY KEY)".to_string(),
            "CALL CREATE_FTS_INDEX('File', 'files', ['label'])".to_string(),
        ];

        assert_eq!(
            statement_phases(false, &statements),
            DatabaseStatementPhases {
                pre_copy: vec![
                    "INSTALL json",
                    "CREATE NODE TABLE `File` (`id` STRING PRIMARY KEY)",
                ],
                post_copy: Vec::new(),
            }
        );
    }
}
