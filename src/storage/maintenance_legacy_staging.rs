//! Bounded recognition of historical COPY/JSON staging without a DB/manifest.
//! Row/schema/source facts never authorize mutation or legacy enrollment.
use super::locks::{self, FileIdentity, FixedControlKind, FixedFileStamp, RootedDirectory};
use super::maintenance_budget::{CleanupMemory, ResourceLimit};
use super::maintenance_json::{JsonFailure, JsonToken, MaintenanceJsonReader, TokenKind};
use super::maintenance_validation::{
    self as validation, ByteSource, KeyEncoding, NumberKind, Span, ValidationIssue,
};
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{self, Read};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StagingIssue {
    Resource(ResourceLimit),
    Io(io::ErrorKind),
    Unknown,
    Changed,
    Cancelled,
}
impl From<ResourceLimit> for StagingIssue {
    fn from(e: ResourceLimit) -> Self {
        Self::Resource(e)
    }
}
impl From<io::Error> for StagingIssue {
    fn from(e: io::Error) -> Self {
        Self::Io(e.kind())
    }
}
impl From<JsonFailure> for StagingIssue {
    fn from(e: JsonFailure) -> Self {
        match e {
            JsonFailure::Resource(e) => Self::Resource(e),
            JsonFailure::Io(e) => Self::Io(e),
            _ => Self::Unknown,
        }
    }
}
impl From<ValidationIssue> for StagingIssue {
    fn from(e: ValidationIssue) -> Self {
        match e {
            ValidationIssue::Resource(e) => Self::Resource(e),
            ValidationIssue::Io(e) => Self::Io(e),
            _ => Self::Unknown,
        }
    }
}
fn file_stamp(file: &File) -> Result<FixedFileStamp, StagingIssue> {
    FixedFileStamp::capture(file, FixedControlKind::File).map_err(|_| StagingIssue::Changed)
}
fn recheck(stamp: FixedFileStamp, file: &File) -> Result<(), StagingIssue> {
    stamp.recheck(file).map_err(|_| StagingIssue::Changed)
}
fn extend_chain(chain: &mut [u8; 32], bytes: &[u8]) {
    let mut hash = Sha256::new();
    hash.update(b"CGLegacyStagingRowChain01");
    hash.update(*chain);
    hash.update(bytes);
    *chain = hash.finalize().into();
}
fn hash(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}
fn put64(out: &mut [u8], value: u64) {
    out[..8].copy_from_slice(&value.to_le_bytes());
}
fn get64(bytes: &[u8]) -> u64 {
    u64::from_le_bytes(bytes[..8].try_into().unwrap())
}
fn put_id(out: &mut [u8], id: FileIdentity) {
    let (a, b) = id.fixed_words();
    put64(out, a);
    put64(&mut out[8..], b);
}
fn get_id(bytes: &[u8]) -> FileIdentity {
    FileIdentity::from_fixed_words(get64(bytes), get64(&bytes[8..]))
}
fn finish(out: &mut [u8]) {
    let at = out.len() - 32;
    let digest = hash(&out[..at]);
    out[at..].copy_from_slice(&digest);
}
fn checked(bytes: &[u8]) -> bool {
    bytes.len() >= 32 && hash(&bytes[..bytes.len() - 32]) == bytes[bytes.len() - 32..]
}

/// Pinned producer table spellings. Classification selects a parser only;
/// recognized origin additionally requires full bundle/source association.
const NODES: [&str; 37] = [
    "APIEndpoint",
    "Assignment",
    "CallExpression",
    "Class",
    "ClassAttribute",
    "Component",
    "Constant",
    "ControlFlowBlock",
    "Decorator",
    "Dependency",
    "DocumentationChunk",
    "DocumentationSource",
    "ExceptionFlow",
    "ExportDeclaration",
    "Expression",
    "File",
    "Function",
    "ImportDeclaration",
    "InstanceAttribute",
    "Literal",
    "Method",
    "Module",
    "Parameter",
    "Property",
    "Query",
    "Reference",
    "Repository",
    "ReturnType",
    "Route",
    "Scope",
    "SecretRef",
    "SourceRoot",
    "Symbol",
    "SyntaxCapture",
    "TypeAlias",
    "TypeAnnotation",
    "Variable",
];
const EDGES: [&str; 25] = [
    "Assigns",
    "Calls",
    "Contains",
    "Declares",
    "DecoratedBy",
    "Defines",
    "DependsOn",
    "DerivedFrom",
    "Documents",
    "EvidencedBy",
    "ExecutesQuery",
    "Exports",
    "Exposes",
    "Handles",
    "HasParameter",
    "HasReturnType",
    "HasScope",
    "HasTypeAnnotation",
    "Imports",
    "Raises",
    "References",
    "ResolvesTo",
    "RoutesTo",
    "SyntaxChild",
    "UsesSecret",
];
fn table_name(index: u8) -> Option<&'static str> {
    if index < 37 {
        NODES.get(index as usize).copied()
    } else {
        EDGES.get(usize::from(index) - 37).copied()
    }
}
fn stem_equal(raw: &[u8], table: &str) -> bool {
    raw.len() == table.len()
        && raw
            .iter()
            .zip(table.bytes())
            .all(|(a, b)| *a == b.to_ascii_lowercase())
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StagingPayloadKind {
    Node,
    Edge,
    Connector,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct StagingProfile {
    pub(crate) kind: StagingPayloadKind,
    pub(crate) table: u8,
    pub(crate) from: u8,
    pub(crate) to: u8,
    pub(crate) chunk: u64,
}
/// Exact flat producer filenames/chunking. Unknown names remain protected.
pub(crate) fn staging_profile(name: &[u8]) -> Option<StagingProfile> {
    if name.is_empty()
        || name.len() > 255
        || name
            .iter()
            .any(|b| !b.is_ascii_alphanumeric() && !matches!(b, b'_' | b'.'))
    {
        return None;
    }
    let (mut stem, csv) = if let Some(s) = name.strip_suffix(b".json") {
        (s, false)
    } else if let Some(s) = name.strip_suffix(b".csv") {
        (s, true)
    } else {
        return None;
    };
    let mut chunk = 0u64;
    if let Some(at) = stem.windows(2).rposition(|s| s == b"__") {
        let tail = &stem[at + 2..];
        if tail.len() >= 6 && tail.iter().all(u8::is_ascii_digit) {
            if tail.len() > 6 && tail[0] == b'0' {
                return None;
            }
            for b in tail {
                chunk = chunk
                    .checked_mul(10u64)?
                    .checked_add(u64::from(*b - b'0'))?;
            }
            if chunk == 0 {
                return None;
            }
            stem = &stem[..at];
        }
    }
    if !csv {
        for index in 0..62 {
            let table = table_name(index)?;
            if stem_equal(stem, table) {
                return Some(StagingProfile {
                    kind: if index < 37 {
                        StagingPayloadKind::Node
                    } else {
                        StagingPayloadKind::Edge
                    },
                    table: index,
                    from: 0,
                    to: 0,
                    chunk,
                });
            }
        }
        return None;
    }
    for (edge, relation) in EDGES.iter().enumerate() {
        for (side, prefix) in [(0, b"from_".as_slice()), (1, b"to_".as_slice())] {
            let Some(rest) = stem.strip_prefix(prefix) else {
                continue;
            };
            let relation_len = relation.len();
            if rest.len() <= relation_len + 2
                || !stem_equal(&rest[..relation_len], relation)
                || &rest[relation_len..relation_len + 2] != b"__"
            {
                continue;
            }
            let endpoints = &rest[relation_len + 2..];
            let at = endpoints.windows(2).position(|v| v == b"__")?;
            let (a, b) = (&endpoints[..at], &endpoints[at + 2..]);
            for from in 0..62 {
                let from_name = table_name(from)?;
                if !stem_equal(a, from_name) {
                    continue;
                }
                for to in 0..62 {
                    if stem_equal(b, table_name(to)?) {
                        return Some(StagingProfile {
                            kind: StagingPayloadKind::Connector,
                            table: (62 + side * 25 + edge) as u8,
                            from,
                            to,
                            chunk,
                        });
                    }
                }
            }
        }
    }
    None
}

struct Source<'a> {
    file: &'a File,
    len: u64,
}
impl ByteSource for Source<'_> {
    fn len(&self) -> u64 {
        self.len
    }
    fn read_at(&self, offset: u64, out: &mut [u8]) -> Result<usize, ValidationIssue> {
        if offset > self.len {
            return Err(ValidationIssue::InvalidSpan);
        }
        let n = usize::try_from((self.len - offset).min(out.len() as u64))
            .map_err(|_| ValidationIssue::Overflow)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::FileExt;
            self.file
                .read_at(&mut out[..n], offset)
                .map_err(|e| ValidationIssue::Io(e.kind()))
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::FileExt;
            self.file
                .seek_read(&mut out[..n], offset)
                .map_err(|e| ValidationIssue::Io(e.kind()))
        }
        #[cfg(not(any(unix, windows)))]
        {
            let _ = (offset, out, n);
            Err(ValidationIssue::Io(io::ErrorKind::Unsupported))
        }
    }
}
struct Sequential<'a> {
    source: &'a Source<'a>,
    offset: u64,
    prefix_array: bool,
    cancel: &'a dyn Fn() -> bool,
}
impl Read for Sequential<'_> {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        if (self.cancel)() {
            return Err(io::ErrorKind::Interrupted.into());
        }
        if self.prefix_array && !out.is_empty() {
            out[0] = b'[';
            self.prefix_array = false;
            return Ok(1);
        }
        let n = self
            .source
            .read_at(self.offset, out)
            .map_err(|_| io::Error::from(io::ErrorKind::InvalidData))?;
        self.offset = self
            .offset
            .checked_add(n as u64)
            .ok_or(io::ErrorKind::InvalidData)?;
        Ok(n)
    }
}

