//! Controlled historical association facts. These never enroll a root or grant
//! Ready, publication, migration, native-open, restoration or deletion authority.
use super::locks::{FileIdentity, FixedControlKind, FixedFileStamp, RootedDirectory};
use super::maintenance_budget::{CleanupMemory, MemoryReservation, ResourceLimit};
use super::maintenance_json::{JsonFailure, JsonToken, MaintenanceJsonReader, TokenKind};
use super::maintenance_native_projection::{
    self as native, NativeCatalogEvent, NativeDefaultFacts, NativeSelectedTable,
    NativeStringLocator, NativeStringSource, NativeUnknown,
};
use super::maintenance_ownership::{ReferenceDomain, ReferenceEpoch};
use super::maintenance_validation::{
    self as validation, ByteSource, KeyEncoding, ReplayWitnesses, Span, ValidationIssue,
    WitnessEvent,
};
use sha2::{Digest, Sha256};
use std::cmp::Ordering;
use std::fs::File;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LegacyIssue {
    Resource(ResourceLimit),
    Io(io::ErrorKind),
    Unknown,
    Changed,
    Cancelled,
}
impl From<ResourceLimit> for LegacyIssue {
    fn from(e: ResourceLimit) -> Self {
        Self::Resource(e)
    }
}
impl From<io::Error> for LegacyIssue {
    fn from(e: io::Error) -> Self {
        Self::Io(e.kind())
    }
}
impl From<ValidationIssue> for LegacyIssue {
    fn from(e: ValidationIssue) -> Self {
        match e {
            ValidationIssue::Resource(r) => Self::Resource(r),
            ValidationIssue::Io(k) => Self::Io(k),
            _ => Self::Unknown,
        }
    }
}
impl From<JsonFailure> for LegacyIssue {
    fn from(e: JsonFailure) -> Self {
        match e {
            JsonFailure::Resource(r) => Self::Resource(r),
            JsonFailure::Io(k) => Self::Io(k),
            _ => Self::Unknown,
        }
    }
}
fn fixed(e: super::locks::FixedControlError) -> LegacyIssue {
    match e {
        super::locks::FixedControlError::Resource(r) => LegacyIssue::Resource(r),
        super::locks::FixedControlError::Io(e) => e.into(),
        _ => LegacyIssue::Changed,
    }
}
fn native_error(e: NativeUnknown) -> LegacyIssue {
    match e {
        NativeUnknown::Source(e) => e.into(),
        NativeUnknown::Cancelled => LegacyIssue::Cancelled,
        #[cfg(test)]
        NativeUnknown::SourceChanged => LegacyIssue::Changed,
        _ => LegacyIssue::Unknown,
    }
}
fn validation_error(e: LegacyIssue) -> ValidationIssue {
    match e {
        LegacyIssue::Resource(r) => ValidationIssue::Resource(r),
        LegacyIssue::Io(k) => ValidationIssue::Io(k),
        _ => ValidationIssue::Schema,
    }
}
struct FileSource<'a> {
    file: &'a File,
    len: u64,
}
impl ByteSource for FileSource<'_> {
    fn len(&self) -> u64 {
        self.len
    }
    fn read_at(&self, o: u64, b: &mut [u8]) -> Result<usize, ValidationIssue> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::FileExt;
            self.file
                .read_at(b, o)
                .map_err(|e| ValidationIssue::Io(e.kind()))
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::FileExt;
            self.file
                .seek_read(b, o)
                .map_err(|e| ValidationIssue::Io(e.kind()))
        }
        #[cfg(not(any(unix, windows)))]
        {
            let _ = (o, b);
            Err(ValidationIssue::Schema)
        }
    }
}
struct Slice<'a>(&'a [u8]);
impl ByteSource for Slice<'_> {
    fn len(&self) -> u64 {
        self.0.len() as u64
    }
    fn read_at(&self, o: u64, b: &mut [u8]) -> Result<usize, ValidationIssue> {
        self.0.read_at(o, b)
    }
}
struct Window<'a> {
    file: &'a File,
    at: u64,
    end: u64,
    prefix: bool,
}
impl Read for Window<'_> {
    fn read(&mut self, b: &mut [u8]) -> io::Result<usize> {
        if self.prefix && !b.is_empty() {
            self.prefix = false;
            b[0] = b'{';
            return Ok(1);
        }
        let n = (self.end - self.at).min(b.len() as u64) as usize;
        let source = FileSource {
            file: self.file,
            len: self.end,
        };
        let n = source
            .read_at(self.at, &mut b[..n])
            .map_err(|_| io::ErrorKind::InvalidData)?;
        self.at += n as u64;
        Ok(n)
    }
}
fn span(t: JsonToken) -> Span {
    Span {
        start: t.start,
        end: t.end,
    }
}
fn raw_equal(source: &dyn ByteSource, s: Span, bytes: &[u8]) -> Result<bool, LegacyIssue> {
    if s.end.checked_sub(s.start) != Some(bytes.len() as u64) {
        return Ok(false);
    }
    let mut buf = [0; 256];
    let mut offset = 0;
    while offset < bytes.len() {
        let n = (bytes.len() - offset).min(buf.len());
        let got = source.read_at(s.start + offset as u64, &mut buf[..n])?;
        if got != n || buf[..n] != bytes[offset..offset + n] {
            return Ok(false);
        }
        offset += n;
    }
    Ok(true)
}
fn same_json(
    source: &dyn ByteSource,
    a: Span,
    b: Span,
    memory: &CleanupMemory,
) -> Result<bool, LegacyIssue> {
    Ok(validation::compare_decoded_spans(source, a, source, b, memory)? == Ordering::Equal)
}
fn json_literal(
    source: &dyn ByteSource,
    a: Span,
    literal: &[u8],
    memory: &CleanupMemory,
) -> Result<bool, LegacyIssue> {
    let right = Slice(literal);
    Ok(validation::compare_native_json(
        &right,
        Span {
            start: 0,
            end: literal.len() as u64,
        },
        source,
        a,
        memory,
    )? == Ordering::Equal)
}

struct Scanner<'a> {
    source: &'a dyn ByteSource,
    reader: MaintenanceJsonReader<Window<'a>>,
    memory: &'a CleanupMemory,
    cancel: &'a dyn Fn() -> bool,
    base: u64,
}
impl<'a> Scanner<'a> {
    fn new(
        file: &'a File,
        source: &'a dyn ByteSource,
        range: Span,
        memory: &'a CleanupMemory,
        cancel: &'a dyn Fn() -> bool,
    ) -> Result<Self, LegacyIssue> {
        Ok(Self {
            source,
            reader: MaintenanceJsonReader::new(
                Window {
                    file,
                    at: range.start,
                    end: range.end,
                    prefix: false,
                },
                memory,
            )?,
            memory,
            cancel,
            base: range.start,
        })
    }
    fn token(&mut self) -> Result<JsonToken, LegacyIssue> {
        if (self.cancel)() {
            return Err(LegacyIssue::Cancelled);
        }
        let mut t = self.reader.next_token()?.ok_or(LegacyIssue::Unknown)?;
        t.start += self.offset();
        t.end += self.offset();
        Ok(t)
    }
    // All current Scanner instances start at zero. Nested-range cursors below
    // retain their explicit base, rather than rebadging parser-relative spans.
    fn offset(&self) -> u64 {
        self.base
    }
    fn field(&self, t: JsonToken, fields: &[&str]) -> Result<Option<usize>, LegacyIssue> {
        if !matches!(t.kind, TokenKind::String { key: true, .. }) {
            return Err(LegacyIssue::Unknown);
        }
        for (i, f) in fields.iter().enumerate() {
            if json_literal(self.source, span(t), f.as_bytes(), self.memory)? {
                return Ok(Some(i));
            }
        }
        Ok(None)
    }
    fn string(&self, t: JsonToken) -> Result<Span, LegacyIssue> {
        if matches!(t.kind, TokenKind::String { key: false, .. }) {
            Ok(span(t))
        } else {
            Err(LegacyIssue::Unknown)
        }
    }
    fn skip(&mut self, t: JsonToken) -> Result<Span, LegacyIssue> {
        let mut depth = usize::from(matches!(
            t.kind,
            TokenKind::ObjectStart | TokenKind::ArrayStart
        ));
        let mut end = t.end;
        while depth > 0 {
            let next = self.token()?;
            end = next.end;
            match next.kind {
                TokenKind::ObjectStart | TokenKind::ArrayStart => depth += 1,
                TokenKind::ObjectEnd | TokenKind::ArrayEnd => depth -= 1,
                _ => {}
            }
        }
        Ok(Span {
            start: t.start,
            end,
        })
    }
    fn key(&self, t: JsonToken) -> Result<[u8; 32], LegacyIssue> {
        self.string(t)?;
        let mut bytes = [0; 64];
        if validation::copy_decoded_key(
            self.source,
            span(t),
            KeyEncoding::JsonDecoded,
            &mut bytes,
            self.memory,
        )? != 64
        {
            return Err(LegacyIssue::Unknown);
        }
        let mut key = [0; 32];
        for (i, pair) in bytes.chunks_exact(2).enumerate() {
            let digit = |b| match b {
                b'0'..=b'9' => Some(b - b'0'),
                b'a'..=b'f' => Some(b - b'a' + 10),
                _ => None,
            };
            key[i] = digit(pair[0]).ok_or(LegacyIssue::Unknown)? * 16
                + digit(pair[1]).ok_or(LegacyIssue::Unknown)?;
        }
        Ok(key)
    }
    fn uint(&self, t: JsonToken) -> Result<u64, LegacyIssue> {
        if !matches!(t.kind, TokenKind::Number) {
            return Err(LegacyIssue::Unknown);
        }
        let mut raw = [0; 32];
        let len = t.end.checked_sub(t.start).ok_or(LegacyIssue::Unknown)? as usize;
        if len > raw.len() {
            return Err(LegacyIssue::Unknown);
        }
        let n = self.source.read_at(t.start, &mut raw[..len])?;
        if n != len {
            return Err(LegacyIssue::Unknown);
        }
        serde_json::from_slice::<u64>(&raw[..len]).map_err(|_| LegacyIssue::Unknown)
    }
    fn strings(&mut self, t: JsonToken, map: bool) -> Result<u64, LegacyIssue> {
        if t.kind
            != if map {
                TokenKind::ObjectStart
            } else {
                TokenKind::ArrayStart
            }
        {
            return Err(LegacyIssue::Unknown);
        }
        let mut previous = None;
        let mut count = 0u64;
        loop {
            let next = self.token()?;
            if next.kind
                == if map {
                    TokenKind::ObjectEnd
                } else {
                    TokenKind::ArrayEnd
                }
            {
                break;
            }
            if map {
                if !matches!(next.kind, TokenKind::String { key: true, .. }) {
                    return Err(LegacyIssue::Unknown);
                }
                if let Some(p) = previous {
                    if validation::compare_decoded_spans(
                        self.source,
                        p,
                        self.source,
                        span(next),
                        self.memory,
                    )? != Ordering::Less
                    {
                        return Err(LegacyIssue::Unknown);
                    }
                }
                previous = Some(span(next));
                let value = self.token()?;
                self.string(value)?;
            } else {
                self.string(next)?;
            }
            count = count.checked_add(1).ok_or(LegacyIssue::Unknown)?;
        }
        Ok(count)
    }
    fn search_backend(&mut self, t: JsonToken) -> Result<(), LegacyIssue> {
        if t.kind == TokenKind::Null {
            return Ok(());
        }
        if t.kind != TokenKind::ObjectStart {
            return Err(LegacyIssue::Unknown);
        }
        let mut seen = 0u8;
        loop {
            let key = self.token()?;
            if key.kind == TokenKind::ObjectEnd {
                break;
            }
            let field = self.field(
                key,
                &[
                    "backend",
                    "format_version",
                    "files",
                    "document_count",
                    "term_count",
                    "total_tokens",
                ],
            )?;
            let value = self.token()?;
            if let Some(i) = field {
                if seen & (1 << i) != 0 {
                    return Err(LegacyIssue::Unknown);
                }
                seen |= 1 << i;
                match i {
                    0 => {
                        self.string(value)?;
                    }
                    2 => {
                        self.strings(value, true)?;
                    }
                    _ => {
                        self.uint(value)?;
                    }
                }
            } else {
                self.skip(value)?;
            }
        }
        if seen & 3 != 3 {
            return Err(LegacyIssue::Unknown);
        }
        Ok(())
    }
    fn manifest_entry(&mut self) -> Result<ManifestRecord, LegacyIssue> {
        let mut r = ManifestRecord::default();
        let mut seen = 0u16;
        if self.token()?.kind != TokenKind::ObjectStart {
            return Err(LegacyIssue::Unknown);
        }
        loop {
            let k = self.token()?;
            if k.kind == TokenKind::ObjectEnd {
                break;
            }
            let f = self.field(
                k,
                &[
                    "path",
                    "content_hash",
                    "language",
                    "partition_id",
                    "artifact_key",
                    "node_count",
                    "edge_count",
                    "node_ids",
                    "edge_ids",
                    "node_types",
                    "edge_types",
                    "materialized_at",
                ],
            )?;
            let v = self.token()?;
            if let Some(i) = f {
                if seen & (1 << i) != 0 {
                    return Err(LegacyIssue::Unknown);
                }
                seen |= 1 << i;
                match i {
                    0 => r.path = self.string(v)?,
                    1 => r.hash = self.string(v)?,
                    2 => r.language = self.string(v)?,
                    3 => r.partition = self.string(v)?,
                    4 => r.key = self.key(v)?,
                    5 => r.nodes = self.uint(v)?,
                    6 => r.edges = self.uint(v)?,
                    7..=10 => {
                        self.strings(v, i >= 9)?;
                    }
                    11 => {
                        self.string(v)?;
                    }
                    _ => unreachable!(),
                }
            } else {
                self.skip(v)?;
            }
        }
        if seen & 31 != 31 {
            return Err(LegacyIssue::Unknown);
        }
        Ok(r)
    }
}
#[derive(Debug, Clone, Copy, Default)]
struct ManifestRecord {
    path: Span,
    hash: Span,
    language: Span,
    partition: Span,
    key: [u8; 32],
    nodes: u64,
    edges: u64,
}

