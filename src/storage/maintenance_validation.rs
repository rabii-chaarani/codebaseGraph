//! Pure validation facts. Nothing here constructs ownership, loader usability,
//! publication or deletion authority. Sources must remain immutable per replay.
use super::maintenance_budget::{CleanupMemory, MemoryReservation, ReservedBytes, ResourceLimit};
use super::maintenance_json::{JsonFailure, JsonToken, MaintenanceJsonReader, TokenKind};
use sha2::{Digest, Sha256};
use std::cmp::Ordering;
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
    let mut a = Decoded::new(source, a)?;
    let mut b = Decoded::new(source, b)?;
    loop {
        match (a.next()?, b.next()?) {
            (None, None) => return Ok(Ordering::Equal),
            (None, _) => return Ok(Ordering::Less),
            (_, None) => return Ok(Ordering::Greater),
            (Some(a), Some(b)) if a != b => return Ok(a.cmp(&b)),
            _ => {}
        }
    }
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
        b[0] = u8::from(r.occupied);
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
        let mut empty = None;
        for delta in 0..self.capacity {
            let index = (start + delta) % self.capacity;
            let row = self.row(index);
            if !row.occupied {
                empty.get_or_insert(index);
                continue;
            }
            if row.domain == event.domain
                && row.hash == hash
                && compare(source, row.id, event.id)? == Ordering::Equal
            {
                return Ok(Ok(index));
            }
        }
        Ok(Err(empty.unwrap_or(self.capacity)))
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::partition_builder::GraphPartition;
    use crate::protocol::ManifestEntry;
    use crate::syntax_materializer::{GraphEdgeRow, GraphNodeRow};
    use std::collections::BTreeMap;
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
}
