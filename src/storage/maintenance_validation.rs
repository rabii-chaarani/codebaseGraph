//! Pure validation facts. Nothing here constructs ownership, loader usability,
//! publication or deletion authority. Sources must remain immutable per replay.
use super::maintenance_budget::{CleanupMemory, MemoryReservation, ReservedBytes, ResourceLimit};
use super::maintenance_json::{JsonFailure, JsonToken, MaintenanceJsonReader, TokenKind};
use sha2::{Digest, Sha256};
use std::cell::Cell;
use std::cmp::Ordering;
#[cfg(test)]
use std::fs::File;
use std::io::{self, Read};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ValidationIssue {
    Resource(ResourceLimit),
    Json(JsonFailure),
    InvalidSpan,
    Schema,
    NonProducer,
    Numeric,
    RawRowsDiffer,
    Overflow,
    Io(io::ErrorKind),
    #[cfg(not(any(unix, windows)))]
    UnsupportedPositionedRead,
}
impl From<ResourceLimit> for ValidationIssue {
    fn from(e: ResourceLimit) -> Self {
        Self::Resource(e)
    }
}
impl From<JsonFailure> for ValidationIssue {
    fn from(e: JsonFailure) -> Self {
        Self::Json(e)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Span {
    pub(crate) start: u64,
    pub(crate) end: u64,
}
impl Span {
    fn len(self) -> Result<u64, ValidationIssue> {
        self.end
            .checked_sub(self.start)
            .ok_or(ValidationIssue::InvalidSpan)
    }
}

/// Read-only random access. Any backing buffers are the caller's responsibility
/// in the SAME account; this interface never owns/copies a whole source.
pub(crate) trait ByteSource {
    fn len(&self) -> u64;
    fn read_at(&self, offset: u64, output: &mut [u8]) -> Result<usize, ValidationIssue>;
}
impl ByteSource for [u8] {
    fn len(&self) -> u64 {
        self.len() as u64
    }
    fn read_at(&self, offset: u64, output: &mut [u8]) -> Result<usize, ValidationIssue> {
        let offset = usize::try_from(offset).map_err(|_| ValidationIssue::InvalidSpan)?;
        let source = self.get(offset..).ok_or(ValidationIssue::InvalidSpan)?;
        let count = source.len().min(output.len());
        output[..count].copy_from_slice(&source[..count]);
        Ok(count)
    }
}
/// Borrows a dedicated, already-open preparation handle; no path is allocated
/// or opened and no file identity/association/immutability is certified.
///
/// The mutable borrow excludes concurrent ordinary cursor use through this
/// handle. The caller must also exclude cursor-sharing clones. On Windows,
/// seek_read moves the native cursor; Unix read_at does not. Never borrow a
/// producer's sequential-writing handle here. File/type/stamp authority remains
/// the caller's later noncreating S2 preparation, not this length observation.
#[cfg(test)]
pub(crate) struct BorrowedFileSource<'a> {
    file: &'a File,
    length: u64,
    _floor: MemoryReservation,
}
#[cfg(test)]
impl<'a> BorrowedFileSource<'a> {
    pub(crate) fn new(file: &'a mut File, memory: &CleanupMemory) -> Result<Self, ValidationIssue> {
        let floor = std::mem::size_of::<Self>()
            .checked_add(std::mem::size_of::<std::fs::Metadata>())
            .and_then(|n| n.checked_add(128))
            .ok_or(ValidationIssue::Overflow)?;
        let reservation = memory.reserve(floor)?;
        let metadata = file.metadata().map_err(|e| ValidationIssue::Io(e.kind()))?;
        if !metadata.is_file() {
            return Err(ValidationIssue::Schema);
        }
        Ok(Self {
            file,
            length: metadata.len(),
            _floor: reservation,
        })
    }
}
#[cfg(test)]
impl ByteSource for BorrowedFileSource<'_> {
    fn len(&self) -> u64 {
        self.length
    }
    fn read_at(&self, offset: u64, output: &mut [u8]) -> Result<usize, ValidationIssue> {
        let remaining = self
            .length
            .checked_sub(offset)
            .ok_or(ValidationIssue::InvalidSpan)?;
        let count = remaining.min(output.len() as u64) as usize;
        let output = &mut output[..count];
        loop {
            #[cfg(unix)]
            let result = std::os::unix::fs::FileExt::read_at(self.file, output, offset);
            #[cfg(windows)]
            let result = std::os::windows::fs::FileExt::seek_read(self.file, output, offset);
            #[cfg(not(any(unix, windows)))]
            return Err(ValidationIssue::UnsupportedPositionedRead);
            #[cfg(any(unix, windows))]
            match result {
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(ValidationIssue::Io(e.kind())),
                Ok(n) => return Ok(n),
            }
        }
    }
}
struct SourceReader<'a, S: ?Sized> {
    source: &'a S,
    offset: u64,
}
impl<S: ByteSource + ?Sized> Read for SourceReader<'_, S> {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        let n = self
            .source
            .read_at(self.offset, output)
            .map_err(|_| io::ErrorKind::InvalidData)?;
        self.offset = self
            .offset
            .checked_add(n as u64)
            .ok_or(io::ErrorKind::InvalidData)?;
        Ok(n)
    }
}
struct SpanReader<'a, S: ?Sized> {
    source: &'a S,
    offset: u64,
    end: u64,
    bytes: [u8; 256],
    at: usize,
    filled: usize,
}
impl<'a, S: ByteSource + ?Sized> SpanReader<'a, S> {
    fn new(source: &'a S, span: Span) -> Result<Self, ValidationIssue> {
        if span.start > span.end || span.end > source.len() {
            return Err(ValidationIssue::InvalidSpan);
        }
        Ok(Self {
            source,
            offset: span.start,
            end: span.end,
            bytes: [0; 256],
            at: 0,
            filled: 0,
        })
    }
    fn next(&mut self) -> Result<Option<u8>, ValidationIssue> {
        if self.at == self.filled {
            let len = (self.end - self.offset).min(256) as usize;
            if len == 0 {
                return Ok(None);
            }
            self.filled = self.source.read_at(self.offset, &mut self.bytes[..len])?;
            self.at = 0;
            if self.filled == 0 || self.filled > len {
                return Err(ValidationIssue::InvalidSpan);
            }
            self.offset += self.filled as u64;
        }
        let byte = self.bytes[self.at];
        self.at += 1;
        Ok(Some(byte))
    }
    fn required(&mut self) -> Result<u8, ValidationIssue> {
        self.next()?.ok_or(ValidationIssue::InvalidSpan)
    }
}
impl<S: ByteSource + ?Sized> Read for SpanReader<'_, S> {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        let mut count = 0;
        for byte in output {
            match self.next().map_err(|_| io::ErrorKind::InvalidData)? {
                Some(value) => {
                    *byte = value;
                    count += 1;
                }
                None => break,
            }
        }
        Ok(count)
    }
}
struct Decoded<'a, S: ?Sized> {
    raw: SpanReader<'a, S>,
    pending: [u8; 4],
    at: usize,
    filled: usize,
    ended: bool,
}
impl<'a, S: ByteSource + ?Sized> Decoded<'a, S> {
    fn new(source: &'a S, span: Span) -> Result<Self, ValidationIssue> {
        let mut raw = SpanReader::new(source, span)?;
        if raw.required()? != b'"' {
            return Err(ValidationIssue::InvalidSpan);
        }
        Ok(Self {
            raw,
            pending: [0; 4],
            at: 0,
            filled: 0,
            ended: false,
        })
    }
    fn hex(&mut self) -> Result<u32, ValidationIssue> {
        let mut value = 0;
        for _ in 0..4 {
            let b = self.raw.required()?;
            let v = match b {
                b'0'..=b'9' => b - b'0',
                b'a'..=b'f' => b - b'a' + 10,
                b'A'..=b'F' => b - b'A' + 10,
                _ => return Err(ValidationIssue::NonProducer),
            };
            value = value * 16 + u32::from(v);
        }
        Ok(value)
    }
    fn next(&mut self) -> Result<Option<u8>, ValidationIssue> {
        if self.at < self.filled {
            let b = self.pending[self.at];
            self.at += 1;
            return Ok(Some(b));
        }
        if self.ended {
            return Ok(None);
        }
        let byte = self.raw.required()?;
        if byte == b'"' {
            self.ended = true;
            if self.raw.next()?.is_some() {
                return Err(ValidationIssue::InvalidSpan);
            }
            return Ok(None);
        }
        if byte != b'\\' {
            if byte < 32 {
                return Err(ValidationIssue::NonProducer);
            }
            if byte >= 0x80 {
                self.filled = match byte {
                    0xc2..=0xdf => 2,
                    0xe0..=0xef => 3,
                    0xf0..=0xf4 => 4,
                    _ => return Err(ValidationIssue::NonProducer),
                };
                self.pending[0] = byte;
                for value in &mut self.pending[1..self.filled] {
                    *value = self.raw.required()?;
                }
                std::str::from_utf8(&self.pending[..self.filled])
                    .map_err(|_| ValidationIssue::NonProducer)?;
                self.at = 1;
            }
            return Ok(Some(byte));
        }
        let escape = self.raw.required()?;
        Ok(Some(match escape {
            b'"' | b'\\' | b'/' => escape,
            b'b' => 8,
            b'f' => 12,
            b'n' => 10,
            b'r' => 13,
            b't' => 9,
            b'u' => {
                let first = self.hex()?;
                let value = if (0xd800..=0xdbff).contains(&first) {
                    if self.raw.required()? != b'\\' || self.raw.required()? != b'u' {
                        return Err(ValidationIssue::NonProducer);
                    }
                    let low = self.hex()?;
                    if !(0xdc00..=0xdfff).contains(&low) {
                        return Err(ValidationIssue::NonProducer);
                    }
                    0x10000 + ((first - 0xd800) << 10) + low - 0xdc00
                } else {
                    first
                };
                let c = char::from_u32(value).ok_or(ValidationIssue::NonProducer)?;
                self.filled = c.encode_utf8(&mut self.pending).len();
                self.at = 1;
                self.pending[0]
            }
            _ => return Err(ValidationIssue::NonProducer),
        }))
    }
}
fn compare<S: ByteSource + ?Sized>(
    source: &S,
    a: Span,
    b: Span,
) -> Result<Ordering, ValidationIssue> {
    compare_complete(Decoded::new(source, a)?, Decoded::new(source, b)?)
}
trait KeyBytes {
    fn next_key_byte(&mut self) -> Result<Option<u8>, ValidationIssue>;
}
impl<S: ByteSource + ?Sized> KeyBytes for Decoded<'_, S> {
    fn next_key_byte(&mut self) -> Result<Option<u8>, ValidationIssue> {
        self.next()
    }
}
struct NativeUtf8<'a, S: ByteSource + ?Sized> {
    raw: SpanReader<'a, S>,
    carry: [u8; 4],
    at: usize,
    filled: usize,
}
impl<'a, S: ByteSource + ?Sized> NativeUtf8<'a, S> {
    fn new(source: &'a S, span: Span) -> Result<Self, ValidationIssue> {
        Ok(Self {
            raw: SpanReader::new(source, span)?,
            carry: [0; 4],
            at: 0,
            filled: 0,
        })
    }
}
impl<S: ByteSource + ?Sized> KeyBytes for NativeUtf8<'_, S> {
    fn next_key_byte(&mut self) -> Result<Option<u8>, ValidationIssue> {
        if self.at < self.filled {
            let byte = self.carry[self.at];
            self.at += 1;
            return Ok(Some(byte));
        }
        let Some(byte) = self.raw.next()? else {
            return Ok(None);
        };
        if byte >= 0x80 {
            self.filled = match byte {
                0xc2..=0xdf => 2,
                0xe0..=0xef => 3,
                0xf0..=0xf4 => 4,
                _ => return Err(ValidationIssue::NonProducer),
            };
            self.carry[0] = byte;
            for value in &mut self.carry[1..self.filled] {
                *value = self.raw.required()?;
            }
            std::str::from_utf8(&self.carry[..self.filled])
                .map_err(|_| ValidationIssue::NonProducer)?;
            self.at = 1;
        }
        Ok(Some(byte))
    }
}
fn compare_complete<A: KeyBytes, B: KeyBytes>(
    mut left: A,
    mut right: B,
) -> Result<Ordering, ValidationIssue> {
    let mut ordering = Ordering::Equal;
    loop {
        let pair = (left.next_key_byte()?, right.next_key_byte()?);
        if pair == (None, None) {
            return Ok(ordering);
        }
        // Ordering is decided by the first difference, but both complete spans
        // must still validate before that ordering can become a successful fact.
        if ordering == Ordering::Equal {
            ordering = match pair {
                (None, Some(_)) => Ordering::Less,
                (Some(_), None) => Ordering::Greater,
                (Some(a), Some(b)) => a.cmp(&b),
                (None, None) => unreachable!(),
            };
        }
    }
}
/// Native spans address a LOGICAL UTF8 view, not compressed physical bytes.
/// Source/encoding/locator identity remains explicit and confers no origin.
pub(crate) fn compare_native_json<A: ByteSource + ?Sized, B: ByteSource + ?Sized>(
    native: &A,
    native_span: Span,
    json: &B,
    json_span: Span,
    memory: &CleanupMemory,
) -> Result<Ordering, ValidationIssue> {
    let floor = std::mem::size_of::<NativeUtf8<'_, A>>()
        .checked_add(std::mem::size_of::<Decoded<'_, B>>())
        .and_then(|n| n.checked_add(128))
        .ok_or(ValidationIssue::Overflow)?;
    let _reservation = memory.reserve(floor)?;
    compare_complete(
        NativeUtf8::new(native, native_span)?,
        Decoded::new(json, json_span)?,
    )
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum KeyEncoding {
    NativeUtf8,
    JsonDecoded,
}
/// Exact complete-key comparison for independently encoded/bound sources.
/// Ordering is returned only after both entire encodings validate.
pub(crate) fn compare_encoded_keys<A: ByteSource + ?Sized, B: ByteSource + ?Sized>(
    left: &A,
    left_span: Span,
    left_encoding: KeyEncoding,
    right: &B,
    right_span: Span,
    right_encoding: KeyEncoding,
    memory: &CleanupMemory,
) -> Result<Ordering, ValidationIssue> {
    let floor = std::mem::size_of::<NativeUtf8<'_, A>>()
        .checked_add(std::mem::size_of::<Decoded<'_, A>>())
        .and_then(|n| n.checked_add(std::mem::size_of::<NativeUtf8<'_, B>>()))
        .and_then(|n| n.checked_add(std::mem::size_of::<Decoded<'_, B>>()))
        .and_then(|n| n.checked_add(128))
        .ok_or(ValidationIssue::Overflow)?;
    let _reservation = memory.reserve(floor)?;
    match (left_encoding, right_encoding) {
        (KeyEncoding::NativeUtf8, KeyEncoding::NativeUtf8) => compare_complete(
            NativeUtf8::new(left, left_span)?,
            NativeUtf8::new(right, right_span)?,
        ),
        (KeyEncoding::NativeUtf8, KeyEncoding::JsonDecoded) => compare_complete(
            NativeUtf8::new(left, left_span)?,
            Decoded::new(right, right_span)?,
        ),
        (KeyEncoding::JsonDecoded, KeyEncoding::NativeUtf8) => compare_complete(
            Decoded::new(left, left_span)?,
            NativeUtf8::new(right, right_span)?,
        ),
        (KeyEncoding::JsonDecoded, KeyEncoding::JsonDecoded) => compare_complete(
            Decoded::new(left, left_span)?,
            Decoded::new(right, right_span)?,
        ),
    }
}
/// Domain separation and decoded bytes are shared across encodings. A digest
/// is an accelerator only; callers still compare complete exact key streams.
pub(crate) fn key_stream_hash<S: ByteSource + ?Sized>(
    source: &S,
    span: Span,
    encoding: KeyEncoding,
    domain: u64,
    memory: &CleanupMemory,
) -> Result<[u8; 32], ValidationIssue> {
    let floor = std::mem::size_of::<NativeUtf8<'_, S>>()
        .checked_add(std::mem::size_of::<Decoded<'_, S>>())
        .and_then(|n| n.checked_add(std::mem::size_of::<Sha256>()))
        .and_then(|n| n.checked_add(128))
        .ok_or(ValidationIssue::Overflow)?;
    let _reservation = memory.reserve(floor)?;
    fn hash(mut reader: impl KeyBytes, domain: u64) -> Result<[u8; 32], ValidationIssue> {
        let mut hash = Sha256::new();
        hash.update(domain.to_le_bytes());
        while let Some(byte) = reader.next_key_byte()? {
            hash.update([byte]);
        }
        Ok(hash.finalize().into())
    }
    match encoding {
        KeyEncoding::NativeUtf8 => hash(NativeUtf8::new(source, span)?, domain),
        KeyEncoding::JsonDecoded => hash(Decoded::new(source, span)?, domain),
    }
}
/// Decode into caller-owned fixed storage. Oversized keys are fully validated
/// before Schema is returned; no truncated prefix can become a key witness.
pub(crate) fn copy_decoded_key<S: ByteSource + ?Sized>(
    source: &S,
    span: Span,
    encoding: KeyEncoding,
    out: &mut [u8],
    memory: &CleanupMemory,
) -> Result<usize, ValidationIssue> {
    let floor = std::mem::size_of::<NativeUtf8<'_, S>>()
        .checked_add(std::mem::size_of::<Decoded<'_, S>>())
        .and_then(|n| n.checked_add(128))
        .ok_or(ValidationIssue::Overflow)?;
    let _reservation = memory.reserve(floor)?;
    fn copy(mut reader: impl KeyBytes, out: &mut [u8]) -> Result<usize, ValidationIssue> {
        let mut length = 0usize;
        while let Some(byte) = reader.next_key_byte()? {
            if length < out.len() {
                out[length] = byte;
            }
            length = length.checked_add(1).ok_or(ValidationIssue::Overflow)?;
        }
        if length > out.len() {
            Err(ValidationIssue::Schema)
        } else {
            Ok(length)
        }
    }
    match encoding {
        KeyEncoding::NativeUtf8 => copy(NativeUtf8::new(source, span)?, out),
        KeyEncoding::JsonDecoded => copy(Decoded::new(source, span)?, out),
    }
}
/// Complete SHA1 of the strict decoded stream. Graph stable IDs use the first
/// ten digest bytes; the returned twenty bytes are never equality authority.
pub(crate) fn key_sha1_prefix20<S: ByteSource + ?Sized>(
    source: &S,
    span: Span,
    encoding: KeyEncoding,
    memory: &CleanupMemory,
) -> Result<[u8; 20], ValidationIssue> {
    let floor = std::mem::size_of::<NativeUtf8<'_, S>>()
        .checked_add(std::mem::size_of::<Decoded<'_, S>>())
        .and_then(|n| n.checked_add(768))
        .ok_or(ValidationIssue::Overflow)?;
    let _reservation = memory.reserve(floor)?;
    fn hash(mut input: impl KeyBytes) -> Result<[u8; 20], ValidationIssue> {
        let mut state = [0x67452301, 0xefcdab89, 0x98badcfe, 0x10325476, 0xc3d2e1f0];
        let mut block = [0; 64];
        let mut used = 0usize;
        let mut len = 0u64;
        while let Some(byte) = input.next_key_byte()? {
            block[used] = byte;
            used += 1;
            len = len.checked_add(1).ok_or(ValidationIssue::Overflow)?;
            if used == 64 {
                sha1_block(&mut state, &block);
                used = 0;
            }
        }
        let bits = len.checked_mul(8).ok_or(ValidationIssue::Overflow)?;
        block[used] = 0x80;
        used += 1;
        if used > 56 {
            block[used..].fill(0);
            sha1_block(&mut state, &block);
            block.fill(0);
        } else {
            block[used..56].fill(0);
        }
        block[56..].copy_from_slice(&bits.to_be_bytes());
        sha1_block(&mut state, &block);
        let mut digest = [0; 20];
        for (i, word) in state.iter().enumerate() {
            digest[i * 4..i * 4 + 4].copy_from_slice(&word.to_be_bytes());
        }
        Ok(digest)
    }
    match encoding {
        KeyEncoding::NativeUtf8 => hash(NativeUtf8::new(source, span)?),
        KeyEncoding::JsonDecoded => hash(Decoded::new(source, span)?),
    }
}

/// Exact decoded bytes across independently bound sources. Digest equality is
/// never substituted for equality; long strings stream through fixed buffers.
pub(crate) fn compare_decoded_spans<A: ByteSource + ?Sized, B: ByteSource + ?Sized>(
    left: &A,
    left_span: Span,
    right: &B,
    right_span: Span,
    memory: &CleanupMemory,
) -> Result<Ordering, ValidationIssue> {
    let bytes = std::mem::size_of::<Decoded<'_, A>>()
        .checked_add(std::mem::size_of::<Decoded<'_, B>>())
        .and_then(|n| n.checked_add(128))
        .ok_or(ValidationIssue::Overflow)?;
    let _floor = memory.reserve(bytes)?;
    compare_complete(
        Decoded::new(left, left_span)?,
        Decoded::new(right, right_span)?,
    )
}
fn equals<S: ByteSource + ?Sized>(
    source: &S,
    span: Span,
    expected: &[u8],
) -> Result<bool, ValidationIssue> {
    let mut decoded = Decoded::new(source, span)?;
    for b in expected {
        if decoded.next()? != Some(*b) {
            return Ok(false);
        }
    }
    Ok(decoded.next()?.is_none())
}
fn producer_string<S: ByteSource + ?Sized>(
    source: &S,
    span: Span,
) -> Result<bool, ValidationIssue> {
    let mut decoded = Decoded::new(source, span)?;
    let mut raw = SpanReader::new(source, span)?;
    if raw.required()? != b'"' {
        return Ok(false);
    }
    while let Some(b) = decoded.next()? {
        let mut encoded = [0u8; 6];
        let bytes: &[u8] = match b {
            b'"' => b"\\\"",
            b'\\' => b"\\\\",
            8 => b"\\b",
            12 => b"\\f",
            10 => b"\\n",
            13 => b"\\r",
            9 => b"\\t",
            0..=31 => {
                encoded[..4].copy_from_slice(b"\\u00");
                encoded[4] = b"0123456789abcdef"[(b >> 4) as usize];
                encoded[5] = b"0123456789abcdef"[(b & 15) as usize];
                &encoded
            }
            _ => {
                encoded[0] = b;
                &encoded[..1]
            }
        };
        for b in bytes {
            if raw.next()? != Some(*b) {
                return Ok(false);
            }
        }
    }
    Ok(raw.next()? == Some(b'"') && raw.next()?.is_none())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NumberKind {
    F64,
    Value,
    I64,
    U64,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct NumberFacts {
    pub(crate) typed: bool,
    pub(crate) stable: bool,
}
/// The pinned parser/serializer is the oracle for EVERY float and Value number.
/// Finite alone is insufficient when float_roundtrip is disabled.
pub(crate) fn number_facts(
    bytes: &[u8],
    kind: NumberKind,
    memory: &CleanupMemory,
) -> Result<NumberFacts, ValidationIssue> {
    // Fixed token/output buffers, pinned scalar parser/serializer scratch and
    // its bounded error allocation. No arbitrary diagnostic is formatted.
    let _workspace = memory.reserve(1024)?;
    if bytes.len() > 64 || !number_token(bytes) {
        return Ok(NumberFacts {
            typed: false,
            stable: false,
        });
    }
    let mut output = [0u8; 64];
    let mut cursor = io::Cursor::new(&mut output[..]);
    let result = match kind {
        NumberKind::F64 => serde_json::from_slice::<f64>(bytes)
            .ok()
            .filter(|v| v.is_finite())
            .and_then(|v| serde_json::to_writer(&mut cursor, &v).ok()),
        NumberKind::Value => serde_json::from_slice::<serde_json::Number>(bytes)
            .ok()
            .and_then(|v| serde_json::to_writer(&mut cursor, &v).ok()),
        NumberKind::I64 => serde_json::from_slice::<i64>(bytes)
            .ok()
            .and_then(|v| serde_json::to_writer(&mut cursor, &v).ok()),
        NumberKind::U64 => serde_json::from_slice::<u64>(bytes)
            .ok()
            .and_then(|v| serde_json::to_writer(&mut cursor, &v).ok()),
    };
    let len = cursor.position() as usize;
    Ok(NumberFacts {
        typed: result.is_some(),
        stable: result.is_some() && output.get(..len) == Some(bytes),
    })
}
fn number_token(bytes: &[u8]) -> bool {
    let mut index = 0;
    if bytes.first() == Some(&b'-') {
        index += 1;
    }
    match bytes.get(index) {
        Some(b'0') => index += 1,
        Some(b'1'..=b'9') => {
            index += 1;
            while bytes.get(index).is_some_and(u8::is_ascii_digit) {
                index += 1;
            }
        }
        _ => return false,
    }
    if bytes.get(index) == Some(&b'.') {
        index += 1;
        let start = index;
        while bytes.get(index).is_some_and(u8::is_ascii_digit) {
            index += 1;
        }
        if start == index {
            return false;
        }
    }
    if matches!(bytes.get(index), Some(b'e' | b'E')) {
        index += 1;
        if matches!(bytes.get(index), Some(b'+' | b'-')) {
            index += 1;
        }
        let start = index;
        while bytes.get(index).is_some_and(u8::is_ascii_digit) {
            index += 1;
        }
        if start == index {
            return false;
        }
    }
    index == bytes.len()
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum JoinKind {
    Raw { ordinal: u64, kind: Span },
    DeclaredId,
    DeclaredType { kind: Span },
    UniqueKey,
}
#[derive(Debug, Clone, Copy)]
pub(crate) struct JoinEvent {
    pub(crate) domain: u64,
    pub(crate) id: Span,
    pub(crate) kind: JoinKind,
}
/// Domain 0=nodes,1=edges; >=2 is a separate metadata-object key namespace.
pub(crate) trait ReplayEvents: ByteSource {
    fn replay(
        &self,
        memory: &CleanupMemory,
        emit: &mut dyn FnMut(JoinEvent) -> Result<(), ValidationIssue>,
    ) -> Result<(), ValidationIssue>;
}
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct RawRowsMatch {
    pub(crate) passes: u64,
    pub(crate) events: u64,
    pub(crate) collision_batches: u64,
    pub(crate) lookup_probes: u64,
}

const SLOT: usize = 128;
#[derive(Clone, Copy, Default)]
struct Row {
    occupied: bool,
    domain: u64,
    hash: [u8; 32],
    id: Span,
    raw: u64,
    declared: u64,
    maps: u64,
    ordinal: u64,
    raw_kind: Span,
    map_kind: Span,
}
struct Table {
    bytes: ReservedBytes,
    capacity: usize,
    probes: Cell<u64>,
}
impl Table {
    fn new(memory: &CleanupMemory, capacity: usize) -> Result<Self, ValidationIssue> {
        if capacity == 0 {
            return Err(ValidationIssue::Resource(ResourceLimit::CeilingExceeded));
        }
        let len = capacity
            .checked_mul(SLOT)
            .ok_or(ValidationIssue::Overflow)?;
        Ok(Self {
            bytes: memory.reserve(len)?.bytes(len)?,
            capacity,
            probes: Cell::new(0),
        })
    }
    fn clear(&mut self) {
        self.bytes.as_mut_slice().fill(0);
    }
    fn row(&self, index: usize) -> Row {
        let b = &self.bytes.as_slice()[index * SLOT..(index + 1) * SLOT];
        let u = |o| u64::from_le_bytes(b[o..o + 8].try_into().unwrap());
        Row {
            occupied: b[0] != 0,
            domain: u(1),
            hash: b[9..41].try_into().unwrap(),
            id: Span {
                start: u(41),
                end: u(49),
            },
            raw: u(57),
            declared: u(65),
            maps: u(73),
            ordinal: u(81),
            raw_kind: Span {
                start: u(89),
                end: u(97),
            },
            map_kind: Span {
                start: u(105),
                end: u(113),
            },
        }
    }
    fn put(&mut self, index: usize, r: Row) {
        let b = &mut self.bytes.as_mut_slice()[index * SLOT..(index + 1) * SLOT];
        b.fill(0);
        // Insert/replace only. There is no single-slot deletion operation that
        // could invalidate first-empty termination of an open-address probe.
        b[0] = 1;
        b[9..41].copy_from_slice(&r.hash);
        for (o, v) in [
            (1, r.domain),
            (41, r.id.start),
            (49, r.id.end),
            (57, r.raw),
            (65, r.declared),
            (73, r.maps),
            (81, r.ordinal),
            (89, r.raw_kind.start),
            (97, r.raw_kind.end),
            (105, r.map_kind.start),
            (113, r.map_kind.end),
        ] {
            b[o..o + 8].copy_from_slice(&v.to_le_bytes());
        }
    }
    fn find<S: ByteSource + ?Sized>(
        &self,
        source: &S,
        event: JoinEvent,
        hash: [u8; 32],
    ) -> Result<Result<usize, usize>, ValidationIssue> {
        let start =
            (u64::from_le_bytes(hash[..8].try_into().unwrap()) % self.capacity as u64) as usize;
        for delta in 0..self.capacity {
            self.probes.set(
                self.probes
                    .get()
                    .checked_add(1)
                    .ok_or(ValidationIssue::Overflow)?,
            );
            let index = (start + delta) % self.capacity;
            let row = self.row(index);
            if !row.occupied {
                // Buckets have no deletion holes: insertion uses this first
                // empty slot, and collision selection replaces only FULL
                // tables. Therefore the key cannot occur beyond this empty.
                return Ok(Err(index));
            }
            if row.domain == event.domain
                && row.hash == hash
                && compare(source, row.id, event.id)? == Ordering::Equal
            {
                return Ok(Ok(index));
            }
        }
        Ok(Err(self.capacity))
    }
    fn apply(
        &mut self,
        index: usize,
        event: JoinEvent,
        hash: [u8; 32],
    ) -> Result<(), ValidationIssue> {
        let mut row = self.row(index);
        if !row.occupied {
            row = Row {
                occupied: true,
                domain: event.domain,
                hash,
                id: event.id,
                ..Row::default()
            };
        }
        match event.kind {
            JoinKind::Raw { ordinal, kind } => {
                row.raw = row.raw.checked_add(1).ok_or(ValidationIssue::Overflow)?;
                if row.raw == 1 || ordinal >= row.ordinal {
                    row.ordinal = ordinal;
                    row.raw_kind = kind;
                }
            }
            JoinKind::DeclaredId => {
                row.declared = row
                    .declared
                    .checked_add(1)
                    .ok_or(ValidationIssue::Overflow)?
            }
            JoinKind::DeclaredType { kind } => {
                row.maps = row.maps.checked_add(1).ok_or(ValidationIssue::Overflow)?;
                row.map_kind = kind;
            }
            JoinKind::UniqueKey => {
                row.raw = row.raw.checked_add(1).ok_or(ValidationIssue::Overflow)?
            }
        }
        self.put(index, row);
        Ok(())
    }
    fn verify<S: ByteSource + ?Sized>(&self, source: &S) -> Result<(), ValidationIssue> {
        for i in 0..self.capacity {
            let r = self.row(i);
            if !r.occupied {
                continue;
            }
            if r.domain >= 2 {
                if r.raw != 1 || r.declared != 0 || r.maps != 0 {
                    return Err(ValidationIssue::RawRowsDiffer);
                }
            } else if r.raw == 0
                || r.raw != r.declared
                || r.maps != 1
                || compare(source, r.raw_kind, r.map_kind)? != Ordering::Equal
            {
                return Err(ValidationIssue::RawRowsDiffer);
            }
        }
        Ok(())
    }
}
fn key_compare<S: ByteSource + ?Sized>(
    source: &S,
    a: Row,
    b: Row,
) -> Result<Ordering, ValidationIssue> {
    if a.domain != b.domain {
        return Ok(a.domain.cmp(&b.domain));
    }
    compare(source, a.id, b.id)
}
fn digest<S: ByteSource + ?Sized>(
    source: &S,
    event: JoinEvent,
    forced: bool,
) -> Result<[u8; 32], ValidationIssue> {
    if forced {
        return Ok([0; 32]);
    }
    let mut hash = Sha256::new();
    hash.update(event.domain.to_le_bytes());
    let mut decoded = Decoded::new(source, event.id)?;
    while let Some(byte) = decoded.next()? {
        hash.update([byte]);
    }
    Ok(hash.finalize().into())
}
fn matches_prefix(hash: &[u8; 32], prefix: &[u8; 32], bits: usize) -> bool {
    let whole = bits / 8;
    hash[..whole] == prefix[..whole]
        && (bits % 8 == 0 || ((hash[whole] ^ prefix[whole]) & (0xff << (8 - bits % 8))) == 0)
}
fn replay<S: ReplayEvents + ?Sized>(
    source: &S,
    memory: &CleanupMemory,
    stats: &mut RawRowsMatch,
    mut emit: impl FnMut(JoinEvent) -> Result<(), ValidationIssue>,
) -> Result<(), ValidationIssue> {
    stats.passes = stats
        .passes
        .checked_add(1)
        .ok_or(ValidationIssue::Overflow)?;
    source.replay(memory, &mut |event| {
        stats.events = stats
            .events
            .checked_add(1)
            .ok_or(ValidationIssue::Overflow)?;
        emit(event)
    })
}

/// All raw IDs, declared IDs AND declared map keys participate. Extra map-only
/// entries cannot disappear in a hash bucket or collision fallback.
pub(crate) fn join_raw_rows<S: ReplayEvents + ?Sized>(
    source: &S,
    memory: &CleanupMemory,
    capacity: usize,
) -> Result<RawRowsMatch, ValidationIssue> {
    join_inner(source, memory, capacity, false)
}
fn join_inner<S: ReplayEvents + ?Sized>(
    source: &S,
    memory: &CleanupMemory,
    capacity: usize,
    forced: bool,
) -> Result<RawRowsMatch, ValidationIssue> {
    let _floor = memory.reserve(8192)?;
    let mut table = Table::new(memory, capacity)?;
    let mut stats = RawRowsMatch::default();
    let mut prefix = [0u8; 32];
    let mut bits = 0;
    loop {
        table.clear();
        let mut overflow = false;
        let mut first_hash = None;
        let mut all_same_hash = true;
        replay(source, memory, &mut stats, |event| {
            let hash = digest(source, event, forced)?;
            if !matches_prefix(&hash, &prefix, bits) {
                return Ok(());
            }
            if let Some(first) = first_hash {
                all_same_hash &= hash == first;
            } else {
                first_hash = Some(hash);
            }
            match table.find(source, event, hash)? {
                Ok(index) => table.apply(index, event, hash)?,
                Err(index) if index < capacity => table.apply(index, event, hash)?,
                Err(_) => overflow = true,
            }
            Ok(())
        })?;
        if overflow && !all_same_hash && bits < 256 {
            bits += 1;
            prefix[(bits - 1) / 8] &= !(0x80 >> ((bits - 1) % 8));
            continue;
        }
        if overflow {
            // A complete replay observed ONE exact hash for the whole bucket:
            // skip 256 empty sibling scans, while exact spans still separate
            // distinct keys. This is evidence from all events, not a guess.
            let hash = first_hash.ok_or(ValidationIssue::RawRowsDiffer)?;
            collision_batches(source, memory, &mut table, &hash, forced, &mut stats)?;
        } else {
            table.verify(source)?;
        }
        while bits > 0 && (prefix[(bits - 1) / 8] & (0x80 >> ((bits - 1) % 8))) != 0 {
            prefix[(bits - 1) / 8] &= !(0x80 >> ((bits - 1) % 8));
            bits -= 1;
        }
        if bits == 0 {
            break;
        }
        prefix[(bits - 1) / 8] |= 0x80 >> ((bits - 1) % 8);
    }
    stats.lookup_probes = table.probes.get();
    Ok(stats)
}
fn collision_batches<S: ReplayEvents + ?Sized>(
    source: &S,
    memory: &CleanupMemory,
    table: &mut Table,
    prefix: &[u8; 32],
    forced: bool,
    stats: &mut RawRowsMatch,
) -> Result<(), ValidationIssue> {
    let mut lower: Option<Row> = None;
    loop {
        table.clear();
        let mut count = 0;
        replay(source, memory, stats, |event| {
            let hash = digest(source, event, forced)?;
            if &hash != prefix {
                return Ok(());
            }
            let key = Row {
                occupied: true,
                domain: event.domain,
                id: event.id,
                hash,
                ..Row::default()
            };
            if let Some(lower) = lower {
                if key_compare(source, key, lower)? != Ordering::Greater {
                    return Ok(());
                }
            }
            match table.find(source, event, hash)? {
                Ok(_) => {}
                Err(index) if index < table.capacity => {
                    table.put(index, key);
                    count += 1;
                }
                Err(_) => {
                    let mut largest = 0;
                    for i in 1..table.capacity {
                        if key_compare(source, table.row(i), table.row(largest))?
                            == Ordering::Greater
                        {
                            largest = i;
                        }
                    }
                    if key_compare(source, key, table.row(largest))? == Ordering::Less {
                        table.put(largest, key);
                    }
                }
            }
            Ok(())
        })?;
        if count == 0 {
            return Ok(());
        }
        let mut upper = None;
        for i in 0..table.capacity {
            let r = table.row(i);
            if r.occupied {
                let replace = match upper {
                    None => true,
                    Some(u) => key_compare(source, r, u)? == Ordering::Greater,
                };
                if replace {
                    upper = Some(r);
                }
            }
        }
        // A separate replay aggregates only the selected exact keys; selection
        // never loses multiplicity when replacing a temporary largest key.
        replay(source, memory, stats, |event| {
            let hash = digest(source, event, forced)?;
            if &hash == prefix {
                if let Ok(index) = table.find(source, event, hash)? {
                    table.apply(index, event, hash)?;
                }
            }
            Ok(())
        })?;
        table.verify(source)?;
        stats.collision_batches = stats
            .collision_batches
            .checked_add(1)
            .ok_or(ValidationIssue::Overflow)?;
        lower = upper;
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ArtifactFacts {
    pub(crate) key: [u8; 32],
    pub(crate) payload_digest: [u8; 32],
    pub(crate) partition: Span,
    pub(crate) producer_tokens: bool,
    pub(crate) typed_numbers: bool,
    pub(crate) stable_numbers: bool,
    /// Independent ordering fact: later loader compatibility must consult the
    /// actual resolved serde_json Map backend, not infer it from Cargo.lock.
    pub(crate) metadata_keys_sorted: bool,
    pub(crate) declared_nodes: u64,
    pub(crate) declared_edges: u64,
    pub(crate) actual_nodes: u64,
    pub(crate) actual_edges: u64,
    pub(crate) path: Span,
    pub(crate) partition_id: Span,
}
impl Default for ArtifactFacts {
    fn default() -> Self {
        Self {
            key: [0; 32],
            payload_digest: [0; 32],
            partition: Span::default(),
            producer_tokens: true,
            typed_numbers: true,
            stable_numbers: true,
            metadata_keys_sorted: true,
            declared_nodes: 0,
            declared_edges: 0,
            actual_nodes: 0,
            actual_edges: 0,
            path: Span::default(),
            partition_id: Span::default(),
        }
    }
}

/// Replayable, borrowed source adapter. Facts and join results are intentionally
/// separate, and neither is a LoaderUsable or origin capability.
pub(crate) struct ArtifactEvents<'a, S: ?Sized> {
    pub(crate) source: &'a S,
}
impl<S: ByteSource + ?Sized> ByteSource for ArtifactEvents<'_, S> {
    fn len(&self) -> u64 {
        self.source.len()
    }
    fn read_at(&self, o: u64, b: &mut [u8]) -> Result<usize, ValidationIssue> {
        self.source.read_at(o, b)
    }
}
impl<S: ByteSource + ?Sized> ReplayEvents for ArtifactEvents<'_, S> {
    fn replay(
        &self,
        memory: &CleanupMemory,
        emit: &mut dyn FnMut(JoinEvent) -> Result<(), ValidationIssue>,
    ) -> Result<(), ValidationIssue> {
        scan_artifact(self.source, memory, emit).map(|_| ())
    }
}

struct Scanner<'a, S: ?Sized> {
    source: &'a S,
    memory: &'a CleanupMemory,
    reader: MaintenanceJsonReader<SourceReader<'a, S>>,
    peeked: Option<JsonToken>,
    facts: ArtifactFacts,
    emit: &'a mut dyn FnMut(JoinEvent) -> Result<(), ValidationIssue>,
    _floor: MemoryReservation,
}
impl<'a, S: ByteSource + ?Sized> Scanner<'a, S> {
    fn token(&mut self) -> Result<JsonToken, ValidationIssue> {
        self.peeked.take().map(Ok).unwrap_or_else(|| {
            self.reader
                .next_token()
                .map_err(ValidationIssue::from)?
                .ok_or(ValidationIssue::Schema)
        })
    }
    fn peek(&mut self) -> Result<JsonToken, ValidationIssue> {
        if self.peeked.is_none() {
            self.peeked = Some(self.reader.next_token()?.ok_or(ValidationIssue::Schema)?);
        }
        Ok(self.peeked.unwrap())
    }
    fn expect(&mut self, kind: TokenKind) -> Result<JsonToken, ValidationIssue> {
        let t = self.token()?;
        if t.kind != kind {
            return Err(ValidationIssue::Schema);
        }
        Ok(t)
    }
    fn string(&mut self) -> Result<Span, ValidationIssue> {
        let t = self.token()?;
        if !matches!(t.kind, TokenKind::String { key: false, .. }) {
            return Err(ValidationIssue::Schema);
        }
        let span = Span {
            start: t.start,
            end: t.end,
        };
        self.facts.producer_tokens &= producer_string(self.source, span)?;
        Ok(span)
    }
    fn optional_string(&mut self) -> Result<(), ValidationIssue> {
        if self.peek()?.kind == TokenKind::Null {
            self.token()?;
            self.facts.producer_tokens = false;
        } else {
            self.string()?;
        }
        Ok(())
    }
    fn raw_number(&self, span: Span, output: &mut [u8; 64]) -> Result<usize, ValidationIssue> {
        let len = usize::try_from(span.len()?).map_err(|_| ValidationIssue::Numeric)?;
        if len > 64 {
            return Err(ValidationIssue::Numeric);
        }
        let mut reader = SpanReader::new(self.source, span)?;
        for b in &mut output[..len] {
            *b = reader.required()?;
        }
        Ok(len)
    }
    fn number(&mut self, kind: NumberKind, nullable: bool) -> Result<Option<u64>, ValidationIssue> {
        if self.peek()?.kind == TokenKind::Null {
            self.token()?;
            if !nullable {
                self.facts.typed_numbers = false;
                self.facts.stable_numbers = false;
            }
            return Ok(None);
        }
        let t = self.expect(TokenKind::Number)?;
        let mut bytes = [0u8; 64];
        let len = self.raw_number(
            Span {
                start: t.start,
                end: t.end,
            },
            &mut bytes,
        )?;
        let facts = number_facts(&bytes[..len], kind, self.memory)?;
        self.facts.typed_numbers &= facts.typed;
        self.facts.stable_numbers &= facts.stable;
        if kind == NumberKind::U64 {
            return serde_json::from_slice(&bytes[..len])
                .map(Some)
                .map_err(|_| ValidationIssue::Numeric);
        }
        Ok(None)
    }
    fn object(
        &mut self,
        names: &[&str],
        required: u64,
        mut field: impl FnMut(&mut Self, usize) -> Result<(), ValidationIssue>,
    ) -> Result<(), ValidationIssue> {
        self.expect(TokenKind::ObjectStart)?;
        let mut seen = 0u64;
        let mut previous = None;
        while self.peek()?.kind != TokenKind::ObjectEnd {
            let key = self.token()?;
            if !matches!(key.kind, TokenKind::String { key: true, .. }) {
                return Err(ValidationIssue::Schema);
            }
            let span = Span {
                start: key.start,
                end: key.end,
            };
            self.facts.producer_tokens &= producer_string(self.source, span)?;
            let mut index = None;
            for (i, name) in names.iter().enumerate() {
                if equals(self.source, span, name.as_bytes())? {
                    index = Some(i);
                    break;
                }
            }
            let index = index.ok_or(ValidationIssue::Schema)?;
            if seen & (1 << index) != 0 {
                return Err(ValidationIssue::NonProducer);
            }
            if previous.is_some_and(|p| index < p) {
                self.facts.producer_tokens = false;
            }
            previous = Some(index);
            seen |= 1 << index;
            field(self, index)?;
        }
        self.token()?;
        if seen & required != required {
            return Err(ValidationIssue::Schema);
        }
        Ok(())
    }
    fn hex_string(&mut self) -> Result<[u8; 32], ValidationIssue> {
        let span = self.string()?;
        let mut decoded = Decoded::new(self.source, span)?;
        let mut result = [0u8; 32];
        for b in &mut result {
            let mut value = 0;
            for _ in 0..2 {
                let d = decoded.next()?.ok_or(ValidationIssue::Schema)?;
                value = value * 16
                    + match d {
                        b'0'..=b'9' => d - b'0',
                        b'a'..=b'f' => d - b'a' + 10,
                        _ => return Err(ValidationIssue::Schema),
                    };
            }
            *b = value;
        }
        if decoded.next()?.is_some() {
            return Err(ValidationIssue::Schema);
        }
        Ok(result)
    }
    fn metadata(&mut self) -> Result<(), ValidationIssue> {
        let mut objects = [None; 127];
        let _previous_key_floor = self
            .memory
            .reserve(std::mem::size_of::<[Option<Span>; 127]>())?;
        let mut previous = [None; 127];
        let mut depth = 0usize;
        loop {
            let t = self.token()?;
            match t.kind {
                TokenKind::ObjectStart | TokenKind::ArrayStart => {
                    previous[depth] = None;
                    objects[depth] = if t.kind == TokenKind::ObjectStart {
                        Some(t.start.checked_add(2).ok_or(ValidationIssue::Overflow)?)
                    } else {
                        None
                    };
                    depth += 1;
                }
                TokenKind::ObjectEnd | TokenKind::ArrayEnd => {
                    depth -= 1;
                }
                TokenKind::String { key, .. } => {
                    let span = Span {
                        start: t.start,
                        end: t.end,
                    };
                    self.facts.producer_tokens &= producer_string(self.source, span)?;
                    if key {
                        let domain = objects[depth - 1].ok_or(ValidationIssue::Schema)?;
                        if let Some(old) = previous[depth - 1] {
                            self.facts.metadata_keys_sorted &=
                                compare(self.source, old, span)? == Ordering::Less;
                        }
                        previous[depth - 1] = Some(span);
                        (self.emit)(JoinEvent {
                            domain,
                            id: span,
                            kind: JoinKind::UniqueKey,
                        })?;
                    }
                }
                TokenKind::Number => {
                    let mut bytes = [0u8; 64];
                    let n = self.raw_number(
                        Span {
                            start: t.start,
                            end: t.end,
                        },
                        &mut bytes,
                    )?;
                    let f = number_facts(&bytes[..n], NumberKind::Value, self.memory)?;
                    self.facts.typed_numbers &= f.typed;
                    self.facts.stable_numbers &= f.stable;
                }
                _ => {}
            }
            if depth == 0 {
                break;
            }
        }
        Ok(())
    }
    fn ids(&mut self, domain: u64) -> Result<(), ValidationIssue> {
        self.expect(TokenKind::ArrayStart)?;
        let mut previous = None;
        let mut count = 0u64;
        while self.peek()?.kind != TokenKind::ArrayEnd {
            let id = self.string()?;
            if let Some(old) = previous {
                if compare(self.source, old, id)? == Ordering::Greater {
                    return Err(ValidationIssue::RawRowsDiffer);
                }
            }
            previous = Some(id);
            (self.emit)(JoinEvent {
                domain,
                id,
                kind: JoinKind::DeclaredId,
            })?;
            count = count.checked_add(1).ok_or(ValidationIssue::Overflow)?;
        }
        self.token()?;
        if count == 0 {
            self.facts.producer_tokens = false;
        }
        Ok(())
    }
    fn types(&mut self, domain: u64) -> Result<(), ValidationIssue> {
        self.expect(TokenKind::ObjectStart)?;
        let mut previous = None;
        let mut count = 0u64;
        while self.peek()?.kind != TokenKind::ObjectEnd {
            let key = self.token()?;
            if !matches!(key.kind, TokenKind::String { key: true, .. }) {
                return Err(ValidationIssue::Schema);
            }
            let id = Span {
                start: key.start,
                end: key.end,
            };
            self.facts.producer_tokens &= producer_string(self.source, id)?;
            if let Some(old) = previous {
                if compare(self.source, old, id)? != Ordering::Less {
                    return Err(ValidationIssue::NonProducer);
                }
            }
            previous = Some(id);
            let kind = self.string()?;
            (self.emit)(JoinEvent {
                domain,
                id,
                kind: JoinKind::DeclaredType { kind },
            })?;
            count += 1;
        }
        self.token()?;
        if count == 0 {
            self.facts.producer_tokens = false;
        }
        Ok(())
    }
    fn entry(&mut self) -> Result<(), ValidationIssue> {
        const N: &[&str] = &[
            "path",
            "content_hash",
            "language",
            "partition_id",
            "artifact_key",
            "node_ids",
            "edge_ids",
            "node_types",
            "edge_types",
            "node_count",
            "edge_count",
            "materialized_at",
        ];
        self.object(
            N,
            0b111 | (1 << 3) | (1 << 4) | (1 << 9) | (1 << 10) | (1 << 11),
            |s, i| {
                match i {
                    0 => s.facts.path = s.string()?,
                    1 | 2 | 3 | 11 => {
                        let span = s.string()?;
                        if i == 3 {
                            s.facts.partition_id = span;
                        }
                    }
                    4 => {
                        let key = s.hex_string()?;
                        if key != s.facts.key {
                            return Err(ValidationIssue::Schema);
                        }
                    }
                    5 => s.ids(0)?,
                    6 => s.ids(1)?,
                    7 => s.types(0)?,
                    8 => s.types(1)?,
                    9 => {
                        s.facts.declared_nodes = s
                            .number(NumberKind::U64, false)?
                            .ok_or(ValidationIssue::Numeric)?
                    }
                    10 => {
                        s.facts.declared_edges = s
                            .number(NumberKind::U64, false)?
                            .ok_or(ValidationIssue::Numeric)?
                    }
                    _ => unreachable!(),
                }
                Ok(())
            },
        )
    }
    fn row(&mut self, edge: bool, ordinal: u64) -> Result<(), ValidationIssue> {
        const NODE: &[&str] = &[
            "id",
            "table",
            "label",
            "kind",
            "language",
            "grammar_version",
            "path",
            "qualified_name",
            "scope_id",
            "line_start",
            "line_end",
            "byte_start",
            "byte_end",
            "tree_sitter_node_type",
            "capture_name",
            "summary",
            "metadata",
        ];
        const EDGE: &[&str] = &[
            "id",
            "edge_type",
            "source_id",
            "target_id",
            "kind",
            "confidence",
            "field_name",
            "child_index",
            "line_start",
            "line_end",
            "byte_start",
            "byte_end",
            "metadata",
        ];
        let names = if edge { EDGE } else { NODE };
        let optional = if edge { (1 << 6) | (1 << 7) } else { 1 << 5 };
        let required = ((1u64 << names.len()) - 1) & !optional;
        let mut id = None;
        let mut kind = None;
        self.object(names, required, |s, i| {
            if i == 0 {
                id = Some(s.string()?);
            } else if i == 1 {
                kind = Some(s.string()?);
            } else if (edge && i == 12) || (!edge && i == 16) {
                s.metadata()?;
            } else if edge && i == 5 {
                s.number(NumberKind::F64, false)?;
            } else if (edge && (7..=11).contains(&i)) || (!edge && (9..=12).contains(&i)) {
                if edge && i == 7 && s.peek()?.kind == TokenKind::Null {
                    s.facts.producer_tokens = false;
                }
                s.number(NumberKind::I64, true)?;
            } else if (edge && i == 6) || (!edge && i == 5) {
                s.optional_string()?;
            } else {
                s.string()?;
            }
            Ok(())
        })?;
        (self.emit)(JoinEvent {
            domain: u64::from(edge),
            id: id.ok_or(ValidationIssue::Schema)?,
            kind: JoinKind::Raw {
                ordinal,
                kind: kind.ok_or(ValidationIssue::Schema)?,
            },
        })
    }
    fn rows(&mut self, edge: bool) -> Result<u64, ValidationIssue> {
        self.expect(TokenKind::ArrayStart)?;
        let mut count = 0u64;
        while self.peek()?.kind != TokenKind::ArrayEnd {
            self.row(edge, count)?;
            count = count.checked_add(1).ok_or(ValidationIssue::Overflow)?;
        }
        self.token()?;
        Ok(count)
    }
    fn partition(&mut self) -> Result<(), ValidationIssue> {
        let start = self.peek()?.start;
        self.object(&["entry", "nodes", "edges"], 7, |s, i| {
            match i {
                0 => s.entry()?,
                1 => s.facts.actual_nodes = s.rows(false)?,
                2 => s.facts.actual_edges = s.rows(true)?,
                _ => unreachable!(),
            }
            Ok(())
        })?;
        let end = self.reader.position();
        self.facts.partition = Span { start, end };
        Ok(())
    }
}

pub(crate) fn scan_artifact<S: ByteSource + ?Sized>(
    source: &S,
    memory: &CleanupMemory,
    emit: &mut dyn FnMut(JoinEvent) -> Result<(), ValidationIssue>,
) -> Result<ArtifactFacts, ValidationIssue> {
    let floor = memory.reserve(8192)?;
    let reader = MaintenanceJsonReader::new(SourceReader { source, offset: 0 }, memory)?;
    let mut scanner = Scanner {
        source,
        memory,
        reader,
        peeked: None,
        facts: ArtifactFacts::default(),
        emit,
        _floor: floor,
    };
    scanner.object(
        &[
            "format_version",
            "artifact_key",
            "partition_payload_sha256",
            "partition",
        ],
        15,
        |s, i| {
            match i {
                0 => {
                    if s.number(NumberKind::U64, false)? != Some(2) {
                        return Err(ValidationIssue::Schema);
                    }
                }
                1 => s.facts.key = s.hex_string()?,
                2 => s.facts.payload_digest = s.hex_string()?,
                3 => s.partition()?,
                _ => unreachable!(),
            }
            Ok(())
        },
    )?;
    if scanner.reader.next_token()?.is_some() {
        return Err(ValidationIssue::Schema);
    }
    if scanner.facts.declared_nodes != scanner.facts.actual_nodes
        || scanner.facts.declared_edges != scanner.facts.actual_edges
    {
        return Err(ValidationIssue::RawRowsDiffer);
    }
    if !partition_id_matches(source, scanner.facts.path, scanner.facts.partition_id)? {
        return Err(ValidationIssue::RawRowsDiffer);
    }
    usize::try_from(scanner.facts.declared_nodes).map_err(|_| ValidationIssue::Numeric)?;
    usize::try_from(scanner.facts.declared_edges).map_err(|_| ValidationIssue::Numeric)?;
    Ok(scanner.facts)
}

/// Whitespace-compacted wire checksum fact. Typed loader equivalence also needs
/// stable scalar forms, complete schema/defaults, the actual loader Map order,
/// and raw-row equality. Cargo.lock's indexmap alone does not establish that
/// the root runtime (rather than a build dependency) enables preserve_order.
pub(crate) fn wire_checksum_matches<S: ByteSource + ?Sized>(
    source: &S,
    facts: &ArtifactFacts,
    memory: &CleanupMemory,
) -> Result<bool, ValidationIssue> {
    let reader = MaintenanceJsonReader::new(SpanReader::new(source, facts.partition)?, memory)?;
    Ok(reader.finish()?.1 == facts.payload_digest)
}

fn sha1_block(state: &mut [u32; 5], block: &[u8; 64]) {
    let mut words = [0u32; 80];
    for (i, word) in words[..16].iter_mut().enumerate() {
        *word = u32::from_be_bytes(block[i * 4..i * 4 + 4].try_into().unwrap());
    }
    for i in 16..80 {
        words[i] = (words[i - 3] ^ words[i - 8] ^ words[i - 14] ^ words[i - 16]).rotate_left(1);
    }
    let [mut a, mut b, mut c, mut d, mut e] = *state;
    for (i, word) in words.iter().enumerate() {
        let (f, k) = match i {
            0..=19 => ((b & c) | (!b & d), 0x5a827999),
            20..=39 => (b ^ c ^ d, 0x6ed9eba1),
            40..=59 => ((b & c) | (b & d) | (c & d), 0x8f1bbcdc),
            _ => (b ^ c ^ d, 0xca62c1d6),
        };
        let temp = a
            .rotate_left(5)
            .wrapping_add(f)
            .wrapping_add(e)
            .wrapping_add(k)
            .wrapping_add(*word);
        e = d;
        d = c;
        c = b.rotate_left(30);
        b = a;
        a = temp;
    }
    for (s, v) in state.iter_mut().zip([a, b, c, d, e]) {
        *s = s.wrapping_add(v);
    }
}
fn partition_id_matches<S: ByteSource + ?Sized>(
    source: &S,
    path: Span,
    id: Span,
) -> Result<bool, ValidationIssue> {
    let mut state = [0x67452301, 0xefcdab89, 0x98badcfe, 0x10325476, 0xc3d2e1f0];
    let mut block = [0u8; 64];
    let mut used = 0;
    let mut len = 0u64;
    let mut input = Decoded::new(source, path)?;
    while let Some(byte) = input.next()? {
        block[used] = byte;
        used += 1;
        len = len.checked_add(1).ok_or(ValidationIssue::Overflow)?;
        if used == 64 {
            sha1_block(&mut state, &block);
            used = 0;
        }
    }
    let bit_len = len.checked_mul(8).ok_or(ValidationIssue::Overflow)?;
    block[used] = 0x80;
    used += 1;
    if used > 56 {
        block[used..].fill(0);
        sha1_block(&mut state, &block);
        block.fill(0);
    } else {
        block[used..56].fill(0);
    }
    block[56..].copy_from_slice(&bit_len.to_be_bytes());
    sha1_block(&mut state, &block);
    let mut expected = [0u8; 20];
    for i in 0..10 {
        let byte = state[i / 4].to_be_bytes()[i % 4];
        expected[2 * i] = b"0123456789abcdef"[(byte >> 4) as usize];
        expected[2 * i + 1] = b"0123456789abcdef"[(byte & 15) as usize];
    }
    equals(source, id, &expected)
}

/// Fixed, caller-owned key addresses (including source incarnation) only. A
/// replay must include every expected/native selected ID, including extras.
#[derive(Debug, Clone, Copy)]
pub(crate) struct WitnessEvent<K: Copy> {
    pub(crate) domain: u64,
    pub(crate) native: bool,
    /// Producer source order; lowest nonempty field wins independently.
    pub(crate) ordinal: u64,
    pub(crate) fields: [Option<K>; 5],
}
pub(crate) trait ReplayWitnesses {
    type Key: Copy;
    /// Validate each complete row/key before emission, including later
    /// duplicate fields that FIRSTNONEMPTY will not retain.
    fn replay(
        &self,
        memory: &CleanupMemory,
        emit: &mut dyn FnMut(WitnessEvent<Self::Key>) -> Result<(), ValidationIssue>,
    ) -> Result<(), ValidationIssue>;
    /// Validate complete decoded keys, including suffixes, in all three calls.
    /// Reopening must check the captured incarnation, never refresh its stamp.
    fn hash(
        &self,
        key: Self::Key,
        domain: u64,
        memory: &CleanupMemory,
    ) -> Result<[u8; 32], ValidationIssue>;
    fn compare(
        &self,
        left: Self::Key,
        right: Self::Key,
        memory: &CleanupMemory,
    ) -> Result<Ordering, ValidationIssue>;
    fn empty(&self, key: Self::Key, memory: &CleanupMemory) -> Result<bool, ValidationIssue>;
}
#[derive(Clone, Copy)]
struct WitnessSlot<K: Copy> {
    key: Option<K>,
    domain: u64,
    hash: [u8; 32],
    native_count: u64,
    expected_count: u64,
    native: [Option<K>; 4],
    expected: [Option<K>; 4],
    ordinal: [u64; 4],
}
impl<K: Copy> WitnessSlot<K> {
    fn empty() -> Self {
        Self {
            key: None,
            domain: 0,
            hash: [0; 32],
            native_count: 0,
            expected_count: 0,
            native: [None; 4],
            expected: [None; 4],
            ordinal: [u64::MAX; 4],
        }
    }
}
struct WitnessTable<K: Copy> {
    pointer: std::ptr::NonNull<WitnessSlot<K>>,
    capacity: usize,
    layout: std::alloc::Layout,
    probes: Cell<u64>,
    _charge: MemoryReservation,
}
impl<K: Copy> WitnessTable<K> {
    fn new(memory: &CleanupMemory, capacity: usize) -> Result<Self, ValidationIssue> {
        if capacity == 0 {
            return Err(ValidationIssue::Resource(ResourceLimit::CeilingExceeded));
        }
        let layout = std::alloc::Layout::array::<WitnessSlot<K>>(capacity)
            .map_err(|_| ValidationIssue::Overflow)?;
        let charge = memory.reserve(layout.size())?;
        // SAFETY: the exact typed layout is precharged; every slot is initialized
        // before reading and no references escape table mutation or disposal.
        let pointer =
            std::ptr::NonNull::new(unsafe { std::alloc::alloc(layout) }.cast::<WitnessSlot<K>>())
                .ok_or(ValidationIssue::Resource(ResourceLimit::AllocationFailed))?;
        let mut table = Self {
            pointer,
            capacity,
            layout,
            probes: Cell::new(0),
            _charge: charge,
        };
        table.clear();
        Ok(table)
    }
    fn clear(&mut self) {
        for i in 0..self.capacity {
            self.put(i, WitnessSlot::empty());
        }
    }
    fn get(&self, i: usize) -> WitnessSlot<K> {
        assert!(i < self.capacity);
        // SAFETY: i is bounded and every allocated slot was initialized.
        unsafe { self.pointer.as_ptr().add(i).read() }
    }
    fn put(&mut self, i: usize, row: WitnessSlot<K>) {
        assert!(i < self.capacity);
        // SAFETY: i is bounded; Copy slots own no destructors and no borrow escapes.
        unsafe {
            self.pointer.as_ptr().add(i).write(row);
        }
    }
    fn find<R: ReplayWitnesses<Key = K> + ?Sized>(
        &self,
        r: &R,
        memory: &CleanupMemory,
        event: WitnessEvent<K>,
        hash: [u8; 32],
    ) -> Result<Result<usize, usize>, ValidationIssue> {
        let key = event.fields[0].ok_or(ValidationIssue::Schema)?;
        let start =
            (u64::from_le_bytes(hash[..8].try_into().unwrap()) % self.capacity as u64) as usize;
        for delta in 0..self.capacity {
            self.probes.set(
                self.probes
                    .get()
                    .checked_add(1)
                    .ok_or(ValidationIssue::Overflow)?,
            );
            let i = (start + delta) % self.capacity;
            let slot = self.get(i);
            let Some(old) = slot.key else {
                return Ok(Err(i));
            };
            if slot.domain == event.domain
                && slot.hash == hash
                && r.compare(old, key, memory)? == Ordering::Equal
            {
                return Ok(Ok(i));
            }
        }
        Ok(Err(self.capacity))
    }
    fn apply<R: ReplayWitnesses<Key = K> + ?Sized>(
        &mut self,
        r: &R,
        memory: &CleanupMemory,
        i: usize,
        event: WitnessEvent<K>,
        hash: [u8; 32],
    ) -> Result<(), ValidationIssue> {
        let mut slot = self.get(i);
        if slot.key.is_none() {
            slot.key = Some(event.fields[0].ok_or(ValidationIssue::Schema)?);
            slot.domain = event.domain;
            slot.hash = hash;
        }
        if event.native {
            slot.native_count = slot
                .native_count
                .checked_add(1)
                .ok_or(ValidationIssue::Overflow)?;
            slot.native.copy_from_slice(&event.fields[1..]);
        } else {
            slot.expected_count = slot
                .expected_count
                .checked_add(1)
                .ok_or(ValidationIssue::Overflow)?;
            for j in 0..4 {
                if let Some(key) = event.fields[j + 1] {
                    if (slot.expected[j].is_none() || event.ordinal < slot.ordinal[j])
                        && !r.empty(key, memory)?
                    {
                        slot.expected[j] = Some(key);
                        slot.ordinal[j] = event.ordinal;
                    }
                }
            }
        }
        self.put(i, slot);
        Ok(())
    }
    fn verify<R: ReplayWitnesses<Key = K> + ?Sized>(
        &self,
        r: &R,
        memory: &CleanupMemory,
    ) -> Result<(), ValidationIssue> {
        for i in 0..self.capacity {
            let slot = self.get(i);
            if slot.key.is_none() {
                continue;
            }
            if slot.native_count != 1 || slot.expected_count == 0 {
                return Err(ValidationIssue::RawRowsDiffer);
            }
            for j in 0..4 {
                let native = slot.native[j].ok_or(ValidationIssue::RawRowsDiffer)?;
                let equal = if let Some(expected) = slot.expected[j] {
                    r.compare(native, expected, memory)? == Ordering::Equal
                } else {
                    r.empty(native, memory)?
                };
                if !equal {
                    return Err(ValidationIssue::RawRowsDiffer);
                }
            }
        }
        Ok(())
    }
}
impl<K: Copy> Drop for WitnessTable<K> {
    fn drop(&mut self) {
        // SAFETY: this owner holds the original pointer and exact allocation
        // layout; Copy slots need no element drops. The charge drops afterward.
        unsafe {
            std::alloc::dealloc(self.pointer.as_ptr().cast(), self.layout);
        }
    }
}
fn witness_order<R: ReplayWitnesses + ?Sized>(
    r: &R,
    memory: &CleanupMemory,
    left: (u64, R::Key),
    right: (u64, R::Key),
) -> Result<Ordering, ValidationIssue> {
    if left.0 != right.0 {
        Ok(left.0.cmp(&right.0))
    } else {
        r.compare(left.1, right.1, memory)
    }
}
/// Exact requested join layout, excluding the caller's source/resolver and
/// decoder owners. Use the real retained Key type, including its full stamp.
pub(crate) fn witness_join_requested_bytes<K: Copy>(
    capacity: usize,
) -> Result<usize, ValidationIssue> {
    if capacity == 0 {
        return Err(ValidationIssue::Resource(ResourceLimit::CeilingExceeded));
    }
    let floor = std::mem::size_of::<WitnessTable<K>>()
        .checked_add(
            std::mem::size_of::<WitnessSlot<K>>()
                .checked_mul(6)
                .ok_or(ValidationIssue::Overflow)?,
        )
        .and_then(|n| n.checked_add(std::mem::size_of::<WitnessEvent<K>>() * 3))
        .and_then(|n| n.checked_add(1024))
        .ok_or(ValidationIssue::Overflow)?;
    let layout = std::alloc::Layout::array::<WitnessSlot<K>>(capacity)
        .map_err(|_| ValidationIssue::Overflow)?;
    floor
        .checked_add(layout.size())
        .ok_or(ValidationIssue::Overflow)
}
/// Hash partitioning handles ordinary inventories; complete-hash collisions use
/// finite lexicographic batches and exact equality. Capacity one makes progress.
/// No key/source inventory, file registry, input-sized decoder or spill is owned.
pub(crate) fn join_selected_witnesses<R: ReplayWitnesses + ?Sized>(
    r: &R,
    memory: &CleanupMemory,
    capacity: usize,
) -> Result<RawRowsMatch, ValidationIssue> {
    let total = witness_join_requested_bytes::<R::Key>(capacity)?;
    let slots = std::alloc::Layout::array::<WitnessSlot<R::Key>>(capacity)
        .map_err(|_| ValidationIssue::Overflow)?
        .size();
    let floor = total.checked_sub(slots).ok_or(ValidationIssue::Overflow)?;
    let _floor = memory.reserve(floor)?;
    let mut table = WitnessTable::new(memory, capacity)?;
    let mut stats = RawRowsMatch::default();
    let mut prefix = [0; 32];
    let mut bits = 0;
    loop {
        table.clear();
        let mut overflow = false;
        let mut first = None;
        let mut same = true;
        stats.passes = stats
            .passes
            .checked_add(1)
            .ok_or(ValidationIssue::Overflow)?;
        r.replay(memory, &mut |event| {
            stats.events = stats
                .events
                .checked_add(1)
                .ok_or(ValidationIssue::Overflow)?;
            let hash = r.hash(
                event.fields[0].ok_or(ValidationIssue::Schema)?,
                event.domain,
                memory,
            )?;
            if !matches_prefix(&hash, &prefix, bits) {
                return Ok(());
            }
            if let Some(h) = first {
                same &= h == hash;
            } else {
                first = Some(hash);
            }
            match table.find(r, memory, event, hash)? {
                Ok(i) => table.apply(r, memory, i, event, hash)?,
                Err(i) if i < capacity => table.apply(r, memory, i, event, hash)?,
                _ => overflow = true,
            }
            Ok(())
        })?;
        if overflow && !same && bits < 256 {
            bits += 1;
            prefix[(bits - 1) / 8] &= !(0x80 >> ((bits - 1) % 8));
            continue;
        }
        if overflow {
            let bucket = first.ok_or(ValidationIssue::Schema)?;
            let mut lower = None;
            loop {
                table.clear();
                stats.passes = stats
                    .passes
                    .checked_add(1)
                    .ok_or(ValidationIssue::Overflow)?;
                let mut selected = 0usize;
                r.replay(memory, &mut |event| {
                    stats.events = stats
                        .events
                        .checked_add(1)
                        .ok_or(ValidationIssue::Overflow)?;
                    let key = event.fields[0].ok_or(ValidationIssue::Schema)?;
                    let hash = r.hash(key, event.domain, memory)?;
                    if hash != bucket {
                        return Ok(());
                    }
                    if let Some(bound) = lower {
                        if witness_order(r, memory, (event.domain, key), bound)?
                            != Ordering::Greater
                        {
                            return Ok(());
                        }
                    }
                    for i in 0..selected {
                        table.probes.set(
                            table
                                .probes
                                .get()
                                .checked_add(1)
                                .ok_or(ValidationIssue::Overflow)?,
                        );
                        let old = table.get(i);
                        if old.domain == event.domain
                            && r.compare(old.key.unwrap(), key, memory)? == Ordering::Equal
                        {
                            return Ok(());
                        }
                    }
                    if selected < capacity {
                        let mut slot = WitnessSlot::empty();
                        slot.key = Some(key);
                        slot.domain = event.domain;
                        slot.hash = hash;
                        table.put(selected, slot);
                        selected += 1;
                    } else {
                        let mut largest = 0;
                        for i in 1..capacity {
                            let a = table.get(i);
                            let b = table.get(largest);
                            if witness_order(
                                r,
                                memory,
                                (a.domain, a.key.unwrap()),
                                (b.domain, b.key.unwrap()),
                            )? == Ordering::Greater
                            {
                                largest = i;
                            }
                        }
                        let old = table.get(largest);
                        if witness_order(
                            r,
                            memory,
                            (event.domain, key),
                            (old.domain, old.key.unwrap()),
                        )? == Ordering::Less
                        {
                            let mut slot = WitnessSlot::empty();
                            slot.key = Some(key);
                            slot.domain = event.domain;
                            slot.hash = hash;
                            table.put(largest, slot);
                        }
                    }
                    Ok(())
                })?;
                if selected == 0 {
                    break;
                }
                // Selection uses dense slots, so aggregate by exact scan rather
                // than open addressing (replacement would introduce holes).
                stats.passes = stats
                    .passes
                    .checked_add(1)
                    .ok_or(ValidationIssue::Overflow)?;
                r.replay(memory, &mut |event| {
                    stats.events = stats
                        .events
                        .checked_add(1)
                        .ok_or(ValidationIssue::Overflow)?;
                    let key = event.fields[0].ok_or(ValidationIssue::Schema)?;
                    let hash = r.hash(key, event.domain, memory)?;
                    if hash != bucket {
                        return Ok(());
                    }
                    for i in 0..selected {
                        table.probes.set(
                            table
                                .probes
                                .get()
                                .checked_add(1)
                                .ok_or(ValidationIssue::Overflow)?,
                        );
                        let slot = table.get(i);
                        if slot.domain == event.domain
                            && r.compare(slot.key.unwrap(), key, memory)? == Ordering::Equal
                        {
                            table.apply(r, memory, i, event, hash)?;
                            break;
                        }
                    }
                    Ok(())
                })?;
                table.verify(r, memory)?;
                let mut largest = table.get(0);
                for i in 1..selected {
                    let slot = table.get(i);
                    if witness_order(
                        r,
                        memory,
                        (slot.domain, slot.key.unwrap()),
                        (largest.domain, largest.key.unwrap()),
                    )? == Ordering::Greater
                    {
                        largest = slot;
                    }
                }
                lower = Some((largest.domain, largest.key.unwrap()));
                stats.collision_batches = stats
                    .collision_batches
                    .checked_add(1)
                    .ok_or(ValidationIssue::Overflow)?;
            }
        } else {
            table.verify(r, memory)?;
        }
        while bits > 0 && (prefix[(bits - 1) / 8] & (0x80 >> ((bits - 1) % 8))) != 0 {
            prefix[(bits - 1) / 8] &= !(0x80 >> ((bits - 1) % 8));
            bits -= 1;
        }
        if bits == 0 {
            break;
        }
        prefix[(bits - 1) / 8] |= 0x80 >> ((bits - 1) % 8);
    }
    stats.lookup_probes = table.probes.get();
    Ok(stats)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::partition_builder::GraphPartition;
    use crate::protocol::ManifestEntry;
    use crate::syntax_materializer::{GraphEdgeRow, GraphNodeRow};
    use std::collections::BTreeMap;
    use std::io::{Seek, SeekFrom, Write};
    struct TestFile {
        path: std::path::PathBuf,
        file: Option<File>,
    }
    impl TestFile {
        fn new(bytes: &[u8]) -> Self {
            static SEQUENCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let nonce = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let seq = SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "cleanup-validation-source-{}-{nonce}-{seq}",
                std::process::id()
            ));
            let mut file = std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .create_new(true)
                .open(&path)
                .unwrap();
            file.write_all(bytes).unwrap();
            file.sync_all().unwrap();
            Self {
                path,
                file: Some(file),
            }
        }
    }
    impl Drop for TestFile {
        fn drop(&mut self) {
            drop(self.file.take());
            let _ = std::fs::remove_file(&self.path);
        }
    }
    fn node(id: &str, kind: &str) -> GraphNodeRow {
        GraphNodeRow {
            id: id.into(),
            table: kind.into(),
            label: "label".into(),
            kind: "definition".into(),
            language: "rust".into(),
            grammar_version: None,
            path: "src/lib.rs".into(),
            qualified_name: id.into(),
            scope_id: "scope".into(),
            line_start: None,
            line_end: None,
            byte_start: None,
            byte_end: None,
            tree_sitter_node_type: "identifier".into(),
            capture_name: "capture".into(),
            summary: "summary".into(),
            metadata: serde_json::json!({"z":1,"a":["text",null]}),
        }
    }
    fn edge(id: &str, kind: &str, confidence: f64) -> GraphEdgeRow {
        GraphEdgeRow {
            id: id.into(),
            edge_type: kind.into(),
            source_id: "n1".into(),
            target_id: "n2".into(),
            kind: "reference".into(),
            confidence,
            field_name: None,
            child_index: None,
            line_start: None,
            line_end: None,
            byte_start: None,
            byte_end: None,
            metadata: serde_json::json!({"z":true,"a":false}),
        }
    }
    fn partition(nodes: Vec<GraphNodeRow>, edges: Vec<GraphEdgeRow>) -> GraphPartition {
        let mut node_ids: Vec<_> = nodes.iter().map(|r| r.id.clone()).collect();
        node_ids.sort();
        let mut edge_ids: Vec<_> = edges.iter().map(|r| r.id.clone()).collect();
        edge_ids.sort();
        let entry = ManifestEntry {
            path: "src/lib.rs".into(),
            content_hash: "hash".into(),
            language: "rust".into(),
            partition_id: crate::hash::partition_id("src/lib.rs"),
            artifact_key: Some("a".repeat(64)),
            node_ids,
            edge_ids,
            node_types: nodes
                .iter()
                .map(|r| (r.id.clone(), r.table.clone()))
                .collect::<BTreeMap<_, _>>(),
            edge_types: edges
                .iter()
                .map(|r| (r.id.clone(), r.edge_type.clone()))
                .collect::<BTreeMap<_, _>>(),
            node_count: nodes.len(),
            edge_count: edges.len(),
            materialized_at: "stamp".into(),
        };
        GraphPartition {
            entry,
            nodes,
            edges,
        }
    }
    fn envelope(partition: &GraphPartition) -> Vec<u8> {
        #[derive(serde::Serialize)]
        struct Envelope<'a> {
            format_version: u64,
            artifact_key: &'a str,
            partition_payload_sha256: String,
            partition: &'a GraphPartition,
        }
        let checksum = Sha256::digest(serde_json::to_vec(partition).unwrap());
        serde_json::to_vec_pretty(&Envelope {
            format_version: 2,
            artifact_key: partition.entry.artifact_key.as_deref().unwrap(),
            partition_payload_sha256: checksum.iter().map(|b| format!("{b:02x}")).collect(),
            partition,
        })
        .unwrap()
    }
    fn facts(input: &[u8], memory: &CleanupMemory) -> Result<ArtifactFacts, ValidationIssue> {
        scan_artifact(input, memory, &mut |_| Ok(()))
    }
    fn joined(partition: &GraphPartition, capacity: usize, force: bool) -> bool {
        let bytes = envelope(partition);
        let memory = CleanupMemory::new(131072).unwrap();
        join_inner(
            &ArtifactEvents {
                source: bytes.as_slice(),
            },
            &memory,
            capacity,
            force,
        )
        .is_ok()
    }

    #[test]
    fn dc_validation_exact_join_matches_raw_rows_duplicates_last_wins_and_extras() {
        let p = partition(
            vec![node("same", "Old"), node("z", "Z"), node("same", "New")],
            vec![edge("same-edge", "Old", 0.5), edge("same-edge", "New", 1.0)],
        );
        assert!(p.validate_raw_rows().is_ok());
        for forced in [false, true] {
            for capacity in [1, 2, 32] {
                assert!(joined(&p, capacity, forced));
                let mut bad = p.clone();
                bad.entry
                    .node_types
                    .insert("map-only".into(), "Foreign".into());
                assert!(bad.validate_raw_rows().is_err());
                assert!(!joined(&bad, capacity, forced));
                let mut bad = p.clone();
                bad.entry.node_ids.pop();
                assert!(!joined(&bad, capacity, forced));
                let mut bad = p.clone();
                bad.entry.node_ids.push("zz-array-only".into());
                bad.entry.node_ids.sort();
                assert!(!joined(&bad, capacity, forced));
                let mut bad = p.clone();
                bad.entry.node_types.insert("same".into(), "Old".into());
                assert!(!joined(&bad, capacity, forced));
                let mut bad = p.clone();
                bad.entry
                    .edge_types
                    .insert("edge-map-only".into(), "Unknown".into());
                assert!(!joined(&bad, capacity, forced));
            }
        }
    }
    #[test]
    fn dc_validation_oracle_schema_hash_numeric_and_nonfinite() {
        let memory = CleanupMemory::new(131072).unwrap();
        for confidence in [
            0.5,
            1.0,
            -0.0,
            f64::NAN,
            f64::INFINITY,
            f64::NEG_INFINITY,
            0.9966544878724393,
        ] {
            let p = partition(vec![node("n", "N")], vec![edge("e", "E", confidence)]);
            assert!(p.validate_raw_rows().is_ok());
            let bytes = envelope(&p);
            let f = facts(&bytes, &memory).unwrap();
            assert!(f.producer_tokens);
            assert!(wire_checksum_matches(bytes.as_slice(), &f, &memory).unwrap());
            assert_eq!(f.typed_numbers, confidence.is_finite());
            let literal = serde_json::to_vec(&confidence).unwrap();
            let oracle = number_facts(&literal, NumberKind::F64, &memory).unwrap();
            assert_eq!(f.stable_numbers, oracle.stable);
        }
        let typed = number_facts(b"0.9966544878724393", NumberKind::F64, &memory).unwrap();
        let value = number_facts(b"0.9966544878724393", NumberKind::Value, &memory).unwrap();
        let parsed: f64 = serde_json::from_slice(b"0.9966544878724393").unwrap();
        let serialized = serde_json::to_string(&parsed).unwrap();
        eprintln!(
            "pinned scalar oracle: {serialized}; bits={:x}",
            parsed.to_bits()
        );
        assert!(typed.typed && value.typed);
        assert_eq!(typed.stable, serialized == "0.9966544878724393");
        assert_eq!(typed.stable, value.stable);
        assert!(
            !typed.stable,
            "pinned default parser feature drift must be reviewed"
        );
    }
    #[test]
    fn dc_validation_control_escape_oracle_and_unbounded_strings() {
        let memory = CleanupMemory::new(131072).unwrap();
        for value in 0u8..=31 {
            let text = String::from_utf8(vec![value]).unwrap();
            let bytes = serde_json::to_vec(&text).unwrap();
            assert!(producer_string(
                bytes.as_slice(),
                Span {
                    start: 0,
                    end: bytes.len() as u64
                }
            )
            .unwrap());
        }
        for text in ["é😀\u{2028}\u{2029}", "\"\\/", ""] {
            let bytes = serde_json::to_vec(text).unwrap();
            assert!(producer_string(
                bytes.as_slice(),
                Span {
                    start: 0,
                    end: bytes.len() as u64
                }
            )
            .unwrap());
        }
        for bytes in [br#""\u0061""#.as_slice(), br#""\/""#, br#""\u000A""#] {
            assert!(!producer_string(
                bytes,
                Span {
                    start: 0,
                    end: bytes.len() as u64
                }
            )
            .unwrap());
        }
        let mut p = partition(vec![node("long", "N")], vec![]);
        p.nodes[0].summary = "é".repeat(100_000);
        let bytes = envelope(&p);
        let before = memory.used();
        let f = facts(&bytes, &memory).unwrap();
        assert!(f.producer_tokens && f.stable_numbers);
        join_raw_rows(
            &ArtifactEvents {
                source: bytes.as_slice(),
            },
            &memory,
            8,
        )
        .unwrap();
        assert_eq!(memory.used(), before);
        assert!(memory.peak() < 131072);
    }
    #[test]
    fn dc_validation_metadata_duplicate_keys_and_preserved_order() {
        let memory = CleanupMemory::new(131072).unwrap();
        let p = partition(vec![node("n", "N")], vec![]);
        let bytes = envelope(&p);
        let f = facts(&bytes, &memory).unwrap();
        assert!(f.producer_tokens);
        let text = String::from_utf8(bytes).unwrap();
        let altered = text.replacen("\"z\": 1", "\"z\": 1, \"z\": 2", 1);
        assert_ne!(text, altered);
        assert!(join_raw_rows(
            &ArtifactEvents {
                source: altered.as_bytes()
            },
            &memory,
            1
        )
        .is_err());
    }
    #[test]
    fn dc_validation_actual_runtime_map_oracle_and_order_fact() {
        let mut map = serde_json::Map::new();
        map.insert("z".into(), serde_json::Value::Null);
        map.insert("a".into(), serde_json::Value::Null);
        let actual = serde_json::to_string(&map).unwrap();
        eprintln!("actual ROOT serde_json Map serialization: {actual}");
        assert_eq!(
            actual, r#"{"a":null,"z":null}"#,
            "runtime dependency feature drift needs explicit review"
        );
        let memory = CleanupMemory::new(131072).unwrap();
        let mut p = partition(vec![node("n", "N")], vec![]);
        p.nodes[0].metadata = serde_json::json!({"a":1,"z":2});
        let text = String::from_utf8(envelope(&p)).unwrap();
        assert!(
            facts(text.as_bytes(), &memory)
                .unwrap()
                .metadata_keys_sorted
        );
        let start = text.find("\"metadata\": {").unwrap();
        let end = start + text[start..].find('}').unwrap() + 1;
        let mut unsorted = text.clone();
        unsorted.replace_range(start..end, "\"metadata\": {\"z\":2,\"a\":1}");
        let f = facts(unsorted.as_bytes(), &memory).unwrap();
        assert!(!f.metadata_keys_sorted);
        assert!(f.stable_numbers);
        assert!(!wire_checksum_matches(unsorted.as_bytes(), &f, &memory).unwrap());
    }
    #[test]
    fn dc_validation_hash_prefix_replay_and_capacity_one_collision_progress() {
        let p = partition(
            (0..64).map(|i| node(&format!("n{i:03}"), "N")).collect(),
            vec![],
        );
        let bytes = envelope(&p);
        let memory = CleanupMemory::new(131072).unwrap();
        let stats = join_raw_rows(
            &ArtifactEvents {
                source: bytes.as_slice(),
            },
            &memory,
            16,
        )
        .unwrap();
        assert!(stats.passes > 1);
        let small = partition(vec![node("a", "A"), node("b", "B")], vec![]);
        let bytes = envelope(&small);
        let stats = join_inner(
            &ArtifactEvents {
                source: bytes.as_slice(),
            },
            &memory,
            1,
            true,
        )
        .unwrap();
        assert!(stats.collision_batches >= 2);
        assert!(stats.passes < 1000);
        assert!(join_raw_rows(
            &ArtifactEvents {
                source: bytes.as_slice()
            },
            &memory,
            0
        )
        .is_err());
    }
    #[test]
    fn dc_validation_partition_id_streaming_oracle_and_schema_rejects() {
        for len in [0, 1, 55, 56, 63, 64, 65, 127, 4097] {
            let path = "é".repeat(len);
            let id = crate::hash::partition_id(&path);
            let a = serde_json::to_vec(&path).unwrap();
            let b = serde_json::to_vec(&id).unwrap();
            let mut source = a.clone();
            let first = Span {
                start: 0,
                end: a.len() as u64,
            };
            let second = Span {
                start: source.len() as u64,
                end: (source.len() + b.len()) as u64,
            };
            source.extend(b);
            assert!(partition_id_matches(source.as_slice(), first, second).unwrap());
        }
        let memory = CleanupMemory::new(131072).unwrap();
        let mut p = partition(vec![], vec![]);
        p.entry.partition_id = "bad".into();
        assert!(p.validate_raw_rows().is_err());
        assert!(facts(&envelope(&p), &memory).is_err());
        let p = partition(vec![], vec![]);
        let mut bytes = envelope(&p);
        bytes.extend_from_slice(b"null");
        assert!(facts(&bytes, &memory).is_err());
    }
    #[test]
    fn dc_validation_metadata_value_numeric_uses_same_oracle() {
        let memory = CleanupMemory::new(131072).unwrap();
        let mut p = partition(vec![node("n", "N")], vec![]);
        p.nodes[0].metadata = serde_json::json!({"value":0.9966544878724393});
        let bytes = envelope(&p);
        let f = facts(&bytes, &memory).unwrap();
        let token = serde_json::to_vec(&p.nodes[0].metadata["value"]).unwrap();
        assert_eq!(
            f.stable_numbers,
            number_facts(&token, NumberKind::Value, &memory)
                .unwrap()
                .stable
        );
        assert!(!f.stable_numbers);
    }
    #[test]
    fn dc_validation_full_typed_loader_depth_oracle() {
        #[derive(serde::Deserialize)]
        struct Envelope {
            format_version: u64,
            artifact_key: String,
            partition_payload_sha256: String,
            partition: GraphPartition,
        }
        let memory = CleanupMemory::new(131072).unwrap();
        let mut accepted = 0;
        let mut rejected = 0;
        for depth in [120, 121, 122, 123, 124, 125, 126, 127, 128] {
            let mut value = serde_json::Value::Null;
            for _ in 0..depth {
                value = serde_json::Value::Array(vec![value]);
            }
            let mut p = partition(vec![node("n", "N")], vec![]);
            p.nodes[0].metadata = value;
            let bytes = envelope(&p);
            let typed = serde_json::from_slice::<Envelope>(&bytes);
            assert_eq!(
                facts(&bytes, &memory).is_ok(),
                typed.is_ok(),
                "metadata depth {depth}"
            );
            if let Ok(typed) = typed {
                assert_eq!(typed.format_version, 2);
                assert_eq!(typed.artifact_key, "a".repeat(64));
                assert_eq!(typed.partition_payload_sha256.len(), 64);
                assert!(typed.partition.validate_raw_rows().is_ok());
                accepted += 1;
            } else {
                rejected += 1;
            }
        }
        assert!(accepted > 0 && rejected > 0);
    }
    #[test]
    fn dc_validation_borrowed_fd_position_and_exact_bounded_reads() {
        let mut fixture = TestFile::new(b"0123456789abcdef");
        let memory = CleanupMemory::new(4096).unwrap();
        fixture
            .file
            .as_mut()
            .unwrap()
            .seek(SeekFrom::Start(3))
            .unwrap();
        {
            let source = BorrowedFileSource::new(fixture.file.as_mut().unwrap(), &memory).unwrap();
            let mut bytes = [0u8; 4];
            assert_eq!(source.len(), 16);
            assert_eq!(source.read_at(7, &mut bytes).unwrap(), 4);
            assert_eq!(&bytes, b"789a");
            assert_eq!(
                source.read_at(17, &mut bytes),
                Err(ValidationIssue::InvalidSpan)
            );
        }
        #[cfg(unix)]
        assert_eq!(fixture.file.as_mut().unwrap().stream_position().unwrap(), 3);
        #[cfg(windows)]
        assert_eq!(
            fixture.file.as_mut().unwrap().stream_position().unwrap(),
            11
        );
    }
    #[test]
    fn dc_validation_borrowed_fd_schema_replay_without_payload_copy() {
        let p = partition(vec![node("n", "N")], vec![edge("e", "E", 0.5)]);
        let bytes = envelope(&p);
        let mut fixture = TestFile::new(&bytes);
        let memory = CleanupMemory::new(131072).unwrap();
        let before = memory.used();
        {
            let source = BorrowedFileSource::new(fixture.file.as_mut().unwrap(), &memory).unwrap();
            let f = facts_from_source(&source, &memory).unwrap();
            assert!(f.stable_numbers && f.metadata_keys_sorted);
            assert!(wire_checksum_matches(&source, &f, &memory).unwrap());
            join_raw_rows(&ArtifactEvents { source: &source }, &memory, 8).unwrap();
        }
        assert_eq!(memory.used(), before);
        assert_eq!(std::fs::read(&fixture.path).unwrap(), bytes);
    }
    fn facts_from_source<S: ByteSource + ?Sized>(
        source: &S,
        memory: &CleanupMemory,
    ) -> Result<ArtifactFacts, ValidationIssue> {
        scan_artifact(source, memory, &mut |_| Ok(()))
    }
    #[test]
    fn dc_validation_cross_source_exact_unicode_long_strings_and_memory() {
        let memory = CleanupMemory::new(16384).unwrap();
        let left = br#""\uD83D\uDE00\n""#;
        let right = "\"😀\\n\"".as_bytes();
        let span = |n: usize| Span {
            start: 0,
            end: n as u64,
        };
        assert_eq!(
            compare_decoded_spans(
                left.as_slice(),
                span(left.len()),
                right,
                span(right.len()),
                &memory
            )
            .unwrap(),
            Ordering::Equal
        );
        let a = serde_json::to_vec(&format!("{}a", "é".repeat(100_000))).unwrap();
        let b = serde_json::to_vec(&format!("{}b", "é".repeat(100_000))).unwrap();
        let floor = memory.used();
        assert_eq!(
            compare_decoded_spans(
                a.as_slice(),
                span(a.len()),
                b.as_slice(),
                span(b.len()),
                &memory
            )
            .unwrap(),
            Ordering::Less
        );
        assert_eq!(memory.used(), floor);
        assert!(memory.peak() < 16384);
        let small = CleanupMemory::new(64).unwrap();
        let used = small.used();
        assert!(compare_decoded_spans(
            left.as_slice(),
            span(left.len()),
            right,
            span(right.len()),
            &small
        )
        .is_err());
        assert_eq!(small.used(), used);
    }
    #[test]
    fn dc_validation_native_json_stream_keys_hash_unicode_nul_and_suffixes() {
        let memory = CleanupMemory::new(4096).unwrap();
        let floor = memory.used();
        let span = |bytes: &[u8]| Span {
            start: 0,
            end: bytes.len() as u64,
        };
        for text in [
            String::new(),
            "a\0é😀".to_owned(),
            format!("{}😀\0é", "x".repeat(255)),
            "é".repeat(100_000),
        ] {
            let native = text.as_bytes();
            let json = serde_json::to_vec(&text).unwrap();
            assert_eq!(
                compare_native_json(native, span(native), json.as_slice(), span(&json), &memory),
                Ok(Ordering::Equal)
            );
            let a =
                key_stream_hash(native, span(native), KeyEncoding::NativeUtf8, 7, &memory).unwrap();
            assert_eq!(
                a,
                key_stream_hash(
                    json.as_slice(),
                    span(&json),
                    KeyEncoding::JsonDecoded,
                    7,
                    &memory
                )
                .unwrap()
            );
            assert_ne!(
                a,
                key_stream_hash(native, span(native), KeyEncoding::NativeUtf8, 8, &memory).unwrap()
            );
        }
        let json = br#""b""#;
        for native in [
            b"a\xff".as_slice(),
            b"a\xc0\x80".as_slice(),
            b"a\xed\xa0\x80".as_slice(),
            b"a\xf4\x90\x80\x80".as_slice(),
            b"a\xe2\x82".as_slice(),
        ] {
            assert!(compare_native_json(
                native,
                span(native),
                json.as_slice(),
                span(json),
                &memory
            )
            .is_err());
            assert!(
                key_stream_hash(native, span(native), KeyEncoding::NativeUtf8, 7, &memory).is_err()
            );
        }
        let native = b"a";
        let malformed = br#""b\q""#;
        assert!(compare_native_json(
            native.as_slice(),
            span(native),
            malformed.as_slice(),
            span(malformed),
            &memory
        )
        .is_err());
        assert_eq!(
            compare_native_json(
                b"azzz".as_slice(),
                span(b"azzz"),
                br#""baaa""#.as_slice(),
                span(br#""baaa""#),
                &memory
            ),
            Ok(Ordering::Less)
        );
        assert_eq!(memory.used(), floor);
        let small = CleanupMemory::new(64).unwrap();
        assert!(compare_native_json(
            native.as_slice(),
            span(native),
            json.as_slice(),
            span(json),
            &small
        )
        .is_err());
    }
    #[test]
    fn dc_validation_cross_source_propagates_late_read_failure_after_ordering() {
        struct LateFailure<'a>(&'a [u8]);
        impl ByteSource for LateFailure<'_> {
            fn len(&self) -> u64 {
                self.0.len() as u64
            }
            fn read_at(&self, offset: u64, output: &mut [u8]) -> Result<usize, ValidationIssue> {
                if offset >= 256 {
                    return Err(ValidationIssue::Io(io::ErrorKind::PermissionDenied));
                }
                self.0.read_at(offset, output)
            }
        }
        let memory = CleanupMemory::new(4096).unwrap();
        let bytes = serde_json::to_vec(&format!("a{}", "x".repeat(512))).unwrap();
        let failing = LateFailure(&bytes);
        let long = Span {
            start: 0,
            end: bytes.len() as u64,
        };
        let short = Span { start: 0, end: 3 };
        assert_eq!(
            compare_decoded_spans(&failing, long, br#""b""#.as_slice(), short, &memory),
            Err(ValidationIssue::Io(io::ErrorKind::PermissionDenied))
        );
        assert_eq!(
            compare_decoded_spans(br#""b""#.as_slice(), short, &failing, long, &memory),
            Err(ValidationIssue::Io(io::ErrorKind::PermissionDenied))
        );
    }
    #[test]
    fn dc_validation_cross_source_rejects_invalid_utf8_and_incomplete_spans() {
        let memory = CleanupMemory::new(4096).unwrap();
        let invalid = b"\"\xff\"";
        let span = Span { start: 0, end: 3 };
        assert!(
            compare_decoded_spans(invalid.as_slice(), span, invalid.as_slice(), span, &memory)
                .is_err()
        );
        let valid = br#""a""#;
        assert!(compare_decoded_spans(
            valid.as_slice(),
            Span { start: 0, end: 2 },
            valid.as_slice(),
            span,
            &memory
        )
        .is_err());
    }
    #[test]
    fn dc_validation_cross_source_validates_suffixes_after_first_difference() {
        let memory = CleanupMemory::new(4096).unwrap();
        let floor = memory.used();
        let span = |s: &[u8]| Span {
            start: 0,
            end: s.len() as u64,
        };
        for (left, right, error) in [
            (
                b"\"a\xff\"".as_slice(),
                b"\"b\"".as_slice(),
                ValidationIssue::NonProducer,
            ),
            (
                b"\"a".as_slice(),
                b"\"b\"".as_slice(),
                ValidationIssue::InvalidSpan,
            ),
            (
                b"\"a\"trailing".as_slice(),
                b"\"b\"".as_slice(),
                ValidationIssue::InvalidSpan,
            ),
            (
                b"\"\"".as_slice(),
                b"\"a\xff\"".as_slice(),
                ValidationIssue::NonProducer,
            ),
            (
                br#""a\q""#.as_slice(),
                br#""b""#.as_slice(),
                ValidationIssue::NonProducer,
            ),
            (
                b"\"a\x01\"".as_slice(),
                br#""b""#.as_slice(),
                ValidationIssue::NonProducer,
            ),
            (
                br#""a\uD800""#.as_slice(),
                br#""b""#.as_slice(),
                ValidationIssue::NonProducer,
            ),
        ] {
            assert_eq!(
                compare_decoded_spans(left, span(left), right, span(right), &memory),
                Err(error)
            );
            assert_eq!(
                compare_decoded_spans(right, span(right), left, span(left), &memory),
                Err(error)
            );
        }
        for (left, right, expected) in [
            (
                br#""azzz""#.as_slice(),
                br#""baaa""#.as_slice(),
                Ordering::Less,
            ),
            (
                br#""azzz""#.as_slice(),
                br#""a""#.as_slice(),
                Ordering::Greater,
            ),
            (br#""""#.as_slice(), br#""a""#.as_slice(), Ordering::Less),
        ] {
            assert_eq!(
                compare_decoded_spans(left, span(left), right, span(right), &memory).unwrap(),
                expected
            );
        }
        assert_eq!(memory.used(), floor);
    }
    #[test]
    fn dc_validation_sparse_lookup_work_and_forced_collision_probe() {
        let memory = CleanupMemory::new(4 * 1024 * 1024).unwrap();
        let p = partition(
            (0..64).map(|i| node(&format!("n{i:03}"), "N")).collect(),
            vec![],
        );
        let bytes = envelope(&p);
        let started = std::time::Instant::now();
        let stats = join_raw_rows(
            &ArtifactEvents {
                source: bytes.as_slice(),
            },
            &memory,
            16384,
        )
        .unwrap();
        eprintln!("normal join bytes={} capacity=16384 passes={} events={} lookup_probes={} elapsed_us={}",bytes.len(),stats.passes,stats.events,stats.lookup_probes,started.elapsed().as_micros());
        assert_eq!(stats.passes, 1);
        assert!(
            stats.lookup_probes < stats.events * 3,
            "a missing key must stop at the first empty slot, not scan all16384 slots"
        );
        assert!(memory.peak() < 4 * 1024 * 1024);
        let small = partition(
            vec![node("a", "A"), node("b", "B"), node("a", "Latest")],
            vec![],
        );
        let bytes = envelope(&small);
        let stats = join_inner(
            &ArtifactEvents {
                source: bytes.as_slice(),
            },
            &memory,
            1,
            true,
        )
        .unwrap();
        eprintln!(
            "forced collision passes={} events={} probes={} batches={}",
            stats.passes, stats.events, stats.lookup_probes, stats.collision_batches
        );
        assert!(stats.collision_batches > 0);
        assert!(stats.lookup_probes > 0);
        assert!(small.validate_raw_rows().is_ok());
        let mut extra = small.clone();
        extra
            .entry
            .node_types
            .insert("map-only-extra".into(), "N".into());
        assert!(!joined(&extra, 1, true));
    }
}

#[cfg(test)]
mod witness_tests {
    use super::*;
    #[derive(Clone, Copy)]
    struct Key(&'static str);
    struct Replay {
        rows: Vec<WitnessEvent<Key>>,
        collision: bool,
    }
    impl ReplayWitnesses for Replay {
        type Key = Key;
        fn replay(
            &self,
            _: &CleanupMemory,
            emit: &mut dyn FnMut(WitnessEvent<Key>) -> Result<(), ValidationIssue>,
        ) -> Result<(), ValidationIssue> {
            for row in &self.rows {
                emit(*row)?;
            }
            Ok(())
        }
        fn hash(
            &self,
            key: Key,
            domain: u64,
            _: &CleanupMemory,
        ) -> Result<[u8; 32], ValidationIssue> {
            if self.collision {
                Ok([0; 32])
            } else {
                let mut hash = Sha256::new();
                hash.update(domain.to_le_bytes());
                hash.update(key.0.as_bytes());
                Ok(hash.finalize().into())
            }
        }
        fn compare(
            &self,
            left: Key,
            right: Key,
            _: &CleanupMemory,
        ) -> Result<Ordering, ValidationIssue> {
            Ok(left.0.cmp(right.0))
        }
        fn empty(&self, key: Key, _: &CleanupMemory) -> Result<bool, ValidationIssue> {
            Ok(key.0.is_empty())
        }
    }
    fn row(id: &'static str, native: bool, ordinal: u64, value: &'static str) -> WitnessEvent<Key> {
        WitnessEvent {
            domain: 0,
            native,
            ordinal,
            fields: [
                Some(Key(id)),
                Some(Key(value)),
                Some(Key("kind")),
                Some(Key("path")),
                Some(Key("canonical")),
            ],
        }
    }
    #[test]
    fn dc_witness_first_nonempty_all_ids_multiplicity_and_capacity_one_collisions() {
        for collision in [false, true] {
            for capacity in [1, 2, 10] {
                let memory = CleanupMemory::new(100_000).unwrap();
                let floor = memory.used();
                let replay = Replay {
                    collision,
                    rows: vec![
                        row("b", false, 0, ""),
                        row("a", true, 0, "first"),
                        row("b", true, 0, "first"),
                        row("b", false, 2, "later"),
                        row("a", false, 1, "first"),
                        row("b", false, 1, "first"),
                    ],
                };
                let facts = join_selected_witnesses(&replay, &memory, capacity).unwrap();
                assert!(facts.passes >= 1);
                assert_eq!(memory.used(), floor);
                let mut bad = replay;
                bad.rows.push(row("extra", true, 0, "first"));
                assert_eq!(
                    join_selected_witnesses(&bad, &memory, capacity).unwrap_err(),
                    ValidationIssue::RawRowsDiffer
                );
                bad.rows.pop();
                bad.rows.push(row("extra", false, 0, "first"));
                assert_eq!(
                    join_selected_witnesses(&bad, &memory, capacity).unwrap_err(),
                    ValidationIssue::RawRowsDiffer
                );
                bad.rows.pop();
                bad.rows.push(row("a", true, 0, "first"));
                assert_eq!(
                    join_selected_witnesses(&bad, &memory, capacity).unwrap_err(),
                    ValidationIssue::RawRowsDiffer
                );
            }
        }
    }
    #[test]
    fn dc_witness_each_field_chooses_first_nonempty_and_allocation_failure() {
        let memory = CleanupMemory::new(100_000).unwrap();
        let mut first = row("id", false, 0, "");
        first.fields[2] = Some(Key(""));
        let second = row("id", false, 1, "value");
        let native = row("id", true, 0, "value");
        let replay = Replay {
            rows: vec![first, second, native],
            collision: true,
        };
        assert!(join_selected_witnesses(&replay, &memory, 1).is_ok());
        assert!(join_selected_witnesses(&replay, &memory, 0).is_err());
        let small = CleanupMemory::new(64).unwrap();
        assert!(join_selected_witnesses(&replay, &small, 1).is_err());
        assert_eq!(small.used(), CleanupMemory::new(64).unwrap().used());
    }
}

#[cfg(test)]
mod witness_large_tests {
    use super::*;
    struct Large {
        keys: Vec<String>,
        collision: bool,
    }
    impl ReplayWitnesses for Large {
        type Key = usize;
        fn replay(
            &self,
            _: &CleanupMemory,
            emit: &mut dyn FnMut(WitnessEvent<usize>) -> Result<(), ValidationIssue>,
        ) -> Result<(), ValidationIssue> {
            for id in 4..self.keys.len() {
                for native in [false, true] {
                    emit(WitnessEvent {
                        domain: 3,
                        native,
                        ordinal: id as u64,
                        fields: [Some(id), Some(1), Some(1), Some(2), Some(3)],
                    })?;
                }
            }
            Ok(())
        }
        fn hash(
            &self,
            key: usize,
            domain: u64,
            _: &CleanupMemory,
        ) -> Result<[u8; 32], ValidationIssue> {
            if self.collision {
                Ok([0; 32])
            } else {
                let mut hash = Sha256::new();
                hash.update(domain.to_le_bytes());
                hash.update(self.keys[key].as_bytes());
                Ok(hash.finalize().into())
            }
        }
        fn compare(
            &self,
            a: usize,
            b: usize,
            _: &CleanupMemory,
        ) -> Result<Ordering, ValidationIssue> {
            Ok(self.keys[a].cmp(&self.keys[b]))
        }
        fn empty(&self, key: usize, _: &CleanupMemory) -> Result<bool, ValidationIssue> {
            Ok(self.keys[key].is_empty())
        }
    }
    #[test]
    fn dc_witness_large_inventory_peak_independent_of_row_count_and_collision_progress() {
        let mut peaks = Vec::new();
        for count in [17, 4097] {
            let mut keys = vec!["".into(), "value".into(), "path".into(), "canonical".into()];
            keys.extend((0..count).map(|i| format!("id-{i:08}")));
            let replay = Large {
                keys,
                collision: false,
            };
            let memory = CleanupMemory::new(100_000).unwrap();
            let before = memory.used();
            let result = join_selected_witnesses(&replay, &memory, 32).unwrap();
            assert!(result.lookup_probes > 0);
            assert_eq!(memory.used(), before);
            peaks.push(memory.peak());
        }
        assert_eq!(peaks[0], peaks[1]);
        let mut keys = vec!["".into(), "value".into(), "path".into(), "canonical".into()];
        keys.extend((0..33).map(|i| format!("id-{i:08}")));
        let replay = Large {
            keys,
            collision: true,
        };
        let memory = CleanupMemory::new(100_000).unwrap();
        let result = join_selected_witnesses(&replay, &memory, 1).unwrap();
        assert_eq!(result.collision_batches, 33);
        assert!(result.passes <= 70);
    }
}

#[cfg(test)]
mod projected_key_tests {
    use super::*;
    #[test]
    fn sha1_decoded_native_and_json_match_actual_stable_id_digest() {
        let memory = CleanupMemory::new(32768).unwrap();
        let expected = [
            0xa9, 0x99, 0x3e, 0x36, 0x47, 0x06, 0x81, 0x6a, 0xba, 0x3e, 0x25, 0x71, 0x78, 0x50,
            0xc2, 0x6c, 0x9c, 0xd0, 0xd8, 0x9d,
        ];
        assert_eq!(
            key_sha1_prefix20(
                &b"abc"[..],
                Span { start: 0, end: 3 },
                KeyEncoding::NativeUtf8,
                &memory
            )
            .unwrap(),
            expected
        );
        let quoted = br#""a\u0062c""#;
        assert_eq!(
            key_sha1_prefix20(
                &quoted[..],
                Span {
                    start: 0,
                    end: quoted.len() as u64
                },
                KeyEncoding::JsonDecoded,
                &memory
            )
            .unwrap(),
            expected
        );
    }
    #[test]
    fn projected_key_never_truncates_or_skips_invalid_suffix() {
        let memory = CleanupMemory::new(32768).unwrap();
        let json = br#""a\u0062c""#;
        let mut out = [0; 3];
        assert_eq!(
            copy_decoded_key(
                &json[..],
                Span {
                    start: 0,
                    end: json.len() as u64
                },
                KeyEncoding::JsonDecoded,
                &mut out,
                &memory
            )
            .unwrap(),
            3
        );
        assert_eq!(&out, b"abc");
        let mut tiny = [0; 1];
        assert_eq!(
            copy_decoded_key(
                &json[..],
                Span {
                    start: 0,
                    end: json.len() as u64
                },
                KeyEncoding::JsonDecoded,
                &mut tiny,
                &memory
            ),
            Err(ValidationIssue::Schema)
        );
        let bad = b"\"ab\xff\"";
        assert_eq!(
            copy_decoded_key(
                &bad[..],
                Span {
                    start: 0,
                    end: bad.len() as u64
                },
                KeyEncoding::JsonDecoded,
                &mut tiny,
                &memory
            ),
            Err(ValidationIssue::NonProducer)
        );
    }
}

#[cfg(test)]
mod encoded_key_comparison_tests {
    use super::*;
    #[test]
    fn encoded_keys_compare_all_modes_and_finish_invalid_suffixes() {
        let memory = CleanupMemory::new(32768).unwrap();
        for (left, le) in [
            (&b"abc"[..], KeyEncoding::NativeUtf8),
            (&br#""a\u0062c""#[..], KeyEncoding::JsonDecoded),
        ] {
            for (right, re) in [
                (&b"abc"[..], KeyEncoding::NativeUtf8),
                (&br#""abc""#[..], KeyEncoding::JsonDecoded),
            ] {
                assert_eq!(
                    compare_encoded_keys(
                        left,
                        Span {
                            start: 0,
                            end: left.len() as u64
                        },
                        le,
                        right,
                        Span {
                            start: 0,
                            end: right.len() as u64
                        },
                        re,
                        &memory
                    )
                    .unwrap(),
                    Ordering::Equal
                );
            }
        }
        let bad = &b"a\xff"[..];
        let right = &b"b"[..];
        assert_eq!(
            compare_encoded_keys(
                bad,
                Span {
                    start: 0,
                    end: bad.len() as u64
                },
                KeyEncoding::NativeUtf8,
                right,
                Span { start: 0, end: 1 },
                KeyEncoding::NativeUtf8,
                &memory
            ),
            Err(ValidationIssue::NonProducer)
        );
    }
}