// Exact producer schema from assets/graph_schema.json and catalog_support.rs.
const TABLE_NAMES: [&str; 112] = [
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
    "FROM_Assigns",
    "FROM_Calls",
    "FROM_Contains",
    "FROM_Declares",
    "FROM_DecoratedBy",
    "FROM_Defines",
    "FROM_DependsOn",
    "FROM_DerivedFrom",
    "FROM_Documents",
    "FROM_EvidencedBy",
    "FROM_ExecutesQuery",
    "FROM_Exports",
    "FROM_Exposes",
    "FROM_Handles",
    "FROM_HasParameter",
    "FROM_HasReturnType",
    "FROM_HasScope",
    "FROM_HasTypeAnnotation",
    "FROM_Imports",
    "FROM_Raises",
    "FROM_References",
    "FROM_ResolvesTo",
    "FROM_RoutesTo",
    "FROM_SyntaxChild",
    "FROM_UsesSecret",
    "TO_Assigns",
    "TO_Calls",
    "TO_Contains",
    "TO_Declares",
    "TO_DecoratedBy",
    "TO_Defines",
    "TO_DependsOn",
    "TO_DerivedFrom",
    "TO_Documents",
    "TO_EvidencedBy",
    "TO_ExecutesQuery",
    "TO_Exports",
    "TO_Exposes",
    "TO_Handles",
    "TO_HasParameter",
    "TO_HasReturnType",
    "TO_HasScope",
    "TO_HasTypeAnnotation",
    "TO_Imports",
    "TO_Raises",
    "TO_References",
    "TO_ResolvesTo",
    "TO_RoutesTo",
    "TO_SyntaxChild",
    "TO_UsesSecret",
];
const ENDPOINTS: &[(usize, usize, usize)] = &[
    (64, 3, 39),
    (64, 5, 39),
    (64, 15, 39),
    (64, 16, 39),
    (64, 20, 39),
    (64, 21, 39),
    (64, 26, 39),
    (64, 29, 39),
    (64, 31, 39),
    (89, 39, 0),
    (89, 39, 1),
    (89, 39, 2),
    (89, 39, 3),
    (89, 39, 4),
    (89, 39, 5),
    (89, 39, 6),
    (89, 39, 7),
    (89, 39, 8),
    (89, 39, 10),
    (89, 39, 11),
    (89, 39, 12),
    (89, 39, 13),
    (89, 39, 14),
    (89, 39, 15),
    (89, 39, 16),
    (89, 39, 17),
    (89, 39, 18),
    (89, 39, 19),
    (89, 39, 20),
    (89, 39, 21),
    (89, 39, 22),
    (89, 39, 23),
    (89, 39, 24),
    (89, 39, 25),
    (89, 39, 27),
    (89, 39, 28),
    (89, 39, 29),
    (89, 39, 30),
    (89, 39, 31),
    (89, 39, 32),
    (89, 39, 34),
    (89, 39, 35),
    (89, 39, 36),
    (67, 3, 42),
    (67, 5, 42),
    (67, 15, 42),
    (67, 16, 42),
    (67, 20, 42),
    (67, 21, 42),
    (67, 29, 42),
    (92, 42, 0),
    (92, 42, 1),
    (92, 42, 3),
    (92, 42, 4),
    (92, 42, 5),
    (92, 42, 6),
    (92, 42, 8),
    (92, 42, 16),
    (92, 42, 18),
    (92, 42, 20),
    (92, 42, 22),
    (92, 42, 23),
    (92, 42, 24),
    (92, 42, 27),
    (92, 42, 28),
    (92, 42, 30),
    (92, 42, 32),
    (92, 42, 34),
    (92, 42, 35),
    (92, 42, 36),
    (80, 15, 55),
    (80, 21, 55),
    (80, 29, 55),
    (105, 55, 9),
    (105, 55, 17),
    (105, 55, 21),
    (105, 55, 32),
    (73, 5, 48),
    (73, 15, 48),
    (73, 21, 48),
    (73, 29, 48),
    (98, 48, 0),
    (98, 48, 1),
    (98, 48, 3),
    (98, 48, 4),
    (98, 48, 5),
    (98, 48, 6),
    (98, 48, 8),
    (98, 48, 13),
    (98, 48, 16),
    (98, 48, 18),
    (98, 48, 20),
    (98, 48, 22),
    (98, 48, 23),
    (98, 48, 24),
    (98, 48, 27),
    (98, 48, 28),
    (98, 48, 30),
    (98, 48, 32),
    (98, 48, 34),
    (98, 48, 35),
    (98, 48, 36),
    (65, 1, 40),
    (65, 3, 40),
    (65, 16, 40),
    (65, 20, 40),
    (65, 21, 40),
    (65, 29, 40),
    (90, 40, 0),
    (90, 40, 1),
    (90, 40, 3),
    (90, 40, 4),
    (90, 40, 5),
    (90, 40, 6),
    (90, 40, 8),
    (90, 40, 16),
    (90, 40, 18),
    (90, 40, 20),
    (90, 40, 22),
    (90, 40, 23),
    (90, 40, 24),
    (90, 40, 27),
    (90, 40, 28),
    (90, 40, 30),
    (90, 40, 32),
    (90, 40, 34),
    (90, 40, 35),
    (90, 40, 36),
    (78, 0, 53),
    (78, 1, 53),
    (78, 2, 53),
    (78, 3, 53),
    (78, 4, 53),
    (78, 5, 53),
    (78, 6, 53),
    (78, 7, 53),
    (78, 8, 53),
    (78, 12, 53),
    (78, 14, 53),
    (78, 15, 53),
    (78, 16, 53),
    (78, 18, 53),
    (78, 19, 53),
    (78, 20, 53),
    (78, 21, 53),
    (78, 22, 53),
    (78, 23, 53),
    (78, 24, 53),
    (78, 25, 53),
    (78, 27, 53),
    (78, 28, 53),
    (78, 30, 53),
    (78, 32, 53),
    (78, 34, 53),
    (78, 35, 53),
    (78, 36, 53),
    (103, 53, 29),
    (76, 0, 51),
    (76, 2, 51),
    (76, 16, 51),
    (76, 20, 51),
    (76, 28, 51),
    (101, 51, 22),
    (77, 0, 52),
    (77, 16, 52),
    (77, 20, 52),
    (102, 52, 27),
    (79, 4, 54),
    (79, 6, 54),
    (79, 18, 54),
    (79, 22, 54),
    (79, 27, 54),
    (79, 32, 54),
    (79, 34, 54),
    (79, 36, 54),
    (104, 54, 19),
    (104, 54, 25),
    (104, 54, 35),
    (62, 1, 37),
    (62, 4, 37),
    (62, 6, 37),
    (62, 18, 37),
    (62, 23, 37),
    (62, 36, 37),
    (87, 37, 2),
    (87, 37, 4),
    (87, 37, 6),
    (87, 37, 14),
    (87, 37, 18),
    (87, 37, 19),
    (87, 37, 23),
    (87, 37, 36),
    (82, 1, 57),
    (82, 2, 57),
    (82, 7, 57),
    (82, 8, 57),
    (82, 14, 57),
    (82, 24, 57),
    (82, 25, 57),
    (82, 30, 57),
    (82, 35, 57),
    (107, 57, 3),
    (107, 57, 4),
    (107, 57, 6),
    (107, 57, 9),
    (107, 57, 16),
    (107, 57, 18),
    (107, 57, 20),
    (107, 57, 21),
    (107, 57, 22),
    (107, 57, 23),
    (107, 57, 32),
    (107, 57, 36),
    (63, 0, 38),
    (63, 2, 38),
    (63, 5, 38),
    (63, 8, 38),
    (63, 16, 38),
    (63, 20, 38),
    (63, 28, 38),
    (88, 38, 0),
    (88, 38, 2),
    (88, 38, 3),
    (88, 38, 16),
    (88, 38, 20),
    (66, 0, 41),
    (66, 3, 41),
    (66, 5, 41),
    (66, 16, 41),
    (66, 20, 41),
    (66, 23, 41),
    (66, 28, 41),
    (91, 41, 2),
    (91, 41, 8),
    (91, 41, 25),
    (83, 2, 58),
    (83, 8, 58),
    (83, 17, 58),
    (83, 25, 58),
    (83, 35, 58),
    (108, 58, 3),
    (108, 58, 6),
    (108, 58, 9),
    (108, 58, 16),
    (108, 58, 20),
    (108, 58, 21),
    (108, 58, 22),
    (108, 58, 32),
    (108, 58, 36),
    (68, 5, 43),
    (68, 9, 43),
    (68, 15, 43),
    (68, 17, 43),
    (68, 21, 43),
    (68, 26, 43),
    (68, 31, 43),
    (93, 43, 5),
    (93, 43, 9),
    (93, 43, 21),
    (93, 43, 30),
    (70, 10, 45),
    (70, 11, 45),
    (70, 19, 45),
    (95, 45, 0),
    (95, 45, 1),
    (95, 45, 3),
    (95, 45, 4),
    (95, 45, 5),
    (95, 45, 6),
    (95, 45, 8),
    (95, 45, 15),
    (95, 45, 16),
    (95, 45, 18),
    (95, 45, 20),
    (95, 45, 21),
    (95, 45, 22),
    (95, 45, 23),
    (95, 45, 24),
    (95, 45, 26),
    (95, 45, 27),
    (95, 45, 28),
    (95, 45, 30),
    (95, 45, 32),
    (95, 45, 34),
    (95, 45, 35),
    (95, 45, 36),
    (84, 0, 59),
    (84, 5, 59),
    (84, 28, 59),
    (109, 59, 0),
    (109, 59, 5),
    (109, 59, 16),
    (109, 59, 20),
    (74, 0, 49),
    (74, 5, 49),
    (74, 21, 49),
    (74, 26, 49),
    (74, 28, 49),
    (99, 49, 0),
    (99, 49, 5),
    (99, 49, 13),
    (99, 49, 16),
    (99, 49, 20),
    (99, 49, 28),
    (72, 0, 47),
    (72, 2, 47),
    (72, 5, 47),
    (72, 16, 47),
    (72, 20, 47),
    (97, 47, 24),
    (86, 0, 61),
    (86, 2, 61),
    (86, 5, 61),
    (86, 9, 61),
    (86, 16, 61),
    (86, 20, 61),
    (111, 61, 30),
    (81, 2, 56),
    (81, 7, 56),
    (81, 16, 56),
    (81, 20, 56),
    (106, 56, 12),
    (75, 7, 50),
    (75, 12, 50),
    (75, 16, 50),
    (75, 20, 50),
    (100, 50, 12),
    (85, 33, 60),
    (110, 60, 33),
    (69, 0, 44),
    (69, 1, 44),
    (69, 2, 44),
    (69, 3, 44),
    (69, 4, 44),
    (69, 5, 44),
    (69, 6, 44),
    (69, 7, 44),
    (69, 8, 44),
    (69, 10, 44),
    (69, 11, 44),
    (69, 12, 44),
    (69, 13, 44),
    (69, 14, 44),
    (69, 16, 44),
    (69, 17, 44),
    (69, 18, 44),
    (69, 19, 44),
    (69, 20, 44),
    (69, 21, 44),
    (69, 22, 44),
    (69, 23, 44),
    (69, 24, 44),
    (69, 25, 44),
    (69, 27, 44),
    (69, 28, 44),
    (69, 30, 44),
    (69, 32, 44),
    (69, 34, 44),
    (69, 35, 44),
    (69, 36, 44),
    (94, 44, 33),
    (71, 0, 46),
    (71, 1, 46),
    (71, 2, 46),
    (71, 3, 46),
    (71, 4, 46),
    (71, 5, 46),
    (71, 6, 46),
    (71, 7, 46),
    (71, 8, 46),
    (71, 9, 46),
    (71, 10, 46),
    (71, 11, 46),
    (71, 12, 46),
    (71, 14, 46),
    (71, 15, 46),
    (71, 16, 46),
    (71, 18, 46),
    (71, 19, 46),
    (71, 20, 46),
    (71, 21, 46),
    (71, 22, 46),
    (71, 23, 46),
    (71, 24, 46),
    (71, 25, 46),
    (71, 26, 46),
    (71, 27, 46),
    (71, 28, 46),
    (71, 30, 46),
    (71, 32, 46),
    (71, 34, 46),
    (71, 35, 46),
    (71, 36, 46),
    (96, 46, 10),
    (96, 46, 15),
    (96, 46, 33),
];
/// Current program schema facts for the separately guarded legacy COPY lane.
pub(crate) fn staging_endpoint_allowed(table: u8, from: u8, to: u8) -> bool {
    ENDPOINTS.contains(&(table as usize, from as usize, to as usize))
}

const NODE_FIELDS: [(&[u8], u8, u8); 17] = [
    (b"id", 50, 20),
    (b"label", 50, 20),
    (b"kind", 50, 20),
    (b"language", 50, 20),
    (b"grammar_version", 50, 20),
    (b"path", 50, 20),
    (b"qualified_name", 50, 20),
    (b"scope_id", 50, 20),
    (b"line_start", 23, 2),
    (b"line_end", 23, 2),
    (b"byte_start", 23, 2),
    (b"byte_end", 23, 2),
    (b"tree_sitter_node_type", 50, 20),
    (b"capture_name", 50, 20),
    (b"summary", 50, 20),
    (b"text", 50, 20),
    (b"metadata", 60, 21),
];
const EDGE_FIELDS: [(&[u8], u8, u8); 10] = [
    (b"id", 50, 20),
    (b"kind", 50, 20),
    (b"source_id", 50, 20),
    (b"target_id", 50, 20),
    (b"confidence", 32, 11),
    (b"field_name", 50, 20),
    (b"child_index", 23, 2),
    (b"line_start", 23, 2),
    (b"line_end", 23, 2),
    (b"metadata", 60, 21),
];
#[derive(Clone, Copy)]
struct CatalogCell {
    id: u64,
    seen: bool,
    properties: u32,
    columns: u32,
    group: bool,
}
#[derive(Clone, Copy)]
struct Endpoint {
    table: u64,
    source: u64,
    target: u64,
}
fn recognize_catalog(
    source: &dyn ByteSource,
    memory: &CleanupMemory,
    cancel: &dyn Fn() -> bool,
) -> Result<(), LegacyIssue> {
    let _floor = memory.reserve(
        std::mem::size_of::<[CatalogCell; 112]>()
            + std::mem::size_of::<[Endpoint; ENDPOINTS.len()]>()
            + 2048,
    )?;
    let mut cells = [CatalogCell {
        id: u64::MAX,
        seen: false,
        properties: 0,
        columns: 0,
        group: false,
    }; 112];
    let mut endpoints = [Endpoint {
        table: u64::MAX,
        source: 0,
        target: 0,
    }; ENDPOINTS.len()];
    let mut endpoint_count = 0usize;
    let result = native::trace_native_catalog43(source, memory, &mut |event| {
        if cancel() {
            return Err(NativeUnknown::Cancelled);
        }
        let run = (|| -> Result<(), LegacyIssue> {
            match event {
                NativeCatalogEvent::Table { id, name, node } => {
                    let mut index = None;
                    for (i, n) in TABLE_NAMES.iter().enumerate() {
                        if raw_equal(source, name, n.as_bytes())? {
                            index = Some(i);
                            break;
                        }
                    }
                    let i = index.ok_or(LegacyIssue::Unknown)?;
                    if cells[i].seen || node != (i < 62) {
                        return Err(LegacyIssue::Unknown);
                    }
                    cells[i].seen = true;
                    cells[i].id = id;
                }
                NativeCatalogEvent::Property {
                    table,
                    id,
                    name,
                    logical,
                    physical,
                    default,
                } => {
                    let i = cells
                        .iter()
                        .position(|c| c.seen && c.id == table)
                        .ok_or(LegacyIssue::Unknown)?;
                    let p = id as usize;
                    let expected = if i < 37 {
                        if p < NODE_FIELDS.len() {
                            NODE_FIELDS[p]
                        } else if TABLE_NAMES[i] == "File" && p == 17 {
                            (b"content_hash".as_slice(), 50, 20)
                        } else {
                            return Err(LegacyIssue::Unknown);
                        }
                    } else if i < 62 {
                        *EDGE_FIELDS.get(p).ok_or(LegacyIssue::Unknown)?
                    } else {
                        match p {
                            0 => (b"_ID".as_slice(), 42, 14),
                            1 => (b"role".as_slice(), 50, 20),
                            _ => return Err(LegacyIssue::Unknown),
                        }
                    };
                    if cells[i].properties & (1 << p) != 0
                        || !raw_equal(source, name, expected.0)?
                        || (logical, physical) != (expected.1, expected.2)
                    {
                        return Err(LegacyIssue::Unknown);
                    }
                    match default {
                        NativeDefaultFacts::Null => {
                            if i >= 62 && p == 1 {
                                return Err(LegacyIssue::Unknown);
                            }
                        }
                        NativeDefaultFacts::Text(s) => {
                            if i < 62
                                || p != 1
                                || !raw_equal(
                                    source,
                                    s,
                                    if i < 87 { b"source" } else { b"target" },
                                )?
                            {
                                return Err(LegacyIssue::Unknown);
                            }
                        }
                    }
                    cells[i].properties |= 1 << p;
                }
                NativeCatalogEvent::Column {
                    table,
                    property,
                    column,
                } => {
                    let i = cells
                        .iter()
                        .position(|c| c.seen && c.id == table)
                        .ok_or(LegacyIssue::Unknown)?;
                    if property.checked_add(u32::from(i >= 62)) != Some(column)
                        || property >= 18
                        || cells[i].columns & (1 << property) != 0
                    {
                        return Err(LegacyIssue::Unknown);
                    }
                    cells[i].columns |= 1 << property;
                }
                NativeCatalogEvent::RelationshipGroup {
                    table,
                    source_multiplicity,
                    target_multiplicity,
                    direction,
                } => {
                    let i = cells
                        .iter()
                        .position(|c| c.seen && c.id == table)
                        .ok_or(LegacyIssue::Unknown)?;
                    if i < 62
                        || cells[i].group
                        || (source_multiplicity, target_multiplicity, direction) != (0, 0, 2)
                    {
                        return Err(LegacyIssue::Unknown);
                    }
                    cells[i].group = true;
                }
                NativeCatalogEvent::RelationshipEndpoint {
                    table,
                    source,
                    target,
                    source_flag,
                    target_flag,
                    ..
                } => {
                    if source_flag || target_flag || endpoint_count >= endpoints.len() {
                        return Err(LegacyIssue::Unknown);
                    }
                    endpoints[endpoint_count] = Endpoint {
                        table,
                        source,
                        target,
                    };
                    endpoint_count += 1;
                }
                _ => {}
            }
            Ok(())
        })();
        run.map_err(|e| match e {
            LegacyIssue::Resource(r) => NativeUnknown::Source(ValidationIssue::Resource(r)),
            LegacyIssue::Cancelled => NativeUnknown::Cancelled,
            _ => NativeUnknown::UnsupportedLayout,
        })
    })
    .map_err(native_error)?;
    if result.tables != 112 || result.nodes != 62 || endpoint_count != ENDPOINTS.len() {
        return Err(LegacyIssue::Unknown);
    }
    for (i, c) in cells.iter().enumerate() {
        let count = if i < 37 {
            if TABLE_NAMES[i] == "File" {
                18
            } else {
                17
            }
        } else if i < 62 {
            10
        } else {
            2
        };
        let mask = (1 << count) - 1;
        if !c.seen || c.properties != mask || c.columns != mask || c.group != (i >= 62) {
            return Err(LegacyIssue::Unknown);
        }
    }
    let mut matched = [false; ENDPOINTS.len()];
    for e in &endpoints[..endpoint_count] {
        let t = cells
            .iter()
            .position(|c| c.id == e.table)
            .ok_or(LegacyIssue::Unknown)?;
        let s = cells
            .iter()
            .position(|c| c.id == e.source)
            .ok_or(LegacyIssue::Unknown)?;
        let d = cells
            .iter()
            .position(|c| c.id == e.target)
            .ok_or(LegacyIssue::Unknown)?;
        let i = ENDPOINTS
            .iter()
            .position(|v| *v == (t, s, d))
            .ok_or(LegacyIssue::Unknown)?;
        if matched[i] {
            return Err(LegacyIssue::Unknown);
        }
        matched[i] = true;
    }
    if matched.iter().any(|v| !*v) {
        return Err(LegacyIssue::Unknown);
    }
    Ok(())
}

