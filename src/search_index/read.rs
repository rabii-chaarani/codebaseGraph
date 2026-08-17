use super::build::tokenize;
use super::io::{
    read_document_at, read_document_stat_at, read_lexicon_entry, read_magic, read_u32, read_u64,
    sha256_file,
};
use super::model::{
    RankedDocument, SearchLayer, SidecarFileMetadata, BACKEND_NAME, DOCUMENTS_MAGIC,
    DOCUMENTS_SUFFIX, DOCUMENT_LENGTHS_MAGIC, DOCUMENT_LENGTHS_SUFFIX, DOCUMENT_OFFSETS_MAGIC,
    DOCUMENT_OFFSETS_SUFFIX, DOCUMENT_OFFSET_BYTES, DOCUMENT_STAT_BYTES, FORMAT_VERSION,
    LEXICON_MAGIC, LEXICON_SUFFIX, METADATA_SUFFIX, POSTINGS_MAGIC, POSTINGS_SUFFIX, POSTING_BYTES,
    SIDECAR_SUFFIXES,
};
use super::sidecar_path;
use crate::error::NativeError;
use crate::protocol::SearchBackendMetadata;
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet, BinaryHeap};
use std::fs::{self, File};
use std::io::{BufReader, Seek, SeekFrom};
use std::path::Path;

const MAX_TERM_BYTES: usize = 256;
const MAX_QUERY_TERMS: usize = 32;
const MAX_DOCUMENT_RECORD_BYTES: usize = 1024 * 1024;
const MAX_METADATA_BYTES: u64 = 64 * 1024;

pub(crate) fn validate(
    database_path: &Path,
    metadata: &SearchBackendMetadata,
) -> Result<(), NativeError> {
    validate_metadata(database_path, metadata, true)?;
    let mut offsets = File::open(sidecar_path(database_path, DOCUMENT_OFFSETS_SUFFIX))?;
    let mut lengths = File::open(sidecar_path(database_path, DOCUMENT_LENGTHS_SUFFIX))?;
    let mut documents = File::open(sidecar_path(database_path, DOCUMENTS_SUFFIX))?;
    read_magic(&mut offsets, DOCUMENT_OFFSETS_MAGIC)?;
    read_magic(&mut lengths, DOCUMENT_LENGTHS_MAGIC)?;
    read_magic(&mut documents, DOCUMENTS_MAGIC)?;

    let expected_offsets = (DOCUMENT_OFFSETS_MAGIC.len() as u64)
        .checked_add(
            metadata
                .document_count
                .checked_mul(DOCUMENT_OFFSET_BYTES)
                .ok_or_else(|| invalid("search document-offset size overflow"))?,
        )
        .ok_or_else(|| invalid("search document-offset size overflow"))?;
    let expected_lengths = (DOCUMENT_LENGTHS_MAGIC.len() as u64)
        .checked_add(
            metadata
                .document_count
                .checked_mul(DOCUMENT_STAT_BYTES)
                .ok_or_else(|| invalid("search document-length size overflow"))?,
        )
        .ok_or_else(|| invalid("search document-length size overflow"))?;
    if offsets.metadata()?.len() != expected_offsets {
        return Err(invalid("search document-offset table has invalid length"));
    }
    if lengths.metadata()?.len() != expected_lengths {
        return Err(invalid("search document-length table has invalid length"));
    }
    for document_id in 0..metadata.document_count {
        let offset = read_u64(&mut offsets)?;
        let stat = read_document_stat_at(
            &mut lengths,
            DOCUMENT_LENGTHS_MAGIC.len() as u64,
            document_id,
        )?;
        let document = read_document_at(&mut documents, offset, MAX_DOCUMENT_RECORD_BYTES)?;
        if document.index_order != stat.index_order || document.layer != stat.layer {
            return Err(invalid(
                "search document metadata does not match length table",
            ));
        }
    }
    validate_lexicon_and_postings(database_path, metadata)
}

