use super::io::{
    read_magic, sha256_file, write_document, write_document_stat, write_lexicon_entry, write_magic,
    write_u32, write_u64,
};
use super::model::{
    DocumentStat, LexiconEntry, SearchDocument, SearchLayer, SidecarFileMetadata, StagedNode,
    BACKEND_NAME, DOCUMENTS_MAGIC, DOCUMENTS_SUFFIX, DOCUMENT_LENGTHS_MAGIC,
    DOCUMENT_LENGTHS_SUFFIX, DOCUMENT_OFFSETS_MAGIC, DOCUMENT_OFFSETS_SUFFIX, FORMAT_VERSION,
    LEXICON_MAGIC, LEXICON_SUFFIX, METADATA_SUFFIX, OCCURRENCES_MAGIC, POSTINGS_MAGIC,
    POSTINGS_SUFFIX, SIDECAR_SUFFIXES,
};
use super::{memory_budget_error, sidecar_path};
use crate::error::NativeError;
use crate::protocol::SearchBackendMetadata;
use serde::de::{DeserializeSeed, Error as DeError, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer};
use std::cmp::Ordering;
use std::collections::BinaryHeap;
use std::fmt;
use std::fs::{self, File};
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};

const MAX_TERM_BYTES: usize = 256;
const MAX_MERGE_FAN_IN: usize = 32;
const MIN_CHUNK_BYTES: usize = 4096;
const MERGE_READER_ACCOUNTED_BYTES: usize = 768;
const MERGE_READER_BUFFER_BYTES: usize = 256;

#[derive(Clone, Copy, Debug)]
pub(crate) struct SearchIndexBuildRequest<'a> {
    pub(crate) db_path: &'a Path,
    pub(crate) staging_dir: &'a Path,
    pub(crate) chunk_bytes: usize,
}

#[derive(Clone, Debug)]
pub(crate) struct SearchIndexBuildResult {
    pub(crate) metadata: SearchBackendMetadata,
    pub(crate) spill_bytes: u64,
    pub(crate) high_water_bytes: u64,
}

pub(crate) fn build(
    request: SearchIndexBuildRequest<'_>,
) -> Result<SearchIndexBuildResult, NativeError> {
    if request.chunk_bytes < MIN_CHUNK_BYTES {
        return Err(memory_budget_error(
            "search_index_build",
            request.chunk_bytes,
            MIN_CHUNK_BYTES,
        ));
    }
    ensure_targets_absent(request.db_path)?;
    let run_root = PathBuf::from(format!(
        "{}.search-runs-{}",
        request.db_path.display(),
        std::process::id()
    ));
    fs::create_dir_all(&run_root)?;

    let result = build_inner(request, &run_root);
    let cleanup = fs::remove_dir_all(&run_root);
    match (result, cleanup) {
        (Ok(result), Ok(())) => Ok(result),
        (Ok(_), Err(error)) => Err(error.into()),
        (Err(error), _) => Err(error),
    }
}