fn project_manifest(
    file: &File,
    memory: &CleanupMemory,
    cancel: &dyn Fn() -> bool,
) -> Result<Span, LegacyIssue> {
    let stamp = FixedFileStamp::capture(file, FixedControlKind::File).map_err(fixed)?;
    let source = FileSource {
        file,
        len: stamp.len(),
    };
    let mut scanner = Scanner::new(
        file,
        &source,
        Span {
            start: 0,
            end: source.len(),
        },
        memory,
        cancel,
    )?;
    let mut fields = 0u8;
    let mut files = None;
    if scanner.token()?.kind != TokenKind::ObjectStart {
        return Err(LegacyIssue::Unknown);
    }
    loop {
        let k = scanner.token()?;
        if k.kind == TokenKind::ObjectEnd {
            break;
        }
        let f = scanner.field(
            k,
            &[
                "schema_version",
                "ontology",
                "parser_version",
                "graph_build_digest",
                "search_backend",
                "files",
            ],
        )?;
        let v = scanner.token()?;
        if let Some(i) = f {
            if fields & (1 << i) != 0 {
                return Err(LegacyIssue::Unknown);
            }
            fields |= 1 << i;
            match i {
                0 => {
                    if scanner.uint(v)? != 5 {
                        return Err(LegacyIssue::Unknown);
                    }
                }
                1 | 2 => {
                    scanner.string(v)?;
                }
                3 => {
                    if v.kind != TokenKind::Null {
                        scanner.string(v)?;
                    }
                }
                4 => {
                    scanner.search_backend(v)?;
                }
                5 => {
                    if v.kind != TokenKind::ObjectStart {
                        return Err(LegacyIssue::Unknown);
                    }
                    files = Some(scanner.skip(v)?);
                }
                _ => unreachable!(),
            }
        } else {
            scanner.skip(v)?;
        }
    }
    if fields & 39 != 39 || scanner.reader.next_token()?.is_some() {
        return Err(LegacyIssue::Unknown);
    }
    stamp.recheck(file).map_err(fixed)?;
    files.ok_or(LegacyIssue::Unknown)
}
// Retain logical progress, not duplicate native descriptors. Each next() owns
// a charged bounded reader over the same borrowed manifest with an independent
// positional offset. Replaying its one-token lookahead preserves exact spans.
struct ManifestCursor {
    end: u64,
    previous: Option<Span>,
    next_offset: u64,
    done: bool,
}
impl ManifestCursor {
    fn new(file: &File, range: Span, memory: &CleanupMemory) -> Result<Self, LegacyIssue> {
        let mut reader = MaintenanceJsonReader::new(
            Window {
                file,
                at: range.start,
                end: range.end,
                prefix: false,
            },
            memory,
        )?;
        if reader.next_token()?.map(|token| token.kind) != Some(TokenKind::ObjectStart) {
            return Err(LegacyIssue::Unknown);
        }
        Ok(Self {
            end: range.end,
            previous: None,
            next_offset: range.start + 1,
            done: false,
        })
    }
    fn resume(
        file: &File,
        range: Span,
        next_offset: u64,
        previous: Option<Span>,
        memory: &CleanupMemory,
    ) -> Result<Self, LegacyIssue> {
        if next_offset <= range.start || next_offset >= range.end {
            return Err(LegacyIssue::Unknown);
        }
        let mut reader = MaintenanceJsonReader::new(
            Window {
                file,
                at: next_offset,
                end: range.end,
                prefix: true,
            },
            memory,
        )?;
        if reader.next_token()?.map(|token| token.kind) != Some(TokenKind::ObjectStart) {
            return Err(LegacyIssue::Unknown);
        }
        Ok(Self {
            end: range.end,
            previous,
            next_offset,
            done: false,
        })
    }
    fn next(
        &mut self,
        file: &File,
        memory: &CleanupMemory,
        cancel: &dyn Fn() -> bool,
    ) -> Result<Option<ManifestRecord>, LegacyIssue> {
        if self.done {
            return Ok(None);
        }
        let source = FileSource {
            file,
            len: file.metadata()?.len(),
        };
        let mut reader = MaintenanceJsonReader::new(
            Window {
                file,
                at: self.next_offset,
                end: self.end,
                prefix: true,
            },
            memory,
        )?;
        if reader.next_token()?.map(|token| token.kind) != Some(TokenKind::ObjectStart) {
            return Err(LegacyIssue::Unknown);
        }
        let mut scanner = Scanner {
            source: &source,
            reader,
            memory,
            cancel,
            base: self.next_offset - 1,
        };
        let key = scanner.token()?;
        if key.kind == TokenKind::ObjectEnd {
            self.next_offset = key.start;
            self.done = true;
            return Ok(None);
        }
        if !matches!(key.kind, TokenKind::String { key: true, .. }) {
            return Err(LegacyIssue::Unknown);
        }
        let keyspan = span(key);
        if let Some(previous) = self.previous {
            if validation::compare_decoded_spans(&source, previous, &source, keyspan, memory)?
                != Ordering::Less
            {
                return Err(LegacyIssue::Unknown);
            }
        }
        let record = scanner.manifest_entry()?;
        if !same_json(&source, keyspan, record.path, memory)? {
            return Err(LegacyIssue::Unknown);
        }
        self.previous = Some(keyspan);
        self.next_offset = scanner.token()?.start;
        Ok(Some(record))
    }
}

fn hex(key: [u8; 32]) -> [u8; 64] {
    let mut out = [0; 64];
    let digits = b"0123456789abcdef";
    for (i, b) in key.iter().enumerate() {
        out[2 * i] = digits[(b >> 4) as usize];
        out[2 * i + 1] = digits[(b & 15) as usize];
    }
    out
}
#[derive(Clone, Copy)]
struct BackupCache {
    key: [u8; 32],
    name: [u8; 128],
    len: u8,
    directory: FileIdentity,
    stamp: FixedFileStamp,
}
fn backup_name(key: [u8; 32], name: &[u8]) -> bool {
    if name.len() < 76
        || name.len() > 114
        || &name[..10] != b".replaced-"
        || name[10..74] != hex(key)
        || name[74] != b'-'
    {
        return false;
    }
    let digits = &name[75..];
    if digits.len() > 1 && digits[0] == b'0' {
        return false;
    }
    digits
        .iter()
        .try_fold(0u128, |n, b| {
            if b.is_ascii_digit() {
                n.checked_mul(10)?.checked_add((b - b'0') as u128)
            } else {
                None
            }
        })
        .is_some()
}
fn open_cache(
    root: &RootedDirectory,
    key: [u8; 32],
    backup: Option<BackupCache>,
) -> Result<File, LegacyIssue> {
    let name = hex(key);
    let prefix = root.open_directory_readonly(&name[..2])?;
    let directory = if let Some(b) = backup.filter(|b| b.key == key) {
        match prefix.open_directory_readonly(&name) {
            Ok(_) => return Err(LegacyIssue::Changed),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
        let directory = prefix.open_directory_readonly(&b.name[..usize::from(b.len)])?;
        if directory.identity() != b.directory {
            return Err(LegacyIssue::Changed);
        }
        directory
    } else {
        prefix.open_directory_readonly(&name)?
    };
    let file = directory.open_readonly_file(b"partition.json")?;
    if let Some(b) = backup.filter(|b| b.key == key) {
        b.stamp.recheck(&file).map_err(fixed)?;
    }
    Ok(file)
}

/// Key comparators may nest two logical sources while native DB and manifest
/// remain pinned. Each positional read borrows one payload descriptor and drops
/// it before the other source reads, preserving the aggregate three-FD bound.
struct CacheKeySource<'a> {
    root: &'a RootedDirectory,
    key: [u8; 32],
    backup: Option<BackupCache>,
    stamp: FixedFileStamp,
}
impl ByteSource for CacheKeySource<'_> {
    fn len(&self) -> u64 {
        self.stamp.len()
    }
    fn read_at(&self, offset: u64, bytes: &mut [u8]) -> Result<usize, ValidationIssue> {
        let file = open_cache(self.root, self.key, self.backup).map_err(validation_error)?;
        self.stamp
            .recheck(&file)
            .map_err(|_| ValidationIssue::Schema)?;
        let source = FileSource {
            file: &file,
            len: self.stamp.len(),
        };
        let read = source.read_at(offset, bytes)?;
        self.stamp
            .recheck(&file)
            .map_err(|_| ValidationIssue::Schema)?;
        Ok(read)
    }
}

fn file_digest(
    file: &File,
    stamp: FixedFileStamp,
    cancel: &dyn Fn() -> bool,
) -> Result<[u8; 32], LegacyIssue> {
    let source = FileSource {
        file,
        len: stamp.len(),
    };
    let mut buffer = [0; 4096];
    let mut at = 0;
    let mut hash = Sha256::new();
    while at < source.len() {
        if cancel() {
            return Err(LegacyIssue::Cancelled);
        }
        let n = ((source.len() - at) as usize).min(buffer.len());
        let got = source.read_at(at, &mut buffer[..n])?;
        if got == 0 {
            return Err(LegacyIssue::Changed);
        }
        hash.update(&buffer[..got]);
        at += got as u64;
    }
    stamp.recheck(file).map_err(fixed)?;
    Ok(hash.finalize().into())
}