pub(crate) fn search(
    database_path: &Path,
    metadata: &SearchBackendMetadata,
    query: &str,
    layer: &str,
    limit: usize,
) -> Result<Vec<RankedDocument>, NativeError> {
    if limit == 0 || metadata.document_count == 0 {
        return Ok(Vec::new());
    }
    validate_metadata(database_path, metadata, false)?;
    let requested_layer = match layer {
        "semantic" => SearchLayer::Semantic,
        "syntax" => SearchLayer::Syntax,
        _ => {
            return Err(NativeError::InvalidInput(format!(
                "sidecar search requires semantic or syntax layer, received {layer}"
            )))
        }
    };
    let mut query_terms = BTreeSet::new();
    tokenize(query, |term| {
        if query_terms.len() < MAX_QUERY_TERMS || query_terms.contains(&term) {
            query_terms.insert(term);
        }
        Ok(())
    })?;
    if query_terms.is_empty() {
        return Ok(Vec::new());
    }
    let lexicon_entries = find_lexicon_entries(database_path, &query_terms)?;
    if lexicon_entries.is_empty() {
        return Ok(Vec::new());
    }

    let postings_path = sidecar_path(database_path, POSTINGS_SUFFIX);
    let mut cursors = Vec::new();
    for entry in lexicon_entries.values() {
        cursors.push(PostingCursor::open(
            &postings_path,
            entry.postings_offset,
            entry.document_frequency,
            inverse_document_frequency(metadata.document_count, entry.document_frequency),
        )?);
    }
    let mut heap = BinaryHeap::new();
    for (cursor_index, cursor) in cursors.iter_mut().enumerate() {
        if let Some(posting) = cursor.next()? {
            heap.push(HeapPosting {
                document_id: posting.0,
                term_frequency: posting.1,
                cursor_index,
            });
        }
    }

    let mut lengths = File::open(sidecar_path(database_path, DOCUMENT_LENGTHS_SUFFIX))?;
    read_magic(&mut lengths, DOCUMENT_LENGTHS_MAGIC)?;
    let average_length = if metadata.document_count == 0 {
        0.0
    } else {
        metadata.total_tokens as f64 / metadata.document_count as f64
    };
    let mut best = Vec::new();
    while let Some(first) = heap.pop() {
        let document_id = first.document_id;
        let mut postings = vec![first];
        while heap
            .peek()
            .is_some_and(|posting| posting.document_id == document_id)
        {
            if let Some(posting) = heap.pop() {
                postings.push(posting);
            }
        }
        let stat = read_document_stat_at(
            &mut lengths,
            DOCUMENT_LENGTHS_MAGIC.len() as u64,
            document_id,
        )?;
        let mut score = 0.0_f64;
        for posting in postings {
            score += bm25_term_score(
                posting.term_frequency,
                stat.length,
                average_length,
                cursors[posting.cursor_index].idf,
            );
            if let Some(next) = cursors[posting.cursor_index].next()? {
                heap.push(HeapPosting {
                    document_id: next.0,
                    term_frequency: next.1,
                    cursor_index: posting.cursor_index,
                });
            }
        }
        if stat.layer == requested_layer {
            retain_best(
                &mut best,
                Candidate {
                    document_id,
                    index_order: stat.index_order,
                    score: round6(score),
                },
                limit,
            );
        }
    }
    best.sort_by(candidate_order);

    let mut offsets = File::open(sidecar_path(database_path, DOCUMENT_OFFSETS_SUFFIX))?;
    let mut documents = File::open(sidecar_path(database_path, DOCUMENTS_SUFFIX))?;
    read_magic(&mut offsets, DOCUMENT_OFFSETS_MAGIC)?;
    read_magic(&mut documents, DOCUMENTS_MAGIC)?;
    let mut results = Vec::new();
    results.try_reserve_exact(best.len()).map_err(|_| {
        super::memory_budget_error(
            "search_results",
            limit.saturating_mul(std::mem::size_of::<RankedDocument>()),
            best.len()
                .saturating_mul(std::mem::size_of::<RankedDocument>()),
        )
    })?;
    for candidate in best {
        let offset_position = (DOCUMENT_OFFSETS_MAGIC.len() as u64)
            .checked_add(
                candidate
                    .document_id
                    .checked_mul(DOCUMENT_OFFSET_BYTES)
                    .ok_or_else(|| invalid("search document-offset position overflow"))?,
            )
            .ok_or_else(|| invalid("search document-offset position overflow"))?;
        offsets.seek(SeekFrom::Start(offset_position))?;
        let document_offset = read_u64(&mut offsets)?;
        let document =
            read_document_at(&mut documents, document_offset, MAX_DOCUMENT_RECORD_BYTES)?;
        results.push(RankedDocument {
            id: document.id,
            node_type: document.node_type,
            index_order: document.index_order as usize,
            layer: document.layer.as_str().to_string(),
            score: candidate.score,
        });
    }
    Ok(results)
}

