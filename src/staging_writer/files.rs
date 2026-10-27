use crate::error::NativeError;
use std::fs::File;
use std::io::{self, BufWriter, Write};
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

pub(crate) fn copy_path(path: &Path) -> String {
    let mut copy_path = path.to_string_lossy().replace('\\', "/");
    if let Some(stripped) = copy_path.strip_prefix("//?/UNC/") {
        copy_path = format!("//{stripped}");
    } else if let Some(stripped) = copy_path.strip_prefix("//?/") {
        copy_path = stripped.to_string();
    }
    copy_path.replace('"', "\\\"")
}

/// Tracking is optional producer metadata. Only the factory's genuine new-file
/// token may produce Origin; ordinary caller-owned or preexisting paths retain
/// their historical File::create behavior without being adopted.
pub(super) enum StagingOutputFile {
    Tracked(Box<crate::storage::maintenance_controls::WorkspaceOutputFile>),
    Plain(File),
}
impl StagingOutputFile {
    pub(super) fn create(path: &Path) -> Result<Self, NativeError> {
        match crate::storage::maintenance_controls::WorkspaceOutputFile::create_new(path) {
            Ok(Some(output)) => {
                let _ =
                    crate::storage::maintenance_origin::record_created_workspace_output(&output);
                Ok(Self::Tracked(Box::new(output)))
            }
            Ok(None) | Err(_) => Ok(Self::Plain(File::create(path)?)),
        }
    }
    fn finish(self) {
        if let Self::Tracked(output) = self {
            if let Ok(completed) = (*output).finish() {
                // Keep the actual completed-output guards through receipt
                // publication. Failure preserves the primary staging result;
                // the unproven output remains ineligible for collection.
                let _ = crate::storage::maintenance_origin::record_completed_workspace_output(
                    &completed,
                );
            }
        }
    }
}
impl Write for StagingOutputFile {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        match self {
            Self::Tracked(output) => output.write(bytes),
            Self::Plain(output) => output.write(bytes),
        }
    }
    fn flush(&mut self) -> std::io::Result<()> {
        match self {
            Self::Tracked(output) => output.flush(),
            Self::Plain(output) => output.flush(),
        }
    }
}
pub(super) fn finish_output(mut writer: BufWriter<StagingOutputFile>) -> Result<(), NativeError> {
    writer.flush()?;
    let output = writer.into_inner().map_err(|error| error.into_error())?;
    output.finish();
    Ok(())
}

/// Only an actual exclusive directory-creation token may attest namespace
/// birth. Existing/unregistered paths preserve primary creation behavior.
pub(super) fn create_staging_directory(path: &Path) -> Result<(), NativeError> {
    match crate::storage::maintenance_controls::WorkspaceCreatedDirectory::create_new(path) {
        Ok(Some(created)) => {
            let _ =
                crate::storage::maintenance_origin::record_created_workspace_directory(&created);
            Ok(())
        }
        Ok(None) | Err(_) => Ok(std::fs::create_dir_all(path)?),
    }
}