#[derive(Clone, Copy)]
enum WitnessKey {
    Cache {
        key: [u8; 32],
        stamp: FixedFileStamp,
        span: Span,
    },
    Native {
        locator: NativeStringLocator,
        span: Span,
        json: bool,
    },
}
struct Witnesses<'a> {
    database: &'a File,
    db_stamp: FixedFileStamp,
    manifest: &'a File,
    manifest_stamp: FixedFileStamp,
    root: &'a RootedDirectory,
    backup: Option<BackupCache>,
    certificate: [u8; 32],
    cache_count: u64,
    certificates: &'a dyn ReplayLegacyCertificates,
    cancel: &'a dyn Fn() -> bool,
}
impl Witnesses<'_> {
    fn with_key<T>(
        &self,
        key: WitnessKey,
        memory: &CleanupMemory,
        f: impl FnOnce(&dyn ByteSource, Span, KeyEncoding) -> Result<T, ValidationIssue>,
    ) -> Result<T, ValidationIssue> {
        if (self.cancel)() {
            return Err(ValidationIssue::Schema);
        }
        match key {
            WitnessKey::Cache { key, stamp, span } => {
                let _floor = memory.reserve(2048)?;
                let source = CacheKeySource {
                    root: self.root,
                    key,
                    backup: self.backup,
                    stamp,
                };
                f(&source, span, KeyEncoding::JsonDecoded)
            }
            WitnessKey::Native {
                locator,
                span,
                json,
            } => {
                self.db_stamp
                    .recheck(self.database)
                    .map_err(|_| ValidationIssue::Schema)?;
                let source = FileSource {
                    file: self.database,
                    len: self.db_stamp.len(),
                };
                let logical = NativeStringSource::new(&source, locator, memory)
                    .map_err(|_| ValidationIssue::Schema)?;
                let value = f(
                    &logical,
                    span,
                    if json {
                        KeyEncoding::JsonDecoded
                    } else {
                        KeyEncoding::NativeUtf8
                    },
                )?;
                self.db_stamp
                    .recheck(self.database)
                    .map_err(|_| ValidationIssue::Schema)?;
                Ok(value)
            }
        }
    }
    fn compare_keys(
        &self,
        a: WitnessKey,
        b: WitnessKey,
        memory: &CleanupMemory,
    ) -> Result<Ordering, ValidationIssue> {
        self.with_key(a, memory, |left, ls, le| {
            self.with_key(b, memory, |right, rs, re| match (le, re) {
                (KeyEncoding::JsonDecoded, KeyEncoding::JsonDecoded) => {
                    validation::compare_decoded_spans(left, ls, right, rs, memory)
                }
                (KeyEncoding::NativeUtf8, KeyEncoding::JsonDecoded) => {
                    validation::compare_native_json(left, ls, right, rs, memory)
                }
                (KeyEncoding::JsonDecoded, KeyEncoding::NativeUtf8) => {
                    validation::compare_native_json(right, rs, left, ls, memory)
                        .map(Ordering::reverse)
                }
                _ => {
                    validation::key_stream_hash(left, ls, le, 0, memory)?;
                    validation::key_stream_hash(right, rs, re, 0, memory)?;
                    let mut ordering = Ordering::Equal;
                    let mut l = [0; 256];
                    let mut r = [0; 256];
                    let mut at = 0;
                    let llen = ls.end - ls.start;
                    let rlen = rs.end - rs.start;
                    while at < llen.max(rlen) {
                        let ln = ((llen.saturating_sub(at)).min(l.len() as u64)) as usize;
                        let rn = ((rlen.saturating_sub(at)).min(r.len() as u64)) as usize;
                        if left.read_at(ls.start + at, &mut l[..ln])? != ln
                            || right.read_at(rs.start + at, &mut r[..rn])? != rn
                        {
                            return Err(ValidationIssue::InvalidSpan);
                        }
                        if ordering == Ordering::Equal {
                            ordering = l[..ln].cmp(&r[..rn]);
                        }
                        at += l.len() as u64;
                    }
                    Ok(ordering)
                }
            })
        })
    }
}
fn id_bytes(
    source: &dyn ByteSource,
    key: Span,
    encoding: KeyEncoding,
    prefix: &[u8],
    memory: &CleanupMemory,
) -> Result<([u8; 64], usize), LegacyIssue> {
    let hash = validation::key_sha1_prefix20(source, key, encoding, memory)?;
    let mut out = [0; 64];
    if prefix.len() + 21 > out.len() {
        return Err(LegacyIssue::Unknown);
    }
    out[..prefix.len()].copy_from_slice(prefix);
    out[prefix.len()] = b':';
    let digits = b"0123456789abcdef";
    for (i, b) in hash[..10].iter().enumerate() {
        out[prefix.len() + 1 + 2 * i] = digits[(b >> 4) as usize];
        out[prefix.len() + 2 + 2 * i] = digits[(b & 15) as usize];
    }
    Ok((out, prefix.len() + 21))
}
impl Scanner<'_> {
    fn metadata_key(&mut self, t: JsonToken) -> Result<Span, LegacyIssue> {
        if t.kind != TokenKind::ObjectStart {
            return Err(LegacyIssue::Unknown);
        }
        let mut canonical = None;
        loop {
            let k = self.token()?;
            if k.kind == TokenKind::ObjectEnd {
                break;
            }
            let field = self.field(k, &["canonical_key"])?;
            let v = self.token()?;
            if field.is_some() {
                if canonical.is_some() {
                    return Err(LegacyIssue::Unknown);
                }
                canonical = Some(self.string(v)?);
            } else {
                self.skip(v)?;
            }
        }
        canonical.ok_or(LegacyIssue::Unknown)
    }
    // Projection keeps exact source spans/stamps and accounted validation callbacks as independent inputs.
    #[allow(clippy::too_many_arguments)]
    fn selected_row(
        &mut self,
        t: JsonToken,
        edge: bool,
        record: ManifestRecord,
        stamp: FixedFileStamp,
        repository: &[u8],
        label: &[u8],
        ordinal: u64,
        emit: &mut dyn FnMut(WitnessEvent<WitnessKey>) -> Result<(), ValidationIssue>,
    ) -> Result<(), LegacyIssue> {
        if t.kind != TokenKind::ObjectStart {
            return Err(LegacyIssue::Unknown);
        }
        let mut fields = [None; 8];
        loop {
            let k = self.token()?;
            if k.kind == TokenKind::ObjectEnd {
                break;
            }
            let f = self.field(
                k,
                &[
                    "id",
                    if edge { "edge_type" } else { "table" },
                    "label",
                    "kind",
                    "path",
                    "source_id",
                    "target_id",
                    "metadata",
                ],
            )?;
            let v = self.token()?;
            if let Some(i) = f {
                if fields[i].is_some() {
                    return Err(LegacyIssue::Unknown);
                }
                fields[i] = Some(if i == 7 {
                    self.metadata_key(v)?
                } else {
                    self.string(v)?
                });
            } else {
                self.skip(v)?;
            }
        }
        let get = |i: usize| fields[i].ok_or(LegacyIssue::Unknown);
        let table = get(1)?;
        let domain = if json_literal(self.source, table, b"Repository", self.memory)? {
            1
        } else if json_literal(self.source, table, b"SourceRoot", self.memory)? {
            2
        } else if json_literal(self.source, table, b"File", self.memory)? {
            3
        } else if edge && json_literal(self.source, table, b"Contains", self.memory)? {
            4
        } else {
            return Ok(());
        };
        let id = get(0)?;
        let kind = get(3)?;
        let canonical = get(7)?;
        let prefix = match domain {
            1 => b"Repository".as_slice(),
            2 => b"SourceRoot",
            3 => b"File",
            4 => b"edge",
            _ => unreachable!(),
        };
        let (expected_id, id_len) = id_bytes(
            self.source,
            canonical,
            KeyEncoding::JsonDecoded,
            prefix,
            self.memory,
        )?;
        if !json_literal(self.source, id, &expected_id[..id_len], self.memory)? {
            return Err(LegacyIssue::Unknown);
        }
        let projected = match domain {
            1 => {
                if edge
                    || !json_literal(self.source, kind, b"repository", self.memory)?
                    || !json_literal(self.source, get(4)?, b"", self.memory)?
                    || !json_literal(self.source, canonical, label, self.memory)?
                    || !same_json(self.source, get(2)?, canonical, self.memory)?
                {
                    return Err(LegacyIssue::Unknown);
                }
                [id, get(2)?, kind, get(4)?, canonical]
            }
            2 => {
                if edge
                    || !json_literal(self.source, kind, b"sourceroot", self.memory)?
                    || !json_literal(self.source, canonical, repository, self.memory)?
                    || !same_json(self.source, get(2)?, canonical, self.memory)?
                    || !same_json(self.source, get(4)?, canonical, self.memory)?
                {
                    return Err(LegacyIssue::Unknown);
                }
                [id, get(2)?, kind, get(4)?, canonical]
            }
            3 => {
                if edge
                    || !json_literal(self.source, kind, b"file", self.memory)?
                    || !same_json(self.source, get(4)?, record.path, self.memory)?
                    || !same_json(self.source, canonical, record.path, self.memory)?
                {
                    return Err(LegacyIssue::Unknown);
                }
                [id, kind, get(4)?, record.hash, canonical]
            }
            4 => {
                if !edge {
                    return Err(LegacyIssue::Unknown);
                }
                let role =
                    if json_literal(self.source, kind, b"repository_source_root", self.memory)? {
                        0
                    } else if json_literal(self.source, kind, b"source_root_file", self.memory)? {
                        1
                    } else {
                        return Ok(());
                    };
                let root_source = Slice(repository);
                let label_source = Slice(label);
                let (root_id, root_len) = id_bytes(
                    &root_source,
                    Span {
                        start: 0,
                        end: repository.len() as u64,
                    },
                    KeyEncoding::NativeUtf8,
                    b"SourceRoot",
                    self.memory,
                )?;
                let (repo_id, repo_len) = id_bytes(
                    &label_source,
                    Span {
                        start: 0,
                        end: label.len() as u64,
                    },
                    KeyEncoding::NativeUtf8,
                    b"Repository",
                    self.memory,
                )?;
                let (file_id, file_len) = id_bytes(
                    self.source,
                    record.path,
                    KeyEncoding::JsonDecoded,
                    b"File",
                    self.memory,
                )?;
                let (source_id, target_id) = if role == 0 {
                    (&repo_id[..repo_len], &root_id[..root_len])
                } else {
                    (&root_id[..root_len], &file_id[..file_len])
                };
                if !json_literal(self.source, get(5)?, source_id, self.memory)?
                    || !json_literal(self.source, get(6)?, target_id, self.memory)?
                {
                    return Err(LegacyIssue::Unknown);
                }
                let role = if role == 0 {
                    b"repository_source_root".as_slice()
                } else {
                    b"source_root_file"
                };
                let mut expected = [0; 256];
                let mut len = 0;
                for part in [
                    b"Contains|".as_slice(),
                    source_id,
                    b"|",
                    target_id,
                    b"|",
                    role,
                ] {
                    let end = len + part.len();
                    if end > expected.len() {
                        return Err(LegacyIssue::Unknown);
                    }
                    expected[len..end].copy_from_slice(part);
                    len = end;
                }
                if !json_literal(self.source, canonical, &expected[..len], self.memory)? {
                    return Err(LegacyIssue::Unknown);
                }
                [id, kind, get(5)?, get(6)?, canonical]
            }
            _ => unreachable!(),
        };
        emit(WitnessEvent {
            domain,
            native: false,
            ordinal,
            fields: projected.map(|span| {
                Some(WitnessKey::Cache {
                    key: record.key,
                    stamp,
                    span,
                })
            }),
        })?;
        Ok(())
    }
    // Projection keeps exact source spans/stamps and accounted validation callbacks as independent inputs.
    #[allow(clippy::too_many_arguments)]
    fn selected_array(
        &mut self,
        t: JsonToken,
        edge: bool,
        record: ManifestRecord,
        stamp: FixedFileStamp,
        repository: &[u8],
        label: &[u8],
        ordinal: &mut u64,
        emit: &mut dyn FnMut(WitnessEvent<WitnessKey>) -> Result<(), ValidationIssue>,
    ) -> Result<(), LegacyIssue> {
        if t.kind != TokenKind::ArrayStart {
            return Err(LegacyIssue::Unknown);
        }
        loop {
            let t = self.token()?;
            if t.kind == TokenKind::ArrayEnd {
                break;
            }
            self.selected_row(t, edge, record, stamp, repository, label, *ordinal, emit)?;
            *ordinal = ordinal.checked_add(1).ok_or(LegacyIssue::Unknown)?;
        }
        Ok(())
    }
    fn cache_entry(&mut self, t: JsonToken) -> Result<ManifestRecord, LegacyIssue> {
        if t.kind != TokenKind::ObjectStart {
            return Err(LegacyIssue::Unknown);
        }
        let mut fields = [None; 5];
        let mut key = None;
        let mut nodes = None;
        let mut edges = None;
        loop {
            let k = self.token()?;
            if k.kind == TokenKind::ObjectEnd {
                break;
            }
            let f = self.field(
                k,
                &[
                    "path",
                    "content_hash",
                    "language",
                    "partition_id",
                    "artifact_key",
                    "node_count",
                    "edge_count",
                ],
            )?;
            let v = self.token()?;
            match f {
                Some(i @ 0..=3) => {
                    if fields[i].is_some() {
                        return Err(LegacyIssue::Unknown);
                    }
                    fields[i] = Some(self.string(v)?);
                }
                Some(4) => {
                    if key.is_some() {
                        return Err(LegacyIssue::Unknown);
                    }
                    key = Some(self.key(v)?);
                }
                Some(5) => nodes = Some(self.uint(v)?),
                Some(6) => edges = Some(self.uint(v)?),
                _ => {
                    self.skip(v)?;
                }
            }
        }
        Ok(ManifestRecord {
            path: fields[0].ok_or(LegacyIssue::Unknown)?,
            hash: fields[1].ok_or(LegacyIssue::Unknown)?,
            language: fields[2].ok_or(LegacyIssue::Unknown)?,
            partition: fields[3].ok_or(LegacyIssue::Unknown)?,
            key: key.ok_or(LegacyIssue::Unknown)?,
            nodes: nodes.ok_or(LegacyIssue::Unknown)?,
            edges: edges.ok_or(LegacyIssue::Unknown)?,
        })
    }
}

// Projection keeps exact source spans/stamps and accounted validation callbacks as independent inputs.
#[allow(clippy::too_many_arguments)]
fn project_cache(
    file: &File,
    stamp: FixedFileStamp,
    key: [u8; 32],
    manifest: &FileSource<'_>,
    expected: ManifestRecord,
    repository: &[u8],
    label: &[u8],
    memory: &CleanupMemory,
    cancel: &dyn Fn() -> bool,
    ordinal: &mut u64,
    emit: &mut dyn FnMut(WitnessEvent<WitnessKey>) -> Result<(), ValidationIssue>,
) -> Result<[u8; 32], LegacyIssue> {
    let source = FileSource {
        file,
        len: stamp.len(),
    };
    let mut scanner = Scanner::new(
        file,
        &source,
        Span {
            start: 0,
            end: source.len(),
        },
        memory,
        cancel,
    )?;
    let mut partition = false;
    if scanner.token()?.kind != TokenKind::ObjectStart {
        return Err(LegacyIssue::Unknown);
    }
    loop {
        let k = scanner.token()?;
        if k.kind == TokenKind::ObjectEnd {
            break;
        }
        let f = scanner.field(k, &["partition"])?;
        let v = scanner.token()?;
        if f.is_some() {
            if partition || v.kind != TokenKind::ObjectStart {
                return Err(LegacyIssue::Unknown);
            }
            partition = true;
            let mut entry = None;
            let mut seen = 0u8;
            loop {
                let k = scanner.token()?;
                if k.kind == TokenKind::ObjectEnd {
                    break;
                }
                let f = scanner.field(k, &["entry", "nodes", "edges"])?;
                let v = scanner.token()?;
                if let Some(i) = f {
                    if seen & (1 << i) != 0 {
                        return Err(LegacyIssue::Unknown);
                    }
                    seen |= 1 << i;
                    match i {
                        0 => {
                            let record = scanner.cache_entry(v)?;
                            if record.key != key
                                || record.nodes != expected.nodes
                                || record.edges != expected.edges
                            {
                                return Err(LegacyIssue::Unknown);
                            }
                            for (a, b) in [
                                (record.path, expected.path),
                                (record.hash, expected.hash),
                                (record.language, expected.language),
                                (record.partition, expected.partition),
                            ] {
                                if validation::compare_decoded_spans(
                                    &source, a, manifest, b, memory,
                                )? != Ordering::Equal
                                {
                                    return Err(LegacyIssue::Unknown);
                                }
                            }
                            entry = Some(record);
                        }
                        1 | 2 => scanner.selected_array(
                            v,
                            i == 2,
                            entry.ok_or(LegacyIssue::Unknown)?,
                            stamp,
                            repository,
                            label,
                            ordinal,
                            emit,
                        )?,
                        _ => unreachable!(),
                    }
                } else {
                    return Err(LegacyIssue::Unknown);
                }
            }
            if seen != 7 {
                return Err(LegacyIssue::Unknown);
            }
        } else {
            scanner.skip(v)?;
        }
    }
    if !partition {
        return Err(LegacyIssue::Unknown);
    }
    let digest = scanner.reader.finish()?.0;
    stamp.recheck(file).map_err(fixed)?;
    Ok(digest)
}
impl ReplayWitnesses for Witnesses<'_> {
    type Key = WitnessKey;
    fn replay(
        &self,
        memory: &CleanupMemory,
        emit: &mut dyn FnMut(WitnessEvent<Self::Key>) -> Result<(), ValidationIssue>,
    ) -> Result<(), ValidationIssue> {
        let _floor = memory.reserve(std::mem::size_of::<Self>() + 16384)?;
        self.db_stamp
            .recheck(self.database)
            .map_err(|_| ValidationIssue::Schema)?;
        self.manifest_stamp
            .recheck(self.manifest)
            .map_err(|_| ValidationIssue::Schema)?;
        let source = FileSource {
            file: self.database,
            len: self.db_stamp.len(),
        };
        let mut failure = None;
        native::scan_native_selected43(
            &source,
            memory,
            &mut || {
                if (self.cancel)() {
                    Err(NativeUnknown::Cancelled)
                } else {
                    Ok(())
                }
            },
            &mut |row| {
                let domain = match row.table {
                    NativeSelectedTable::Repository => 1,
                    NativeSelectedTable::SourceRoot => 2,
                    NativeSelectedTable::File => 3,
                    NativeSelectedTable::Contains => 4,
                };
                let mut fields = [None; 5];
                for (i, f) in row.fields.iter().enumerate() {
                    let Some(locator) = f else {
                        return Err(NativeUnknown::UnsupportedLayout);
                    };
                    fields[i] = Some(WitnessKey::Native {
                        locator: *locator,
                        span: if i == 4 {
                            row.canonical_key.ok_or(NativeUnknown::UnsupportedLayout)?
                        } else {
                            Span {
                                start: 0,
                                end: locator.length,
                            }
                        },
                        json: i == 4,
                    });
                }
                if let Err(e) = emit(WitnessEvent {
                    domain,
                    native: true,
                    ordinal: row.ordinal,
                    fields,
                }) {
                    failure = Some(e);
                    return Err(NativeUnknown::UnsupportedLayout);
                }
                Ok(())
            },
        )
        .map_err(|_| failure.unwrap_or(ValidationIssue::Schema))?;
        let mut chain = [0u8; 32];
        let mut count = 0u64;
        let mut ordinal_offset = 0u64;
        self.certificates
            .replay(memory, &mut |certificate| {
                if (self.cancel)() {
                    return Err(LegacyIssue::Cancelled);
                }
                let file = open_cache(self.root, certificate.key, self.backup)?;
                certificate.stamp.recheck(&file).map_err(fixed)?;
                drop(file);
                for fact in certificate.facts {
                    emit(WitnessEvent {
                        domain: fact.domain,
                        native: false,
                        ordinal: ordinal_offset
                            .checked_add(fact.ordinal)
                            .ok_or(ValidationIssue::Overflow)?,
                        fields: fact.fields.map(|span| {
                            Some(WitnessKey::Cache {
                                key: certificate.key,
                                stamp: certificate.stamp,
                                span,
                            })
                        }),
                    })?;
                }
                ordinal_offset = ordinal_offset
                    .checked_add(certificate.rows)
                    .ok_or(LegacyIssue::Unknown)?;
                chain_certificate(&mut chain, &certificate)?;
                count = count.checked_add(1).ok_or(LegacyIssue::Unknown)?;
                let file = open_cache(self.root, certificate.key, self.backup)?;
                certificate.stamp.recheck(&file).map_err(fixed)?;
                Ok(())
            })
            .map_err(validation_error)?;
        if chain != self.certificate || count != self.cache_count {
            return Err(ValidationIssue::Schema);
        }
        self.db_stamp
            .recheck(self.database)
            .map_err(|_| ValidationIssue::Schema)?;
        self.manifest_stamp
            .recheck(self.manifest)
            .map_err(|_| ValidationIssue::Schema)?;
        Ok(())
    }
    fn hash(
        &self,
        key: Self::Key,
        domain: u64,
        memory: &CleanupMemory,
    ) -> Result<[u8; 32], ValidationIssue> {
        self.with_key(key, memory, |s, p, e| {
            validation::key_stream_hash(s, p, e, domain, memory)
        })
    }
    fn compare(
        &self,
        left: Self::Key,
        right: Self::Key,
        memory: &CleanupMemory,
    ) -> Result<Ordering, ValidationIssue> {
        self.compare_keys(left, right, memory)
    }
    fn empty(&self, key: Self::Key, memory: &CleanupMemory) -> Result<bool, ValidationIssue> {
        self.with_key(key, memory, |source, span, encoding| {
            validation::key_stream_hash(source, span, encoding, 0, memory)?;
            Ok(span.end - span.start
                == match encoding {
                    KeyEncoding::NativeUtf8 => 0,
                    KeyEncoding::JsonDecoded => 2,
                })
        })
    }
}