pub(crate) const STAGING_ROW_BYTES: usize = 768;
pub(crate) const STAGING_PAYLOAD_BYTES: usize = 768;
/// Self-contained locator: no borrowed file cursor or unbounded FD registry.
/// These rows are not replayable until their complete payload certificate and
/// matching witness-chain digest have been durably committed by the caller.
#[derive(Clone, Copy)]
pub(crate) struct StagingRowWitness {
    pub(crate) profile: StagingProfile,
    pub(crate) parent: FileIdentity,
    pub(crate) name: [u8; 255],
    pub(crate) name_len: u8,
    pub(crate) stamp: FixedFileStamp,
    pub(crate) ordinal: u64,
    /// JSON: id,label,kind,path,canonical,source,target,content_hash.
    /// CSV: from_id,to_id,role, then empty spans. CSV spans are raw ASCII.
    pub(crate) fields: [Span; 8],
}
impl StagingRowWitness {
    pub(crate) fn encode(&self, out: &mut [u8; STAGING_ROW_BYTES]) {
        out.fill(0);
        out[..8].copy_from_slice(b"CGLGSR01");
        out[8] = match self.profile.kind {
            StagingPayloadKind::Node => 0,
            StagingPayloadKind::Edge => 1,
            StagingPayloadKind::Connector => 2,
        };
        out[9] = self.profile.table;
        out[10] = self.profile.from;
        out[11] = self.profile.to;
        out[12] = self.name_len;
        put64(&mut out[16..], self.profile.chunk);
        out[32..287].copy_from_slice(&self.name);
        put_id(&mut out[288..304], self.parent);
        self.stamp
            .encode_checkpoint((&mut out[304..384]).try_into().unwrap());
        put64(&mut out[384..], self.ordinal);
        for (i, s) in self.fields.iter().enumerate() {
            put64(&mut out[400 + i * 16..], s.start);
            put64(&mut out[408 + i * 16..], s.end);
        }
        finish(out);
    }
    pub(crate) fn decode(bytes: &[u8; STAGING_ROW_BYTES]) -> Result<Self, StagingIssue> {
        if &bytes[..8] != b"CGLGSR01"
            || !checked(bytes)
            || bytes[13..16]
                .iter()
                .chain(bytes[24..32].iter())
                .chain(bytes[287..288].iter())
                .chain(bytes[392..400].iter())
                .chain(bytes[528..736].iter())
                .any(|b| *b != 0)
        {
            return Err(StagingIssue::Unknown);
        }
        let len = usize::from(bytes[12]);
        if len == 0 || bytes[32 + len..287].iter().any(|b| *b != 0) {
            return Err(StagingIssue::Unknown);
        }
        let profile = staging_profile(&bytes[32..32 + len]).ok_or(StagingIssue::Unknown)?;
        if bytes[8]
            != match profile.kind {
                StagingPayloadKind::Node => 0,
                StagingPayloadKind::Edge => 1,
                StagingPayloadKind::Connector => 2,
            }
            || bytes[9] != profile.table
            || bytes[10] != profile.from
            || bytes[11] != profile.to
            || get64(&bytes[16..]) != profile.chunk
        {
            return Err(StagingIssue::Unknown);
        }
        let stamp = FixedFileStamp::decode_checkpoint((&bytes[304..384]).try_into().unwrap())
            .map_err(|_| StagingIssue::Changed)?;
        let mut fields = [Span::default(); 8];
        for (i, s) in fields.iter_mut().enumerate() {
            s.start = get64(&bytes[400 + i * 16..]);
            s.end = get64(&bytes[408 + i * 16..]);
            if s.start > s.end || s.end > stamp.len() {
                return Err(StagingIssue::Unknown);
            }
        }
        Ok(Self {
            profile,
            parent: get_id(&bytes[288..304]),
            name: bytes[32..287].try_into().unwrap(),
            name_len: bytes[12],
            stamp,
            ordinal: get64(&bytes[384..]),
            fields,
        })
    }
}
#[derive(Clone, Copy)]
pub(crate) struct StagingPayloadCertificate {
    row: StagingRowWitness,
    pub(crate) digest: [u8; 32],
    pub(crate) rows: u64,
    pub(crate) witness_chain: [u8; 32],
    pub(crate) source_context: [u8; 32],
}
impl StagingPayloadCertificate {
    pub(crate) fn witness_address(&self) -> &StagingRowWitness {
        &self.row
    }
    pub(crate) fn encode(&self, out: &mut [u8; STAGING_PAYLOAD_BYTES]) {
        self.row.encode(out);
        out[..8].copy_from_slice(b"CGLGSP01");
        out[400..528].fill(0);
        out[400..432].copy_from_slice(&self.digest);
        put64(&mut out[432..], self.rows);
        out[440..472].copy_from_slice(&self.witness_chain);
        out[472..504].copy_from_slice(&self.source_context);
        finish(out);
    }
    pub(crate) fn decode(bytes: &[u8; STAGING_PAYLOAD_BYTES]) -> Result<Self, StagingIssue> {
        if &bytes[..8] != b"CGLGSP01" || !checked(bytes) || bytes[504..736].iter().any(|b| *b != 0)
        {
            return Err(StagingIssue::Unknown);
        }
        let mut row = *bytes;
        row[..8].copy_from_slice(b"CGLGSR01");
        row[400..528].fill(0);
        finish(&mut row);
        let row = StagingRowWitness::decode(&row)?;
        if row.ordinal != 0 {
            return Err(StagingIssue::Unknown);
        }
        Ok(Self {
            row,
            digest: bytes[400..432].try_into().unwrap(),
            rows: get64(&bytes[432..]),
            witness_chain: bytes[440..472].try_into().unwrap(),
            source_context: bytes[472..504].try_into().unwrap(),
        })
    }
    pub(crate) fn certificate_digest(&self) -> [u8; 32] {
        let mut bytes = [0; STAGING_PAYLOAD_BYTES];
        self.encode(&mut bytes);
        hash(&bytes)
    }
}

struct Scanner<'a> {
    source: &'a Source<'a>,
    reader: MaintenanceJsonReader<Sequential<'a>>,
    memory: &'a CleanupMemory,
    cancel: &'a dyn Fn() -> bool,
    adjustment: Option<u64>,
}
impl Scanner<'_> {
    fn token(&mut self) -> Result<JsonToken, StagingIssue> {
        if (self.cancel)() {
            return Err(StagingIssue::Cancelled);
        }
        let mut token = self.reader.next_token()?.ok_or(StagingIssue::Unknown)?;
        if let Some(base) = self.adjustment {
            token.start = token.start.checked_add(base).ok_or(StagingIssue::Unknown)?;
            token.end = token.end.checked_add(base).ok_or(StagingIssue::Unknown)?;
        }
        Ok(token)
    }
    fn string(&self, t: JsonToken) -> Result<Span, StagingIssue> {
        if matches!(t.kind, TokenKind::String { key: false, .. }) {
            Ok(Span {
                start: t.start,
                end: t.end,
            })
        } else {
            Err(StagingIssue::Unknown)
        }
    }
    fn field(&self, t: JsonToken, names: &[&[u8]]) -> Result<usize, StagingIssue> {
        if !matches!(t.kind, TokenKind::String { key: true, .. }) {
            return Err(StagingIssue::Unknown);
        }
        let mut key = [0; 32];
        let n = validation::copy_decoded_key(
            self.source,
            Span {
                start: t.start,
                end: t.end,
            },
            KeyEncoding::JsonDecoded,
            &mut key,
            self.memory,
        )?;
        names
            .iter()
            .position(|name| *name == &key[..n])
            .ok_or(StagingIssue::Unknown)
    }
    fn literal(&self, span: Span, expected: &[u8]) -> Result<bool, StagingIssue> {
        Ok(validation::compare_native_json(
            expected,
            Span {
                start: 0,
                end: expected.len() as u64,
            },
            self.source,
            span,
            self.memory,
        )? == std::cmp::Ordering::Equal)
    }
    fn number(&self, t: JsonToken, kind: NumberKind) -> Result<(), StagingIssue> {
        if t.kind != TokenKind::Number || t.end < t.start || t.end - t.start > 64 {
            return Err(StagingIssue::Unknown);
        }
        let mut bytes = [0; 64];
        let n = (t.end - t.start) as usize;
        if self.source.read_at(t.start, &mut bytes[..n])? != n {
            return Err(StagingIssue::Changed);
        }
        let facts = validation::number_facts(&bytes[..n], kind, self.memory)?;
        if facts.typed && facts.stable {
            Ok(())
        } else {
            Err(StagingIssue::Unknown)
        }
    }
    fn metadata(&mut self, first: JsonToken) -> Result<Span, StagingIssue> {
        if first.kind != TokenKind::ObjectStart {
            return Err(StagingIssue::Unknown);
        }
        let mut prior = [None; 127];
        let mut depth = 1usize;
        let mut canonical = None;
        let mut canonical_next = false;
        while depth > 0 {
            let t = self.token()?;
            match t.kind {
                TokenKind::String { key: true, .. } => {
                    let span = Span {
                        start: t.start,
                        end: t.end,
                    };
                    if let Some(p) = prior[depth - 1] {
                        if validation::compare_decoded_spans(
                            self.source,
                            p,
                            self.source,
                            span,
                            self.memory,
                        )? != std::cmp::Ordering::Less
                        {
                            return Err(StagingIssue::Unknown);
                        }
                    }
                    prior[depth - 1] = Some(span);
                    canonical_next = depth == 1 && self.literal(span, b"canonical_key")?;
                }
                TokenKind::ObjectStart | TokenKind::ArrayStart => {
                    if canonical_next || depth == prior.len() {
                        return Err(StagingIssue::Unknown);
                    }
                    prior[depth] = None;
                    depth += 1;
                }
                TokenKind::ObjectEnd | TokenKind::ArrayEnd => {
                    if canonical_next {
                        return Err(StagingIssue::Unknown);
                    }
                    depth -= 1;
                }
                TokenKind::Number => {
                    if canonical_next {
                        return Err(StagingIssue::Unknown);
                    }
                    self.number(t, NumberKind::Value)?;
                }
                TokenKind::String { key: false, .. } => {
                    if canonical_next {
                        if canonical.is_some() {
                            return Err(StagingIssue::Unknown);
                        }
                        canonical = Some(self.string(t)?);
                        canonical_next = false;
                    }
                }
                _ => {
                    if canonical_next {
                        return Err(StagingIssue::Unknown);
                    }
                }
            }
        }
        canonical.ok_or(StagingIssue::Unknown)
    }
    fn validate_id(&self, row: &StagingRowWitness) -> Result<(), StagingIssue> {
        let canonical = row.fields[4];
        let sha = validation::key_sha1_prefix20(
            self.source,
            canonical,
            KeyEncoding::JsonDecoded,
            self.memory,
        )?;
        let prefix = if row.profile.kind == StagingPayloadKind::Edge {
            b"edge".as_slice()
        } else {
            table_name(row.profile.table)
                .ok_or(StagingIssue::Unknown)?
                .as_bytes()
        };
        let mut expected = [0; 64];
        if prefix.len() + 21 > expected.len() {
            return Err(StagingIssue::Unknown);
        }
        expected[..prefix.len()].copy_from_slice(prefix);
        expected[prefix.len()] = b':';
        for (i, b) in sha[..10].iter().enumerate() {
            expected[prefix.len() + 1 + i * 2] = b"0123456789abcdef"[(b >> 4) as usize];
            expected[prefix.len() + 2 + i * 2] = b"0123456789abcdef"[(b & 15) as usize];
        }
        if !self.literal(row.fields[0], &expected[..prefix.len() + 21])? {
            return Err(StagingIssue::Unknown);
        }
        Ok(())
    }
    fn row(&mut self, template: StagingRowWitness) -> Result<StagingRowWitness, StagingIssue> {
        const NODE_FIELDS: [&[u8]; 18] = [
            b"id",
            b"label",
            b"kind",
            b"language",
            b"grammar_version",
            b"path",
            b"qualified_name",
            b"scope_id",
            b"line_start",
            b"line_end",
            b"byte_start",
            b"byte_end",
            b"tree_sitter_node_type",
            b"capture_name",
            b"summary",
            b"text",
            b"metadata",
            b"content_hash",
        ];
        const EDGE_FIELDS: [&[u8]; 12] = [
            b"id",
            b"kind",
            b"source_id",
            b"target_id",
            b"confidence",
            b"field_name",
            b"child_index",
            b"line_start",
            b"line_end",
            b"byte_start",
            b"byte_end",
            b"metadata",
        ];
        let node = template.profile.kind == StagingPayloadKind::Node;
        let names = if node {
            NODE_FIELDS.as_slice()
        } else {
            EDGE_FIELDS.as_slice()
        };
        let mut row = template;
        let mut seen = 0u32;
        loop {
            let t = self.token()?;
            if t.kind == TokenKind::ObjectEnd {
                break;
            }
            let index = self.field(t, names)?;
            let bit = 1u32 << index;
            if seen & bit != 0 {
                return Err(StagingIssue::Unknown);
            }
            seen |= bit;
            let value = self.token()?;
            if (node && index == 16) || (!node && index == 11) {
                row.fields[4] = self.metadata(value)?;
                continue;
            }
            let numeric = if node {
                (8..=11).contains(&index)
            } else {
                index == 4 || (6..=10).contains(&index)
            };
            if numeric {
                if (node || index != 4) && value.kind == TokenKind::Null {
                    continue;
                }
                self.number(
                    value,
                    if !node && index == 4 {
                        NumberKind::F64
                    } else {
                        NumberKind::I64
                    },
                )?;
                continue;
            }
            let span = self.string(value)?;
            let projected = if node {
                match index {
                    0 => Some(0),
                    1 => Some(1),
                    2 => Some(2),
                    5 => Some(3),
                    17 => Some(7),
                    _ => None,
                }
            } else {
                match index {
                    0 => Some(0),
                    1 => Some(2),
                    2 => Some(5),
                    3 => Some(6),
                    _ => None,
                }
            };
            if let Some(i) = projected {
                row.fields[i] = span;
            }
        }
        let required = if node {
            ((1u32 << 18) - 1) & !(1 << 4) & !(1 << 17)
        } else {
            ((1u32 << 12) - 1) & !(1 << 5) & !(1 << 6)
        };
        if seen & required != required {
            return Err(StagingIssue::Unknown);
        }
        self.validate_id(&row)?;
        if !node {
            let mut from = [0; 64];
            let mut to = [0; 64];
            let mut kind = [0; 256];
            let from_len = validation::copy_decoded_key(
                self.source,
                row.fields[5],
                KeyEncoding::JsonDecoded,
                &mut from,
                self.memory,
            )?;
            let to_len = validation::copy_decoded_key(
                self.source,
                row.fields[6],
                KeyEncoding::JsonDecoded,
                &mut to,
                self.memory,
            )?;
            let kind_len = validation::copy_decoded_key(
                self.source,
                row.fields[2],
                KeyEncoding::JsonDecoded,
                &mut kind,
                self.memory,
            )?;
            let table = table_name(row.profile.table)
                .ok_or(StagingIssue::Unknown)?
                .as_bytes();
            let mut canonical = [0; 512];
            let mut length = 0;
            for part in [
                table,
                b"|",
                &from[..from_len],
                b"|",
                &to[..to_len],
                b"|",
                &kind[..kind_len],
            ] {
                canonical[length..length + part.len()].copy_from_slice(part);
                length += part.len();
            }
            if !self.literal(row.fields[4], &canonical[..length])? {
                return Err(StagingIssue::Unknown);
            }
        }
        if node && row.profile.table == 15 {
            #[cfg(windows)]
            const PATH_LIMIT: usize = 131072;
            #[cfg(not(windows))]
            const PATH_LIMIT: usize = 4096;
            let mut path_bytes = self.memory.reserve(PATH_LIMIT)?.bytes(PATH_LIMIT)?;
            let path_len = validation::copy_decoded_key(
                self.source,
                row.fields[3],
                KeyEncoding::JsonDecoded,
                path_bytes.as_mut_slice(),
                self.memory,
            )?;
            let path = std::str::from_utf8(&path_bytes.as_slice()[..path_len])
                .map_err(|_| StagingIssue::Unknown)?;
            if path.is_empty()
                || path.as_bytes().iter().any(|b| matches!(b, 0 | b'\\'))
                || path.split('/').any(|part| part.is_empty())
                || !std::path::Path::new(path)
                    .components()
                    .all(|part| matches!(part, std::path::Component::Normal(_)))
            {
                return Err(StagingIssue::Unknown);
            }
            if row.fields[7] == Span::default()
                || validation::compare_decoded_spans(
                    self.source,
                    row.fields[3],
                    self.source,
                    row.fields[4],
                    self.memory,
                )? != std::cmp::Ordering::Equal
            {
                return Err(StagingIssue::Unknown);
            }
            let mut content_hash = [0; 64];
            let n = validation::copy_decoded_key(
                self.source,
                row.fields[7],
                KeyEncoding::JsonDecoded,
                &mut content_hash,
                self.memory,
            )?;
            if n != 64
                || content_hash
                    .iter()
                    .any(|b| !b.is_ascii_hexdigit() || b.is_ascii_uppercase())
            {
                return Err(StagingIssue::Unknown);
            }
        }
        Ok(row)
    }
}