fn build_inner(
    request: SearchIndexBuildRequest<'_>,
    run_root: &Path,
) -> Result<SearchIndexBuildResult, NativeError> {
    let schema: SearchSchema =
        serde_json::from_str(include_str!("../../assets/graph_schema.json"))?;
    let mut occurrences = OccurrenceSorter::new(run_root, request.chunk_bytes)?;
    let mut documents = BufWriter::new(File::create(sidecar_path(
        request.db_path,
        DOCUMENTS_SUFFIX,
    ))?);
    let mut document_offsets = BufWriter::new(File::create(sidecar_path(
        request.db_path,
        DOCUMENT_OFFSETS_SUFFIX,
    ))?);
    let mut document_lengths = BufWriter::new(File::create(sidecar_path(
        request.db_path,
        DOCUMENT_LENGTHS_SUFFIX,
    ))?);
    write_magic(&mut documents, DOCUMENTS_MAGIC)?;
    write_magic(&mut document_offsets, DOCUMENT_OFFSETS_MAGIC)?;
    write_magic(&mut document_lengths, DOCUMENT_LENGTHS_MAGIC)?;

    let mut document_count = 0_u64;
    let mut total_tokens = 0_u64;
    let mut index_order = 0_u32;
    for index in schema.search_indexes {
        let layer = SearchLayer::from_schema(&index.layer);
        for node_type in index.node_types {
            for staged_path in StagedNodeChunks::new(request.staging_dir, &node_type) {
                stream_staged_nodes(&staged_path, |node| {
                    let document_id = document_count;
                    let mut document_length = 0_u32;
                    for field in &index.fields {
                        tokenize(node.field(field), |term| {
                            occurrences.push(Occurrence { term, document_id })?;
                            document_length = document_length.checked_add(1).ok_or_else(|| {
                                NativeError::InvalidInput(
                                    "search document token count overflow".to_string(),
                                )
                            })?;
                            Ok(())
                        })?;
                    }
                    let offset = write_document(
                        &mut documents,
                        &SearchDocument {
                            id: node.id,
                            node_type: node_type.clone(),
                            index_order,
                            layer,
                        },
                        request.chunk_bytes,
                    )?;
                    write_u64(&mut document_offsets, offset)?;
                    write_document_stat(
                        &mut document_lengths,
                        &DocumentStat {
                            length: document_length,
                            index_order,
                            layer,
                        },
                    )?;
                    document_count = document_count.checked_add(1).ok_or_else(|| {
                        NativeError::InvalidInput("search document count overflow".to_string())
                    })?;
                    total_tokens = total_tokens
                        .checked_add(document_length as u64)
                        .ok_or_else(|| {
                            NativeError::InvalidInput("search token count overflow".to_string())
                        })?;
                    Ok(())
                })?;
            }
            index_order = index_order.checked_add(1).ok_or_else(|| {
                NativeError::InvalidInput("search index order overflow".to_string())
            })?;
        }
    }
    documents.flush()?;
    document_offsets.flush()?;
    document_lengths.flush()?;
    drop((documents, document_offsets, document_lengths));

    let occurrence_path = occurrences.finish()?;
    let (term_count, final_spill_bytes) = write_final_index(request.db_path, &occurrence_path)?;
    fs::remove_file(&occurrence_path)?;

    let file_metadata = SidecarFileMetadata {
        backend: BACKEND_NAME.to_string(),
        format_version: FORMAT_VERSION as u64,
        document_count,
        term_count,
        total_tokens,
    };
    let metadata_path = sidecar_path(request.db_path, METADATA_SUFFIX);
    fs::write(
        &metadata_path,
        format!("{}\n", serde_json::to_string_pretty(&file_metadata)?),
    )?;

    let mut files = std::collections::BTreeMap::new();
    for suffix in SIDECAR_SUFFIXES {
        files.insert(
            suffix.to_string(),
            sha256_file(&sidecar_path(request.db_path, suffix))?,
        );
    }
    let spill_bytes = occurrences
        .spill_bytes
        .checked_add(final_spill_bytes)
        .ok_or_else(|| NativeError::InvalidInput("search spill byte count overflow".to_string()))?;
    Ok(SearchIndexBuildResult {
        metadata: SearchBackendMetadata {
            backend: BACKEND_NAME.to_string(),
            format_version: FORMAT_VERSION as u64,
            files,
            document_count,
            term_count,
            total_tokens,
        },
        spill_bytes,
        high_water_bytes: occurrences.high_water_bytes as u64,
    })
}

struct StagedNodeChunks {
    staging_dir: PathBuf,
    stem: String,
    index: usize,
    finished: bool,
}

impl StagedNodeChunks {
    fn new(staging_dir: &Path, node_type: &str) -> Self {
        Self {
            staging_dir: staging_dir.to_path_buf(),
            stem: stage_file_stem(node_type),
            index: 0,
            finished: false,
        }
    }
}

impl Iterator for StagedNodeChunks {
    type Item = PathBuf;

    fn next(&mut self) -> Option<Self::Item> {
        if self.finished {
            return None;
        }
        let path = if self.index == 0 {
            self.staging_dir.join(format!("{}.json", self.stem))
        } else {
            self.staging_dir
                .join(format!("{}__{:06}.json", self.stem, self.index))
        };
        if !path.is_file() {
            self.finished = true;
            return None;
        }
        self.index = match self.index.checked_add(1) {
            Some(index) => index,
            None => {
                self.finished = true;
                return None;
            }
        };
        Some(path)
    }
}

fn ensure_targets_absent(database_path: &Path) -> Result<(), NativeError> {
    for suffix in SIDECAR_SUFFIXES {
        let path = sidecar_path(database_path, suffix);
        if path.exists() {
            return Err(NativeError::InvalidInput(format!(
                "search sidecar target already exists: {}",
                path.display()
            )));
        }
    }
    Ok(())
}

