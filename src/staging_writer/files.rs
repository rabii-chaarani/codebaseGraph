use super::connectors::ConnectorRow;
use crate::error::NativeError;
use serde::Serialize;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

pub(super) fn write_json_rows<'a, T: Serialize + 'a>(
    path: &Path,
    rows: impl IntoIterator<Item = &'a T>,
) -> Result<(), NativeError> {
    let mut writer = BufWriter::new(File::create(path)?);
    writer.write_all(b"[")?;
    for (row_index, row) in rows.into_iter().enumerate() {
        if row_index > 0 {
            writer.write_all(b",")?;
        }
        serde_json::to_writer(&mut writer, row)?;
    }
    writer.write_all(b"]\n")?;
    Ok(())
}

pub(super) fn write_csv_rows<'a>(
    path: &Path,
    rows: impl IntoIterator<Item = &'a ConnectorRow>,
) -> Result<(), NativeError> {
    let mut writer = BufWriter::new(File::create(path)?);
    writer.write_all(b"from_id,to_id,role\r\n")?;
    for row in rows {
        writer.write_all(csv_field(&row.from_id).as_bytes())?;
        writer.write_all(b",")?;
        writer.write_all(csv_field(&row.to_id).as_bytes())?;
        writer.write_all(b",")?;
        writer.write_all(csv_field(&row.role).as_bytes())?;
        writer.write_all(b"\r\n")?;
    }
    Ok(())
}

fn csv_field(value: &str) -> String {
    if value
        .chars()
        .any(|character| matches!(character, ',' | '"' | '\n' | '\r'))
    {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

pub(super) fn stage_file_stem(name: &str) -> String {
    let stem = name
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect::<String>()
        .trim_matches('_')
        .to_string();
    if stem.is_empty() {
        "table".to_string()
    } else {
        stem
    }
}

pub(super) fn copy_path(path: &Path) -> String {
    path.to_string_lossy()
        .replace('\\', "/")
        .replace('"', "\\\"")
}