/// The caller stages each fixed row record through its admitted private-stream
/// protocol. A callback success does not make rows committed: only the matching
/// final certificate permits replay. Cancellation/error leaves uncommitted data.
#[cfg(test)]
pub(crate) fn validate_staging_payload(
    parent: &RootedDirectory,
    name: &[u8],
    source_context: [u8; 32],
    memory: &CleanupMemory,
    cancel: &dyn Fn() -> bool,
    emit: &mut dyn FnMut(&StagingRowWitness) -> Result<(), StagingIssue>,
) -> Result<StagingPayloadCertificate, StagingIssue> {
    if cancel() {
        return Err(StagingIssue::Cancelled);
    }
    let profile = staging_profile(name).ok_or(StagingIssue::Unknown)?;
    let _floor = memory.reserve(
        std::mem::size_of::<StagingRowWitness>() * 5
            + std::mem::size_of::<StagingPayloadCertificate>() * 3
            + std::mem::size_of::<Scanner<'_>>()
            + std::mem::size_of::<[Option<Span>; 127]>()
            + STAGING_ROW_BYTES * 2
            + 16384,
    )?;
    let file = parent.open_readonly_file(name)?;
    let before = file_stamp(&file)?;
    let source = Source {
        file: &file,
        len: before.len(),
    };
    let mut address = [0; 255];
    address[..name.len()].copy_from_slice(name);
    let template = StagingRowWitness {
        profile,
        parent: parent.identity(),
        name: address,
        name_len: name.len() as u8,
        stamp: before,
        ordinal: 0,
        fields: [Span::default(); 8],
    };
    if profile.kind == StagingPayloadKind::Connector {
        return validate_connector(&source, template, source_context, memory, cancel, emit);
    }
    let reader = MaintenanceJsonReader::new(
        Sequential {
            source: &source,
            offset: 0,
            prefix_array: false,
            cancel,
        },
        memory,
    )?;
    let mut scan = Scanner {
        source: &source,
        reader,
        memory,
        cancel,
        adjustment: None,
    };
    if scan.token()?.kind != TokenKind::ArrayStart {
        return Err(StagingIssue::Unknown);
    }
    let mut count = 0u64;
    let mut chain = [0u8; 32];
    let mut previous = None;
    loop {
        let t = scan.token()?;
        if t.kind == TokenKind::ArrayEnd {
            break;
        }
        if t.kind != TokenKind::ObjectStart {
            return Err(StagingIssue::Unknown);
        }
        let mut row = template;
        row.ordinal = count;
        let row = scan.row(row)?;
        if let Some(prior) = previous {
            if validation::compare_decoded_spans(&source, prior, &source, row.fields[0], memory)?
                != std::cmp::Ordering::Less
            {
                return Err(StagingIssue::Unknown);
            }
        }
        previous = Some(row.fields[0]);
        let mut bytes = [0; STAGING_ROW_BYTES];
        row.encode(&mut bytes);
        extend_chain(&mut chain, &bytes);
        emit(&row)?;
        count = count.checked_add(1).ok_or(StagingIssue::Unknown)?;
    }
    if scan.reader.next_token()?.is_some() {
        return Err(StagingIssue::Unknown);
    }
    let (digest, _) = scan.reader.finish()?;
    if count == 0 {
        return Err(StagingIssue::Unknown);
    }
    recheck(before, &file)?;
    if cancel() {
        return Err(StagingIssue::Cancelled);
    }
    Ok(StagingPayloadCertificate {
        row: template,
        digest,
        rows: count,
        witness_chain: chain,
        source_context,
    })
}

struct Csv<'a> {
    source: &'a Source<'a>,
    bytes: [u8; 4096],
    cursor: usize,
    filled: usize,
    loaded: u64,
    offset: u64,
    hash: Sha256,
    cancel: &'a dyn Fn() -> bool,
}
impl Csv<'_> {
    fn byte(&mut self) -> Result<Option<u8>, StagingIssue> {
        if self.cursor == self.filled {
            if (self.cancel)() {
                return Err(StagingIssue::Cancelled);
            }
            if self.loaded == self.source.len {
                return Ok(None);
            }
            self.filled = self.source.read_at(self.loaded, &mut self.bytes)?;
            if self.filled == 0 {
                return Err(StagingIssue::Changed);
            }
            self.loaded = self
                .loaded
                .checked_add(self.filled as u64)
                .ok_or(StagingIssue::Unknown)?;
            self.hash.update(&self.bytes[..self.filled]);
            self.cursor = 0;
        }
        let b = self.bytes[self.cursor];
        self.cursor += 1;
        self.offset = self.offset.checked_add(1).ok_or(StagingIssue::Unknown)?;
        Ok(Some(b))
    }
}
fn id_literal(bytes: &[u8], table: u8) -> bool {
    let Some(table) = table_name(table) else {
        return false;
    };
    let prefix = if EDGES.contains(&table) {
        b"edge".as_slice()
    } else {
        table.as_bytes()
    };
    bytes.len() == prefix.len() + 21
        && bytes.starts_with(prefix)
        && bytes[prefix.len()] == b':'
        && bytes[prefix.len() + 1..]
            .iter()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(b))
}
#[cfg(test)]
fn validate_connector(
    source: &Source<'_>,
    template: StagingRowWitness,
    source_context: [u8; 32],
    memory: &CleanupMemory,
    cancel: &dyn Fn() -> bool,
    emit: &mut dyn FnMut(&StagingRowWitness) -> Result<(), StagingIssue>,
) -> Result<StagingPayloadCertificate, StagingIssue> {
    let _floor = memory.reserve(
        std::mem::size_of::<Csv<'_>>()
            + 4096
            + 192 * 2
            + STAGING_ROW_BYTES
            + std::mem::size_of::<StagingRowWitness>() * 2,
    )?;
    let p = template.profile;
    if !super::maintenance_legacy_association::staging_endpoint_allowed(p.table, p.from, p.to) {
        return Err(StagingIssue::Unknown);
    }
    let target = p.table >= 87;
    let relation = if target {
        p.table.checked_sub(87)
    } else {
        p.table.checked_sub(62)
    }
    .filter(|index| *index < 25)
    .ok_or(StagingIssue::Unknown)?;
    if (!target && (p.from >= 37 || p.to != 37 + relation))
        || (target && (p.from != 37 + relation || p.to >= 37))
    {
        return Err(StagingIssue::Unknown);
    }
    let mut csv = Csv {
        source,
        bytes: [0; 4096],
        cursor: 0,
        filled: 0,
        loaded: 0,
        offset: 0,
        hash: Sha256::new(),
        cancel,
    };
    for expected in b"from_id,to_id,role\r\n" {
        if csv.byte()? != Some(*expected) {
            return Err(StagingIssue::Unknown);
        }
    }
    let mut count = 0u64;
    let mut chain = [0u8; 32];
    let mut previous = [0; 192];
    let mut previous_len = 0usize;
    loop {
        let start = csv.offset;
        let Some(first) = csv.byte()? else { break };
        let mut line = [0; 192];
        line[0] = first;
        let mut n = 1;
        loop {
            let b = csv.byte()?.ok_or(StagingIssue::Unknown)?;
            if n == line.len() {
                return Err(StagingIssue::Unknown);
            }
            line[n] = b;
            n += 1;
            if b == b'\n' {
                break;
            }
        }
        if n < 2
            || line[n - 2] != b'\r'
            || (previous_len != 0 && previous[..previous_len] >= line[..n])
        {
            return Err(StagingIssue::Unknown);
        }
        let body = &line[..n - 2];
        let a = body
            .iter()
            .position(|b| *b == b',')
            .ok_or(StagingIssue::Unknown)?;
        let b = a
            + 1
            + body[a + 1..]
                .iter()
                .position(|b| *b == b',')
                .ok_or(StagingIssue::Unknown)?;
        if !id_literal(&body[..a], p.from)
            || !id_literal(&body[a + 1..b], p.to)
            || &body[b + 1..]
                != if target {
                    b"target".as_slice()
                } else {
                    b"source"
                }
        {
            return Err(StagingIssue::Unknown);
        }
        let mut row = template;
        row.ordinal = count;
        row.fields[0] = Span {
            start,
            end: start + a as u64,
        };
        row.fields[1] = Span {
            start: start + a as u64 + 1,
            end: start + b as u64,
        };
        row.fields[2] = Span {
            start: start + b as u64 + 1,
            end: start + (n - 2) as u64,
        };
        let mut bytes = [0; STAGING_ROW_BYTES];
        row.encode(&mut bytes);
        extend_chain(&mut chain, &bytes);
        emit(&row)?;
        previous[..n].copy_from_slice(&line[..n]);
        previous_len = n;
        count = count.checked_add(1).ok_or(StagingIssue::Unknown)?;
    }
    if count == 0 {
        return Err(StagingIssue::Unknown);
    }
    recheck(template.stamp, source.file)?;
    if cancel() {
        return Err(StagingIssue::Cancelled);
    }
    Ok(StagingPayloadCertificate {
        row: template,
        digest: csv.hash.finalize().into(),
        rows: count,
        witness_chain: chain,
        source_context,
    })
}

/// Real program payload fixture only. This copies actual successful public
/// producer COPY/JSON outputs; no receipt, authority, DB or manifest is made up.
#[cfg(test)]
pub(crate) struct CopiedOrphanStagingFixture {
    pub(crate) directory: std::path::PathBuf,
    pub(crate) payloads: Vec<std::path::PathBuf>,
}
#[cfg(test)]
pub(crate) fn copy_public_orphan_staging_fixture(
    repository: &std::path::Path,
    storage: &std::path::Path,
) -> Result<CopiedOrphanStagingFixture, String> {
    let runs = std::fs::read_dir(storage.join("runs")).map_err(|e| e.to_string())?;
    let staging = runs
        .filter_map(Result::ok)
        .map(|entry| entry.path().join("staging"))
        .find(|path| {
            path.join("repository.json").is_file()
                && path.join("sourceroot.json").is_file()
                && path.join("file.json").is_file()
        })
        .ok_or("actual public producer COPY fixture was not retained")?;
    let directory = repository.join(".codebaseGraph/native-staging");
    std::fs::create_dir(&directory).map_err(|e| e.to_string())?;
    let mut payloads = Vec::new();
    for entry in std::fs::read_dir(staging).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if staging_profile(name.as_bytes()).is_some() {
            let target = directory.join(name);
            std::fs::copy(entry.path(), &target).map_err(|e| e.to_string())?;
            payloads.push(target);
        }
    }
    payloads.sort();
    if payloads.is_empty() {
        return Err("actual public COPY bundle was empty".into());
    }
    if directory.join("graph.ldb").exists()
        || directory.join("manifest.json").exists()
        || directory.join(".origin-registry").exists()
    {
        return Err("fixture unexpectedly contains DB/manifest/modern registry".into());
    }
    Ok(CopiedOrphanStagingFixture {
        directory,
        payloads,
    })
}