fn write_final_index(
    database_path: &Path,
    occurrences_path: &Path,
) -> Result<(u64, u64), NativeError> {
    let mut occurrences = OccurrenceReader::open(occurrences_path)?;
    let lexicon_path = sidecar_path(database_path, LEXICON_SUFFIX);
    let postings_path = sidecar_path(database_path, POSTINGS_SUFFIX);
    let mut lexicon = BufWriter::new(File::create(&lexicon_path)?);
    let mut postings = BufWriter::new(File::create(&postings_path)?);
    write_magic(&mut lexicon, LEXICON_MAGIC)?;
    write_magic(&mut postings, POSTINGS_MAGIC)?;

    let mut current_term: Option<String> = None;
    let mut current_document = 0_u64;
    let mut term_frequency = 0_u32;
    let mut document_frequency = 0_u32;
    let mut postings_offset = POSTINGS_MAGIC.len() as u64;
    let mut term_count = 0_u64;

    while let Some(occurrence) = occurrences.next()? {
        match current_term.as_deref() {
            None => {
                current_term = Some(occurrence.term);
                current_document = occurrence.document_id;
                term_frequency = 1;
            }
            Some(term) if term == occurrence.term && current_document == occurrence.document_id => {
                term_frequency = term_frequency.checked_add(1).ok_or_else(|| {
                    NativeError::InvalidInput("search term frequency overflow".to_string())
                })?;
            }
            Some(term) if term == occurrence.term => {
                write_posting(&mut postings, current_document, term_frequency)?;
                document_frequency = document_frequency.checked_add(1).ok_or_else(|| {
                    NativeError::InvalidInput("search document frequency overflow".to_string())
                })?;
                current_document = occurrence.document_id;
                term_frequency = 1;
            }
            Some(_) => {
                write_posting(&mut postings, current_document, term_frequency)?;
                document_frequency = document_frequency.checked_add(1).ok_or_else(|| {
                    NativeError::InvalidInput("search document frequency overflow".to_string())
                })?;
                write_lexicon_entry(
                    &mut lexicon,
                    &LexiconEntry {
                        term: current_term.take().unwrap_or_default(),
                        postings_offset,
                        document_frequency,
                    },
                )?;
                term_count = term_count.checked_add(1).ok_or_else(|| {
                    NativeError::InvalidInput("search term count overflow".to_string())
                })?;
                postings_offset = postings_offset
                    .checked_add(document_frequency as u64 * super::model::POSTING_BYTES)
                    .ok_or_else(|| {
                        NativeError::InvalidInput("search postings offset overflow".to_string())
                    })?;
                current_term = Some(occurrence.term);
                current_document = occurrence.document_id;
                term_frequency = 1;
                document_frequency = 0;
            }
        }
    }
    if let Some(term) = current_term {
        write_posting(&mut postings, current_document, term_frequency)?;
        document_frequency = document_frequency.checked_add(1).ok_or_else(|| {
            NativeError::InvalidInput("search document frequency overflow".to_string())
        })?;
        write_lexicon_entry(
            &mut lexicon,
            &LexiconEntry {
                term,
                postings_offset,
                document_frequency,
            },
        )?;
        term_count = term_count
            .checked_add(1)
            .ok_or_else(|| NativeError::InvalidInput("search term count overflow".to_string()))?;
    }
    lexicon.flush()?;
    postings.flush()?;
    drop((lexicon, postings));
    let bytes = fs::metadata(lexicon_path)?
        .len()
        .checked_add(fs::metadata(postings_path)?.len())
        .ok_or_else(|| NativeError::InvalidInput("search index size overflow".to_string()))?;
    Ok((term_count, bytes))
}

fn write_posting(
    writer: &mut impl Write,
    document_id: u64,
    term_frequency: u32,
) -> Result<(), NativeError> {
    write_u64(writer, document_id)?;
    write_u32(writer, term_frequency)?;
    Ok(())
}