fn validate_metadata(
    database_path: &Path,
    metadata: &SearchBackendMetadata,
    checksums: bool,
) -> Result<(), NativeError> {
    if metadata.backend != BACKEND_NAME || metadata.format_version != FORMAT_VERSION as u64 {
        return Err(invalid("unsupported search sidecar backend metadata"));
    }
    let metadata_path = sidecar_path(database_path, METADATA_SUFFIX);
    if fs::metadata(&metadata_path)?.len() > MAX_METADATA_BYTES {
        return Err(invalid("search sidecar metadata exceeds the read limit"));
    }
    let file_metadata: SidecarFileMetadata = serde_json::from_slice(&fs::read(metadata_path)?)?;
    if file_metadata.backend != metadata.backend
        || file_metadata.format_version != metadata.format_version
        || file_metadata.document_count != metadata.document_count
        || file_metadata.term_count != metadata.term_count
        || file_metadata.total_tokens != metadata.total_tokens
    {
        return Err(invalid(
            "search sidecar file metadata does not match manifest metadata",
        ));
    }
    for suffix in SIDECAR_SUFFIXES {
        let expected = metadata
            .files
            .get(suffix)
            .ok_or_else(|| invalid(&format!("search backend metadata is missing {suffix}")))?;
        let path = sidecar_path(database_path, suffix);
        if !path.is_file() {
            return Err(invalid(&format!(
                "search sidecar file is missing: {}",
                path.display()
            )));
        }
        if checksums && sha256_file(&path)? != *expected {
            return Err(invalid(&format!(
                "search sidecar checksum mismatch: {}",
                path.display()
            )));
        }
    }
    Ok(())
}

fn validate_lexicon_and_postings(
    database_path: &Path,
    metadata: &SearchBackendMetadata,
) -> Result<(), NativeError> {
    let mut lexicon = BufReader::new(File::open(sidecar_path(database_path, LEXICON_SUFFIX))?);
    let mut postings = File::open(sidecar_path(database_path, POSTINGS_SUFFIX))?;
    read_magic(&mut lexicon, LEXICON_MAGIC)?;
    read_magic(&mut postings, POSTINGS_MAGIC)?;
    let postings_length = postings.metadata()?.len();
    let mut previous_term: Option<String> = None;
    let mut expected_offset = POSTINGS_MAGIC.len() as u64;
    let mut term_count = 0_u64;
    while let Some(entry) = read_lexicon_entry(&mut lexicon, MAX_TERM_BYTES)? {
        if previous_term
            .as_ref()
            .is_some_and(|term| term >= &entry.term)
        {
            return Err(invalid("search lexicon is not strictly sorted"));
        }
        if entry.postings_offset != expected_offset {
            return Err(invalid(
                "search lexicon has a non-contiguous postings offset",
            ));
        }
        postings.seek(SeekFrom::Start(entry.postings_offset))?;
        let mut previous_document = None;
        for _ in 0..entry.document_frequency {
            let document_id = read_u64(&mut postings)?;
            let term_frequency = read_u32(&mut postings)?;
            if document_id >= metadata.document_count || term_frequency == 0 {
                return Err(invalid("search posting contains invalid values"));
            }
            if previous_document.is_some_and(|previous| previous >= document_id) {
                return Err(invalid("search postings are not strictly document-sorted"));
            }
            previous_document = Some(document_id);
        }
        expected_offset = expected_offset
            .checked_add(entry.document_frequency as u64 * POSTING_BYTES)
            .ok_or_else(|| invalid("search postings size overflow"))?;
        previous_term = Some(entry.term);
        term_count = term_count
            .checked_add(1)
            .ok_or_else(|| invalid("search term count overflow"))?;
    }
    if term_count != metadata.term_count || expected_offset != postings_length {
        return Err(invalid(
            "search lexicon/postings counts do not match metadata",
        ));
    }
    Ok(())
}