/// Tiny real producer payload for transport-only tests. Invoke before capturing
/// target/source bindings: the fallback public producer legitimately advances E.
/// This single row file is intentionally NOT a complete association/GC proof.
#[cfg(test)]
/// Noncreating fixture observation of the actual administrative source record;
/// it grants no authority and does not acquire an owner or consume confirmation.
#[cfg(test)]
pub(crate) struct LegacyStagingJobObservation {
    pub(crate) member: FileIdentity,
    pub(crate) version: u64,
    pub(crate) job: [u8; 32],
}
#[cfg(test)]
pub(crate) fn observe_accepted_staging_job(
    repository: &std::path::Path,
) -> Result<Option<LegacyStagingJobObservation>, String> {
    use super::maintenance_legacy_guard as guard;
    let path = repository.join(".codebaseGraph");
    let root = RootedDirectory::open_readonly_root(
        &path,
        locks::path_identity(&path).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    let open = |name: &[u8]| -> Result<Option<File>, String> {
        match root.open_readonly_file(name) {
            Ok(file) => Ok(Some(file)),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.to_string()),
        }
    };
    let (Some(gate), Some(state)) = (open(guard::STAGING_GATE)?, open(guard::STAGING_STATE)?)
    else {
        return Ok(None);
    };
    let observed = guard::observe_files_named(&root, &gate, &state, guard::STAGING_OWNER)
        .map_err(|e| format!("actual staging source observation: {e:?}"))?;
    if observed.binding.kind != guard::LegacyScopeKind::NativeStaging
        || observed.binding.repository
            != locks::path_identity(repository).map_err(|e| e.to_string())?
        || observed.record.database.is_some()
        || observed.record.manifest.is_some()
    {
        return Err("staging source binding differs from actual fixture".into());
    }
    if !matches!(
        observed.record.phase,
        guard::LegacyPhase::Preparing | guard::LegacyPhase::Retiring | guard::LegacyPhase::Removed
    ) || observed.record.job == [0; 32]
    {
        return Ok(None);
    }
    Ok(Some(LegacyStagingJobObservation {
        member: observed.record.member,
        version: observed.record.version,
        job: observed.record.job,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::{execute_materialization, MaterializeOptions};
    struct FixtureReplay(Vec<StagingReplayEvent>);
    impl ReplayStagingCertificates for FixtureReplay {
        fn replay(
            &self,
            emit: &mut dyn FnMut(StagingReplayEvent) -> Result<(), StagingIssue>,
        ) -> Result<(), StagingIssue> {
            for event in &self.0 {
                emit(*event)?;
            }
            Ok(())
        }
    }
    #[test]
    fn dc_staging_actual_public_copy_json_csv_profiles_and_streamed_certificates() {
        let base = std::env::temp_dir().join(format!(
            "dc-staging-actual-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(base.join("src")).unwrap();
        let base = std::fs::canonicalize(base).unwrap();
        std::fs::write(base.join("src/lib.rs"), "pub fn alpha(x:i64)->i64{x+1}\n").unwrap();
        let storage = base.join(".codebaseGraph/storage");
        let options = MaterializeOptions {
            source_root: Some(base.clone()),
            storage_root: Some(storage.clone()),
            mode: "full".into(),
            parallel: false,
            use_git: false,
            ..MaterializeOptions::default()
        };
        execute_materialization(&options).unwrap();
        let copied = copy_public_orphan_staging_fixture(&base, &storage).unwrap();
        let legacy = copied.directory;
        let count = copied.payloads.len() as u64;
        assert!(count > 10);
        assert!(!legacy.join("graph.ldb").exists());
        assert!(!legacy.join("manifest.json").exists());
        let parent =
            RootedDirectory::open_readonly_root(&legacy, locks::path_identity(&legacy).unwrap())
                .unwrap();
        let memory = CleanupMemory::new(16 * 1024 * 1024).unwrap();
        let baseline = memory.used();
        let mut validated = 0;
        let mut rows = 0;
        let mut entries: Vec<_> = std::fs::read_dir(&legacy)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_str().unwrap().to_owned())
            .collect();
        entries.sort();
        // Fixture-only independent oracle storage, never a production witness registry.
        let mut replay = FixtureReplay(Vec::new());
        for name in &entries {
            let mut chain = [0u8; 32];
            let mut emitted = 0;
            let cert = validate_staging_payload(
                &parent,
                name.as_bytes(),
                [7; 32],
                &memory,
                &|| false,
                &mut |row| {
                    let mut bytes = [0; STAGING_ROW_BYTES];
                    row.encode(&mut bytes);
                    assert!(StagingRowWitness::decode(&bytes).is_ok());
                    extend_chain(&mut chain, &bytes);
                    emitted += 1;
                    Ok(())
                },
            )
            .unwrap_or_else(|e| panic!("actual producer profile {name}: {e:?}"));
            assert_eq!(cert.rows, emitted);
            assert_eq!(cert.witness_chain, chain);
            let mut bytes = [0; STAGING_PAYLOAD_BYTES];
            cert.encode(&mut bytes);
            assert_eq!(
                StagingPayloadCertificate::decode(&bytes)
                    .unwrap()
                    .certificate_digest(),
                cert.certificate_digest()
            );
            let mut projection =
                StagingProjection::new(&parent, name.as_bytes(), [7; 32], &memory).unwrap();
            let mut projected_rows = Vec::new();
            let mut final_cert = None;
            loop {
                let Some(batch) = projection.advance(&parent, &memory, &|| false).unwrap() else {
                    break;
                };
                assert!(
                    projection.advance(&parent, &memory, &|| false).is_err(),
                    "unacknowledged segment cannot advance"
                );
                for row in batch.rows.iter().flatten() {
                    projected_rows.push(*row);
                }
                if let Some(cert) = batch.certificate {
                    final_cert = Some(cert);
                }
                let mut checkpoint = [0; STAGING_CHECKPOINT_BYTES];
                batch.checkpoint.encode(&mut checkpoint);
                let restored = StagingProjectionCheckpoint::decode(&checkpoint).unwrap();
                assert_eq!(restored.digest(), batch.checkpoint.digest());
                std::fs::write(legacy.join(".fixture-progress"), checkpoint).unwrap();
                projection.acknowledge(restored.digest()).unwrap();
                drop(batch);
                // Every acknowledged boundary survives a fresh job object; no
                // prefix parse, payload-sized list, or sha2 layout is persisted.
                drop(projection);
                projection =
                    StagingProjection::restore(&parent, restored, [7; 32], &memory).unwrap();
            }
            let projected = final_cert.unwrap();
            assert_eq!(projected.certificate_digest(), cert.certificate_digest());
            assert_eq!(projected_rows.len() as u64, cert.rows);
            drop(projection);
            replay.0.push(StagingReplayEvent::Payload(projected));
            replay
                .0
                .extend(projected_rows.into_iter().map(StagingReplayEvent::Row));
            replay.0.push(StagingReplayEvent::EndPayload);
            validated += 1;
            rows += cert.rows;
        }
        assert_eq!(validated, count);
        assert!(rows > 30);
        assert_eq!(memory.used(), baseline);
        let association = associate_staging_bundle(
            &parent,
            &base,
            [7; 32],
            &replay,
            512,
            &memory,
            &|| false,
            &|| Ok(()),
        )
        .expect("actual orphan COPY bundle full source/endpoint/support association");
        assert_eq!(association.payload_count(), count);
        assert_eq!(association.row_count(), rows);
        let mut proof = [0; 512];
        association.encode(&mut proof);
        assert_eq!(
            StagingAssociation::decode(&proof)
                .unwrap()
                .certificate_set_digest(),
            association.certificate_set_digest()
        );
        assert!(
            associate_staging_bundle(
                &parent,
                base.parent().unwrap(),
                [7; 32],
                &replay,
                512,
                &memory,
                &|| false,
                &|| Ok(())
            )
            .is_err(),
            "same schemas with substituted repository source are not associated"
        );
        let mut incomplete = FixtureReplay(replay.0.clone());
        let end = incomplete
            .0
            .iter()
            .position(|e| matches!(e, StagingReplayEvent::EndPayload))
            .unwrap();
        incomplete.0.drain(..=end);
        assert!(
            associate_staging_bundle(
                &parent,
                &base,
                [7; 32],
                &incomplete,
                512,
                &memory,
                &|| false,
                &|| Ok(())
            )
            .is_err(),
            "missing cross-file certificate cannot cover the real directory"
        );
        assert_eq!(memory.used(), baseline);
        let file = legacy.join("file.json");
        let original = std::fs::read(&file).unwrap();
        let mut row: serde_json::Value = serde_json::from_slice(&original).unwrap();
        row[0]["id"] = serde_json::Value::String("File:00000000000000000000".into());
        std::fs::write(&file, serde_json::to_vec(&row).unwrap()).unwrap();
        assert!(matches!(
            validate_staging_payload(
                &parent,
                b"file.json",
                [7; 32],
                &memory,
                &|| false,
                &mut |_| Ok(())
            ),
            Err(StagingIssue::Unknown)
        ));
        std::fs::write(&file, &original).unwrap();
        assert_eq!(
            validate_staging_payload(
                &parent,
                b"file.json",
                [7; 32],
                &memory,
                &|| true,
                &mut |_| panic!("cancel must not emit")
            )
            .err(),
            Some(StagingIssue::Cancelled)
        );
        let tiny = CleanupMemory::new(128).unwrap();
        assert!(matches!(
            validate_staging_payload(
                &parent,
                b"file.json",
                [7; 32],
                &tiny,
                &|| false,
                &mut |_| panic!("cap before IO/emit")
            ),
            Err(StagingIssue::Resource(_))
        ));
        std::fs::remove_dir_all(base).unwrap();
    }
    #[test]
    fn dc_staging_filenames_select_only_exact_known_profiles() {
        assert!(staging_profile(b"file.json").is_some());
        assert_eq!(staging_profile(b"file__000001.json").unwrap().chunk, 1);
        assert!(staging_profile(b"file__000000.json").is_none());
        assert!(staging_profile(b"../file.json").is_none());
        assert!(staging_profile(b"foreign.json").is_none());
        assert!(staging_profile(b"from_contains__repository__contains.csv").is_some());
        assert!(staging_profile(b"to_contains__contains__file__000001.csv").is_some());
    }
}

#[derive(Clone, Copy)]
pub(crate) enum StagingReplayEvent {
    Payload(StagingPayloadCertificate),
    Row(StagingRowWitness),
    EndPayload,
}
/// Replay ONLY complete durably committed payload segments. A segment starts
/// with its final fixed certificate, followed by its exact row records. A
/// partially appended segment is never a Payload event.
pub(crate) trait ReplayStagingCertificates {
    fn replay(
        &self,
        emit: &mut dyn FnMut(StagingReplayEvent) -> Result<(), StagingIssue>,
    ) -> Result<(), StagingIssue>;
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ReplaySummary {
    payloads: u64,
    rows: u64,
    digest: [u8; 32],
}
fn same_address(a: &StagingRowWitness, b: &StagingRowWitness) -> bool {
    a.parent == b.parent
        && a.name_len == b.name_len
        && a.name == b.name
        && a.profile == b.profile
        && a.stamp == b.stamp
}
fn replay_checked<R: ReplayStagingCertificates + ?Sized>(
    root: &RootedDirectory,
    context: [u8; 32],
    replay: &R,
    memory: &CleanupMemory,
    cancel: &dyn Fn() -> bool,
    emit: &mut dyn FnMut(&StagingRowWitness) -> Result<(), StagingIssue>,
) -> Result<ReplaySummary, StagingIssue> {
    let _floor = memory.reserve(
        std::mem::size_of::<StagingRowWitness>() * 5
            + std::mem::size_of::<StagingPayloadCertificate>() * 3
            + STAGING_ROW_BYTES * 2
            + 16384,
    )?;
    let mut current = None::<StagingPayloadCertificate>;
    let mut opened = None::<File>;
    let mut ordinal = 0u64;
    let mut chain = [0u8; 32];
    let mut complete = Sha256::new();
    let mut summary = ReplaySummary {
        payloads: 0,
        rows: 0,
        digest: [0; 32],
    };
    let mut prior_name = [0; 255];
    let mut prior_len = 0usize;
    let mut previous_row = None::<StagingRowWitness>;
    let mut previous_profile = None::<StagingProfile>;
    replay.replay(&mut |event| {
        if cancel() {
            return Err(StagingIssue::Cancelled);
        }
        match event {
            StagingReplayEvent::Payload(cert) => {
                if current.is_some()
                    || cert.rows == 0
                    || cert.source_context != context
                    || cert.row.parent != root.identity()
                    || cert.row.ordinal != 0
                {
                    return Err(StagingIssue::Unknown);
                }
                let len = usize::from(cert.row.name_len);
                let name = &cert.row.name[..len];
                if prior_len > 0 && prior_name[..prior_len] >= *name {
                    return Err(StagingIssue::Unknown);
                }
                let profile = staging_profile(name).ok_or(StagingIssue::Unknown)?;
                if profile != cert.row.profile {
                    return Err(StagingIssue::Unknown);
                }
                if let Some(prior) = previous_profile {
                    let same = prior.kind == profile.kind
                        && prior.table == profile.table
                        && prior.from == profile.from
                        && prior.to == profile.to;
                    if (same && prior.chunk.checked_add(1) != Some(profile.chunk))
                        || (!same && profile.chunk != 0)
                    {
                        return Err(StagingIssue::Unknown);
                    }
                    if !same {
                        previous_row = None;
                    }
                } else if profile.chunk != 0 {
                    return Err(StagingIssue::Unknown);
                }
                let file = root.open_readonly_file(name)?;
                recheck(cert.row.stamp, &file)?;
                current = Some(cert);
                opened = Some(file);
                ordinal = 0;
                chain = [0; 32];
                prior_name = cert.row.name;
                prior_len = len;
                previous_profile = Some(profile);
            }
            StagingReplayEvent::Row(row) => {
                let cert = current.as_ref().ok_or(StagingIssue::Unknown)?;
                if !same_address(&cert.row, &row) || row.ordinal != ordinal {
                    return Err(StagingIssue::Unknown);
                }
                let mut bytes = [0; STAGING_ROW_BYTES];
                row.encode(&mut bytes);
                StagingRowWitness::decode(&bytes)?;
                if let Some(prior) = previous_row {
                    if row.ordinal == 0 {
                        let left =
                            root.open_readonly_file(&prior.name[..usize::from(prior.name_len)])?;
                        recheck(prior.stamp, &left)?;
                        let left = Source {
                            file: &left,
                            len: prior.stamp.len(),
                        };
                        let right = Source {
                            file: opened.as_ref().unwrap(),
                            len: row.stamp.len(),
                        };
                        let cmp = if row.profile.kind == StagingPayloadKind::Connector {
                            compare_ascii(&left, prior.fields[0], &right, row.fields[0])?.then(
                                compare_ascii(&left, prior.fields[1], &right, row.fields[1])?,
                            )
                        } else {
                            validation::compare_decoded_spans(
                                &left,
                                prior.fields[0],
                                &right,
                                row.fields[0],
                                memory,
                            )?
                        };
                        if cmp != std::cmp::Ordering::Less {
                            return Err(StagingIssue::Unknown);
                        }
                    }
                }
                previous_row = Some(row);
                extend_chain(&mut chain, &bytes);
                emit(&row)?;
                ordinal = ordinal.checked_add(1).ok_or(StagingIssue::Unknown)?;
            }
            StagingReplayEvent::EndPayload => {
                let cert = current.take().ok_or(StagingIssue::Unknown)?;
                if ordinal != cert.rows || chain != cert.witness_chain {
                    return Err(StagingIssue::Changed);
                }
                recheck(cert.row.stamp, opened.as_ref().unwrap())?;
                let named =
                    root.open_readonly_file(&cert.row.name[..usize::from(cert.row.name_len)])?;
                recheck(cert.row.stamp, &named)?;
                opened = None;
                complete.update(cert.certificate_digest());
                summary.payloads = summary
                    .payloads
                    .checked_add(1)
                    .ok_or(StagingIssue::Unknown)?;
                summary.rows = summary
                    .rows
                    .checked_add(ordinal)
                    .ok_or(StagingIssue::Unknown)?;
            }
        }
        Ok(())
    })?;
    if current.is_some() {
        return Err(StagingIssue::Unknown);
    }
    summary.digest = complete.finalize().into();
    Ok(summary)
}
fn compare_ascii(
    left: &dyn ByteSource,
    a: Span,
    right: &dyn ByteSource,
    b: Span,
) -> Result<std::cmp::Ordering, StagingIssue> {
    let mut order = std::cmp::Ordering::Equal;
    let mut index = 0u64;
    let mut x = [0; 128];
    let mut y = [0; 128];
    let alen = a.end.checked_sub(a.start).ok_or(StagingIssue::Unknown)?;
    let blen = b.end.checked_sub(b.start).ok_or(StagingIssue::Unknown)?;
    while index < alen.max(blen) {
        let nx = usize::try_from((alen.saturating_sub(index)).min(128)).unwrap();
        let ny = usize::try_from((blen.saturating_sub(index)).min(128)).unwrap();
        if nx > 0 && left.read_at(a.start + index, &mut x[..nx])? != nx
            || ny > 0 && right.read_at(b.start + index, &mut y[..ny])? != ny
        {
            return Err(StagingIssue::Changed);
        }
        if x[..nx].iter().chain(y[..ny].iter()).any(|b| !b.is_ascii()) {
            return Err(StagingIssue::Unknown);
        }
        if order == std::cmp::Ordering::Equal {
            order = x[..nx].cmp(&y[..ny]);
        }
        index = index.checked_add(128).ok_or(StagingIssue::Unknown)?;
    }
    Ok(order)
}

fn validation_error(e: StagingIssue) -> ValidationIssue {
    match e {
        StagingIssue::Resource(e) => ValidationIssue::Resource(e),
        StagingIssue::Io(e) => ValidationIssue::Io(e),
        StagingIssue::Cancelled => ValidationIssue::Io(io::ErrorKind::Interrupted),
        _ => ValidationIssue::Schema,
    }
}
struct Slice<'a>(&'a [u8]);
impl ByteSource for Slice<'_> {
    fn len(&self) -> u64 {
        self.0.len() as u64
    }
    fn read_at(&self, offset: u64, out: &mut [u8]) -> Result<usize, ValidationIssue> {
        self.0.read_at(offset, out)
    }
}
fn derived_id(
    source: &dyn ByteSource,
    span: Span,
    encoding: KeyEncoding,
    prefix: &[u8],
    memory: &CleanupMemory,
) -> Result<([u8; 64], usize), ValidationIssue> {
    let sha = validation::key_sha1_prefix20(source, span, encoding, memory)?;
    if prefix.len() + 21 > 64 {
        return Err(ValidationIssue::Schema);
    }
    let mut bytes = [0; 64];
    bytes[..prefix.len()].copy_from_slice(prefix);
    bytes[prefix.len()] = b':';
    for (i, b) in sha[..10].iter().enumerate() {
        bytes[prefix.len() + 1 + i * 2] = b"0123456789abcdef"[(b >> 4) as usize];
        bytes[prefix.len() + 2 + i * 2] = b"0123456789abcdef"[(b & 15) as usize];
    }
    Ok((bytes, prefix.len() + 21))
}
fn support_id(
    source: &[u8],
    target: &[u8],
    kind: &[u8],
    memory: &CleanupMemory,
) -> Result<([u8; 64], usize), ValidationIssue> {
    let mut canonical = [0; 256];
    let size = 9usize
        .checked_add(source.len())
        .and_then(|n| n.checked_add(target.len()))
        .and_then(|n| n.checked_add(kind.len()))
        .and_then(|n| n.checked_add(2))
        .ok_or(ValidationIssue::Overflow)?;
    if size > canonical.len() {
        return Err(ValidationIssue::Schema);
    }
    let mut n = 0;
    for part in [b"Contains|".as_slice(), source, b"|", target, b"|", kind] {
        canonical[n..n + part.len()].copy_from_slice(part);
        n += part.len();
    }
    let slice = Slice(&canonical[..n]);
    derived_id(
        &slice,
        Span {
            start: 0,
            end: n as u64,
        },
        KeyEncoding::NativeUtf8,
        b"edge",
        memory,
    )
}
#[derive(Clone, Copy)]
enum StagingKey {
    Empty,
    Known(u8),
    Field { row: StagingRowWitness, field: u8 },
    FileSupportId(StagingRowWitness),
}
// Compact event recipes keep repeated full row witnesses out of visitor
// frames. Expansion happens once in the separate emission stage.
#[derive(Clone, Copy)]
enum StagingField {
    Empty,
    Known(u8),
    Field(u8),
    FileSupportId,
}
struct StagingWitnesses<'a, R: ReplayStagingCertificates + ?Sized> {
    root: &'a RootedDirectory,
    replay: &'a R,
    context: [u8; 32],
    summary: ReplaySummary,
    cancel: &'a dyn Fn() -> bool,
    repository: &'a [u8],
    label: &'a [u8],
    repo_id: ([u8; 64], usize),
    root_id: ([u8; 64], usize),
    contains_id: ([u8; 64], usize),
    force_collision: bool,
}
impl<R: ReplayStagingCertificates + ?Sized> StagingWitnesses<'_, R> {
    fn known(&self, index: u8) -> Result<&[u8], ValidationIssue> {
        Ok(match index {
            0 => self.label,
            1 => self.repository,
            2 => &self.repo_id.0[..self.repo_id.1],
            3 => &self.root_id.0[..self.root_id.1],
            4 => b"repository",
            5 => b"sourceroot",
            6 => b"repository_source_root",
            7 => b"source_root_file",
            8 => &self.contains_id.0[..self.contains_id.1],
            _ => return Err(ValidationIssue::Schema),
        })
    }
    fn resolve<T>(
        &self,
        key: StagingKey,
        memory: &CleanupMemory,
        run: &mut impl FnMut(&dyn ByteSource, Span, KeyEncoding) -> Result<T, ValidationIssue>,
    ) -> Result<T, ValidationIssue> {
        match key {
            StagingKey::Empty => run(&Slice(b""), Span::default(), KeyEncoding::NativeUtf8),
            StagingKey::Known(index) => {
                let bytes = self.known(index)?;
                run(
                    &Slice(bytes),
                    Span {
                        start: 0,
                        end: bytes.len() as u64,
                    },
                    KeyEncoding::NativeUtf8,
                )
            }
            StagingKey::Field { row, .. } | StagingKey::FileSupportId(row) => {
                if row.parent != self.root.identity() {
                    return Err(ValidationIssue::Schema);
                }
                let file = self
                    .root
                    .open_readonly_file(&row.name[..usize::from(row.name_len)])
                    .map_err(|e| ValidationIssue::Io(e.kind()))?;
                row.stamp
                    .recheck(&file)
                    .map_err(|_| ValidationIssue::Schema)?;
                let source = Source {
                    file: &file,
                    len: row.stamp.len(),
                };
                let result = if matches!(key, StagingKey::FileSupportId(_)) {
                    let mut file_id = [0; 64];
                    let n = validation::copy_decoded_key(
                        &source,
                        row.fields[0],
                        KeyEncoding::JsonDecoded,
                        &mut file_id,
                        memory,
                    )?;
                    let (id, n) = support_id(
                        &self.root_id.0[..self.root_id.1],
                        &file_id[..n],
                        b"source_root_file",
                        memory,
                    )?;
                    run(
                        &Slice(&id[..n]),
                        Span {
                            start: 0,
                            end: n as u64,
                        },
                        KeyEncoding::NativeUtf8,
                    )
                } else {
                    let StagingKey::Field { field, .. } = key else {
                        unreachable!()
                    };
                    let span = *row
                        .fields
                        .get(usize::from(field))
                        .ok_or(ValidationIssue::Schema)?;
                    run(
                        &source,
                        span,
                        if row.profile.kind == StagingPayloadKind::Connector {
                            KeyEncoding::NativeUtf8
                        } else {
                            KeyEncoding::JsonDecoded
                        },
                    )
                };
                row.stamp
                    .recheck(&file)
                    .map_err(|_| ValidationIssue::Schema)?;
                result
            }
        }
    }
    fn equals(
        &self,
        key: StagingKey,
        literal: &[u8],
        memory: &CleanupMemory,
    ) -> Result<bool, ValidationIssue> {
        self.resolve(key, memory, &mut |source, span, encoding| {
            Ok(validation::compare_encoded_keys(
                source,
                span,
                encoding,
                &Slice(literal),
                Span {
                    start: 0,
                    end: literal.len() as u64,
                },
                KeyEncoding::NativeUtf8,
                memory,
            )? == std::cmp::Ordering::Equal)
        })
    }
    #[inline(never)]
    fn row_event(
        &self,
        row: &StagingRowWitness,
        domain: u64,
        native: bool,
        ordinal: u64,
        fields: [StagingField; 5],
        emit: &mut dyn FnMut(validation::WitnessEvent<StagingKey>) -> Result<(), ValidationIssue>,
    ) -> Result<(), ValidationIssue> {
        self.event(
            domain,
            native,
            ordinal,
            fields.map(|field| match field {
                StagingField::Empty => StagingKey::Empty,
                StagingField::Known(index) => StagingKey::Known(index),
                StagingField::Field(field) => StagingKey::Field { row: *row, field },
                StagingField::FileSupportId => StagingKey::FileSupportId(*row),
            }),
            emit,
        )
    }
    fn event(
        &self,
        domain: u64,
        native: bool,
        ordinal: u64,
        fields: [StagingKey; 5],
        emit: &mut dyn FnMut(validation::WitnessEvent<StagingKey>) -> Result<(), ValidationIssue>,
    ) -> Result<(), ValidationIssue> {
        emit(validation::WitnessEvent {
            domain,
            native,
            ordinal,
            fields: fields.map(Some),
        })
    }
}
impl<R: ReplayStagingCertificates + ?Sized> validation::ReplayWitnesses
    for StagingWitnesses<'_, R>
{
    type Key = StagingKey;
    fn replay(
        &self,
        memory: &CleanupMemory,
        emit: &mut dyn FnMut(validation::WitnessEvent<StagingKey>) -> Result<(), ValidationIssue>,
    ) -> Result<(), ValidationIssue> {
        use StagingKey::{Empty as E, Known as K};
        let _floor = memory.reserve(std::mem::size_of::<StagingKey>() * 20 + 32768)?;
        self.event(3001, false, 0, [K(2), K(0), K(4), E, K(0)], emit)?;
        self.event(3002, false, 0, [K(3), K(1), K(5), K(1), K(1)], emit)?;
        self.event(3003, false, 0, [K(8), K(6), K(2), K(3), E], emit)?;
        let summary = replay_checked(
            self.root,
            self.context,
            self.replay,
            memory,
            self.cancel,
            &mut |row| {
                use StagingField::{Empty as E, Known as K};
                let field = StagingField::Field;
                let profile = row.profile;
                if profile.kind == StagingPayloadKind::Connector {
                    self.row_event(
                        row,
                        100 + u64::from(profile.from),
                        false,
                        row.ordinal,
                        [field(0), E, E, E, E],
                        emit,
                    )
                    .map_err(StagingIssue::from)?;
                    self.row_event(
                        row,
                        100 + u64::from(profile.to),
                        false,
                        row.ordinal,
                        [field(1), E, E, E, E],
                        emit,
                    )
                    .map_err(StagingIssue::from)?;
                    let target = profile.table >= 87;
                    let edge = if target {
                        profile.table - 87
                    } else {
                        profile.table - 62
                    };
                    self.row_event(
                        row,
                        1000 + u64::from(edge) * 2 + u64::from(target),
                        false,
                        row.ordinal,
                        if target {
                            [field(0), field(1), E, E, E]
                        } else {
                            [field(1), field(0), E, E, E]
                        },
                        emit,
                    )
                    .map_err(StagingIssue::from)?;
                } else {
                    self.row_event(
                        row,
                        100 + u64::from(profile.table),
                        true,
                        row.ordinal,
                        [field(0), E, E, E, E],
                        emit,
                    )
                    .map_err(StagingIssue::from)?;
                    // A declared row need not have connectors; connector endpoint
                    // rows still require exactly one declared typed ID.
                    self.row_event(
                        row,
                        100 + u64::from(profile.table),
                        false,
                        row.ordinal,
                        [field(0), E, E, E, E],
                        emit,
                    )
                    .map_err(StagingIssue::from)?;
                    if profile.kind == StagingPayloadKind::Edge {
                        let edge = profile.table - 37;
                        self.row_event(
                            row,
                            1000 + u64::from(edge) * 2,
                            true,
                            row.ordinal,
                            [field(0), field(5), E, E, E],
                            emit,
                        )
                        .map_err(StagingIssue::from)?;
                        self.row_event(
                            row,
                            1001 + u64::from(edge) * 2,
                            true,
                            row.ordinal,
                            [field(0), field(6), E, E, E],
                            emit,
                        )
                        .map_err(StagingIssue::from)?;
                        if profile.table == 39
                            && (self
                                .equals(
                                    StagingKey::Field {
                                        row: *row,
                                        field: 2,
                                    },
                                    b"repository_source_root",
                                    memory,
                                )
                                .map_err(StagingIssue::from)?
                                || self
                                    .equals(
                                        StagingKey::Field {
                                            row: *row,
                                            field: 2,
                                        },
                                        b"source_root_file",
                                        memory,
                                    )
                                    .map_err(StagingIssue::from)?)
                        {
                            self.row_event(
                                row,
                                3003,
                                true,
                                row.ordinal,
                                [field(0), field(2), field(5), field(6), E],
                                emit,
                            )
                            .map_err(StagingIssue::from)?;
                        }
                    } else if profile.table == 26 {
                        self.row_event(
                            row,
                            3001,
                            true,
                            row.ordinal,
                            [field(0), field(1), field(2), field(3), field(4)],
                            emit,
                        )
                        .map_err(StagingIssue::from)?;
                    } else if profile.table == 31 {
                        self.row_event(
                            row,
                            3002,
                            true,
                            row.ordinal,
                            [field(0), field(1), field(2), field(3), field(4)],
                            emit,
                        )
                        .map_err(StagingIssue::from)?;
                    } else {
                        if profile.table == 15 {
                            if !self
                                .equals(
                                    StagingKey::Field {
                                        row: *row,
                                        field: 2,
                                    },
                                    b"file",
                                    memory,
                                )
                                .map_err(StagingIssue::from)?
                            {
                                return Err(StagingIssue::Unknown);
                            }
                            self.row_event(
                                row,
                                2000,
                                true,
                                row.ordinal,
                                [field(3), E, E, E, E],
                                emit,
                            )
                            .map_err(StagingIssue::from)?;
                            self.row_event(
                                row,
                                3003,
                                false,
                                row.ordinal,
                                [StagingField::FileSupportId, K(7), K(3), field(0), E],
                                emit,
                            )
                            .map_err(StagingIssue::from)?;
                        }
                        if !self
                            .equals(
                                StagingKey::Field {
                                    row: *row,
                                    field: 3,
                                },
                                b"",
                                memory,
                            )
                            .map_err(StagingIssue::from)?
                        {
                            self.row_event(
                                row,
                                2000,
                                false,
                                row.ordinal,
                                [field(3), E, E, E, E],
                                emit,
                            )
                            .map_err(StagingIssue::from)?;
                        }
                    }
                }
                Ok(())
            },
        )
        .map_err(validation_error)?;
        if summary != self.summary {
            return Err(ValidationIssue::Schema);
        }
        Ok(())
    }
    fn hash(
        &self,
        key: StagingKey,
        domain: u64,
        memory: &CleanupMemory,
    ) -> Result<[u8; 32], ValidationIssue> {
        let hash = self.resolve(key, memory, &mut |source, span, encoding| {
            validation::key_stream_hash(source, span, encoding, domain, memory)
        })?;
        Ok(if self.force_collision { [0; 32] } else { hash })
    }
    fn compare(
        &self,
        left: StagingKey,
        right: StagingKey,
        memory: &CleanupMemory,
    ) -> Result<std::cmp::Ordering, ValidationIssue> {
        self.resolve(left, memory, &mut |a, sa, ea| {
            self.resolve(right, memory, &mut |b, sb, eb| {
                validation::compare_encoded_keys(a, sa, ea, b, sb, eb, memory)
            })
        })
    }
    fn empty(&self, key: StagingKey, memory: &CleanupMemory) -> Result<bool, ValidationIssue> {
        self.equals(key, b"", memory)
    }
}

/// Complete source/schema/endpoint/support association facts. This is neither
/// legacy quiescence nor a mutation capability. A durable cohort prefix retains
/// the certificate set before the first leaf is retired.
#[derive(Clone, Copy)]
pub(crate) struct StagingAssociation {
    root: FileIdentity,
    repository: FileIdentity,
    source_context: [u8; 32],
    summary: ReplaySummary,
}
impl StagingAssociation {
    pub(crate) fn root_identity(&self) -> FileIdentity {
        self.root
    }
    pub(crate) fn repository_identity(&self) -> FileIdentity {
        self.repository
    }
    pub(crate) fn source_context(&self) -> [u8; 32] {
        self.source_context
    }
    pub(crate) fn payload_count(&self) -> u64 {
        self.summary.payloads
    }
    pub(crate) fn row_count(&self) -> u64 {
        self.summary.rows
    }
    pub(crate) fn certificate_set_digest(&self) -> [u8; 32] {
        self.summary.digest
    }
    pub(crate) fn encode(&self, out: &mut [u8; 512]) {
        out.fill(0);
        out[..8].copy_from_slice(b"CGSTGA01");
        put_id(&mut out[16..32], self.root);
        put_id(&mut out[32..48], self.repository);
        out[48..80].copy_from_slice(&self.source_context);
        out[80..112].copy_from_slice(&self.summary.digest);
        put64(&mut out[112..], self.summary.payloads);
        put64(&mut out[120..], self.summary.rows);
        put64(&mut out[128..], 1);
        finish(out);
    }
    /// Decodes retained facts only. The caller must authenticate the exact
    /// completed private cohort/source context before using them for admission.
    pub(crate) fn decode(bytes: &[u8; 512]) -> Result<Self, StagingIssue> {
        if &bytes[..8] != b"CGSTGA01"
            || !checked(bytes)
            || get64(&bytes[128..]) != 1
            || bytes[8..16]
                .iter()
                .chain(bytes[136..480].iter())
                .any(|b| *b != 0)
            || get64(&bytes[112..]) == 0
            || get64(&bytes[120..]) == 0
        {
            return Err(StagingIssue::Unknown);
        }
        Ok(Self {
            root: get_id(&bytes[16..32]),
            repository: get_id(&bytes[32..48]),
            source_context: bytes[48..80].try_into().unwrap(),
            summary: ReplaySummary {
                payloads: get64(&bytes[112..]),
                rows: get64(&bytes[120..]),
                digest: bytes[80..112].try_into().unwrap(),
            },
        })
    }
}
fn known_payload_count(
    root: &RootedDirectory,
    cancel: &dyn Fn() -> bool,
) -> Result<u64, StagingIssue> {
    let mut count = 0u64;
    let mut scratch = [0; 4096];
    let mut cancelled = false;
    root.visit_entries(&mut scratch, &mut |name| {
        if cancel() {
            cancelled = true;
            return Err(io::ErrorKind::Interrupted.into());
        }
        if staging_profile(name).is_some() {
            file_stamp(&root.open_readonly_file(name)?)
                .map_err(|_| io::Error::from(io::ErrorKind::InvalidData))?;
            count = count.checked_add(1).ok_or(io::ErrorKind::InvalidData)?;
        }
        Ok(())
    })
    .map_err(|e| {
        if cancelled {
            StagingIssue::Cancelled
        } else {
            StagingIssue::Io(e.kind())
        }
    })?;
    Ok(count)
}
/// No surviving database, native handle, manifest or cache body is used. Every
/// typed row, connector endpoint, File path and mandatory support edge joins
/// exactly through the committed row-span stream, with finite collision fallback.
// Full association keeps rooted source, immutable context, bounded replay and cancellation explicit.
#[allow(clippy::too_many_arguments)]
pub(crate) fn associate_staging_bundle<R: ReplayStagingCertificates + ?Sized>(
    root: &RootedDirectory,
    repository_root: &std::path::Path,
    source_context: [u8; 32],
    replay: &R,
    capacity: usize,
    memory: &CleanupMemory,
    cancel: &dyn Fn() -> bool,
    source_check: &dyn Fn() -> Result<(), StagingIssue>,
) -> Result<StagingAssociation, StagingIssue> {
    associate_staging_inner(
        root,
        repository_root,
        source_context,
        replay,
        capacity,
        memory,
        cancel,
        source_check,
        false,
    )
}
// Full association keeps rooted source, immutable context, bounded replay and cancellation explicit.
#[allow(clippy::too_many_arguments)]
fn associate_staging_inner<R: ReplayStagingCertificates + ?Sized>(
    root: &RootedDirectory,
    repository_root: &std::path::Path,
    source_context: [u8; 32],
    replay: &R,
    capacity: usize,
    memory: &CleanupMemory,
    cancel: &dyn Fn() -> bool,
    source_check: &dyn Fn() -> Result<(), StagingIssue>,
    force_collision: bool,
) -> Result<StagingAssociation, StagingIssue> {
    if cancel() {
        return Err(StagingIssue::Cancelled);
    }
    let _floor = memory.reserve(
        std::mem::size_of::<StagingWitnesses<'_, R>>() * 3
            + std::mem::size_of::<StagingKey>() * 20
            + 32768,
    )?;
    source_check()?;
    if !repository_root.is_absolute() || !super::inventory::real_path(repository_root) {
        return Err(StagingIssue::Unknown);
    }
    let repository = repository_root
        .to_str()
        .ok_or(StagingIssue::Unknown)?
        .as_bytes();
    let label = repository_root
        .file_name()
        .and_then(|v| v.to_str())
        .ok_or(StagingIssue::Unknown)?
        .as_bytes();
    let repository_identity =
        locks::path_identity(repository_root).map_err(|_| StagingIssue::Changed)?;
    let known = known_payload_count(root, cancel)?;
    let summary = replay_checked(
        root,
        source_context,
        replay,
        memory,
        cancel,
        &mut |_| Ok(()),
    )?;
    if known == 0 || known != summary.payloads {
        return Err(StagingIssue::Unknown);
    }
    let repo_id = derived_id(
        &Slice(label),
        Span {
            start: 0,
            end: label.len() as u64,
        },
        KeyEncoding::NativeUtf8,
        b"Repository",
        memory,
    )?;
    let root_id = derived_id(
        &Slice(repository),
        Span {
            start: 0,
            end: repository.len() as u64,
        },
        KeyEncoding::NativeUtf8,
        b"SourceRoot",
        memory,
    )?;
    let contains_id = support_id(
        &repo_id.0[..repo_id.1],
        &root_id.0[..root_id.1],
        b"repository_source_root",
        memory,
    )?;
    let witnesses = StagingWitnesses {
        root,
        replay,
        context: source_context,
        summary,
        cancel,
        repository,
        label,
        repo_id,
        root_id,
        contains_id,
        force_collision,
    };
    validation::join_selected_witnesses(&witnesses, memory, capacity)?;
    if known_payload_count(root, cancel)? != known
        || locks::path_identity(repository_root).map_err(|_| StagingIssue::Changed)?
            != repository_identity
    {
        return Err(StagingIssue::Changed);
    }
    source_check()?;
    if cancel() {
        return Err(StagingIssue::Cancelled);
    }
    Ok(StagingAssociation {
        root: root.identity(),
        repository: repository_identity,
        source_context,
        summary,
    })
}

pub(crate) const STAGING_CHECKPOINT_BYTES: usize = 768;
pub(crate) const STAGING_ROWS_PER_BATCH: usize = 4;
/// Row-boundary progress, never a completed payload certificate. The portable
/// chain is over committed fixed row records, not an opaque sha2 object image.
#[derive(Clone, Copy)]
pub(crate) struct StagingProjectionCheckpoint {
    row: StagingRowWitness,
    source_context: [u8; 32],
    next: u64,
    rows: u64,
    chain: [u8; 32],
    previous: [Span; 3],
    stage: u8,
}
impl StagingProjectionCheckpoint {
    pub(crate) fn payload_name(&self) -> &[u8] {
        &self.row.name[..usize::from(self.row.name_len)]
    }
    pub(crate) fn finished(&self) -> bool {
        self.stage == 2
    }
    pub(crate) fn encode(&self, out: &mut [u8; STAGING_CHECKPOINT_BYTES]) {
        out.fill(0);
        out[..8].copy_from_slice(b"CGSTGP01");
        out[8] = self.stage;
        out[9] = self.row.name_len;
        out[10] = match self.row.profile.kind {
            StagingPayloadKind::Node => 0,
            StagingPayloadKind::Edge => 1,
            StagingPayloadKind::Connector => 2,
        };
        out[11] = self.row.profile.table;
        out[12] = self.row.profile.from;
        out[13] = self.row.profile.to;
        put64(&mut out[16..], self.row.profile.chunk);
        put64(&mut out[24..], self.next);
        out[32..287].copy_from_slice(&self.row.name);
        put_id(&mut out[288..304], self.row.parent);
        self.row
            .stamp
            .encode_checkpoint((&mut out[304..384]).try_into().unwrap());
        out[384..416].copy_from_slice(&self.source_context);
        put64(&mut out[416..], self.rows);
        out[424..456].copy_from_slice(&self.chain);
        for (i, s) in self.previous.iter().enumerate() {
            put64(&mut out[456 + i * 16..], s.start);
            put64(&mut out[464 + i * 16..], s.end);
        }
        finish(out);
    }
    pub(crate) fn decode(bytes: &[u8; STAGING_CHECKPOINT_BYTES]) -> Result<Self, StagingIssue> {
        if &bytes[..8] != b"CGSTGP01"
            || !checked(bytes)
            || bytes[8] > 2
            || bytes[14..16]
                .iter()
                .chain(bytes[287..288].iter())
                .chain(bytes[504..736].iter())
                .any(|b| *b != 0)
        {
            return Err(StagingIssue::Unknown);
        }
        let mut address = *bytes;
        address[..8].copy_from_slice(b"CGLGSR01");
        address[8] = bytes[10];
        address[9] = bytes[11];
        address[10] = bytes[12];
        address[11] = bytes[13];
        address[12] = bytes[9];
        address[13..16].fill(0);
        address[24..32].fill(0);
        address[384..736].fill(0);
        finish(&mut address);
        let row = StagingRowWitness::decode(&address)?;
        let next = get64(&bytes[24..]);
        let rows = get64(&bytes[416..]);
        let mut previous = [Span::default(); 3];
        for (i, s) in previous.iter_mut().enumerate() {
            s.start = get64(&bytes[456 + i * 16..]);
            s.end = get64(&bytes[464 + i * 16..]);
            if s.start > s.end || s.end > row.stamp.len() {
                return Err(StagingIssue::Unknown);
            }
        }
        if next > row.stamp.len()
            || (rows == 0
                && (next != 0 || bytes[8] != 0 || previous.iter().any(|s| *s != Span::default())))
            || (bytes[8] > 0 && next != row.stamp.len())
        {
            return Err(StagingIssue::Unknown);
        }
        Ok(Self {
            row,
            source_context: bytes[384..416].try_into().unwrap(),
            next,
            rows,
            chain: bytes[424..456].try_into().unwrap(),
            previous,
            stage: bytes[8],
        })
    }
    pub(crate) fn digest(&self) -> [u8; 32] {
        let mut b = [0; STAGING_CHECKPOINT_BYTES];
        self.encode(&mut b);
        hash(&b)
    }
    pub(crate) fn source_context(&self) -> [u8; 32] {
        self.source_context
    }
}
pub(crate) struct StagingProjectionBatch {
    pub(crate) rows: [Option<StagingRowWitness>; STAGING_ROWS_PER_BATCH],
    pub(crate) checkpoint: StagingProjectionCheckpoint,
    pub(crate) certificate: Option<StagingPayloadCertificate>,
}
/// An ordinary advisory job; no writer/family/root/selector authority is held.
/// ACK only after the exact <=3840B row+checkpoint segment is durably committed
/// through the caller's real private publication transport.
pub(crate) struct StagingProjection {
    committed: StagingProjectionCheckpoint,
    pending: Option<StagingProjectionCheckpoint>,
    _floor: super::maintenance_budget::MemoryReservation,
}
impl StagingProjection {
    pub(crate) fn new(
        root: &RootedDirectory,
        name: &[u8],
        source_context: [u8; 32],
        memory: &CleanupMemory,
    ) -> Result<super::maintenance_budget::MemoryBox<Self>, StagingIssue> {
        let _temporary = memory.reserve(std::mem::size_of::<Self>() * 3 + 8192)?;
        let profile = staging_profile(name).ok_or(StagingIssue::Unknown)?;
        let file = root.open_readonly_file(name)?;
        let stamp = file_stamp(&file)?;
        let mut copy = [0; 255];
        copy[..name.len()].copy_from_slice(name);
        let cp = StagingProjectionCheckpoint {
            row: StagingRowWitness {
                profile,
                parent: root.identity(),
                name: copy,
                name_len: name.len() as u8,
                stamp,
                ordinal: 0,
                fields: [Span::default(); 8],
            },
            source_context,
            next: 0,
            rows: 0,
            chain: [0; 32],
            previous: [Span::default(); 3],
            stage: 0,
        };
        Self::restore(root, cp, source_context, memory)
    }
    pub(crate) fn restore(
        root: &RootedDirectory,
        checkpoint: StagingProjectionCheckpoint,
        source_context: [u8; 32],
        memory: &CleanupMemory,
    ) -> Result<super::maintenance_budget::MemoryBox<Self>, StagingIssue> {
        let floor = memory.reserve(std::mem::size_of::<Self>() * 3 + 8192)?;
        if checkpoint.source_context != source_context || checkpoint.row.parent != root.identity() {
            return Err(StagingIssue::Changed);
        }
        recheck(
            checkpoint.row.stamp,
            &root
                .open_readonly_file(&checkpoint.row.name[..usize::from(checkpoint.row.name_len)])?,
        )?;
        super::maintenance_budget::MemoryBox::new(
            Self {
                committed: checkpoint,
                pending: None,
                _floor: floor,
            },
            memory,
        )
        .map_err(StagingIssue::Resource)
    }
    pub(crate) fn checkpoint(&self) -> StagingProjectionCheckpoint {
        self.committed
    }
    pub(crate) fn acknowledge(&mut self, published_digest: [u8; 32]) -> Result<(), StagingIssue> {
        let cp = self.pending.ok_or(StagingIssue::Unknown)?;
        if cp.digest() != published_digest {
            return Err(StagingIssue::Changed);
        }
        self.committed = cp;
        self.pending = None;
        Ok(())
    }
    pub(crate) fn advance(
        &mut self,
        root: &RootedDirectory,
        memory: &CleanupMemory,
        cancel: &dyn Fn() -> bool,
    ) -> Result<Option<super::maintenance_budget::MemoryBox<StagingProjectionBatch>>, StagingIssue>
    {
        if cancel() {
            return Err(StagingIssue::Cancelled);
        }
        if self.pending.is_some() {
            return Err(StagingIssue::Unknown);
        }
        if self.committed.stage == 2 {
            return Ok(None);
        }
        let _temporary = memory.reserve(
            std::mem::size_of::<StagingProjectionBatch>() * 4
                + std::mem::size_of::<Scanner<'_>>()
                + std::mem::size_of::<[Option<Span>; 127]>()
                + 32768,
        )?;
        let mut cp = self.committed;
        if root.identity() != cp.row.parent {
            return Err(StagingIssue::Changed);
        }
        let file = root.open_readonly_file(&cp.row.name[..usize::from(cp.row.name_len)])?;
        recheck(cp.row.stamp, &file)?;
        let source = Source {
            file: &file,
            len: cp.row.stamp.len(),
        };
        let mut batch = super::maintenance_budget::MemoryBox::new(
            StagingProjectionBatch {
                rows: [None; STAGING_ROWS_PER_BATCH],
                checkpoint: cp,
                certificate: None,
            },
            memory,
        )?;
        if cp.stage == 1 {
            let mut sha = Sha256::new();
            let mut offset = 0u64;
            let mut bytes = [0; 4096];
            while offset < source.len {
                if cancel() {
                    return Err(StagingIssue::Cancelled);
                }
                let n = source.read_at(offset, &mut bytes)?;
                if n == 0 {
                    return Err(StagingIssue::Changed);
                }
                sha.update(&bytes[..n]);
                offset = offset.checked_add(n as u64).ok_or(StagingIssue::Unknown)?;
            }
            recheck(cp.row.stamp, &file)?;
            batch.certificate = Some(StagingPayloadCertificate {
                row: cp.row,
                digest: sha.finalize().into(),
                rows: cp.rows,
                witness_chain: cp.chain,
                source_context: cp.source_context,
            });
            cp.stage = 2;
        } else if cp.row.profile.kind == StagingPayloadKind::Connector {
            project_csv(&source, &mut cp, &mut batch.rows, memory, cancel)?;
        } else {
            let resumed = cp.next != 0;
            let reader = MaintenanceJsonReader::new(
                Sequential {
                    source: &source,
                    offset: cp.next,
                    prefix_array: resumed,
                    cancel,
                },
                memory,
            )?;
            let mut scan = Scanner {
                source: &source,
                reader,
                memory,
                cancel,
                adjustment: if resumed {
                    Some(cp.next.checked_sub(1).ok_or(StagingIssue::Unknown)?)
                } else {
                    None
                },
            };
            if scan.token()?.kind != TokenKind::ArrayStart {
                return Err(StagingIssue::Unknown);
            }
            let mut top = scan.token()?;
            for slot in &mut batch.rows {
                if top.kind == TokenKind::ArrayEnd {
                    break;
                }
                if top.kind != TokenKind::ObjectStart {
                    return Err(StagingIssue::Unknown);
                }
                let mut row = cp.row;
                row.ordinal = cp.rows;
                let row = scan.row(row)?;
                if cp.rows > 0
                    && validation::compare_decoded_spans(
                        &source,
                        cp.previous[0],
                        &source,
                        row.fields[0],
                        memory,
                    )? != std::cmp::Ordering::Less
                {
                    return Err(StagingIssue::Unknown);
                }
                let mut bytes = [0; STAGING_ROW_BYTES];
                row.encode(&mut bytes);
                extend_chain(&mut cp.chain, &bytes);
                cp.rows = cp.rows.checked_add(1).ok_or(StagingIssue::Unknown)?;
                cp.previous[0] = row.fields[0];
                *slot = Some(row);
                top = scan.token()?;
            }
            if top.kind == TokenKind::ArrayEnd {
                if cp.rows == 0 || scan.reader.next_token()?.is_some() {
                    return Err(StagingIssue::Unknown);
                }
                cp.next = source.len;
                cp.stage = 1;
            } else if top.kind == TokenKind::ObjectStart {
                cp.next = top.start;
            } else {
                return Err(StagingIssue::Unknown);
            }
        }
        recheck(cp.row.stamp, &file)?;
        if cancel() {
            return Err(StagingIssue::Cancelled);
        }
        batch.checkpoint = cp;
        self.pending = Some(cp);
        Ok(Some(batch))
    }
}
fn project_csv(
    source: &Source<'_>,
    cp: &mut StagingProjectionCheckpoint,
    rows: &mut [Option<StagingRowWitness>; STAGING_ROWS_PER_BATCH],
    memory: &CleanupMemory,
    cancel: &dyn Fn() -> bool,
) -> Result<(), StagingIssue> {
    let _floor = memory.reserve(std::mem::size_of::<Csv<'_>>() + 8192)?;
    let p = cp.row.profile;
    if !super::maintenance_legacy_association::staging_endpoint_allowed(p.table, p.from, p.to) {
        return Err(StagingIssue::Unknown);
    }
    let target = p.table >= 87;
    let mut csv = Csv {
        source,
        bytes: [0; 4096],
        cursor: 0,
        filled: 0,
        loaded: cp.next,
        offset: cp.next,
        hash: Sha256::new(),
        cancel,
    };
    if cp.next == 0 {
        for b in b"from_id,to_id,role\r\n" {
            if csv.byte()? != Some(*b) {
                return Err(StagingIssue::Unknown);
            }
        }
    }
    for slot in rows {
        let start = csv.offset;
        let Some(first) = csv.byte()? else { break };
        let mut line = [0; 192];
        line[0] = first;
        let mut n = 1;
        loop {
            let b = csv.byte()?.ok_or(StagingIssue::Unknown)?;
            if n == line.len() {
                return Err(StagingIssue::Unknown);
            }
            line[n] = b;
            n += 1;
            if b == b'\n' {
                break;
            }
        }
        if n < 2 || line[n - 2] != b'\r' {
            return Err(StagingIssue::Unknown);
        }
        let body = &line[..n - 2];
        let a = body
            .iter()
            .position(|b| *b == b',')
            .ok_or(StagingIssue::Unknown)?;
        let b = a
            + 1
            + body[a + 1..]
                .iter()
                .position(|b| *b == b',')
                .ok_or(StagingIssue::Unknown)?;
        if !id_literal(&body[..a], p.from)
            || !id_literal(&body[a + 1..b], p.to)
            || &body[b + 1..]
                != if target {
                    b"target".as_slice()
                } else {
                    b"source"
                }
        {
            return Err(StagingIssue::Unknown);
        }
        let mut row = cp.row;
        row.ordinal = cp.rows;
        row.fields[0] = Span {
            start,
            end: start + a as u64,
        };
        row.fields[1] = Span {
            start: start + a as u64 + 1,
            end: start + b as u64,
        };
        row.fields[2] = Span {
            start: start + b as u64 + 1,
            end: start + (n - 2) as u64,
        };
        if cp.rows > 0
            && compare_ascii(source, cp.previous[0], source, row.fields[0])?.then(compare_ascii(
                source,
                cp.previous[1],
                source,
                row.fields[1],
            )?) != std::cmp::Ordering::Less
        {
            return Err(StagingIssue::Unknown);
        }
        let mut bytes = [0; STAGING_ROW_BYTES];
        row.encode(&mut bytes);
        extend_chain(&mut cp.chain, &bytes);
        cp.rows = cp.rows.checked_add(1).ok_or(StagingIssue::Unknown)?;
        cp.previous.copy_from_slice(&row.fields[..3]);
        *slot = Some(row);
    }
    cp.next = csv.offset;
    if cp.next == source.len {
        if cp.rows == 0 {
            return Err(StagingIssue::Unknown);
        }
        cp.stage = 1;
    }
    Ok(())
}

/// Advisory per-leaf preparation from the actual completed rooted cohort reader.
/// Raw parsed certificates cannot construct this owner or its membership token.
pub(crate) struct PreparedStagingLeaf {
    _payload: File,
    association: StagingAssociation,
    certificate: StagingPayloadCertificate,
    original: super::maintenance_legacy_bootstrap::LegacyClosedVersion,
    current: super::maintenance_legacy_bootstrap::LegacyClosedVersion,
    _floor: super::maintenance_budget::MemoryReservation,
}
pub(crate) fn prepare_staging_leaf(
    cohort: &super::maintenance_candidate::LegacyStagingCohortLeaf,
    owner: &super::maintenance_legacy_bootstrap::LegacySourceOwner,
    root: &RootedDirectory,
    memory: &CleanupMemory,
    cancel: &dyn Fn() -> bool,
) -> Result<super::maintenance_budget::MemoryBox<PreparedStagingLeaf>, StagingIssue> {
    if cancel() {
        return Err(StagingIssue::Cancelled);
    }
    let floor = memory.reserve(
        std::mem::size_of::<PreparedStagingLeaf>() * 3
            + std::mem::size_of::<StagingPayloadCertificate>() * 3
            + 16384,
    )?;
    let association = *cohort.association();
    let certificate = *cohort.certificate();
    let original = *cohort.original_source();
    let current = owner.closed_version();
    if original.phase() != super::maintenance_legacy_guard::LegacyPhase::Preparing
        || current.phase() != super::maintenance_legacy_guard::LegacyPhase::Retiring
        || !current.same_job_source(&original)
        || original.context_digest() != association.source_context
        || certificate.source_context != association.source_context
        || certificate.row.parent != association.root
        || root.identity() != association.root
        || !owner
            .is_current(memory)
            .map_err(|_| StagingIssue::Changed)?
    {
        return Err(StagingIssue::Changed);
    }
    let file =
        root.open_readonly_file(&certificate.row.name[..usize::from(certificate.row.name_len)])?;
    recheck(certificate.row.stamp, &file)?;
    let source = Source {
        file: &file,
        len: certificate.row.stamp.len(),
    };
    let mut sha = Sha256::new();
    let mut offset = 0u64;
    let mut bytes = [0; 4096];
    while offset < source.len {
        if cancel() {
            return Err(StagingIssue::Cancelled);
        }
        let n = source.read_at(offset, &mut bytes)?;
        if n == 0 {
            return Err(StagingIssue::Changed);
        }
        sha.update(&bytes[..n]);
        offset = offset.checked_add(n as u64).ok_or(StagingIssue::Unknown)?;
    }
    if <[u8; 32]>::from(sha.finalize()) != certificate.digest {
        return Err(StagingIssue::Changed);
    }
    recheck(certificate.row.stamp, &file)?;
    recheck(
        certificate.row.stamp,
        &root.open_readonly_file(&certificate.row.name[..usize::from(certificate.row.name_len)])?,
    )?;
    if !owner
        .is_current(memory)
        .map_err(|_| StagingIssue::Changed)?
        || !owner.closed_version().same_job_source(&current)
    {
        return Err(StagingIssue::Changed);
    }
    if cancel() {
        return Err(StagingIssue::Cancelled);
    }
    super::maintenance_budget::MemoryBox::new(
        PreparedStagingLeaf {
            _payload: file,
            association,
            certificate,
            original,
            current,
            _floor: floor,
        },
        memory,
    )
    .map_err(StagingIssue::Resource)
}
/// This borrowed hot frame exists only while the actual closed-source gate and
/// retained source owner exclude producers for this exact incarnation/job.
pub(crate) struct StagingLeafMutation<'a> {
    parent: &'a RootedDirectory,
    leaf: &'a StagingRowWitness,
}
impl StagingLeafMutation<'_> {
    pub(crate) fn source_parent(&self) -> &RootedDirectory {
        self.parent
    }
    pub(crate) fn name(&self) -> &[u8] {
        &self.leaf.name[..usize::from(self.leaf.name_len)]
    }
    pub(crate) fn payload_identity(&self) -> FileIdentity {
        self.leaf.stamp.identity()
    }
}
impl PreparedStagingLeaf {
    pub(crate) fn with_admitted<R>(
        &self,
        scope: &mut super::maintenance_legacy_bootstrap::PreparedLegacyStagingScope,
        owner: &super::maintenance_legacy_bootstrap::LegacySourceOwner,
        grant: &mut super::maintenance_budget::MutationGrant,
        caller_callback_entries: usize,
        memory: &CleanupMemory,
        apply: &mut impl FnMut(
            &StagingLeafMutation<'_>,
            &mut super::maintenance_budget::MutationGrant,
        ) -> Result<R, locks::FixedControlError>,
    ) -> Result<R, locks::FixedControlError> {
        use super::maintenance_legacy_guard::LegacyPhase;
        use locks::{FixedControlError as Error, FixedControlProblem as Problem};
        // Complete caller effects are reserved before any source-role opens.
        // The actual scope helper charges its own observations independently.
        grant.take_entries(
            2usize
                .checked_add(caller_callback_entries)
                .ok_or(Error::Resource(ResourceLimit::ArithmeticOverflow))?,
        )?;
        super::maintenance_legacy_bootstrap::with_closed_staging_source(
            scope,
            owner,
            grant,
            memory,
            &mut |authority, grant| {
                let actual = authority.closed_version();
                if actual.phase() != LegacyPhase::Retiring
                    || !actual.same_job_source(&self.current)
                    || !actual.same_job_source(&self.original)
                    || authority.staging_directory().identity() != self.association.root
                {
                    return Err(Error::Invalid(Problem::Changed));
                }
                grant.check_time()?;
                let parent = authority.staging_directory();
                let file = parent.open_readonly_file(
                    &self.certificate.row.name[..usize::from(self.certificate.row.name_len)],
                )?;
                self.certificate.row.stamp.recheck(&file)?;
                grant.check_time()?;
                apply(
                    &StagingLeafMutation {
                        parent,
                        leaf: &self.certificate.row,
                    },
                    grant,
                )
            },
        )
    }
}