pub(super) fn tokenize(
    text: &str,
    mut emit: impl FnMut(String) -> Result<(), NativeError>,
) -> Result<(), NativeError> {
    let mut term = String::new();
    for character in text.chars() {
        if character.is_alphanumeric() || character == '_' {
            for lowercase in character.to_lowercase() {
                if term.len().saturating_add(lowercase.len_utf8()) <= MAX_TERM_BYTES {
                    term.push(lowercase);
                }
            }
        } else if !term.is_empty() {
            emit(std::mem::take(&mut term))?;
        }
    }
    if !term.is_empty() {
        emit(term)?;
    }
    Ok(())
}

fn stage_file_stem(name: &str) -> String {
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

fn stream_staged_nodes(
    path: &Path,
    mut callback: impl FnMut(StagedNode) -> Result<(), NativeError>,
) -> Result<(), NativeError> {
    let file = File::open(path)?;
    let mut deserializer = serde_json::Deserializer::from_reader(BufReader::new(file));
    let mut callback_error = None;
    let seed = NodeArraySeed {
        callback: &mut callback,
        callback_error: &mut callback_error,
    };
    let decode_result = seed.deserialize(&mut deserializer);
    if let Some(error) = callback_error {
        return Err(error);
    }
    decode_result.map_err(NativeError::Json)
}

struct NodeArraySeed<'a, F> {
    callback: &'a mut F,
    callback_error: &'a mut Option<NativeError>,
}

impl<'de, F> DeserializeSeed<'de> for NodeArraySeed<'_, F>
where
    F: FnMut(StagedNode) -> Result<(), NativeError>,
{
    type Value = ();

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_seq(NodeArrayVisitor {
            callback: self.callback,
            callback_error: self.callback_error,
        })
    }
}

struct NodeArrayVisitor<'a, F> {
    callback: &'a mut F,
    callback_error: &'a mut Option<NativeError>,
}

impl<'de, F> Visitor<'de> for NodeArrayVisitor<'_, F>
where
    F: FnMut(StagedNode) -> Result<(), NativeError>,
{
    type Value = ();

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a JSON array of staged graph nodes")
    }

    fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        while let Some(node) = sequence.next_element::<StagedNode>()? {
            if let Err(error) = (self.callback)(node) {
                *self.callback_error = Some(error);
                return Err(A::Error::custom("staged node callback failed"));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Deserialize)]
struct SearchSchema {
    #[serde(default)]
    search_indexes: Vec<SearchIndexSpec>,
}

#[derive(Debug, Deserialize)]
struct SearchIndexSpec {
    #[serde(default)]
    fields: Vec<String>,
    #[serde(default)]
    layer: String,
    #[serde(default)]
    node_types: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Occurrence {
    term: String,
    document_id: u64,
}

impl Ord for Occurrence {
    fn cmp(&self, other: &Self) -> Ordering {
        self.term
            .cmp(&other.term)
            .then_with(|| self.document_id.cmp(&other.document_id))
    }
}

impl PartialOrd for Occurrence {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

struct OccurrenceSorter {
    root: PathBuf,
    chunk_limit: usize,
    buffered: Vec<Occurrence>,
    buffered_term_bytes: usize,
    buffered_bytes: usize,
    runs: Vec<PathBuf>,
    next_run: usize,
    spill_bytes: u64,
    high_water_bytes: usize,
}

impl OccurrenceSorter {
    fn new(root: &Path, chunk_limit: usize) -> Result<Self, NativeError> {
        if chunk_limit == 0 {
            return Err(memory_budget_error("search_occurrences", 0, 1));
        }
        Ok(Self {
            root: root.to_path_buf(),
            chunk_limit,
            buffered: Vec::new(),
            buffered_term_bytes: 0,
            buffered_bytes: 0,
            runs: Vec::new(),
            next_run: 0,
            spill_bytes: 0,
            high_water_bytes: 0,
        })
    }