/// Exact prepared facts for the selected historical profile. Cache certificate
/// covers only the manifest's required payloads; it does NOT adopt unreferenced
/// or unknown children of the cache root. The caller retains those separately.
pub(crate) struct LegacyAssociation {
    database: FixedFileStamp,
    manifest: FixedFileStamp,
    artifact_root: FileIdentity,
    backup: Option<BackupCache>,
    backup_loader_usable: Option<bool>,
    expected: ReferenceEpoch,
    cache_certificate: [u8; 32],
    #[cfg(test)]
    cache_count: u64,
    #[cfg(test)]
    cache_loader_usable: bool,
    _floor: MemoryReservation,
}
impl LegacyAssociation {
    pub(crate) fn source_epoch(&self) -> &ReferenceEpoch {
        &self.expected
    }
    pub(crate) fn artifact_root_identity(&self) -> FileIdentity {
        self.artifact_root
    }
    pub(crate) fn database_stamp(&self) -> FixedFileStamp {
        self.database
    }
    pub(crate) fn manifest_stamp(&self) -> FixedFileStamp {
        self.manifest
    }
    #[cfg(test)]
    pub(crate) fn cache_count(&self) -> u64 {
        self.cache_count
    }
    #[cfg(test)]
    pub(crate) fn cache_loader_usable(&self) -> bool {
        self.cache_loader_usable
    }
    pub(crate) fn cache_certificate(&self) -> [u8; 32] {
        self.cache_certificate
    }
    pub(crate) fn matches(
        &self,
        database: &File,
        manifest: &File,
        artifact_root: &RootedDirectory,
    ) -> bool {
        self.database.recheck(database).is_ok()
            && self.manifest.recheck(manifest).is_ok()
            && artifact_root.identity() == self.artifact_root
    }
}
pub(crate) enum LegacyProgress {
    Pending,
    CacheValidated(LegacyCacheCertificate),
    Complete(LegacyAssociation),
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Stage {
    Catalog,
    Caches,
    Join,
    Done,
}
/// One job stays on its joined preparation thread: RootedDirectory is an
/// actual readonly anchor and intentionally !Send. No authority is retained.
pub(crate) struct LegacyAssociationPreparation {
    database: File,
    manifest: File,
    artifact_root: RootedDirectory,
    backup: Option<BackupCache>,
    artifact_root_path: PathBuf,
    repository: PathBuf,
    repository_identity: FileIdentity,
    expected: ReferenceEpoch,
    database_stamp: FixedFileStamp,
    manifest_stamp: FixedFileStamp,
    files: Span,
    cursor: Option<ManifestCursor>,
    stage: Stage,
    database_sha256: [u8; 32],
    manifest_sha256: [u8; 32],
    certificate: [u8; 32],
    pending: Option<LegacyCacheCertificate>,
    resume_validation: bool,
    closed_source_authorized: bool,
    cache_count: u64,
    cache_loader_usable: bool,
    _floor: MemoryReservation,
}
impl LegacyAssociationPreparation {
    pub(crate) fn new(
        database: File,
        manifest: File,
        artifact_root: RootedDirectory,
        artifact_root_path: &Path,
        repository_root: &Path,
        source_epoch: ReferenceEpoch,
        memory: &CleanupMemory,
    ) -> Result<Self, LegacyIssue> {
        let charge = std::mem::size_of::<Self>()
            .checked_add(repository_root.as_os_str().as_encoded_bytes().len())
            .and_then(|n| n.checked_add(artifact_root_path.as_os_str().as_encoded_bytes().len()))
            .and_then(|n| n.checked_add(32768))
            .ok_or(ResourceLimit::ArithmeticOverflow)?;
        let floor = memory.reserve(charge)?;
        if !repository_root.is_absolute()
            || repository_root.to_str().is_none()
            || repository_root
                .file_name()
                .and_then(|n| n.to_str())
                .is_none()
        {
            return Err(LegacyIssue::Unknown);
        }
        let mut scratch = super::locks::FixedControlScratch::new(
            memory.reserve(super::locks::FixedControlScratch::required_bytes())?,
        )
        .map_err(fixed)?;
        let repository_file = super::locks::open_fixed_control(
            repository_root,
            FixedControlKind::Directory,
            false,
            &mut scratch,
        )
        .map_err(fixed)?;
        let repository_identity =
            FixedFileStamp::capture(&repository_file, FixedControlKind::Directory)
                .map_err(fixed)?
                .identity();
        let root_file = super::locks::open_fixed_control(
            artifact_root_path,
            FixedControlKind::Directory,
            false,
            &mut scratch,
        )
        .map_err(fixed)?;
        if FixedFileStamp::capture(&root_file, FixedControlKind::Directory)
            .map_err(fixed)?
            .identity()
            != artifact_root.identity()
        {
            return Err(LegacyIssue::Changed);
        }
        let database_stamp =
            FixedFileStamp::capture(&database, FixedControlKind::File).map_err(fixed)?;
        let manifest_stamp =
            FixedFileStamp::capture(&manifest, FixedControlKind::File).map_err(fixed)?;
        Ok(Self {
            database,
            manifest,
            artifact_root,
            backup: None,
            artifact_root_path: artifact_root_path.to_owned(),
            repository: repository_root.to_owned(),
            repository_identity,
            expected: source_epoch,
            database_stamp,
            manifest_stamp,
            files: Span::default(),
            cursor: None,
            stage: Stage::Catalog,
            database_sha256: [0; 32],
            manifest_sha256: [0; 32],
            certificate: [0; 32],
            pending: None,
            resume_validation: false,
            closed_source_authorized: false,
            cache_count: 0,
            cache_loader_usable: true,
            _floor: floor,
        })
    }
    fn current(
        &self,
        domain: &ReferenceDomain,
        memory: &CleanupMemory,
        cancel: &dyn Fn() -> bool,
    ) -> Result<(), LegacyIssue> {
        if cancel() {
            return Err(LegacyIssue::Cancelled);
        }
        self.database_stamp.recheck(&self.database).map_err(fixed)?;
        self.manifest_stamp.recheck(&self.manifest).map_err(fixed)?;
        let mut scratch = super::locks::FixedControlScratch::new(
            memory.reserve(super::locks::FixedControlScratch::required_bytes())?,
        )
        .map_err(fixed)?;
        for (path, expected) in [
            (&self.artifact_root_path, self.artifact_root.identity()),
            (&self.repository, self.repository_identity),
        ] {
            let file = super::locks::open_fixed_control(
                path,
                FixedControlKind::Directory,
                false,
                &mut scratch,
            )
            .map_err(fixed)?;
            if FixedFileStamp::capture(&file, FixedControlKind::Directory)
                .map_err(fixed)?
                .identity()
                != expected
            {
                return Err(LegacyIssue::Changed);
            }
        }
        if !self.closed_source_authorized
            && !super::maintenance_ownership::expected_reference_current(
                domain,
                &self.expected,
                memory,
            )
            .map_err(fixed)?
        {
            return Err(LegacyIssue::Changed);
        }
        Ok(())
    }
    #[cfg(test)]
    pub(crate) fn advance(
        &mut self,
        domain: &ReferenceDomain,
        memory: &CleanupMemory,
        cancel: &dyn Fn() -> bool,
    ) -> Result<LegacyProgress, LegacyIssue> {
        let result = self.advance_inner(domain, memory, cancel, &NoCertificates);
        if result.is_err() {
            self.stage = Stage::Done;
        }
        result
    }
    pub(crate) fn advance_with_certificates(
        &mut self,
        domain: &ReferenceDomain,
        memory: &CleanupMemory,
        cancel: &dyn Fn() -> bool,
        certificates: &dyn ReplayLegacyCertificates,
    ) -> Result<LegacyProgress, LegacyIssue> {
        let result = self.advance_inner(domain, memory, cancel, certificates);
        if result.is_err() {
            self.stage = Stage::Done;
        }
        result
    }
    pub(crate) fn acknowledge_cache(
        &mut self,
        certificate_digest: [u8; 32],
    ) -> Result<(), LegacyIssue> {
        let certificate = self.pending.ok_or(LegacyIssue::Unknown)?;
        if certificate.digest()? != certificate_digest {
            return Err(LegacyIssue::Changed);
        }
        chain_certificate(&mut self.certificate, &certificate)?;
        self.cache_count = self
            .cache_count
            .checked_add(1)
            .ok_or(LegacyIssue::Unknown)?;
        self.cache_loader_usable &= certificate.loader_usable;
        self.pending = None;
        Ok(())
    }
    fn advance_inner(
        &mut self,
        domain: &ReferenceDomain,
        memory: &CleanupMemory,
        cancel: &dyn Fn() -> bool,
        certificates: &dyn ReplayLegacyCertificates,
    ) -> Result<LegacyProgress, LegacyIssue> {
        let _floor = memory.reserve(32768)?;
        self.current(domain, memory, cancel)?;
        if self.resume_validation {
            let source = FileSource {
                file: &self.database,
                len: self.database_stamp.len(),
            };
            recognize_catalog(&source, memory, cancel)?;
            if file_digest(&self.database, self.database_stamp, cancel)? != self.database_sha256
                || file_digest(&self.manifest, self.manifest_stamp, cancel)? != self.manifest_sha256
                || project_manifest(&self.manifest, memory, cancel)? != self.files
            {
                return Err(LegacyIssue::Changed);
            }
            self.current(domain, memory, cancel)?;
            self.resume_validation = false;
            return Ok(LegacyProgress::Pending);
        }
        if let Some(certificate) = self.pending {
            return Ok(LegacyProgress::CacheValidated(certificate));
        }
        match self.stage {
            Stage::Catalog => {
                self.files = project_manifest(&self.manifest, memory, cancel)?;
                self.manifest_sha256 = file_digest(&self.manifest, self.manifest_stamp, cancel)?;
                self.cursor = Some(ManifestCursor::new(&self.manifest, self.files, memory)?);
                let source = FileSource {
                    file: &self.database,
                    len: self.database_stamp.len(),
                };
                recognize_catalog(&source, memory, cancel)?;
                self.database_sha256 = file_digest(&self.database, self.database_stamp, cancel)?;
                self.current(domain, memory, cancel)?;
                self.stage = Stage::Caches;
                Ok(LegacyProgress::Pending)
            }
            Stage::Caches => {
                let Some(record) = self.cursor.as_mut().ok_or(LegacyIssue::Unknown)?.next(
                    &self.manifest,
                    memory,
                    cancel,
                )?
                else {
                    if self.cache_count == 0 {
                        return Err(LegacyIssue::Unknown);
                    }
                    self.stage = Stage::Join;
                    return Ok(LegacyProgress::Pending);
                };
                let file = open_cache(&self.artifact_root, record.key, self.backup)?;
                let stamp =
                    FixedFileStamp::capture(&file, FixedControlKind::File).map_err(fixed)?;
                let source = FileSource {
                    file: &file,
                    len: stamp.len(),
                };
                let facts = validation::scan_artifact(&source, memory, &mut |_| {
                    if cancel() {
                        Err(ValidationIssue::Schema)
                    } else {
                        Ok(())
                    }
                })?;
                if facts.key != record.key
                    || !facts.producer_tokens
                    || !validation::wire_checksum_matches(&source, &facts, memory)?
                {
                    return Err(LegacyIssue::Unknown);
                }
                validation::join_raw_rows(
                    &validation::ArtifactEvents { source: &source },
                    memory,
                    256,
                )?;
                let loader_usable =
                    facts.typed_numbers && facts.stable_numbers && facts.metadata_keys_sorted;
                let manifest = FileSource {
                    file: &self.manifest,
                    len: self.manifest_stamp.len(),
                };
                let mut selected = [LegacyProjectedFact {
                    domain: 0,
                    ordinal: 0,
                    fields: [Span::default(); 5],
                }; 5];
                let mut count = 0usize;
                let mut ordinal = 0;
                let digest = project_cache(
                    &file,
                    stamp,
                    record.key,
                    &manifest,
                    record,
                    self.repository.as_os_str().as_encoded_bytes(),
                    self.repository.file_name().unwrap().as_encoded_bytes(),
                    memory,
                    cancel,
                    &mut ordinal,
                    &mut |event| {
                        if count >= selected.len() {
                            return Err(ValidationIssue::Schema);
                        }
                        let mut fields = [Span::default(); 5];
                        for (i, key) in event.fields.iter().enumerate() {
                            match key {
                                Some(WitnessKey::Cache {
                                    key,
                                    stamp: s,
                                    span,
                                }) if *key == record.key && *s == stamp => fields[i] = *span,
                                _ => return Err(ValidationIssue::Schema),
                            }
                        }
                        selected[count] = LegacyProjectedFact {
                            domain: event.domain,
                            ordinal: event.ordinal,
                            fields,
                        };
                        count += 1;
                        Ok(())
                    },
                )?;
                if count != 5 {
                    return Err(LegacyIssue::Unknown);
                }
                let mut domains = [0u8; 4];
                for fact in selected {
                    let i =
                        usize::try_from(fact.domain.checked_sub(1).ok_or(LegacyIssue::Unknown)?)
                            .map_err(|_| LegacyIssue::Unknown)?;
                    if i >= domains.len() {
                        return Err(LegacyIssue::Unknown);
                    }
                    domains[i] += 1;
                }
                if domains != [1, 1, 1, 2] {
                    return Err(LegacyIssue::Unknown);
                }
                // Recheck the exact current name without retaining a fourth
                // payload descriptor beside DB, manifest and this old handle.
                drop(file);
                let current = open_cache(&self.artifact_root, record.key, self.backup)?;
                stamp.recheck(&current).map_err(fixed)?;
                self.current(domain, memory, cancel)?;
                let certificate = LegacyCacheCertificate {
                    key: record.key,
                    stamp,
                    digest,
                    facts: selected,
                    rows: facts
                        .actual_nodes
                        .checked_add(facts.actual_edges)
                        .ok_or(LegacyIssue::Unknown)?,
                    loader_usable,
                };
                self.pending = Some(certificate);
                Ok(LegacyProgress::CacheValidated(certificate))
            }
            Stage::Join => {
                let mut backup_loader_usable = None;
                if let Some(backup) = self.backup {
                    certificates.replay(memory, &mut |certificate| {
                        if certificate.key == backup.key {
                            if certificate.stamp != backup.stamp || backup_loader_usable.is_some() {
                                return Err(LegacyIssue::Unknown);
                            }
                            backup_loader_usable = Some(certificate.loader_usable);
                        }
                        Ok(())
                    })?;
                    if backup_loader_usable.is_none() {
                        return Err(LegacyIssue::Unknown);
                    }
                }
                let cert = self.certificate;
                let replay = Witnesses {
                    database: &self.database,
                    db_stamp: self.database_stamp,
                    manifest: &self.manifest,
                    manifest_stamp: self.manifest_stamp,
                    root: &self.artifact_root,
                    backup: self.backup,
                    certificate: cert,
                    cache_count: self.cache_count,
                    certificates,
                    cancel,
                };
                validation::join_selected_witnesses(&replay, memory, 256)?;
                self.current(domain, memory, cancel)?;
                self.stage = Stage::Done;
                Ok(LegacyProgress::Complete(LegacyAssociation {
                    database: self.database_stamp,
                    manifest: self.manifest_stamp,
                    artifact_root: self.artifact_root.identity(),
                    backup: self.backup,
                    backup_loader_usable,
                    expected: self.expected.clone(),

                    cache_certificate: cert,
                    #[cfg(test)]
                    cache_count: self.cache_count,
                    #[cfg(test)]
                    cache_loader_usable: self.cache_loader_usable,

                    _floor: memory.reserve(std::mem::size_of::<LegacyAssociation>())?,
                }))
            }
            Stage::Done => Err(LegacyIssue::Unknown),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::layout::{create_fresh_reference_root, managed_generation_id, ManagedLayout};
    use super::super::locks::{open_locked, path_identity, LockMode};
    use super::super::maintenance_ownership::{
        initialize_fresh_reference_domain, observe_reference_epoch, EpochObservation,
        ReferenceMutation,
    };
    use super::*;
    use crate::protocol::{NativeManifest, NativeSyntaxMaterializationRequest};
    #[derive(Default)]
    struct Certificates(std::cell::RefCell<Vec<[u8; LEGACY_CERTIFICATE_BYTES]>>);
    impl ReplayLegacyCertificates for Certificates {
        fn replay(
            &self,
            _: &CleanupMemory,
            emit: &mut dyn FnMut(LegacyCacheCertificate) -> Result<(), LegacyIssue>,
        ) -> Result<(), LegacyIssue> {
            for bytes in self.0.borrow().iter() {
                emit(LegacyCacheCertificate::decode(bytes)?)?;
            }
            Ok(())
        }
    }
    fn persist_certificate(
        job: &mut LegacyAssociationPreparation,
        certificates: &Certificates,
        certificate: LegacyCacheCertificate,
    ) -> Result<(), LegacyIssue> {
        let mut bytes = [0; LEGACY_CERTIFICATE_BYTES];
        certificate.encode(&mut bytes);
        let certificate = LegacyCacheCertificate::decode(&bytes)?;
        let checkpoint = job.checkpoint_after_cache()?;
        let mut checkpoint_bytes = [0; LEGACY_CHECKPOINT_BYTES];
        checkpoint.encode(&mut checkpoint_bytes);
        let _ = LegacyCheckpoint::decode(&checkpoint_bytes)?;
        certificates.0.borrow_mut().push(bytes);
        job.acknowledge_cache(certificate.digest()?)
    }
    struct Fixture {
        root: PathBuf,
        layout: ManagedLayout,
        domain: ReferenceDomain,
        database: PathBuf,
        manifest: PathBuf,
    }
    impl Fixture {
        fn new() -> Self {
            let root = std::env::temp_dir()
                .join(format!("legacy-association-{}", managed_generation_id()));
            std::fs::create_dir(&root).unwrap();
            let root = std::fs::canonicalize(root).unwrap();
            std::fs::create_dir(root.join("src")).unwrap();
            std::fs::write(root.join("src/a.rs"), b"pub fn alpha(v:i64)->i64{v+1}\n").unwrap();
            std::fs::write(root.join("src/b.rs"), b"pub struct Beta{pub value:i64}\n").unwrap();
            let layout = ManagedLayout::new(root.join("storage"));
            let fresh = create_fresh_reference_root(layout.storage_root())
                .unwrap()
                .unwrap();
            layout.ensure_roots().unwrap();
            let writer = open_locked(layout.writer_lock_path(), LockMode::Exclusive).unwrap();
            let domain = ReferenceDomain::managed(&layout).unwrap();
            assert!(initialize_fresh_reference_domain(&domain, &fresh, &writer).unwrap());
            drop(writer);
            let database = root.join("graph.ldb");
            let manifest = root.join("manifest.json");
            let mutation = ReferenceMutation::begin_in_domain(&domain).unwrap();
            let request:NativeSyntaxMaterializationRequest=serde_json::from_value(serde_json::json!({"source_root":root,"repository_label":root.file_name().unwrap().to_str().unwrap(),"mode":"full","parser_version":"legacy-association-test","manifest_schema_version":5,"ontology":"code_ontology_v1","profiles":[],"excluded_parts":[],"artifact_root":layout.artifacts_root(),"db_path":database,"include_fts":false,"staging_dir":root.join("native-staging"),"parallel":false,"worker_memory_mib":768,"rust_memory_mib":384,"spill_chunk_mib":1,"max_parallelism":1,"atomic_rebuild":false})).unwrap();
            let response = crate::execute_materialization_pipeline(&request).unwrap();
            let files = response
                .materialized_entries
                .into_iter()
                .map(|(path, entry)| (path, entry.compact()))
                .collect();
            let value = NativeManifest {
                schema_version: 5,
                ontology: request.ontology,
                parser_version: request.parser_version,
                graph_build_digest: response.graph_build_digest,
                search_backend: response.search_backend,
                files,
            };
            std::fs::write(&manifest, serde_json::to_vec(&value).unwrap()).unwrap();
            drop(mutation);
            Self {
                root,
                layout,
                domain,
                database,
                manifest,
            }
        }
        fn epoch(&self) -> ReferenceEpoch {
            match observe_reference_epoch(&self.domain).unwrap() {
                EpochObservation::Verified(e) => e,
                e => panic!("{e:?}"),
            }
        }
        fn job(&self, memory: &CleanupMemory) -> Result<LegacyAssociationPreparation, LegacyIssue> {
            let root = RootedDirectory::open_readonly_root(
                &self.layout.artifacts_root(),
                path_identity(&self.layout.artifacts_root()).unwrap(),
            )
            .unwrap();
            LegacyAssociationPreparation::new(
                File::open(&self.database).unwrap(),
                File::open(&self.manifest).unwrap(),
                root,
                &self.layout.artifacts_root(),
                &self.root,
                self.epoch(),
                memory,
            )
        }
        fn complete(&self, memory: &CleanupMemory) -> Result<LegacyAssociation, LegacyIssue> {
            let mut job = self.job(memory)?;
            let certificates = Certificates::default();
            for _ in 0..16 {
                match job.advance_with_certificates(
                    &self.domain,
                    memory,
                    &|| false,
                    &certificates,
                )? {
                    LegacyProgress::Complete(f) => return Ok(f),
                    LegacyProgress::Pending => {}
                    LegacyProgress::CacheValidated(certificate) => {
                        persist_certificate(&mut job, &certificates, certificate)?
                    }
                }
            }
            panic!("bounded fixture did not complete")
        }
        fn first_cache(&self) -> PathBuf {
            let manifest: NativeManifest =
                serde_json::from_slice(&std::fs::read(&self.manifest).unwrap()).unwrap();
            let key = manifest
                .files
                .values()
                .next()
                .unwrap()
                .artifact_key
                .as_ref()
                .unwrap();
            self.layout
                .artifacts_root()
                .join(&key[..2])
                .join(key)
                .join("partition.json")
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    #[test]
    fn dc_legacy_actual_native_association_restart_and_backup_keep_three_payload_fds() {
        use std::os::unix::fs::MetadataExt;
        let f = Fixture::new();
        let manifest: NativeManifest =
            serde_json::from_slice(&std::fs::read(&f.manifest).unwrap()).unwrap();
        let selected_key = manifest
            .files
            .values()
            .next()
            .unwrap()
            .artifact_key
            .as_ref()
            .unwrap();
        let canonical = f
            .layout
            .artifacts_root()
            .join(&selected_key[..2])
            .join(selected_key);
        let backup_name = format!(".replaced-{selected_key}-1");
        let backup = canonical.parent().unwrap().join(&backup_name);
        let mutation = ReferenceMutation::begin_in_domain(&f.domain).unwrap();
        std::fs::rename(&canonical, &backup).unwrap();
        drop(mutation);
        let address =
            super::super::maintenance_backup::BackupAddress::parse(backup_name.as_bytes()).unwrap();
        let mut paths = vec![f.database.clone(), f.manifest.clone()];
        for entry in manifest.files.values() {
            let key = entry.artifact_key.as_ref().unwrap();
            paths.push(if key == selected_key {
                backup.join("partition.json")
            } else {
                f.layout
                    .artifacts_root()
                    .join(&key[..2])
                    .join(key)
                    .join("partition.json")
            });
        }
        // fdescfs reports its own device number on macOS while preserving the
        // actual inode/size/times. All uniquely created fixture payloads are on
        // one filesystem; matching these facts counts the real open handles,
        // including duplicates, without opening/duplicating a payload descriptor.
        let payloads: Vec<_> = paths
            .iter()
            .map(|path| {
                let m = std::fs::metadata(path).unwrap();
                (m.ino(), m.len(), m.mtime(), m.mtime_nsec())
            })
            .collect();
        let count = || {
            let table = if cfg!(target_os = "linux") {
                "/proc/self/fd"
            } else {
                "/dev/fd"
            };
            std::fs::read_dir(table)
                .unwrap()
                .filter_map(Result::ok)
                .filter_map(|entry| std::fs::metadata(entry.path()).ok())
                .filter(|m| {
                    m.is_file() && payloads.contains(&(m.ino(), m.len(), m.mtime(), m.mtime_nsec()))
                })
                .count()
        };
        let peak = std::cell::Cell::new(0);
        let cancel = || {
            let live = count();
            peak.set(peak.get().max(live));
            assert!(
                live <= 3,
                "actual aggregate payload FD count {live} exceeds 3"
            );
            false
        };
        let memory = CleanupMemory::new(8 * 1024 * 1024).unwrap();
        let initial_floor = memory.used();
        let mut job = f.job(&memory).unwrap();
        job.select_backup_cache(address.key(), address.name())
            .unwrap();
        assert_eq!(
            count(),
            2,
            "OS probe must observe the actual retained source pair"
        );
        let certificates = Certificates::default();
        let mut association = None;
        let mut restarted = 0;
        for _ in 0..32 {
            match job
                .advance_with_certificates(&f.domain, &memory, &cancel, &certificates)
                .unwrap()
            {
                LegacyProgress::Pending => {}
                LegacyProgress::CacheValidated(certificate) => {
                    persist_certificate(&mut job, &certificates, certificate).unwrap();
                    let checkpoint = job.checkpoint().unwrap();
                    let (database, manifest) = job.into_source_handles();
                    job = LegacyAssociationPreparation::restore(
                        database,
                        manifest,
                        RootedDirectory::open_readonly_root(
                            &f.layout.artifacts_root(),
                            path_identity(&f.layout.artifacts_root()).unwrap(),
                        )
                        .unwrap(),
                        &f.layout.artifacts_root(),
                        &f.root,
                        checkpoint,
                        &memory,
                    )
                    .unwrap();
                    restarted += 1;
                }
                LegacyProgress::Complete(value) => {
                    association = Some(value);
                    break;
                }
            }
            cancel();
        }
        assert_eq!(restarted, 2);
        let association = association.expect("actual native/manifest/backup join must complete");
        let (database, manifest) = job.into_source_handles();
        let prepared = super::super::maintenance_backup::prepare_backup(
            &f.domain,
            &f.epoch(),
            address,
            association,
            database,
            manifest,
            &memory,
            &cancel,
        )
        .unwrap();
        assert_eq!(count(), 3);
        assert_eq!(peak.get(), 3);
        drop(prepared);
        assert_eq!(count(), 0);
        assert_eq!(memory.used(), initial_floor);
        assert!(memory.peak() <= memory.limit());
    }

    #[test]
    fn dc_legacy_real_public_producer_without_receipts_has_complete_native_association() {
        let f = Fixture::new();
        let memory = CleanupMemory::new(8 * 1024 * 1024).unwrap();
        let facts = f.complete(&memory).unwrap();
        assert_eq!(facts.cache_count(), 2);
        assert!(facts.cache_loader_usable());
        let root = RootedDirectory::open_readonly_root(
            &f.layout.artifacts_root(),
            path_identity(&f.layout.artifacts_root()).unwrap(),
        )
        .unwrap();
        assert!(facts.matches(
            &File::open(&f.database).unwrap(),
            &File::open(&f.manifest).unwrap(),
            &root
        ));
        assert!(memory.peak() <= 8 * 1024 * 1024);
    }
    #[test]
    fn dc_legacy_manifest_and_cache_substitution_never_mint_association() {
        let f = Fixture::new();
        let memory = CleanupMemory::new(8 * 1024 * 1024).unwrap();
        let original = std::fs::read(&f.manifest).unwrap();
        let mut value: serde_json::Value = serde_json::from_slice(&original).unwrap();
        value["files"]["src/a.rs"]["content_hash"] = "foreign".into();
        let mutation = ReferenceMutation::begin_in_domain(&f.domain).unwrap();
        std::fs::write(&f.manifest, serde_json::to_vec(&value).unwrap()).unwrap();
        drop(mutation);
        assert!(matches!(f.complete(&memory), Err(LegacyIssue::Unknown)));
        let mutation = ReferenceMutation::begin_in_domain(&f.domain).unwrap();
        std::fs::write(&f.manifest, original).unwrap();
        let cache = f.first_cache();
        std::fs::write(cache, b"{}").unwrap();
        drop(mutation);
        assert!(f.complete(&memory).is_err());
    }
    #[test]
    fn dc_legacy_database_catalog_and_source_root_substitution_are_unknown() {
        let f = Fixture::new();
        let memory = CleanupMemory::new(8 * 1024 * 1024).unwrap();
        let file = File::open(&f.database).unwrap();
        let source = FileSource {
            file: &file,
            len: file.metadata().unwrap().len(),
        };
        let mut property = None;
        native::trace_native_catalog43(&source, &memory, &mut |event| {
            if let NativeCatalogEvent::Property { name, .. } = event {
                if raw_equal(&source, name, b"content_hash").unwrap() {
                    property = Some(name.start);
                }
            }
            Ok(())
        })
        .unwrap();
        let mut bytes = std::fs::read(&f.database).unwrap();
        bytes[property.unwrap() as usize] = b'X';
        let mutation = ReferenceMutation::begin_in_domain(&f.domain).unwrap();
        std::fs::write(&f.database, bytes).unwrap();
        drop(mutation);
        assert!(f.complete(&memory).is_err());
        let foreign = Fixture::new();
        let bytes = std::fs::read(&foreign.database).unwrap();
        let mutation = ReferenceMutation::begin_in_domain(&f.domain).unwrap();
        std::fs::write(&f.database, bytes).unwrap();
        drop(mutation);
        assert!(f.complete(&memory).is_err());
    }
    #[test]
    fn dc_legacy_original_epoch_cancellation_and_cap_fail_closed() {
        let f = Fixture::new();
        let memory = CleanupMemory::new(8 * 1024 * 1024).unwrap();
        let mut job = f.job(&memory).unwrap();
        let mutation = ReferenceMutation::begin_in_domain(&f.domain).unwrap();
        drop(mutation);
        assert!(matches!(
            job.advance(&f.domain, &memory, &|| false),
            Err(LegacyIssue::Changed)
        ));
        let mut job = f.job(&memory).unwrap();
        assert!(matches!(
            job.advance(&f.domain, &memory, &|| true),
            Err(LegacyIssue::Cancelled)
        ));
        let tiny = CleanupMemory::new(1024).unwrap();
        assert!(matches!(f.job(&tiny), Err(LegacyIssue::Resource(_))));
    }
    #[test]
    fn dc_legacy_cache_change_after_certification_invalidates_final_join() {
        let f = Fixture::new();
        let memory = CleanupMemory::new(8 * 1024 * 1024).unwrap();
        let mut job = f.job(&memory).unwrap();
        let certificates = Certificates::default();
        assert!(matches!(
            job.advance_with_certificates(&f.domain, &memory, &|| false, &certificates)
                .unwrap(),
            LegacyProgress::Pending
        ));
        let certificate = match job
            .advance_with_certificates(&f.domain, &memory, &|| false, &certificates)
            .unwrap()
        {
            LegacyProgress::CacheValidated(c) => c,
            _ => panic!("expected one validated cache"),
        };
        persist_certificate(&mut job, &certificates, certificate).unwrap();
        let mut bytes = std::fs::read(f.first_cache()).unwrap();
        bytes.push(b' ');
        std::fs::write(f.first_cache(), bytes).unwrap();
        let mut rejected = false;
        for _ in 0..8 {
            match job.advance_with_certificates(&f.domain, &memory, &|| false, &certificates) {
                Ok(LegacyProgress::Pending) => {}
                Ok(LegacyProgress::CacheValidated(c)) => {
                    persist_certificate(&mut job, &certificates, c).unwrap()
                }
                Ok(LegacyProgress::Complete(_)) => panic!("changed certified cache accepted"),
                Err(_) => {
                    rejected = true;
                    break;
                }
            }
        }
        assert!(rejected);
    }
    #[test]
    fn dc_legacy_durable_checkpoint_resumes_after_one_payload_without_relabeling_epoch() {
        let f = Fixture::new();
        let memory = CleanupMemory::new(8 * 1024 * 1024).unwrap();
        let certificates = Certificates::default();
        let mut job = f.job(&memory).unwrap();
        assert!(matches!(
            job.advance_with_certificates(&f.domain, &memory, &|| false, &certificates)
                .unwrap(),
            LegacyProgress::Pending
        ));
        let certificate = match job
            .advance_with_certificates(&f.domain, &memory, &|| false, &certificates)
            .unwrap()
        {
            LegacyProgress::CacheValidated(c) => c,
            _ => panic!("expected one cache"),
        };
        let checkpoint = job.checkpoint_after_cache().unwrap();
        let context = checkpoint.context_digest();
        let original_epoch = checkpoint.source_epoch().clone();
        let mut checkpoint_bytes = [0; LEGACY_CHECKPOINT_BYTES];
        checkpoint.encode(&mut checkpoint_bytes);
        persist_certificate(&mut job, &certificates, certificate).unwrap();
        std::fs::write(f.root.join("checkpoint.bin"), checkpoint_bytes).unwrap();
        drop(job);
        let bytes: [u8; LEGACY_CHECKPOINT_BYTES] = std::fs::read(f.root.join("checkpoint.bin"))
            .unwrap()
            .try_into()
            .unwrap();
        let checkpoint = LegacyCheckpoint::decode(&bytes).unwrap();
        assert_eq!(checkpoint.certified_cache_count(), 1);
        assert_eq!(checkpoint.source_epoch(), &original_epoch);
        assert_eq!(checkpoint.context_digest(), context);
        let root = RootedDirectory::open_readonly_root(
            &f.layout.artifacts_root(),
            path_identity(&f.layout.artifacts_root()).unwrap(),
        )
        .unwrap();
        let mut job = LegacyAssociationPreparation::restore(
            File::open(&f.database).unwrap(),
            File::open(&f.manifest).unwrap(),
            root,
            &f.layout.artifacts_root(),
            &f.root,
            checkpoint,
            &memory,
        )
        .unwrap();
        let mut complete = None;
        for _ in 0..10 {
            match job
                .advance_with_certificates(&f.domain, &memory, &|| false, &certificates)
                .unwrap()
            {
                LegacyProgress::Pending => {}
                LegacyProgress::CacheValidated(c) => {
                    persist_certificate(&mut job, &certificates, c).unwrap()
                }
                LegacyProgress::Complete(facts) => {
                    complete = Some(facts);
                    break;
                }
            }
        }
        let facts = complete.unwrap();
        assert_eq!(facts.cache_count(), 2);
        assert_eq!(facts.source_epoch(), &original_epoch);
        assert_eq!(certificates.0.borrow().len(), 2);
        for cut in 0..bytes.len() {
            let mut torn = bytes;
            torn[cut..].fill(0);
            if torn != bytes {
                assert!(
                    LegacyCheckpoint::decode(&torn).is_err(),
                    "checkpoint cut {cut}"
                )
            }
        }
    }
}

pub(crate) const LEGACY_CERTIFICATE_BYTES: usize = 768;
pub(crate) const LEGACY_CHECKPOINT_BYTES: usize = 2048;
#[derive(Clone, Copy)]
struct LegacyProjectedFact {
    domain: u64,
    ordinal: u64,
    fields: [Span; 5],
}
/// An opaque fixed batch produced only after one complete actual cache check.
/// Its bytes must be published by the caller's genuine scoped private journal.
#[derive(Clone, Copy)]
pub(crate) struct LegacyCacheCertificate {
    key: [u8; 32],
    stamp: FixedFileStamp,
    digest: [u8; 32],
    facts: [LegacyProjectedFact; 5],
    rows: u64,
    loader_usable: bool,
}
fn put_u64(bytes: &mut [u8], value: u64) {
    bytes[..8].copy_from_slice(&value.to_le_bytes());
}
fn get_u64(bytes: &[u8]) -> u64 {
    u64::from_le_bytes(bytes[..8].try_into().unwrap())
}
fn put_id(bytes: &mut [u8], identity: FileIdentity) {
    let (a, b) = identity.fixed_words();
    put_u64(bytes, a);
    put_u64(&mut bytes[8..], b);
}
fn get_id(bytes: &[u8]) -> FileIdentity {
    FileIdentity::from_fixed_words(get_u64(bytes), get_u64(&bytes[8..]))
}
fn checksum(bytes: &mut [u8]) {
    let end = bytes.len() - 32;
    let hash = Sha256::digest(&bytes[..end]);
    bytes[end..].copy_from_slice(&hash);
}
fn valid_checksum(bytes: &[u8]) -> bool {
    let end = bytes.len() - 32;
    Sha256::digest(&bytes[..end]).as_slice() == &bytes[end..]
}
impl LegacyCacheCertificate {
    pub(crate) fn encode(&self, out: &mut [u8; LEGACY_CERTIFICATE_BYTES]) {
        out.fill(0);
        out[..8].copy_from_slice(b"CGLGCF01");
        out[8] = u8::from(self.loader_usable);
        out[16..48].copy_from_slice(&self.key);
        self.stamp
            .encode_checkpoint((&mut out[48..128]).try_into().unwrap());
        out[128..160].copy_from_slice(&self.digest);
        for (i, fact) in self.facts.iter().enumerate() {
            let row = &mut out[160 + i * 96..256 + i * 96];
            put_u64(row, fact.domain);
            put_u64(&mut row[8..], fact.ordinal);
            for (j, span) in fact.fields.iter().enumerate() {
                put_u64(&mut row[16 + j * 16..], span.start);
                put_u64(&mut row[24 + j * 16..], span.end);
            }
        }
        put_u64(&mut out[640..], self.rows);
        checksum(out);
    }
    pub(crate) fn decode(bytes: &[u8; LEGACY_CERTIFICATE_BYTES]) -> Result<Self, LegacyIssue> {
        if &bytes[..8] != b"CGLGCF01"
            || bytes[8] > 1
            || bytes[9..16].iter().any(|b| *b != 0)
            || bytes[648..736].iter().any(|b| *b != 0)
            || !valid_checksum(bytes)
        {
            return Err(LegacyIssue::Unknown);
        }
        let stamp = FixedFileStamp::decode_checkpoint((&bytes[48..128]).try_into().unwrap())
            .map_err(fixed)?;
        let rows = get_u64(&bytes[640..]);
        let mut facts = [LegacyProjectedFact {
            domain: 0,
            ordinal: 0,
            fields: [Span::default(); 5],
        }; 5];
        let mut domains = [0u8; 4];
        for (i, fact) in facts.iter_mut().enumerate() {
            let row = &bytes[160 + i * 96..256 + i * 96];
            fact.domain = get_u64(row);
            fact.ordinal = get_u64(&row[8..]);
            if !(1..=4).contains(&fact.domain) || fact.ordinal >= rows {
                return Err(LegacyIssue::Unknown);
            }
            domains[(fact.domain - 1) as usize] += 1;
            for (j, span) in fact.fields.iter_mut().enumerate() {
                span.start = get_u64(&row[16 + j * 16..]);
                span.end = get_u64(&row[24 + j * 16..]);
                if span.start >= span.end || span.end > stamp.len() {
                    return Err(LegacyIssue::Unknown);
                }
            }
        }
        if domains != [1, 1, 1, 2] {
            return Err(LegacyIssue::Unknown);
        }
        Ok(Self {
            key: bytes[16..48].try_into().unwrap(),
            stamp,
            digest: bytes[128..160].try_into().unwrap(),
            facts,
            rows,
            loader_usable: bytes[8] == 1,
        })
    }
    pub(crate) fn digest(&self) -> Result<[u8; 32], LegacyIssue> {
        let mut bytes = [0; LEGACY_CERTIFICATE_BYTES];
        self.encode(&mut bytes);
        Ok(Sha256::digest(bytes).into())
    }
}
fn chain_certificate(
    chain: &mut [u8; 32],
    certificate: &LegacyCacheCertificate,
) -> Result<(), LegacyIssue> {
    let mut hash = Sha256::new();
    hash.update(b"CGLegacyCertificateChain01");
    hash.update(*chain);
    hash.update(certificate.digest()?);
    *chain = hash.finalize().into();
    Ok(())
}
/// Implemented by the scoped private publisher. Replay only committed complete
/// certificate frames bound to this exact checkpoint/source incarnation.
pub(crate) trait ReplayLegacyCertificates {
    fn replay(
        &self,
        memory: &CleanupMemory,
        emit: &mut dyn FnMut(LegacyCacheCertificate) -> Result<(), LegacyIssue>,
    ) -> Result<(), LegacyIssue>;
}
#[cfg(test)]
struct NoCertificates;
#[cfg(test)]
impl ReplayLegacyCertificates for NoCertificates {
    fn replay(
        &self,
        _: &CleanupMemory,
        _: &mut dyn FnMut(LegacyCacheCertificate) -> Result<(), LegacyIssue>,
    ) -> Result<(), LegacyIssue> {
        Err(LegacyIssue::Unknown)
    }
}

/// Checksum-protected progress facts only. Publication and recoverable selection
/// of these bytes belong to the existing scoped journal, never this codec.
pub(crate) struct LegacyCheckpoint {
    expected: ReferenceEpoch,
    database: FixedFileStamp,
    manifest: FixedFileStamp,
    cache_root: FileIdentity,
    backup: Option<BackupCache>,
    repository_identity: FileIdentity,
    context: [u8; 32],
    files: Span,
    next: u64,
    previous: Option<Span>,
    count: u64,
    chain: [u8; 32],
    database_sha256: [u8; 32],
    manifest_sha256: [u8; 32],
    loader_usable: bool,
    stage: Stage,
}
fn context_digest(root: &Path, repository: &Path) -> [u8; 32] {
    let mut hash = Sha256::new();
    hash.update(b"CGLegacyContext01");
    hash.update((root.as_os_str().as_encoded_bytes().len() as u64).to_le_bytes());
    hash.update(root.as_os_str().as_encoded_bytes());
    hash.update(repository.as_os_str().as_encoded_bytes());
    hash.finalize().into()
}
impl LegacyCheckpoint {
    pub(crate) fn encode(&self, out: &mut [u8; LEGACY_CHECKPOINT_BYTES]) {
        out.fill(0);
        out[..8].copy_from_slice(b"CGLGCP01");
        out[8] = match self.stage {
            Stage::Catalog => 0,
            Stage::Caches => 1,
            Stage::Join => 2,
            Stage::Done => 3,
        };
        out[9] = u8::from(self.loader_usable);
        out[10] = u8::from(self.previous.is_some());
        self.expected
            .encode_checkpoint((&mut out[16..528]).try_into().unwrap());
        self.database
            .encode_checkpoint((&mut out[528..608]).try_into().unwrap());
        self.manifest
            .encode_checkpoint((&mut out[608..688]).try_into().unwrap());
        put_id(&mut out[688..704], self.cache_root);
        out[704..736].copy_from_slice(&self.context);
        put_u64(&mut out[736..], self.files.start);
        put_u64(&mut out[744..], self.files.end);
        put_u64(&mut out[752..], self.next);
        if let Some(previous) = self.previous {
            put_u64(&mut out[760..], previous.start);
            put_u64(&mut out[768..], previous.end);
        }
        put_u64(&mut out[776..], self.count);
        out[784..816].copy_from_slice(&self.chain);
        out[816..848].copy_from_slice(&self.database_sha256);
        out[848..880].copy_from_slice(&self.manifest_sha256);
        put_id(&mut out[880..896], self.repository_identity);
        if let Some(backup) = self.backup {
            out[11] = 1;
            out[896..928].copy_from_slice(&backup.key);
            out[928..1056].copy_from_slice(&backup.name);
            out[1056] = backup.len;
            put_id(&mut out[1057..1073], backup.directory);
            backup
                .stamp
                .encode_checkpoint((&mut out[1073..1153]).try_into().unwrap());
        }
        checksum(out);
    }
    pub(crate) fn decode(bytes: &[u8; LEGACY_CHECKPOINT_BYTES]) -> Result<Self, LegacyIssue> {
        if &bytes[..8] != b"CGLGCP01"
            || bytes[8] > 3
            || bytes[9] > 1
            || bytes[10] > 1
            || bytes[11] > 1
            || bytes[12..16].iter().any(|b| *b != 0)
            || bytes[1153..2016].iter().any(|b| *b != 0)
            || !valid_checksum(bytes)
        {
            return Err(LegacyIssue::Unknown);
        }
        let expected = ReferenceEpoch::decode_checkpoint((&bytes[16..528]).try_into().unwrap())
            .map_err(fixed)?;
        let database = FixedFileStamp::decode_checkpoint((&bytes[528..608]).try_into().unwrap())
            .map_err(fixed)?;
        let manifest = FixedFileStamp::decode_checkpoint((&bytes[608..688]).try_into().unwrap())
            .map_err(fixed)?;
        let files = Span {
            start: get_u64(&bytes[736..]),
            end: get_u64(&bytes[744..]),
        };
        let next = get_u64(&bytes[752..]);
        let previous = if bytes[10] == 1 {
            Some(Span {
                start: get_u64(&bytes[760..]),
                end: get_u64(&bytes[768..]),
            })
        } else {
            if bytes[760..776].iter().any(|b| *b != 0) {
                return Err(LegacyIssue::Unknown);
            }
            None
        };
        let count = get_u64(&bytes[776..]);
        let stage = match bytes[8] {
            0 => Stage::Catalog,
            1 => Stage::Caches,
            2 => Stage::Join,
            _ => Stage::Done,
        };
        if stage != Stage::Catalog
            && (files.start >= files.end
                || files.end > manifest.len()
                || next <= files.start
                || next >= files.end
                || count > 0 && previous.is_none())
        {
            return Err(LegacyIssue::Unknown);
        }
        if previous.is_some_and(|p| p.start >= p.end || p.end > manifest.len()) {
            return Err(LegacyIssue::Unknown);
        }
        let backup = if bytes[11] == 1 {
            let b = BackupCache {
                key: bytes[896..928].try_into().unwrap(),
                name: bytes[928..1056].try_into().unwrap(),
                len: bytes[1056],
                directory: get_id(&bytes[1057..1073]),
                stamp: FixedFileStamp::decode_checkpoint((&bytes[1073..1153]).try_into().unwrap())
                    .map_err(fixed)?,
            };
            if b.len > 128
                || !backup_name(b.key, &b.name[..usize::from(b.len)])
                || b.name[usize::from(b.len)..].iter().any(|v| *v != 0)
            {
                return Err(LegacyIssue::Unknown);
            }
            Some(b)
        } else {
            if bytes[896..1153].iter().any(|v| *v != 0) {
                return Err(LegacyIssue::Unknown);
            }
            None
        };
        Ok(Self {
            expected,
            database,
            manifest,
            backup,
            cache_root: get_id(&bytes[688..704]),
            repository_identity: get_id(&bytes[880..896]),
            context: bytes[704..736].try_into().unwrap(),
            files,
            next,
            previous,
            count,
            chain: bytes[784..816].try_into().unwrap(),
            database_sha256: bytes[816..848].try_into().unwrap(),
            manifest_sha256: bytes[848..880].try_into().unwrap(),
            loader_usable: bytes[9] == 1,
            stage,
        })
    }
    pub(crate) fn source_epoch(&self) -> &ReferenceEpoch {
        &self.expected
    }
    pub(crate) fn context_digest(&self) -> [u8; 32] {
        let mut epoch = [0; 512];
        self.expected.encode_checkpoint(&mut epoch);
        let mut db = [0; 80];
        self.database.encode_checkpoint(&mut db);
        let mut manifest = [0; 80];
        self.manifest.encode_checkpoint(&mut manifest);
        let (a, b) = self.cache_root.fixed_words();
        let mut hash = Sha256::new();
        hash.update(b"CGLegacyJobContext01");
        hash.update(epoch);
        hash.update(db);
        hash.update(manifest);
        hash.update(a.to_le_bytes());
        hash.update(b.to_le_bytes());
        hash.update(self.context);
        let (a, b) = self.repository_identity.fixed_words();
        hash.update(a.to_le_bytes());
        hash.update(b.to_le_bytes());
        if let Some(b) = self.backup {
            hash.update(b.key);
            hash.update(b.name);
            let mut stamp = [0; 80];
            b.stamp.encode_checkpoint(&mut stamp);
            hash.update(stamp);
            let (a, c) = b.directory.fixed_words();
            hash.update(a.to_le_bytes());
            hash.update(c.to_le_bytes());
        }
        hash.finalize().into()
    }
    #[cfg(test)]
    pub(crate) fn certified_cache_count(&self) -> u64 {
        self.count
    }
}
impl LegacyAssociationPreparation {
    pub(crate) fn checkpoint(&self) -> Result<LegacyCheckpoint, LegacyIssue> {
        if self.pending.is_some() {
            return Err(LegacyIssue::Unknown);
        }
        self.checkpoint_inner(false)
    }
    pub(crate) fn checkpoint_after_cache(&self) -> Result<LegacyCheckpoint, LegacyIssue> {
        if self.pending.is_none() {
            return Err(LegacyIssue::Unknown);
        }
        self.checkpoint_inner(true)
    }
    fn checkpoint_inner(&self, include_pending: bool) -> Result<LegacyCheckpoint, LegacyIssue> {
        let mut count = self.cache_count;
        let mut chain = self.certificate;
        let mut loader_usable = self.cache_loader_usable;
        if include_pending {
            let pending = self.pending.as_ref().ok_or(LegacyIssue::Unknown)?;
            chain_certificate(&mut chain, pending)?;
            count = count.checked_add(1).ok_or(LegacyIssue::Unknown)?;
            loader_usable &= pending.loader_usable;
        }
        Ok(LegacyCheckpoint {
            expected: self.expected.clone(),
            database: self.database_stamp,
            manifest: self.manifest_stamp,
            cache_root: self.artifact_root.identity(),
            backup: self.backup,
            repository_identity: self.repository_identity,
            context: context_digest(&self.artifact_root_path, &self.repository),
            files: self.files,
            next: self.cursor.as_ref().map(|c| c.next_offset).unwrap_or(0),
            previous: self.cursor.as_ref().and_then(|c| c.previous),
            count,
            chain,
            database_sha256: self.database_sha256,
            manifest_sha256: self.manifest_sha256,
            loader_usable,
            stage: self.stage,
        })
    }
    pub(crate) fn restore(
        database: File,
        manifest: File,
        artifact_root: RootedDirectory,
        artifact_root_path: &Path,
        repository_root: &Path,
        checkpoint: LegacyCheckpoint,
        memory: &CleanupMemory,
    ) -> Result<Self, LegacyIssue> {
        if checkpoint.stage == Stage::Done
            || context_digest(artifact_root_path, repository_root) != checkpoint.context
            || checkpoint.cache_root != artifact_root.identity()
        {
            return Err(LegacyIssue::Changed);
        }
        let mut job = Self::new(
            database,
            manifest,
            artifact_root,
            artifact_root_path,
            repository_root,
            checkpoint.expected,
            memory,
        )?;
        job.backup = checkpoint.backup;
        if let Some(b) = job.backup {
            let _ = open_cache(&job.artifact_root, b.key, job.backup)?;
        }
        if job.database_stamp != checkpoint.database || job.manifest_stamp != checkpoint.manifest {
            return Err(LegacyIssue::Changed);
        }
        if checkpoint.stage != Stage::Catalog {
            job.files = checkpoint.files;
            job.cursor = Some(ManifestCursor::resume(
                &job.manifest,
                checkpoint.files,
                checkpoint.next,
                checkpoint.previous,
                memory,
            )?);
            job.stage = checkpoint.stage;
            job.database_sha256 = checkpoint.database_sha256;
            job.manifest_sha256 = checkpoint.manifest_sha256;
            job.certificate = checkpoint.chain;
            job.cache_count = checkpoint.count;
            job.cache_loader_usable = checkpoint.loader_usable;
            job.resume_validation = true;
        }
        Ok(job)
    }
}

#[cfg(test)]
mod checkpoint_tests {
    use super::*;
    #[test]
    fn dc_legacy_fixed_certificate_rejects_reserved_and_torn_frames() {
        let root = std::env::temp_dir().join(format!(
            "legacy-codec-{}",
            super::super::layout::managed_generation_id()
        ));
        std::fs::create_dir(&root).unwrap();
        let path = root.join("payload");
        std::fs::write(&path, b"\"key\"").unwrap();
        let stamp =
            FixedFileStamp::capture(&File::open(&path).unwrap(), FixedControlKind::File).unwrap();
        let fields = [Span { start: 0, end: 5 }; 5];
        let facts = [
            LegacyProjectedFact {
                domain: 1,
                ordinal: 0,
                fields,
            },
            LegacyProjectedFact {
                domain: 2,
                ordinal: 1,
                fields,
            },
            LegacyProjectedFact {
                domain: 3,
                ordinal: 2,
                fields,
            },
            LegacyProjectedFact {
                domain: 4,
                ordinal: 3,
                fields,
            },
            LegacyProjectedFact {
                domain: 4,
                ordinal: 4,
                fields,
            },
        ];
        let certificate = LegacyCacheCertificate {
            key: [1; 32],
            stamp,
            digest: [2; 32],
            facts,
            rows: 5,
            loader_usable: true,
        };
        let mut bytes = [0; LEGACY_CERTIFICATE_BYTES];
        certificate.encode(&mut bytes);
        assert!(LegacyCacheCertificate::decode(&bytes).is_ok());
        for cut in 0..bytes.len() {
            let mut torn = bytes;
            torn[cut..].fill(0);
            if torn != bytes {
                assert!(LegacyCacheCertificate::decode(&torn).is_err(), "cut {cut}")
            }
        }
        let mut reserved = bytes;
        reserved[700] = 1;
        checksum(&mut reserved);
        assert!(LegacyCacheCertificate::decode(&reserved).is_err());
        let mut domain = bytes;
        put_u64(&mut domain[160..], 9);
        checksum(&mut domain);
        assert!(LegacyCacheCertificate::decode(&domain).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn dc_legacy_streaming_native_schema_matches_current_assets() {
        let value: serde_json::Value =
            serde_json::from_str(include_str!("../../assets/graph_schema.json")).unwrap();
        let mut nodes = std::collections::BTreeSet::new();
        for node in value["node_types"].as_array().unwrap() {
            nodes.insert(node["name"].as_str().unwrap());
        }
        for relation in value["relation_types"].as_array().unwrap() {
            for field in ["source_types", "target_types"] {
                for n in relation[field].as_array().unwrap() {
                    nodes.insert(n.as_str().unwrap());
                }
            }
        }
        assert_eq!(nodes.into_iter().collect::<Vec<_>>(), TABLE_NAMES[..37]);
        assert_eq!(ENDPOINTS.len(), 396);
    }
}
impl LegacyAssociationPreparation {
    /// Select one exact historical replacement directory before any scan. The
    /// name is an address only; complete producer/schema/native joins still run.
    pub(crate) fn select_backup_cache(
        &mut self,
        key: [u8; 32],
        name: &[u8],
    ) -> Result<(), LegacyIssue> {
        if self.stage != Stage::Catalog || self.backup.is_some() || !backup_name(key, name) {
            return Err(LegacyIssue::Unknown);
        }
        let hex = hex(key);
        let prefix = self.artifact_root.open_directory_readonly(&hex[..2])?;
        match prefix.open_directory_readonly(&hex) {
            Ok(_) => return Err(LegacyIssue::Unknown),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
        let directory = prefix.open_directory_readonly(name)?;
        let file = directory.open_readonly_file(b"partition.json")?;
        let mut owned = [0; 128];
        owned[..name.len()].copy_from_slice(name);
        self.backup = Some(BackupCache {
            key,
            name: owned,
            len: name.len() as u8,
            directory: directory.identity(),
            stamp: FixedFileStamp::capture(&file, FixedControlKind::File).map_err(fixed)?,
        });
        Ok(())
    }
}
impl LegacyAssociation {
    pub(crate) fn selected_backup_loader_usable(&self) -> Option<bool> {
        self.backup_loader_usable
    }
    pub(crate) fn selected_backup_matches(
        &self,
        key: [u8; 32],
        directory: FileIdentity,
        stamp: FixedFileStamp,
    ) -> bool {
        self.backup
            .is_some_and(|b| b.key == key && b.directory == directory && b.stamp == stamp)
    }
}
impl LegacyCheckpoint {
    pub(crate) fn matches_closed_source(
        &self,
        database: Option<FixedFileStamp>,
        manifest: Option<FixedFileStamp>,
        cache: FileIdentity,
    ) -> bool {
        database == Some(self.database)
            && manifest == Some(self.manifest)
            && cache == self.cache_root
    }
}

#[derive(Default)]
pub(crate) struct SelectedBackupAssociationState {
    pub(super) job: Option<super::maintenance_budget::MemoryBox<LegacyAssociationPreparation>>,
    pub(super) transport: Option<super::maintenance_legacy_stream::AssociationTransportState>,
}
impl SelectedBackupAssociationState {
    pub(crate) fn clear(&mut self) {
        drop(self.job.take());
        drop(self.transport.take());
    }
}
/// Pure bounded preparation. The caller supplies actual resolved source handles
/// and a certified replay reader; this function creates no reference authority.
#[allow(clippy::too_many_arguments)]
impl LegacyAssociationPreparation {
    pub(super) fn source_matches(
        &self,
        database: &File,
        manifest: &File,
        root: &RootedDirectory,
        path: &Path,
        repository: &Path,
        expected: &ReferenceEpoch,
    ) -> bool {
        self.database_stamp.recheck(database).is_ok()
            && self.manifest_stamp.recheck(manifest).is_ok()
            && self.artifact_root.identity() == root.identity()
            && self.artifact_root_path == path
            && self.repository == repository
            && (self.closed_source_authorized || self.expected == *expected)
    }
    pub(super) fn into_source_handles(self) -> (File, File) {
        (self.database, self.manifest)
    }
}

impl SelectedBackupAssociationState {
    pub(crate) fn needs_source_handles(&self) -> bool {
        self.job.is_none() && self.transport.as_ref().is_none_or(|t| !t.has_completed())
    }
}
impl LegacyAssociationPreparation {
    pub(super) fn check_parked_source(
        &self,
        domain: &ReferenceDomain,
        expected: &ReferenceEpoch,
        root: &Path,
        repository: &Path,
        memory: &CleanupMemory,
        cancel: &dyn Fn() -> bool,
    ) -> Result<(), LegacyIssue> {
        if (!self.closed_source_authorized && self.expected != *expected)
            || self.artifact_root_path != root
            || self.repository != repository
        {
            return Err(LegacyIssue::Changed);
        }
        self.current(domain, memory, cancel)
    }
}

impl LegacyCacheCertificate {
    pub(crate) fn key(&self) -> [u8; 32] {
        self.key
    }
    pub(crate) fn stamp(&self) -> FixedFileStamp {
        self.stamp
    }
    pub(crate) fn payload_digest(&self) -> [u8; 32] {
        self.digest
    }
    pub(crate) fn loader_usable(&self) -> bool {
        self.loader_usable
    }
}

impl LegacyAssociationPreparation {
    pub(super) fn authorize_closed_source(
        &mut self,
        owner: &super::maintenance_legacy_bootstrap::LegacySourceOwner,
        memory: &CleanupMemory,
    ) -> Result<(), LegacyIssue> {
        let source = owner.closed_version();
        if owner.is_administrative()
            || !owner.is_current(memory).map_err(fixed)?
            || !matches!(
                source.phase(),
                super::maintenance_legacy_guard::LegacyPhase::Preparing
                    | super::maintenance_legacy_guard::LegacyPhase::Retiring
            )
            || source.database_stamp() != Some(self.database_stamp)
            || source.manifest_stamp() != Some(self.manifest_stamp)
            || source.cache_identity() != self.artifact_root.identity()
        {
            return Err(LegacyIssue::Changed);
        }
        // The original destination epoch remains immutable checkpoint data. A
        // genuinely closed source has its own retained version; unrelated live
        // destination producers do not mutate that closed bundle.
        self.closed_source_authorized = true;
        Ok(())
    }
}
