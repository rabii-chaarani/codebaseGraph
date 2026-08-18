use super::model::{DocumentStat, LexiconEntry, SearchDocument, SearchLayer};
use crate::error::NativeError;
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::Path;

pub(super) fn write_magic(writer: &mut impl Write, magic: &[u8; 8]) -> Result<(), NativeError> {
    writer.write_all(magic)?;
    Ok(())
}

pub(super) fn read_magic(reader: &mut impl Read, expected: &[u8; 8]) -> Result<(), NativeError> {
    let mut actual = [0_u8; 8];
    reader.read_exact(&mut actual)?;
    if actual != *expected {
        return Err(NativeError::InvalidInput(format!(
            "search sidecar has invalid file header: expected {}, found {}",
            String::from_utf8_lossy(expected),
            String::from_utf8_lossy(&actual)
        )));
    }
    Ok(())
}

pub(super) fn write_u32(writer: &mut impl Write, value: u32) -> Result<(), NativeError> {
    writer.write_all(&value.to_le_bytes())?;
    Ok(())
}

pub(super) fn read_u32(reader: &mut impl Read) -> Result<u32, NativeError> {
    let mut bytes = [0_u8; 4];
    reader.read_exact(&mut bytes)?;
    Ok(u32::from_le_bytes(bytes))
}

pub(super) fn read_optional_u32(reader: &mut impl Read) -> Result<Option<u32>, NativeError> {
    let mut bytes = [0_u8; 4];
    match reader.read_exact(&mut bytes) {
        Ok(()) => Ok(Some(u32::from_le_bytes(bytes))),
        Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => Ok(None),
        Err(error) => Err(error.into()),
    }
}

pub(super) fn write_u64(writer: &mut impl Write, value: u64) -> Result<(), NativeError> {
    writer.write_all(&value.to_le_bytes())?;
    Ok(())
}

pub(super) fn read_u64(reader: &mut impl Read) -> Result<u64, NativeError> {
    let mut bytes = [0_u8; 8];
    reader.read_exact(&mut bytes)?;
    Ok(u64::from_le_bytes(bytes))
}

pub(super) fn write_document(
    writer: &mut (impl Write + Seek),
    document: &SearchDocument,
    chunk_limit: usize,
) -> Result<u64, NativeError> {
    let encoded = serde_json::to_vec(document)?;
    if encoded.capacity() > chunk_limit || encoded.len() > u32::MAX as usize {
        return Err(super::memory_budget_error(
            "search_documents",
            chunk_limit,
            encoded.capacity(),
        ));
    }
    let offset = writer.stream_position()?;
    write_u32(writer, encoded.len() as u32)?;
    writer.write_all(&encoded)?;
    Ok(offset)
}

pub(super) fn read_document_at(
    file: &mut File,
    offset: u64,
    max_record_bytes: usize,
) -> Result<SearchDocument, NativeError> {
    file.seek(SeekFrom::Start(offset))?;
    let length = read_u32(file)? as usize;
    if length > max_record_bytes {
        return Err(NativeError::InvalidInput(format!(
            "search document record exceeds configured read limit: {length} > {max_record_bytes}"
        )));
    }
    let mut encoded = Vec::new();
    encoded.try_reserve_exact(length).map_err(|_| {
        super::memory_budget_error("search_read_document", max_record_bytes, length)
    })?;
    encoded.resize(length, 0);
    file.read_exact(&mut encoded)?;
    Ok(serde_json::from_slice(&encoded)?)
}

pub(super) fn write_document_stat(
    writer: &mut impl Write,
    stat: &DocumentStat,
) -> Result<(), NativeError> {
    write_u32(writer, stat.length)?;
    write_u32(writer, stat.index_order)?;
    writer.write_all(&[stat.layer.code(), 0, 0, 0])?;
    Ok(())
}

pub(super) fn read_document_stat_at(
    file: &mut File,
    header_bytes: u64,
    document_id: u64,
) -> Result<DocumentStat, NativeError> {
    let offset = document_id
        .checked_mul(super::model::DOCUMENT_STAT_BYTES)
        .and_then(|value| value.checked_add(header_bytes))
        .ok_or_else(|| {
            NativeError::InvalidInput("search document-stat offset overflow".to_string())
        })?;
    file.seek(SeekFrom::Start(offset))?;
    let length = read_u32(file)?;
    let index_order = read_u32(file)?;
    let mut layer = [0_u8; 4];
    file.read_exact(&mut layer)?;
    let layer = SearchLayer::from_code(layer[0]).ok_or_else(|| {
        NativeError::InvalidInput(format!("invalid search document layer code {}", layer[0]))
    })?;
    Ok(DocumentStat {
        length,
        index_order,
        layer,
    })
}

pub(super) fn write_lexicon_entry(
    writer: &mut impl Write,
    entry: &LexiconEntry,
) -> Result<(), NativeError> {
    let length = u32::try_from(entry.term.len()).map_err(|_| {
        NativeError::InvalidInput("search lexicon term length exceeds u32".to_string())
    })?;
    write_u32(writer, length)?;
    writer.write_all(entry.term.as_bytes())?;
    write_u64(writer, entry.postings_offset)?;
    write_u32(writer, entry.document_frequency)?;
    Ok(())
}

pub(super) fn read_lexicon_entry(
    reader: &mut impl Read,
    max_term_bytes: usize,
) -> Result<Option<LexiconEntry>, NativeError> {
    let Some(length) = read_optional_u32(reader)? else {
        return Ok(None);
    };
    let length = length as usize;
    if length > max_term_bytes {
        return Err(NativeError::InvalidInput(format!(
            "search lexicon term exceeds limit: {length} > {max_term_bytes}"
        )));
    }
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(length)
        .map_err(|_| super::memory_budget_error("search_lexicon_read", max_term_bytes, length))?;
    bytes.resize(length, 0);
    reader.read_exact(&mut bytes)?;
    let term = String::from_utf8(bytes).map_err(|error| {
        NativeError::InvalidInput(format!("invalid UTF-8 search term: {error}"))
    })?;
    Ok(Some(LexiconEntry {
        term,
        postings_offset: read_u64(reader)?,
        document_frequency: read_u32(reader)?,
    }))
}

pub(super) fn sha256_file(path: &Path) -> Result<String, NativeError> {
    let mut file = File::open(path)?;
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    Ok(digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}