    fn push(&mut self, occurrence: Occurrence) -> Result<(), NativeError> {
        let term_bytes = occurrence.term.capacity();
        let record_bytes = term_bytes
            .checked_add(std::mem::size_of::<Occurrence>())
            .ok_or_else(|| {
                memory_budget_error("search_occurrences", self.chunk_limit, usize::MAX)
            })?;
        if record_bytes > self.chunk_limit {
            return Err(memory_budget_error(
                "search_occurrences",
                self.chunk_limit,
                record_bytes,
            ));
        }
        let required_len = self.buffered.len().checked_add(1).ok_or_else(|| {
            memory_budget_error("search_occurrences", self.chunk_limit, usize::MAX)
        })?;
        let projected_capacity = self.buffered.capacity().max(required_len);
        let projected_bytes = projected_capacity
            .checked_mul(std::mem::size_of::<Occurrence>())
            .and_then(|bytes| bytes.checked_add(self.buffered_term_bytes))
            .and_then(|bytes| bytes.checked_add(term_bytes))
            .ok_or_else(|| {
                memory_budget_error("search_occurrences", self.chunk_limit, usize::MAX)
            })?;
        if !self.buffered.is_empty() && projected_bytes > self.chunk_limit {
            self.flush()?;
        }
        self.buffered.try_reserve_exact(1).map_err(|_| {
            memory_budget_error("search_occurrences", self.chunk_limit, projected_bytes)
        })?;
        let accounted_bytes = self
            .buffered
            .capacity()
            .checked_mul(std::mem::size_of::<Occurrence>())
            .and_then(|bytes| bytes.checked_add(self.buffered_term_bytes))
            .and_then(|bytes| bytes.checked_add(term_bytes))
            .ok_or_else(|| {
                memory_budget_error("search_occurrences", self.chunk_limit, usize::MAX)
            })?;
        if accounted_bytes > self.chunk_limit {
            return Err(memory_budget_error(
                "search_occurrences",
                self.chunk_limit,
                accounted_bytes,
            ));
        }
        self.buffered.push(occurrence);
        self.buffered_term_bytes = self
            .buffered_term_bytes
            .checked_add(term_bytes)
            .ok_or_else(|| {
                memory_budget_error("search_occurrences", self.chunk_limit, usize::MAX)
            })?;
        self.buffered_bytes = accounted_bytes;
        self.high_water_bytes = self.high_water_bytes.max(self.buffered_bytes);
        Ok(())
    }

    fn flush(&mut self) -> Result<(), NativeError> {
        if self.buffered.is_empty() {
            return Ok(());
        }
        self.buffered.sort();
        let path = self.root.join(format!("occurrence-{}.run", self.next_run));
        self.next_run = self.next_run.checked_add(1).ok_or_else(|| {
            NativeError::InvalidInput("search occurrence run count overflow".to_string())
        })?;
        let mut writer = BufWriter::new(File::create(&path)?);
        write_magic(&mut writer, OCCURRENCES_MAGIC)?;
        for occurrence in &self.buffered {
            write_occurrence(&mut writer, occurrence)?;
        }
        writer.flush()?;
        drop(writer);
        self.spill_bytes = self
            .spill_bytes
            .checked_add(fs::metadata(&path)?.len())
            .ok_or_else(|| {
                NativeError::InvalidInput("search spill byte count overflow".to_string())
            })?;
        self.runs.push(path);
        self.buffered.clear();
        self.buffered_term_bytes = 0;
        self.buffered_bytes = self
            .buffered
            .capacity()
            .saturating_mul(std::mem::size_of::<Occurrence>());
        Ok(())
    }

