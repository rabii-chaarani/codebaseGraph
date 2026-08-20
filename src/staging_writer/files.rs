use std::io::{self, Write};
use std::path::Path;

pub(super) fn write_csv_field(writer: &mut dyn Write, value: &str) -> io::Result<()> {
    if !value
        .bytes()
        .any(|byte| matches!(byte, b',' | b'"' | b'\n' | b'\r'))
    {
        return writer.write_all(value.as_bytes());
    }
    writer.write_all(b"\"")?;
    let mut start = 0;
    for (index, byte) in value.bytes().enumerate() {
        if byte != b'"' {
            continue;
        }
        writer.write_all(&value.as_bytes()[start..index])?;
        writer.write_all(b"\"\"")?;
        start = index + 1;
    }
    writer.write_all(&value.as_bytes()[start..])?;
    writer.write_all(b"\"")
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
    let mut copy_path = path.to_string_lossy().replace('\\', "/");
    if let Some(stripped) = copy_path.strip_prefix("//?/UNC/") {
        copy_path = format!("//{stripped}");
    } else if let Some(stripped) = copy_path.strip_prefix("//?/") {
        copy_path = stripped.to_string();
    }
    copy_path.replace('"', "\\\"")
}
