use serde::{Deserialize, Serialize};
pub(super) const FORMAT_VERSION: u32 = 1;
pub(super) const BACKEND_NAME: &str = "disk_bm25_v1";
pub(super) const METADATA_SUFFIX: &str = "search.meta.json";
pub(super) const LEXICON_SUFFIX: &str = "search.lexicon.bin";
pub(super) const POSTINGS_SUFFIX: &str = "search.postings.bin";
pub(super) const DOCUMENTS_SUFFIX: &str = "search.documents.bin";
pub(super) const DOCUMENT_OFFSETS_SUFFIX: &str = "search.document_offsets.bin";
pub(super) const DOCUMENT_LENGTHS_SUFFIX: &str = "search.doc_lengths.bin";
pub(super) const SIDECAR_SUFFIXES: [&str; 6] = [
    METADATA_SUFFIX,
    LEXICON_SUFFIX,
    POSTINGS_SUFFIX,
    DOCUMENTS_SUFFIX,
    DOCUMENT_OFFSETS_SUFFIX,
    DOCUMENT_LENGTHS_SUFFIX,
];

pub(super) const LEXICON_MAGIC: &[u8; 8] = b"CBGLX001";
pub(super) const POSTINGS_MAGIC: &[u8; 8] = b"CBGPS001";
pub(super) const DOCUMENTS_MAGIC: &[u8; 8] = b"CBGDC001";
pub(super) const DOCUMENT_OFFSETS_MAGIC: &[u8; 8] = b"CBGDO001";
pub(super) const DOCUMENT_LENGTHS_MAGIC: &[u8; 8] = b"CBGDL001";
pub(super) const OCCURRENCES_MAGIC: &[u8; 8] = b"CBGOC001";

pub(super) const POSTING_BYTES: u64 = 12;
pub(super) const DOCUMENT_STAT_BYTES: u64 = 12;
pub(super) const DOCUMENT_OFFSET_BYTES: u64 = 8;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(super) struct SidecarFileMetadata {
    pub(crate) backend: String,
    pub(crate) format_version: u64,
    pub(crate) document_count: u64,
    pub(crate) term_count: u64,
    pub(crate) total_tokens: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(super) struct SearchDocument {
    pub(super) id: String,
    pub(super) node_type: String,
    pub(super) index_order: u32,
    pub(super) layer: SearchLayer,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum SearchLayer {
    Semantic,
    Syntax,
}

impl SearchLayer {
    pub(super) fn from_schema(value: &str) -> Self {
        if value == "syntax" {
            Self::Syntax
        } else {
            Self::Semantic
        }
    }

    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Semantic => "semantic",
            Self::Syntax => "syntax",
        }
    }

    pub(super) fn code(self) -> u8 {
        match self {
            Self::Semantic => 0,
            Self::Syntax => 1,
        }
    }

    pub(super) fn from_code(code: u8) -> Option<Self> {
        match code {
            0 => Some(Self::Semantic),
            1 => Some(Self::Syntax),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RankedDocument {
    pub(crate) id: String,
    pub(crate) node_type: String,
    pub(crate) index_order: usize,
    pub(crate) layer: String,
    pub(crate) score: f64,
}

#[derive(Clone, Debug)]
pub(super) struct LexiconEntry {
    pub(super) term: String,
    pub(super) postings_offset: u64,
    pub(super) document_frequency: u32,
}

#[derive(Clone, Debug)]
pub(super) struct DocumentStat {
    pub(super) length: u32,
    pub(super) index_order: u32,
    pub(super) layer: SearchLayer,
}

#[derive(Debug, Deserialize)]
pub(super) struct StagedNode {
    pub(super) id: String,
    #[serde(default)]
    pub(super) label: String,
    #[serde(default)]
    pub(super) path: String,
    #[serde(default)]
    pub(super) qualified_name: String,
    #[serde(default)]
    pub(super) summary: String,
    #[serde(default)]
    pub(super) text: String,
    #[serde(default)]
    pub(super) tree_sitter_node_type: String,
}

impl StagedNode {
    pub(super) fn field(&self, name: &str) -> &str {
        match name {
            "label" => &self.label,
            "path" => &self.path,
            "qualified_name" => &self.qualified_name,
            "summary" => &self.summary,
            "text" => &self.text,
            "tree_sitter_node_type" => &self.tree_sitter_node_type,
            _ => "",
        }
    }
}