    fn finish(&mut self) -> Result<PathBuf, NativeError> {
        self.flush()?;
        if self.runs.is_empty() {
            let path = self.root.join("occurrence-empty.run");
            let mut writer = BufWriter::new(File::create(&path)?);
            write_magic(&mut writer, OCCURRENCES_MAGIC)?;
            writer.flush()?;
            self.runs.push(path);
        }
        let merge_fan_in =
            (self.chunk_limit / MERGE_READER_ACCOUNTED_BYTES).clamp(2, MAX_MERGE_FAN_IN);
        while self.runs.len() > merge_fan_in {
            let previous = std::mem::take(&mut self.runs);
            let mut next = Vec::new();
            for group in previous.chunks(merge_fan_in) {
                if group.len() == 1 {
                    next.push(group[0].clone());
                    continue;
                }
                let path = self.root.join(format!("merge-{}.run", self.next_run));
                self.next_run = self.next_run.checked_add(1).ok_or_else(|| {
                    NativeError::InvalidInput("search merge run count overflow".to_string())
                })?;
                self.high_water_bytes = self
                    .high_water_bytes
                    .max(group.len().saturating_mul(MERGE_READER_ACCOUNTED_BYTES));
                merge_occurrence_runs(group, &path)?;
                self.spill_bytes = self
                    .spill_bytes
                    .checked_add(fs::metadata(&path)?.len())
                    .ok_or_else(|| {
                        NativeError::InvalidInput("search spill byte count overflow".to_string())
                    })?;
                for input in group {
                    fs::remove_file(input)?;
                }
                next.push(path);
            }
            self.runs = next;
        }
        if self.runs.len() == 1 {
            return Ok(self
                .runs
                .pop()
                .unwrap_or_else(|| self.root.join("unreachable.run")));
        }
        let path = self.root.join("occurrence-final.run");
        self.high_water_bytes = self
            .high_water_bytes
            .max(self.runs.len().saturating_mul(MERGE_READER_ACCOUNTED_BYTES));
        merge_occurrence_runs(&self.runs, &path)?;
        self.spill_bytes = self
            .spill_bytes
            .checked_add(fs::metadata(&path)?.len())
            .ok_or_else(|| {
                NativeError::InvalidInput("search spill byte count overflow".to_string())
            })?;
        for input in std::mem::take(&mut self.runs) {
            fs::remove_file(input)?;
        }
        Ok(path)
    }
}

fn write_occurrence(writer: &mut impl Write, occurrence: &Occurrence) -> Result<(), NativeError> {
    let length = u32::try_from(occurrence.term.len())
        .map_err(|_| NativeError::InvalidInput("search term length exceeds u32".to_string()))?;
    write_u32(writer, length)?;
    writer.write_all(occurrence.term.as_bytes())?;
    write_u64(writer, occurrence.document_id)?;
    Ok(())
}

struct OccurrenceReader {
    reader: BufReader<File>,
}

impl OccurrenceReader {
    fn open(path: &Path) -> Result<Self, NativeError> {
        let mut reader = BufReader::with_capacity(MERGE_READER_BUFFER_BYTES, File::open(path)?);
        read_magic(&mut reader, OCCURRENCES_MAGIC)?;
        Ok(Self { reader })
    }

    fn next(&mut self) -> Result<Option<Occurrence>, NativeError> {
        let mut length_bytes = [0_u8; 4];
        let read = self.reader.read(&mut length_bytes[..1])?;
        if read == 0 {
            return Ok(None);
        }
        self.reader.read_exact(&mut length_bytes[1..])?;
        let length = u32::from_le_bytes(length_bytes) as usize;
        if length > MAX_TERM_BYTES {
            return Err(NativeError::InvalidInput(format!(
                "search occurrence term exceeds limit: {length} > {MAX_TERM_BYTES}"
            )));
        }
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(length)
            .map_err(|_| memory_budget_error("search_occurrence_read", MAX_TERM_BYTES, length))?;
        bytes.resize(length, 0);
        self.reader.read_exact(&mut bytes)?;
        let term = String::from_utf8(bytes).map_err(|error| {
            NativeError::InvalidInput(format!("invalid UTF-8 search occurrence: {error}"))
        })?;
        let mut document_bytes = [0_u8; 8];
        self.reader.read_exact(&mut document_bytes)?;
        Ok(Some(Occurrence {
            term,
            document_id: u64::from_le_bytes(document_bytes),
        }))
    }
}

#[derive(Eq, PartialEq)]
struct HeapOccurrence {
    occurrence: Occurrence,
    reader_index: usize,
}

impl Ord for HeapOccurrence {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .occurrence
            .cmp(&self.occurrence)
            .then_with(|| other.reader_index.cmp(&self.reader_index))
    }
}

impl PartialOrd for HeapOccurrence {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

fn merge_occurrence_runs(inputs: &[PathBuf], output: &Path) -> Result<(), NativeError> {
    let mut readers = inputs
        .iter()
        .map(|path| OccurrenceReader::open(path))
        .collect::<Result<Vec<_>, _>>()?;
    let mut heap = BinaryHeap::new();
    for (reader_index, reader) in readers.iter_mut().enumerate() {
        if let Some(occurrence) = reader.next()? {
            heap.push(HeapOccurrence {
                occurrence,
                reader_index,
            });
        }
    }
    let mut writer = BufWriter::new(File::create(output)?);
    write_magic(&mut writer, OCCURRENCES_MAGIC)?;
    while let Some(item) = heap.pop() {
        write_occurrence(&mut writer, &item.occurrence)?;
        if let Some(occurrence) = readers[item.reader_index].next()? {
            heap.push(HeapOccurrence {
                occurrence,
                reader_index: item.reader_index,
            });
        }
    }
    writer.flush()?;
    Ok(())
}