fn find_lexicon_entries(
    database_path: &Path,
    query_terms: &BTreeSet<String>,
) -> Result<BTreeMap<String, super::model::LexiconEntry>, NativeError> {
    let mut reader = BufReader::new(File::open(sidecar_path(database_path, LEXICON_SUFFIX))?);
    read_magic(&mut reader, LEXICON_MAGIC)?;
    let mut found = BTreeMap::new();
    let last_query = query_terms
        .iter()
        .next_back()
        .map(String::as_str)
        .unwrap_or("");
    while let Some(entry) = read_lexicon_entry(&mut reader, MAX_TERM_BYTES)? {
        if query_terms.contains(&entry.term) {
            found.insert(entry.term.clone(), entry);
        } else if entry.term.as_str() > last_query {
            break;
        }
    }
    Ok(found)
}

struct PostingCursor {
    file: File,
    remaining: u32,
    idf: f64,
}

impl PostingCursor {
    fn open(path: &Path, offset: u64, count: u32, idf: f64) -> Result<Self, NativeError> {
        let mut file = File::open(path)?;
        file.seek(SeekFrom::Start(offset))?;
        Ok(Self {
            file,
            remaining: count,
            idf,
        })
    }

    fn next(&mut self) -> Result<Option<(u64, u32)>, NativeError> {
        if self.remaining == 0 {
            return Ok(None);
        }
        let document_id = read_u64(&mut self.file)?;
        let term_frequency = read_u32(&mut self.file)?;
        self.remaining -= 1;
        Ok(Some((document_id, term_frequency)))
    }
}

#[derive(Eq, PartialEq)]
struct HeapPosting {
    document_id: u64,
    term_frequency: u32,
    cursor_index: usize,
}

impl Ord for HeapPosting {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .document_id
            .cmp(&self.document_id)
            .then_with(|| other.cursor_index.cmp(&self.cursor_index))
    }
}

impl PartialOrd for HeapPosting {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Clone, Debug)]
struct Candidate {
    document_id: u64,
    index_order: u32,
    score: f64,
}

fn candidate_order(left: &Candidate, right: &Candidate) -> Ordering {
    right
        .score
        .partial_cmp(&left.score)
        .unwrap_or(Ordering::Equal)
        .then_with(|| left.index_order.cmp(&right.index_order))
        .then_with(|| left.document_id.cmp(&right.document_id))
}

fn retain_best(best: &mut Vec<Candidate>, candidate: Candidate, limit: usize) {
    best.push(candidate);
    if best.len() <= limit {
        return;
    }
    if let Some(worst) = best
        .iter()
        .enumerate()
        .max_by(|(_, left), (_, right)| candidate_order(left, right))
        .map(|(index, _)| index)
    {
        best.swap_remove(worst);
    }
}

fn inverse_document_frequency(document_count: u64, document_frequency: u32) -> f64 {
    let n = document_count as f64;
    let df = document_frequency as f64;
    (1.0 + ((n - df + 0.5) / (df + 0.5))).ln()
}

fn bm25_term_score(term_frequency: u32, length: u32, average_length: f64, idf: f64) -> f64 {
    const K1: f64 = 1.2;
    const B: f64 = 0.75;
    let tf = term_frequency as f64;
    let length_ratio = if average_length > 0.0 {
        length as f64 / average_length
    } else {
        0.0
    };
    idf * ((tf * (K1 + 1.0)) / (tf + K1 * (1.0 - B + B * length_ratio)))
}

fn round6(value: f64) -> f64 {
    (value * 1_000_000.0).round() / 1_000_000.0
}

fn invalid(message: &str) -> NativeError {
    NativeError::InvalidInput(message.to_string())
}
