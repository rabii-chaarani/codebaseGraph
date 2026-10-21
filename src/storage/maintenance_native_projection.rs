//! Finite native format facts only. These do not certify a producer profile,
//! database origin, loader usability, source stability or retirement authority.
use super::maintenance_budget::CleanupMemory;
use super::maintenance_validation::{ByteSource, ValidationIssue};

const PAGE_BYTES: u64 = 4096;
const HEADER_BYTES: usize = 49;
const INVALID_PAGE: u32 = u32::MAX;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct NativePageRange {
    pub(crate) start: u32,
    pub(crate) count: u32,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct NativeHeader43Facts {
    pub(crate) catalog: NativePageRange,
    pub(crate) metadata: NativePageRange,
    pub(crate) database_id: [u8; 16],
    pub(crate) data_file_pages: u32,
    pub(crate) available_pages: u64,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NativeUnknown {
    Source(ValidationIssue),
    UnsupportedLayout,
    Truncated,
    InvalidRange,
    Overflow,
    SourceChanged,
    Cancelled,
    UnsupportedField { offset: u64, tag: u64 },
}
impl From<ValidationIssue> for NativeUnknown {
    fn from(value: ValidationIssue) -> Self {
        Self::Source(value)
    }
}

/// Parse only the selected storage43, no-debug, little-endian, 4096-byte-page,
/// format2 header layout. Its structure is not evidence that a program created
/// these bytes. Native page_idx_t is u32; the actual format2 prefix is49 bytes.
pub(crate) fn scan_native_header43<S: ByteSource + ?Sized>(
    source: &S,
    account: &CleanupMemory,
) -> Result<NativeHeader43Facts, NativeUnknown> {
    let floor = HEADER_BYTES
        .checked_add(std::mem::size_of::<NativeHeader43Facts>())
        .and_then(|n| n.checked_add(128))
        .ok_or(NativeUnknown::Overflow)?;
    let _reservation = account.reserve(floor).map_err(ValidationIssue::from)?;
    let length = source.len();
    if length < HEADER_BYTES as u64 {
        return Err(NativeUnknown::Truncated);
    }
    if length % PAGE_BYTES != 0 {
        return Err(NativeUnknown::UnsupportedLayout);
    }
    let available_pages = length / PAGE_BYTES;
    let mut bytes = [0; HEADER_BYTES];
    let mut at = 0;
    while at < bytes.len() {
        let count = source.read_at(at as u64, &mut bytes[at..])?;
        if count == 0 || count > bytes.len() - at {
            return Err(NativeUnknown::Truncated);
        }
        at = at.checked_add(count).ok_or(NativeUnknown::Overflow)?;
    }
    if &bytes[..4] != b"LBUG"
        || u64::from_le_bytes(bytes[4..12].try_into().unwrap()) != 43
        || bytes[44] != 2
    {
        return Err(NativeUnknown::UnsupportedLayout);
    }
    let word = |at: usize| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
    let catalog = NativePageRange {
        start: word(12),
        count: word(16),
    };
    let metadata = NativePageRange {
        start: word(20),
        count: word(24),
    };
    check_range(catalog, available_pages)?;
    check_range(metadata, available_pages)?;
    let data_file_pages = word(45);
    if u64::from(data_file_pages) > available_pages {
        return Err(NativeUnknown::InvalidRange);
    }
    Ok(NativeHeader43Facts {
        catalog,
        metadata,
        database_id: bytes[28..44].try_into().unwrap(),
        data_file_pages,
        available_pages,
    })
}
fn check_range(range: NativePageRange, pages: u64) -> Result<(), NativeUnknown> {
    if range.start == INVALID_PAGE {
        return if range.count == 0 {
            Ok(())
        } else {
            Err(NativeUnknown::InvalidRange)
        };
    }
    let start = u64::from(range.start);
    let count = u64::from(range.count);
    if count == 0 || start >= pages || count > pages - start {
        return Err(NativeUnknown::InvalidRange);
    }
    start
        .checked_mul(PAGE_BYTES)
        .and_then(|s| count.checked_mul(PAGE_BYTES).and_then(|n| s.checked_add(n)))
        .ok_or(NativeUnknown::Overflow)?;
    Ok(())
}

// Catalog-only checkpoint: no row/string-page decoding and no origin factory.
const PROFILE_TABLES: usize = 112;
const PROFILE_PROPERTIES: usize = 18;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NativeDefaultFacts {
    Null,
    Text(super::maintenance_validation::Span),
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NativeCatalogEvent {
    BuiltinJsonType {
        id: u64,
        name: super::maintenance_validation::Span,
    },
    Set {
        number: u8,
        next_id: u64,
        count: u64,
    },
    Table {
        id: u64,
        name: super::maintenance_validation::Span,
        node: bool,
    },
    Property {
        table: u64,
        id: u32,
        name: super::maintenance_validation::Span,
        logical: u8,
        physical: u8,
        default: NativeDefaultFacts,
    },
    Column {
        table: u64,
        property: u32,
        column: u32,
    },
    Index {
        entry: u64,
        table: u64,
        name: super::maintenance_validation::Span,
        kind: super::maintenance_validation::Span,
        property: u32,
    },
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct NativeCatalogTraceFacts {
    pub(crate) tables: u64,
    pub(crate) nodes: u64,
    pub(crate) indexes: u64,
    pub(crate) consumed_bytes: u64,
}
#[derive(Clone, Copy, Default)]
struct PropertyCell {
    id: u32,
    mapped: bool,
    column: u32,
}
#[derive(Clone, Copy)]
struct CatalogTableCell {
    id: u64,
    name: super::maintenance_validation::Span,
    node: bool,
    primary: u32,
    index: u64,
}
struct CatalogCursor<'a, S: ByteSource + ?Sized> {
    source: &'a S,
    at: u64,
    end: u64,
    bytes: [u8; 256],
    index: usize,
    filled: usize,
}
impl<'a, S: ByteSource + ?Sized> CatalogCursor<'a, S> {
    fn new(source: &'a S, start: u64, end: u64) -> Result<Self, NativeUnknown> {
        if start > end || end > source.len() {
            return Err(NativeUnknown::InvalidRange);
        }
        Ok(Self {
            source,
            at: start,
            end,
            bytes: [0; 256],
            index: 0,
            filled: 0,
        })
    }
    fn fail(&self, tag: u64) -> NativeUnknown {
        NativeUnknown::UnsupportedField {
            offset: self.at,
            tag,
        }
    }
    fn byte(&mut self) -> Result<u8, NativeUnknown> {
        if self.at >= self.end {
            return Err(NativeUnknown::Truncated);
        }
        if self.index == self.filled {
            let n = usize::try_from((self.end - self.at).min(256))
                .map_err(|_| NativeUnknown::Overflow)?;
            self.filled = self.source.read_at(self.at, &mut self.bytes[..n])?;
            self.index = 0;
            if self.filled == 0 || self.filled > n {
                return Err(NativeUnknown::Truncated);
            }
        }
        let b = self.bytes[self.index];
        self.index += 1;
        self.at += 1;
        Ok(b)
    }
    fn array<const N: usize>(&mut self) -> Result<[u8; N], NativeUnknown> {
        let mut out = [0; N];
        for b in &mut out {
            *b = self.byte()?;
        }
        Ok(out)
    }
    fn u32(&mut self) -> Result<u32, NativeUnknown> {
        Ok(u32::from_le_bytes(self.array()?))
    }
    fn u64(&mut self) -> Result<u64, NativeUnknown> {
        Ok(u64::from_le_bytes(self.array()?))
    }
    fn flag(&mut self) -> Result<bool, NativeUnknown> {
        match self.byte()? {
            0 => Ok(false),
            1 => Ok(true),
            v => Err(self.fail(u64::from(v))),
        }
    }
    fn skip(&mut self, n: u64) -> Result<(), NativeUnknown> {
        self.at = self.at.checked_add(n).ok_or(NativeUnknown::Overflow)?;
        if self.at > self.end {
            return Err(NativeUnknown::Truncated);
        }
        self.index = 0;
        self.filled = 0;
        Ok(())
    }
    fn text(
        &mut self,
        memory: &CleanupMemory,
    ) -> Result<super::maintenance_validation::Span, NativeUnknown> {
        let len = self.u64()?;
        let span = super::maintenance_validation::Span {
            start: self.at,
            end: self.at.checked_add(len).ok_or(NativeUnknown::Overflow)?,
        };
        if span.end > self.end {
            return Err(NativeUnknown::Truncated);
        }
        super::maintenance_validation::key_stream_hash(
            self.source,
            span,
            super::maintenance_validation::KeyEncoding::NativeUtf8,
            0,
            memory,
        )?;
        self.skip(len)?;
        Ok(span)
    }
    fn matches(
        &self,
        span: super::maintenance_validation::Span,
        text: &[u8],
    ) -> Result<bool, NativeUnknown> {
        if span.end - span.start != text.len() as u64 {
            return Ok(false);
        }
        for (i, b) in text.iter().enumerate() {
            let mut out = [0];
            if self.source.read_at(span.start + i as u64, &mut out)? != 1 {
                return Err(NativeUnknown::Truncated);
            }
            if out[0] != *b {
                return Ok(false);
            }
        }
        Ok(true)
    }
    fn concat_matches(
        &self,
        span: super::maintenance_validation::Span,
        prefix: &[u8],
        middle: super::maintenance_validation::Span,
        suffix: &[u8],
    ) -> Result<bool, NativeUnknown> {
        let len = (prefix.len() as u64)
            .checked_add(middle.end - middle.start)
            .and_then(|n| n.checked_add(suffix.len() as u64))
            .ok_or(NativeUnknown::Overflow)?;
        if span.end - span.start != len {
            return Ok(false);
        }
        let start = span.start + prefix.len() as u64;
        let end = start + middle.end - middle.start;
        if !self.matches(
            super::maintenance_validation::Span {
                start: span.start,
                end: start,
            },
            prefix,
        )? || !self.matches(
            super::maintenance_validation::Span {
                start: end,
                end: span.end,
            },
            suffix,
        )? {
            return Ok(false);
        }
        for i in 0..middle.end - middle.start {
            let mut a = [0];
            let mut b = [0];
            if self.source.read_at(start + i, &mut a)? != 1
                || self.source.read_at(middle.start + i, &mut b)? != 1
            {
                return Err(NativeUnknown::Truncated);
            }
            if a != b {
                return Ok(false);
            }
        }
        Ok(true)
    }
    fn primitive(&mut self) -> Result<(u8, u8), NativeUnknown> {
        let logical = self.byte()?;
        let physical = self.byte()?;
        let category = self.byte()?;
        if category != 0
            || !matches!(
                (logical, physical),
                (0, 0)
                    | (22, 1)
                    | (23, 2)
                    | (27, 6)
                    | (28, 7)
                    | (30, 9)
                    | (32, 11)
                    | (42, 14)
                    | (50, 20)
                    | (60, 21)
            )
        {
            return Err(self.fail(
                (u64::from(logical) << 16) | (u64::from(physical) << 8) | u64::from(category),
            ));
        }
        Ok((logical, physical))
    }
    fn literal(
        &mut self,
        memory: &CleanupMemory,
        expected: (u8, u8),
    ) -> Result<NativeDefaultFacts, NativeUnknown> {
        if self.byte()? != 70 {
            return Err(self.fail(70));
        }
        let alias = self.text(memory)?;
        if alias.start != alias.end {
            return Err(self.fail(71));
        }
        let raw = self.text(memory)?;
        if self.u64()? != 0 {
            return Err(self.fail(72));
        }
        let (logical, physical) = self.primitive()?;
        let null = self.flag()?;
        if self.u32()? != 0 {
            return Err(self.fail(73));
        }
        if null {
            if !self.matches(raw, b"NULL")?
                || ((logical, physical) != (0, 0) && (logical, physical) != expected)
            {
                return Err(self.fail(74));
            }
            // Binder::resolvePropertyDefault creates typed NULLs. Value's native
            // serializer still writes the physical payload; unused numeric bits
            // have no null semantics and are not required to be zero or finite.
            match physical {
                0 => {}
                1 | 9 => self.skip(1)?,
                7 => self.skip(4)?,
                2 | 6 | 11 => self.skip(8)?,
                14 => self.skip(16)?,
                20 | 21 => {
                    let text = self.text(memory)?;
                    if text.start != text.end {
                        return Err(self.fail(86));
                    }
                }
                _ => return Err(self.fail(74)),
            }
            return Ok(NativeDefaultFacts::Null);
        }
        if (logical, physical) == (50, 20) && expected == (50, 20) {
            Ok(NativeDefaultFacts::Text(self.text(memory)?))
        } else {
            Err(self.fail(74))
        }
    }
}
/// Complete supported catalog trace only. Returned events borrow no buffers;
/// sink-owned results, if any, must also be charged by that caller's SAME account.
/// Prefix events must be discarded if any later field or source read fails.
/// This deliberately stops before primitive storage/string DATA interpretation.
pub(crate) fn trace_native_catalog43<S: ByteSource + ?Sized>(
    source: &S,
    memory: &CleanupMemory,
    emit: &mut impl FnMut(NativeCatalogEvent) -> Result<(), NativeUnknown>,
) -> Result<NativeCatalogTraceFacts, NativeUnknown> {
    let header = scan_native_header43(source, memory)?;
    if header.catalog.start == INVALID_PAGE {
        return Err(NativeUnknown::UnsupportedLayout);
    }
    let start = u64::from(header.catalog.start)
        .checked_mul(PAGE_BYTES)
        .ok_or(NativeUnknown::Overflow)?;
    let end = u64::from(header.catalog.count)
        .checked_mul(PAGE_BYTES)
        .and_then(|n| start.checked_add(n))
        .ok_or(NativeUnknown::Overflow)?;
    let floor = std::mem::size_of::<CatalogCursor<'_, S>>()
        .checked_add(std::mem::size_of::<[CatalogTableCell; PROFILE_TABLES]>())
        .and_then(|n| n.checked_add(std::mem::size_of::<[PropertyCell; PROFILE_PROPERTIES]>()))
        .and_then(|n| n.checked_add(512))
        .ok_or(NativeUnknown::Overflow)?;
    let _reservation = memory.reserve(floor).map_err(ValidationIssue::from)?;
    let mut reader = CatalogCursor::new(source, start, end)?;
    let mut tables = [CatalogTableCell {
        id: u64::MAX,
        name: super::maintenance_validation::Span::default(),
        node: false,
        primary: u32::MAX,
        index: u64::MAX,
    }; PROFILE_TABLES];
    let mut nodes = 0;
    let mut indexes = 0;
    for set in 0..10u8 {
        let next_id = reader.u64()?;
        let count = reader.u64()?;
        emit(NativeCatalogEvent::Set {
            number: set,
            next_id,
            count,
        })?;
        if set == 0 && count != PROFILE_TABLES as u64 {
            return Err(reader.fail(count));
        }
        if set == 4 && count > nodes {
            return Err(reader.fail(count));
        }
        if set == 3 && count != 1 {
            return Err(reader.fail(count));
        }
        if set != 0 && set != 3 && set != 4 && count != 0 {
            return Err(reader.fail(count));
        }
        for ordinal in 0..count {
            let tag = reader.byte()?;
            let name = reader.text(memory)?;
            let id = reader.u64()?;
            let parent = reader.flag()?;
            if id >= next_id || parent {
                return Err(reader.fail(id));
            }
            if set == 3 {
                if tag != 41
                    || id != 0
                    || !reader.matches(name, b"JSON")?
                    || reader.primitive()? != (60, 21)
                {
                    return Err(reader.fail(87));
                }
                emit(NativeCatalogEvent::BuiltinJsonType { id, name })?;
                continue;
            }
            if set == 4 {
                if tag != 42 {
                    return Err(reader.fail(u64::from(tag)));
                }
                let kind = reader.text(memory)?;
                if !reader.matches(kind, b"HASH")? {
                    return Err(reader.fail(42));
                }
                let table = reader.u64()?;
                if !tables.iter().any(|v| v.id == table && v.node) {
                    return Err(reader.fail(table));
                }
                let index_name = reader.text(memory)?;
                if reader.u64()? != 1 {
                    return Err(reader.fail(43));
                }
                let property = reader.u32()?;
                let aux = reader.u64()?;
                if aux != 0 {
                    return Err(reader.fail(aux));
                }
                let slot = tables
                    .iter()
                    .position(|v| v.id == table)
                    .ok_or(reader.fail(table))?;
                if tables[slot].index != u64::MAX
                    || tables.iter().any(|v| v.index == id)
                    || property != tables[slot].primary
                {
                    return Err(reader.fail(83));
                }
                if !reader.concat_matches(index_name, b"pk_", tables[slot].name, b"_id")? {
                    return Err(reader.fail(84));
                }
                let mut prefix = [0; 21];
                prefix[20] = b'_';
                let mut cursor = 20;
                let mut number = table;
                loop {
                    cursor -= 1;
                    prefix[cursor] = b'0' + (number % 10) as u8;
                    number /= 10;
                    if number == 0 {
                        break;
                    }
                }
                if !reader.concat_matches(name, &prefix[cursor..], index_name, b"")? {
                    return Err(reader.fail(85));
                }
                tables[slot].index = id;
                emit(NativeCatalogEvent::Index {
                    entry: id,
                    table,
                    name: index_name,
                    kind,
                    property,
                })?;
                indexes += 1;
                continue;
            }
            if set != 0 || !matches!(tag, 0 | 2) {
                return Err(reader.fail(u64::from(tag)));
            }
            if tables[..ordinal as usize].iter().any(|v| v.id == id) {
                return Err(reader.fail(id));
            }
            tables[ordinal as usize].id = id;
            tables[ordinal as usize].name = name;
            let node = tag == 0;
            tables[ordinal as usize].node = node;
            if node {
                nodes += 1
            }
            emit(NativeCatalogEvent::Table { id, name, node })?;
            let comment = reader.text(memory)?;
            if comment.start != comment.end {
                return Err(reader.fail(75));
            }
            let next_column = reader.u32()?;
            let next_property = reader.u32()?;
            let definitions = reader.u64()?;
            if definitions > PROFILE_PROPERTIES as u64
                || next_property > PROFILE_PROPERTIES as u32
                || next_column > PROFILE_PROPERTIES as u32 + 1
            {
                return Err(reader.fail(definitions));
            }
            let mut properties = [PropertyCell::default(); PROFILE_PROPERTIES];
            for p in 0..definitions {
                let property = reader.u32()?;
                if property >= next_property
                    || properties[..p as usize].iter().any(|v| v.id == property)
                {
                    return Err(reader.fail(u64::from(property)));
                }
                let property_name = reader.text(memory)?;
                let (logical, physical) = reader.primitive()?;
                let default = reader.literal(memory, (logical, physical))?;
                properties[p as usize] = PropertyCell {
                    id: property,
                    mapped: false,
                    column: u32::MAX,
                };
                if reader.matches(property_name, b"id")? {
                    tables[ordinal as usize].primary = property;
                }
                emit(NativeCatalogEvent::Property {
                    table: id,
                    id: property,
                    name: property_name,
                    logical,
                    physical,
                    default,
                })?;
            }
            let mappings = reader.u64()?;
            if mappings != definitions {
                return Err(reader.fail(mappings));
            }
            for _ in 0..mappings {
                let property = reader.u32()?;
                let column = reader.u32()?;
                if properties[..definitions as usize]
                    .iter()
                    .any(|v| v.mapped && v.column == column)
                {
                    return Err(reader.fail(u64::from(column)));
                }
                let Some(cell) = properties[..definitions as usize]
                    .iter_mut()
                    .find(|v| v.id == property)
                else {
                    return Err(reader.fail(u64::from(property)));
                };
                if cell.mapped || column >= next_column {
                    return Err(reader.fail(u64::from(column)));
                }
                cell.mapped = true;
                cell.column = column;
                emit(NativeCatalogEvent::Column {
                    table: id,
                    property,
                    column,
                })?;
            }
            if node {
                let primary = reader.text(memory)?;
                if !reader.matches(primary, b"id")? {
                    return Err(reader.fail(76));
                }
                if reader.u64()? != 0 {
                    return Err(reader.fail(77));
                }
                let storage = reader.text(memory)?;
                if storage.start != storage.end || reader.byte()? != 0 {
                    return Err(reader.fail(78));
                }
            } else {
                let src = reader.byte()?;
                let dst = reader.byte()?;
                let direction = reader.byte()?;
                if src > 1 || dst > 1 || direction > 2 {
                    return Err(reader.fail(79));
                }
                let storage = reader.text(memory)?;
                if storage.start != storage.end || reader.flag()? {
                    return Err(reader.fail(80));
                }
                let infos = reader.u64()?;
                for _ in 0..infos {
                    let _source = reader.u64()?;
                    let _target = reader.u64()?;
                    let _inner_id = reader.u64()?;
                    if reader.byte()? > 1 || reader.byte()? > 1 {
                        return Err(reader.fail(81));
                    }
                }
                if reader.byte()? != 0 {
                    return Err(reader.fail(82));
                }
            }
        }
    }
    Ok(NativeCatalogTraceFacts {
        tables: PROFILE_TABLES as u64,
        nodes,
        indexes,
        consumed_bytes: reader.at - start,
    })
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct NativePrimitiveMetadata {
    pub(crate) page: u32,
    pub(crate) pages: u32,
    pub(crate) values: u64,
    pub(crate) min: [u8; 16],
    pub(crate) max: [u8; 16],
    pub(crate) compression: u8,
}
const _: [(); 56] = [(); std::mem::size_of::<NativePrimitiveMetadata>()];
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NativeMetadataEvent {
    Table {
        id: u64,
        groups: u64,
    },
    Group {
        table: u64,
        group: u64,
        columns: u64,
        rows: u64,
        start: u64,
    },
    Chunk {
        table: u64,
        group: u64,
        column: u64,
        segment: u64,
        logical: u8,
        physical: u8,
        metadata: NativePrimitiveMetadata,
    },
    Index {
        table: u64,
        name: super::maintenance_validation::Span,
        kind: super::maintenance_validation::Span,
        column: u32,
        key_type: u8,
        primary: bool,
        builtin: bool,
        storage_bytes: u64,
    },
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct NativeMetadataTraceFacts {
    pub(crate) tables: u64,
    pub(crate) groups: u64,
    pub(crate) chunks: u64,
    pub(crate) indexes: u64,
    pub(crate) consumed_bytes: u64,
}
impl<S: ByteSource + ?Sized> CatalogCursor<'_, S> {
    fn compression_metadata(
        &mut self,
        available: u64,
        allow_alp: bool,
    ) -> Result<NativePrimitiveMetadata, NativeUnknown> {
        let page = self.u32()?;
        let pages = self.u32()?;
        let values = self.u64()?;
        let min = self.array()?;
        let max = self.array()?;
        let compression = self.byte()?;
        if compression > 4 || (!allow_alp && compression == 4) {
            return Err(self.fail(90));
        }
        // Metadata ranges are physical framing facts. Constant/parent-string chunks
        // may have no physical DATA pages. Actual scalar readers impose their own
        // mode/width/page/numValues requirements before any DATA access.
        if pages != 0 {
            check_range(
                NativePageRange {
                    start: page,
                    count: pages,
                },
                available,
            )?;
        }
        if compression == 4 {
            self.skip(10)?; // exp:u8,fac:u8,exceptionCount:u32,exceptionCapacity:u32
            let _child_min = self.array::<16>()?;
            let _child_max = self.array::<16>()?;
            let child = self.byte()?;
            if !matches!(child, 0 | 1 | 3) {
                return Err(self.fail(91));
            }
        }
        Ok(NativePrimitiveMetadata {
            page,
            pages,
            values,
            min,
            max,
            compression,
        })
    }
    fn chunk_data(
        &mut self,
        available: u64,
        depth: u8,
    ) -> Result<(u8, u8, NativePrimitiveMetadata), NativeUnknown> {
        if depth > 1 {
            return Err(self.fail(92));
        }
        let (logical, physical) = self.primitive()?;
        let metadata = self.compression_metadata(available, physical == 11)?;
        let _compression_enabled = self.flag()?;
        let has_null = self.flag()?;
        if has_null {
            let _null = self.compression_metadata(available, false)?;
        }
        if matches!(physical, 20 | 21) {
            if depth != 0 {
                return Err(self.fail(93));
            }
            let (index_l, index_p, index) = self.chunk_data(available, 1)?;
            let (offset_l, offset_p, _offset) = self.chunk_data(available, 1)?;
            let (data_l, data_p, _data) = self.chunk_data(available, 1)?;
            if (index_l, index_p) != (28, 7)
                || (offset_l, offset_p) != (27, 6)
                || (data_l, data_p) != (30, 9)
                || index.values != metadata.values
            {
                return Err(self.fail(94));
            }
        }
        Ok((logical, physical, metadata))
    }
    fn versions(&mut self) -> Result<(), NativeUnknown> {
        if !self.flag()? {
            return Ok(());
        }
        let count = self.u64()?;
        for _ in 0..count {
            if !self.flag()? {
                continue;
            }
            let insert = self.byte()?;
            let deletion = self.byte()?;
            if !matches!(insert, 0 | 2) || deletion > 1 {
                return Err(self.fail(95));
            }
            if deletion == 1 {
                let same = self.u64()?;
                if same == u64::MAX {
                    for _ in 0..2048 {
                        let value = self.u64()?;
                        if value != u64::MAX && value >= (1u64 << 63) {
                            return Err(self.fail(96));
                        }
                    }
                } else if same >= (1u64 << 63) {
                    return Err(self.fail(97));
                }
            }
        }
        // A missing trailing vector entry is implicitly inserted/not deleted; no
        // vector-count equality with ceil(row_count/2048) is asserted here.
        Ok(())
    }
}
/// First-checkpoint metadata framing only: no string DATA/row extraction,
/// physical index loading, origin, source-stability or usability claim.
pub(crate) fn trace_native_metadata43<S: ByteSource + ?Sized>(
    source: &S,
    memory: &CleanupMemory,
    emit: &mut impl FnMut(NativeMetadataEvent) -> Result<(), NativeUnknown>,
) -> Result<NativeMetadataTraceFacts, NativeUnknown> {
    let floor = std::mem::size_of::<[u64; PROFILE_TABLES]>()
        .checked_add(std::mem::size_of::<[bool; PROFILE_TABLES]>())
        .and_then(|n| n.checked_add(std::mem::size_of::<CatalogCursor<'_, S>>()))
        .and_then(|n| n.checked_add(5 * std::mem::size_of::<NativePrimitiveMetadata>()))
        .and_then(|n| n.checked_add(std::mem::size_of::<NativeMetadataEvent>()))
        .and_then(|n| n.checked_add(512))
        .ok_or(NativeUnknown::Overflow)?;
    let _reservation = memory.reserve(floor).map_err(ValidationIssue::from)?;
    let mut nodes = [u64::MAX; PROFILE_TABLES];
    let mut next = 0;
    let catalog = trace_native_catalog43(source, memory, &mut |event| {
        if let NativeCatalogEvent::Table { id, node: true, .. } = event {
            nodes[next] = id;
            next += 1;
        }
        Ok(())
    })?;
    let header = scan_native_header43(source, memory)?;
    if header.metadata.start == INVALID_PAGE {
        return Err(NativeUnknown::UnsupportedLayout);
    }
    let start = u64::from(header.metadata.start)
        .checked_mul(PAGE_BYTES)
        .ok_or(NativeUnknown::Overflow)?;
    let end = u64::from(header.metadata.count)
        .checked_mul(PAGE_BYTES)
        .and_then(|n| start.checked_add(n))
        .ok_or(NativeUnknown::Overflow)?;
    let mut reader = CatalogCursor::new(source, start, end)?;
    let tables = reader.u64()?;
    if tables != catalog.nodes {
        return Err(reader.fail(98));
    }
    let mut seen = [false; PROFILE_TABLES];
    let mut facts = NativeMetadataTraceFacts {
        tables,
        ..Default::default()
    };
    for _ in 0..tables {
        let table = reader.u64()?;
        let Some(slot) = nodes[..next].iter().position(|id| *id == table) else {
            return Err(reader.fail(table));
        };
        if seen[slot] {
            return Err(reader.fail(99));
        }
        seen[slot] = true;
        let groups = reader.u64()?;
        facts.groups = facts
            .groups
            .checked_add(groups)
            .ok_or(NativeUnknown::Overflow)?;
        emit(NativeMetadataEvent::Table { id: table, groups })?;
        for _ in 0..groups {
            let group = reader.u64()?;
            let _compression = reader.flag()?;
            if reader.byte()? != 0 {
                return Err(reader.fail(100));
            }
            let ready = reader.flag()?;
            if !ready {
                emit(NativeMetadataEvent::Group {
                    table,
                    group,
                    columns: 0,
                    rows: 0,
                    start: 0,
                })?;
                continue;
            }
            let columns = reader.u64()?;
            if columns > PROFILE_PROPERTIES as u64 {
                return Err(reader.fail(columns));
            }
            let mut rows = None;
            for column in 0..columns {
                let _compression_enabled = reader.flag()?;
                let segments = reader.u64()?;
                let mut total = 0u64;
                for segment in 0..segments {
                    let (logical, physical, metadata) =
                        reader.chunk_data(header.available_pages, 0)?;
                    total = total
                        .checked_add(metadata.values)
                        .ok_or(NativeUnknown::Overflow)?;
                    facts.chunks = facts.chunks.checked_add(1).ok_or(NativeUnknown::Overflow)?;
                    emit(NativeMetadataEvent::Chunk {
                        table,
                        group,
                        column,
                        segment,
                        logical,
                        physical,
                        metadata,
                    })?;
                }
                if rows.is_some_and(|n| n != total) {
                    return Err(reader.fail(101));
                }
                rows = Some(total);
            }
            let start = reader.u64()?;
            reader.versions()?;
            emit(NativeMetadataEvent::Group {
                table,
                group,
                columns,
                rows: rows.unwrap_or(0),
                start,
            })?;
        }
        let _cardinality = reader.u64()?;
        let stats = reader.u64()?;
        for _ in 0..stats {
            if reader.flag()? {
                reader.skip(64)?;
            }
        }
        let indexes = reader.u64()?;
        facts.indexes = facts
            .indexes
            .checked_add(indexes)
            .ok_or(NativeUnknown::Overflow)?;
        for _ in 0..indexes {
            let name = reader.text(memory)?;
            let kind = reader.text(memory)?;
            if !reader.matches(kind, b"HASH")? || reader.u64()? != table || reader.u64()? != 1 {
                return Err(reader.fail(102));
            }
            let column = reader.u32()?;
            if reader.u64()? != 1 {
                return Err(reader.fail(103));
            }
            let key_type = reader.byte()?;
            let primary = reader.flag()?;
            let builtin = reader.flag()?;
            let storage_bytes = reader.u64()?;
            reader.skip(storage_bytes)?;
            emit(NativeMetadataEvent::Index {
                table,
                name,
                kind,
                column,
                key_type,
                primary,
                builtin,
                storage_bytes,
            })?;
        }
    }
    facts.consumed_bytes = reader.at - start;
    Ok(facts)
}

// Selected strings are resolved before a callback and contain no group-local
// references. The only retained stream is the dictionary's unsigned byte DATA.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct NativeStringLocator {
    source: u8,
    compression: u8,
    bits: u8,
    reserved: [u8; 5],
    page: u32,
    pages: u32,
    values: u64,
    min: u64,
    max: u64,
    pub(crate) start: u64,
    pub(crate) length: u64,
}
const _: [(); 56] = [(); std::mem::size_of::<NativeStringLocator>()];

struct NativeDataScratch {
    page: [u8; 4096],
    tag: Option<u32>,
}
impl NativeDataScratch {
    fn new() -> Self {
        Self {
            page: [0; 4096],
            tag: None,
        }
    }
    fn read<S: ByteSource + ?Sized>(
        &mut self,
        source: &S,
        page: u32,
        offset: usize,
        out: &mut [u8],
    ) -> Result<(), NativeUnknown> {
        if offset.checked_add(out.len()).is_none_or(|n| n > 4096) {
            return Err(NativeUnknown::InvalidRange);
        }
        if self.tag != Some(page) {
            self.tag = None;
            let base = u64::from(page)
                .checked_mul(PAGE_BYTES)
                .ok_or(NativeUnknown::Overflow)?;
            let mut done = 0;
            while done < self.page.len() {
                let n = source.read_at(base + done as u64, &mut self.page[done..])?;
                if n == 0 || n > self.page.len() - done {
                    return Err(NativeUnknown::Truncated);
                }
                done += n;
            }
            self.tag = Some(page);
        }
        out.copy_from_slice(&self.page[offset..offset + out.len()]);
        Ok(())
    }
}
fn unsigned_bounds(m: NativePrimitiveMetadata, bits: u8) -> Result<(u64, u64), NativeUnknown> {
    if !matches!(bits, 1 | 8 | 32 | 64) {
        return Err(NativeUnknown::UnsupportedLayout);
    }
    let bytes = usize::from(bits.div_ceil(8));
    if m.min[bytes..].iter().any(|v| *v != 0) || m.max[bytes..].iter().any(|v| *v != 0) {
        return Err(NativeUnknown::UnsupportedLayout);
    }
    let min = u64::from_le_bytes(m.min[..8].try_into().unwrap());
    let max = u64::from_le_bytes(m.max[..8].try_into().unwrap());
    if min > max || (bits == 1 && max > 1) {
        return Err(NativeUnknown::InvalidRange);
    }
    Ok((min, max))
}
fn scalar<S: ByteSource + ?Sized>(
    source: &S,
    scratch: &mut NativeDataScratch,
    m: NativePrimitiveMetadata,
    bits: u8,
    ordinal: u64,
) -> Result<u64, NativeUnknown> {
    if ordinal >= m.values {
        return Err(NativeUnknown::InvalidRange);
    }
    let (min, max) = unsigned_bounds(m, bits)?;
    let (width, offset) = match m.compression {
        3 => {
            return if min == max {
                Ok(min)
            } else {
                Err(NativeUnknown::InvalidRange)
            }
        }
        0 if bits != 1 => (u32::from(bits), 0),
        2 if bits == 1 => (1, 0),
        1 if bits != 1 => {
            let full = 64 - max.leading_zeros();
            let delta = 64 - (max - min).leading_zeros();
            if min > 0 && delta < full {
                (delta, min)
            } else {
                (full, 0)
            }
        }
        _ => return Err(NativeUnknown::UnsupportedLayout),
    };
    // Equal-value producer chunks use CONSTANT; a zero-width packed mode has
    // not been observed from the qualified writer and is kept unknown.
    if width == 0
        || width > u32::from(bits)
        || m.page == INVALID_PAGE
        || m.page == 0
        || m.pages == 0
    {
        return Err(NativeUnknown::InvalidRange);
    }
    let per_page = PAGE_BYTES * 8 / u64::from(width);
    let page_index = ordinal / per_page;
    if page_index >= u64::from(m.pages) {
        return Err(NativeUnknown::InvalidRange);
    }
    let page = m
        .page
        .checked_add(u32::try_from(page_index).map_err(|_| NativeUnknown::Overflow)?)
        .ok_or(NativeUnknown::Overflow)?;
    let bit = (ordinal % per_page) * u64::from(width);
    let shift = (bit % 8) as u32;
    let bytes = ((shift + width).div_ceil(8)) as usize;
    let mut encoded = [0u8; 12];
    scratch.read(source, page, (bit / 8) as usize, &mut encoded[..bytes])?;
    let mut assembled = 0u128;
    for (i, b) in encoded[..bytes].iter().enumerate() {
        assembled |= u128::from(*b) << (i * 8);
    }
    let mask = (1u128 << width) - 1;
    let value = u64::try_from((assembled >> shift) & mask)
        .map_err(|_| NativeUnknown::Overflow)?
        .checked_add(offset)
        .ok_or(NativeUnknown::Overflow)?;
    // Uncompressed metadata may carry no meaningful extrema, but compressed
    // ranges define the frame and must cover each decoded scalar.
    if m.compression != 0 && (value < min || value > max) {
        return Err(NativeUnknown::InvalidRange);
    }
    Ok(value)
}
impl NativeStringLocator {
    fn metadata(self) -> NativePrimitiveMetadata {
        let mut min = [0; 16];
        let mut max = [0; 16];
        min[..8].copy_from_slice(&self.min.to_le_bytes());
        max[..8].copy_from_slice(&self.max.to_le_bytes());
        NativePrimitiveMetadata {
            page: self.page,
            pages: self.pages,
            values: self.values,
            min,
            max,
            compression: self.compression,
        }
    }
}
/// A logical UTF8 source with an independent DATA cache. Caller retains the
/// immutable source incarnation; this wrapper neither opens nor owns a path.
pub(crate) struct NativeStringSource<'a, S: ByteSource + ?Sized> {
    source: &'a S,
    locator: NativeStringLocator,
    scratch: std::cell::RefCell<NativeDataScratch>,
    _floor: super::maintenance_budget::MemoryReservation,
}
impl<'a, S: ByteSource + ?Sized> NativeStringSource<'a, S> {
    pub(crate) fn new(
        source: &'a S,
        locator: NativeStringLocator,
        memory: &CleanupMemory,
    ) -> Result<Self, NativeUnknown> {
        if locator.source != 0
            || locator.bits != 8
            || locator.reserved != [0; 5]
            || locator
                .start
                .checked_add(locator.length)
                .is_none_or(|end| end > locator.values)
        {
            return Err(NativeUnknown::InvalidRange);
        }
        let floor = memory
            .reserve(std::mem::size_of::<Self>())
            .map_err(ValidationIssue::from)?;
        Ok(Self {
            source,
            locator,
            scratch: std::cell::RefCell::new(NativeDataScratch::new()),
            _floor: floor,
        })
    }
}
impl<S: ByteSource + ?Sized> ByteSource for NativeStringSource<'_, S> {
    fn len(&self) -> u64 {
        self.locator.length
    }
    fn read_at(&self, offset: u64, out: &mut [u8]) -> Result<usize, ValidationIssue> {
        let remaining = self
            .locator
            .length
            .checked_sub(offset)
            .ok_or(ValidationIssue::InvalidSpan)?;
        let n = remaining.min(out.len() as u64) as usize;
        let mut scratch = self.scratch.borrow_mut();
        for (i, b) in out[..n].iter_mut().enumerate() {
            *b = scalar(
                self.source,
                &mut scratch,
                self.locator.metadata(),
                8,
                self.locator.start + offset + i as u64,
            )
            .map_err(native_validation_issue)? as u8;
        }
        Ok(n)
    }
}
fn native_validation_issue(e: NativeUnknown) -> ValidationIssue {
    match e {
        NativeUnknown::Source(e) => e,
        NativeUnknown::Overflow => ValidationIssue::Overflow,
        _ => ValidationIssue::Schema,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NativeSelectedTable {
    Repository,
    SourceRoot,
    File,
    Contains,
}
impl NativeSelectedTable {
    fn fields(self) -> [&'static [u8]; 5] {
        match self {
            Self::Repository | Self::SourceRoot => [b"id", b"label", b"kind", b"path", b"metadata"],
            Self::File => [b"id", b"kind", b"path", b"content_hash", b"metadata"],
            Self::Contains => [b"id", b"kind", b"source_id", b"target_id", b"metadata"],
        }
    }
}
#[derive(Debug, Clone, Copy)]
pub(crate) struct NativeSelectedRow {
    pub(crate) table: NativeSelectedTable,
    pub(crate) ordinal: u64,
    /// SQL NULL is None; an empty string is Some(locator with length zero).
    pub(crate) fields: [Option<NativeStringLocator>; 5],
    /// Quoted JSON string span in the logical metadata field, if present.
    pub(crate) canonical_key: Option<super::maintenance_validation::Span>,
}
#[derive(Clone, Copy)]
struct SelectedMap {
    table: u64,
    kind: NativeSelectedTable,
    properties: [u32; 5],
    columns: [u32; 5],
}
#[derive(Clone, Copy)]
struct StringSegment {
    logical: u8,
    physical: u8,
    parent: NativePrimitiveMetadata,
    null: Option<NativePrimitiveMetadata>,
    index: NativePrimitiveMetadata,
    offsets: NativePrimitiveMetadata,
    data: NativePrimitiveMetadata,
}
impl<S: ByteSource + ?Sized> CatalogCursor<'_, S> {
    fn string_segment(&mut self, available: u64) -> Result<StringSegment, NativeUnknown> {
        let ty = self.primitive()?;
        if !matches!(ty, (50, 20) | (60, 21)) {
            return Err(self.fail(110));
        }
        let parent = self.compression_metadata(available, false)?;
        self.flag()?;
        let null = if self.flag()? {
            Some(self.compression_metadata(available, false)?)
        } else {
            None
        };
        let mut child = |expected| -> Result<NativePrimitiveMetadata, NativeUnknown> {
            if self.primitive()? != expected {
                return Err(self.fail(111));
            }
            let m = self.compression_metadata(available, false)?;
            self.flag()?;
            if self.flag()? {
                return Err(self.fail(112));
            }
            Ok(m)
        };
        let index = child((28, 7))?;
        let offsets = child((27, 6))?;
        let data = child((30, 9))?;
        if index.values != parent.values || null.is_some_and(|n| n.values != parent.values) {
            return Err(self.fail(113));
        }
        Ok(StringSegment {
            logical: ty.0,
            physical: ty.1,
            parent,
            null,
            index,
            offsets,
            data,
        })
    }
}
fn check_selected_segment_ranges(
    segment: StringSegment,
    header: NativeHeader43Facts,
) -> Result<(), NativeUnknown> {
    for m in [
        Some(segment.parent),
        segment.null,
        Some(segment.index),
        Some(segment.offsets),
        Some(segment.data),
    ]
    .into_iter()
    .flatten()
    {
        if m.pages != 0 {
            if m.page == 0 {
                return Err(NativeUnknown::InvalidRange);
            }
            let start = u64::from(m.page);
            let end = start + u64::from(m.pages);
            for range in [header.catalog, header.metadata] {
                let r = u64::from(range.start);
                if start < r + u64::from(range.count) && r < end {
                    return Err(NativeUnknown::InvalidRange);
                }
            }
        }
    }
    Ok(())
}
struct ColumnCursor<'a, S: ByteSource + ?Sized> {
    reader: CatalogCursor<'a, S>,
    remaining: u64,
    end_row: u64,
    start_row: u64,
    current: Option<StringSegment>,
}
impl<'a, S: ByteSource + ?Sized> ColumnCursor<'a, S> {
    fn new(
        source: &'a S,
        span: super::maintenance_validation::Span,
    ) -> Result<Self, NativeUnknown> {
        let mut reader = CatalogCursor::new(source, span.start, span.end)?;
        reader.flag()?;
        let remaining = reader.u64()?;
        Ok(Self {
            reader,
            remaining,
            end_row: 0,
            start_row: 0,
            current: None,
        })
    }
    fn value(
        &mut self,
        row: u64,
        header: NativeHeader43Facts,
        scratch: &mut NativeDataScratch,
    ) -> Result<Option<NativeStringLocator>, NativeUnknown> {
        while self.current.is_none() || row >= self.end_row {
            if self.remaining == 0 {
                return Err(self.reader.fail(114));
            }
            self.current = Some(
                self.reader
                    .string_segment(u64::from(header.data_file_pages))?,
            );
            self.remaining -= 1;
            self.start_row = self.end_row;
            self.end_row = self
                .end_row
                .checked_add(self.current.unwrap().parent.values)
                .ok_or(NativeUnknown::Overflow)?;
        }
        let segment = self.current.unwrap();
        check_selected_segment_ranges(segment, header)?;
        let ordinal = row
            .checked_sub(self.start_row)
            .ok_or(NativeUnknown::InvalidRange)?;
        if let Some(null) = segment.null {
            if scalar(self.reader.source, scratch, null, 1, ordinal)? != 0 {
                return Ok(None);
            }
        }
        let index = scalar(self.reader.source, scratch, segment.index, 32, ordinal)?;
        if index >= segment.offsets.values {
            return Err(self.reader.fail(115));
        }
        let start = scalar(self.reader.source, scratch, segment.offsets, 64, index)?;
        let end = if index + 1 < segment.offsets.values {
            scalar(self.reader.source, scratch, segment.offsets, 64, index + 1)?
        } else {
            segment.data.values
        };
        if start > end || end > segment.data.values {
            return Err(self.reader.fail(116));
        }
        let (min, max) = unsigned_bounds(segment.data, 8)?;
        Ok(Some(NativeStringLocator {
            source: 0,
            compression: segment.data.compression,
            bits: 8,
            reserved: [0; 5],
            page: segment.data.page,
            pages: segment.data.pages,
            values: segment.data.values,
            min,
            max,
            start,
            length: end - start,
        }))
    }
}
struct VersionCursor<'a, S: ByteSource + ?Sized> {
    reader: CatalogCursor<'a, S>,
    vectors: u64,
    insert: bool,
    deletion: u8,
    same: u64,
}
impl<'a, S: ByteSource + ?Sized> VersionCursor<'a, S> {
    fn new(source: &'a S, start: u64, end: u64) -> Result<Self, NativeUnknown> {
        let mut reader = CatalogCursor::new(source, start, end)?;
        let vectors = if reader.flag()? { reader.u64()? } else { 0 };
        Ok(Self {
            reader,
            vectors,
            insert: true,
            deletion: 0,
            same: 0,
        })
    }
    fn visible(&mut self, row: u64) -> Result<bool, NativeUnknown> {
        if row % 2048 == 0 {
            self.insert = true;
            self.deletion = 0;
            self.same = 0;
            if row / 2048 < self.vectors && self.reader.flag()? {
                self.insert = self.reader.byte()? == 2;
                self.deletion = self.reader.byte()?;
                if self.deletion == 1 {
                    self.same = self.reader.u64()?;
                }
            }
        }
        let deleted = if self.deletion == 0 {
            false
        } else if self.same == u64::MAX {
            self.reader.u64()? != u64::MAX
        } else {
            true
        };
        Ok(self.insert && !deleted)
    }
}

struct RowStringSource<'a, 'b, S: ByteSource + ?Sized> {
    source: &'a S,
    locator: NativeStringLocator,
    scratch: &'b std::cell::RefCell<NativeDataScratch>,
}
impl<S: ByteSource + ?Sized> ByteSource for RowStringSource<'_, '_, S> {
    fn len(&self) -> u64 {
        self.locator.length
    }
    fn read_at(&self, offset: u64, out: &mut [u8]) -> Result<usize, ValidationIssue> {
        let n = self
            .locator
            .length
            .checked_sub(offset)
            .ok_or(ValidationIssue::InvalidSpan)?
            .min(out.len() as u64) as usize;
        let mut scratch = self.scratch.borrow_mut();
        for (i, b) in out[..n].iter_mut().enumerate() {
            *b = scalar(
                self.source,
                &mut scratch,
                self.locator.metadata(),
                8,
                self.locator.start + offset + i as u64,
            )
            .map_err(native_validation_issue)? as u8;
        }
        Ok(n)
    }
}
fn validate_logical_utf8(
    source: &impl ByteSource,
    memory: &CleanupMemory,
) -> Result<(), NativeUnknown> {
    super::maintenance_validation::key_stream_hash(
        source,
        super::maintenance_validation::Span {
            start: 0,
            end: source.len(),
        },
        super::maintenance_validation::KeyEncoding::NativeUtf8,
        0,
        memory,
    )?;
    Ok(())
}
fn logical_matches(source: &impl ByteSource, text: &[u8]) -> Result<bool, NativeUnknown> {
    if source.len() != text.len() as u64 {
        return Ok(false);
    }
    let mut byte = [0];
    for (i, b) in text.iter().enumerate() {
        if source.read_at(i as u64, &mut byte)? != 1 {
            return Err(NativeUnknown::Truncated);
        }
        if byte[0] != *b {
            return Ok(false);
        }
    }
    Ok(true)
}
struct LogicalRead<'a, S: ByteSource + ?Sized> {
    source: &'a S,
    at: u64,
    error: &'a std::cell::Cell<Option<ValidationIssue>>,
}
impl<S: ByteSource + ?Sized> std::io::Read for LogicalRead<'_, S> {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        let n = self.source.read_at(self.at, out).map_err(|e| {
            self.error.set(Some(e));
            // The tokenizer retries Interrupted as ordinary I/O; a source's
            // cancellation/stale error is terminal and must retain its type.
            std::io::Error::from(std::io::ErrorKind::InvalidData)
        })?;
        self.at = self.at.checked_add(n as u64).ok_or_else(|| {
            self.error.set(Some(ValidationIssue::Overflow));
            std::io::Error::from(std::io::ErrorKind::InvalidData)
        })?;
        Ok(n)
    }
}
fn metadata_canonical_key(
    source: &impl ByteSource,
    memory: &CleanupMemory,
) -> Result<Option<super::maintenance_validation::Span>, NativeUnknown> {
    use super::maintenance_json::{MaintenanceJsonReader, TokenKind};
    use super::maintenance_validation::{compare_native_json, Span};
    let source_error = std::cell::Cell::new(None);
    let mut reader = MaintenanceJsonReader::new(
        LogicalRead {
            source,
            at: 0,
            error: &source_error,
        },
        memory,
    )
    .map_err(ValidationIssue::from)?;
    let mut depth = 0u64;
    let mut canonical = false;
    let mut result = None;
    while let Some(token) = reader.next_token().map_err(|e| {
        NativeUnknown::Source(
            source_error
                .take()
                .unwrap_or_else(|| ValidationIssue::from(e)),
        )
    })? {
        if canonical {
            if !matches!(token.kind, TokenKind::String { key: false, .. }) || result.is_some() {
                return Err(NativeUnknown::UnsupportedLayout);
            }
            result = Some(Span {
                start: token.start,
                end: token.end,
            });
            canonical = false;
        }
        match token.kind {
            TokenKind::ObjectStart => {
                depth += 1;
                if depth == 1 && token.start != 0 {
                    return Err(NativeUnknown::UnsupportedLayout);
                }
            }
            TokenKind::ArrayStart => {
                if depth == 0 {
                    return Err(NativeUnknown::UnsupportedLayout);
                }
                depth += 1;
            }
            TokenKind::ObjectEnd | TokenKind::ArrayEnd => {
                depth = depth.checked_sub(1).ok_or(NativeUnknown::InvalidRange)?;
            }
            TokenKind::String { key: true, .. } if depth == 1 => {
                canonical = compare_native_json(
                    b"canonical_key".as_slice(),
                    Span { start: 0, end: 13 },
                    source,
                    Span {
                        start: token.start,
                        end: token.end,
                    },
                    memory,
                )? == std::cmp::Ordering::Equal;
            }
            _ if depth == 0 => return Err(NativeUnknown::UnsupportedLayout),
            _ => {}
        }
    }
    if depth != 0 || canonical {
        return Err(NativeUnknown::UnsupportedLayout);
    }
    Ok(result)
}

/// Visit the four selected NODE tables. Framing and DATA facts are revoked by
/// any later error. `check` must enforce cancellation and source-incarnation
/// stability; it runs before/after every pass and before every emitted row.
/// Table/schema provenance and cache association remain separate consumers.
pub(crate) fn scan_native_selected43<S: ByteSource + ?Sized>(
    source: &S,
    memory: &CleanupMemory,
    check: &mut impl FnMut() -> Result<(), NativeUnknown>,
    emit: &mut impl FnMut(NativeSelectedRow) -> Result<(), NativeUnknown>,
) -> Result<u64, NativeUnknown> {
    use super::maintenance_validation::Span;
    check()?;
    let floor = std::mem::size_of::<[SelectedMap; 4]>()
        .checked_add(std::mem::size_of::<[Option<ColumnCursor<'_, S>>; 5]>())
        .and_then(|n| n.checked_add(std::mem::size_of::<CatalogCursor<'_, S>>()))
        .and_then(|n| n.checked_add(std::mem::size_of::<VersionCursor<'_, S>>()))
        .and_then(|n| n.checked_add(std::mem::size_of::<NativeDataScratch>()))
        .and_then(|n| n.checked_add(std::mem::size_of::<NativeSelectedRow>() * 2))
        .and_then(|n| n.checked_add(std::mem::size_of::<StringSegment>() * 2))
        .and_then(|n| n.checked_add(std::mem::size_of::<NativePrimitiveMetadata>() * 5))
        .and_then(|n| n.checked_add(std::mem::size_of::<[Span; 5]>() + 512))
        .ok_or(NativeUnknown::Overflow)?;
    let _floor = memory.reserve(floor).map_err(ValidationIssue::from)?;
    let mut maps = std::array::from_fn::<_, 4, _>(|i| SelectedMap {
        table: u64::MAX,
        kind: [
            NativeSelectedTable::Repository,
            NativeSelectedTable::SourceRoot,
            NativeSelectedTable::File,
            NativeSelectedTable::Contains,
        ][i],
        properties: [u32::MAX; 5],
        columns: [u32::MAX; 5],
    });
    let header = scan_native_header43(source, memory)?;
    let matcher = CatalogCursor::new(source, 0, source.len())?;
    trace_native_catalog43(source, memory, &mut |event| {
        match event {
            NativeCatalogEvent::Table {
                id,
                name,
                node: true,
            } => {
                for (i, n) in [
                    b"Repository".as_slice(),
                    b"SourceRoot",
                    b"File",
                    b"Contains",
                ]
                .iter()
                .enumerate()
                {
                    if matcher.matches(name, n)? {
                        if maps[i].table != u64::MAX {
                            return Err(NativeUnknown::UnsupportedLayout);
                        }
                        maps[i].table = id;
                    }
                }
            }
            NativeCatalogEvent::Property {
                table,
                id,
                name,
                logical,
                physical,
                ..
            } => {
                if let Some(m) = maps.iter_mut().find(|m| m.table == table) {
                    for (i, n) in m.kind.fields().iter().enumerate() {
                        if matcher.matches(name, n)? {
                            if (logical, physical) != if i == 4 { (60, 21) } else { (50, 20) }
                                || m.properties[i] != u32::MAX
                            {
                                return Err(NativeUnknown::UnsupportedLayout);
                            }
                            m.properties[i] = id;
                        }
                    }
                }
            }
            NativeCatalogEvent::Column {
                table,
                property,
                column,
            } => {
                if let Some(m) = maps.iter_mut().find(|m| m.table == table) {
                    for (i, p) in m.properties.iter().enumerate() {
                        if *p == property {
                            m.columns[i] = column;
                        }
                    }
                }
            }
            _ => {}
        }
        Ok(())
    })?;
    drop(matcher);
    if maps
        .iter()
        .any(|m| m.table == u64::MAX || m.columns.contains(&u32::MAX))
    {
        return Err(NativeUnknown::UnsupportedLayout);
    }
    // Complete framing validates duplicate/missing metadata tables and all
    // version trailers before any selected row callback can be accepted.
    trace_native_metadata43(source, memory, &mut |_| check())?;
    check()?;
    if header.catalog.start == 0
        || header.metadata.start == 0
        || header.data_file_pages == 0
        || u64::from(header.catalog.start) + u64::from(header.catalog.count)
            > u64::from(header.data_file_pages)
        || u64::from(header.metadata.start) + u64::from(header.metadata.count)
            > u64::from(header.data_file_pages)
    {
        return Err(NativeUnknown::InvalidRange);
    }
    let a = u64::from(header.catalog.start);
    let b = u64::from(header.metadata.start);
    if a < b + u64::from(header.metadata.count) && b < a + u64::from(header.catalog.count) {
        return Err(NativeUnknown::InvalidRange);
    }
    let start = u64::from(header.metadata.start) * PAGE_BYTES;
    let end = start + u64::from(header.metadata.count) * PAGE_BYTES;
    let mut reader = CatalogCursor::new(source, start, end)?;
    let tables = reader.u64()?;
    let mut emitted = 0u64;
    let scratch = std::cell::RefCell::new(NativeDataScratch::new());
    for _ in 0..tables {
        check()?;
        let table = reader.u64()?;
        let groups = reader.u64()?;
        let selected = maps.iter().find(|m| m.table == table).copied();
        for _ in 0..groups {
            check()?;
            reader.u64()?;
            reader.flag()?;
            if reader.byte()? != 0 {
                return Err(reader.fail(117));
            }
            if !reader.flag()? {
                continue;
            }
            let columns = reader.u64()?;
            let mut spans = [Span::default(); 5];
            let mut rows = 0;
            for column in 0..columns {
                let begin = reader.at;
                reader.flag()?;
                let segments = reader.u64()?;
                let mut total = 0u64;
                for _ in 0..segments {
                    let field = selected
                        .and_then(|m| m.columns.iter().position(|c| u64::from(*c) == column));
                    let values = if let Some(field) = field {
                        let segment = reader.string_segment(u64::from(header.data_file_pages))?;
                        let expected = if field == 4 { (60, 21) } else { (50, 20) };
                        if (segment.logical, segment.physical) != expected {
                            return Err(reader.fail(121));
                        }
                        check_selected_segment_ranges(segment, header)?;
                        segment.parent.values
                    } else {
                        reader.chunk_data(header.available_pages, 0)?.2.values
                    };
                    total = total.checked_add(values).ok_or(NativeUnknown::Overflow)?;
                }
                rows = total;
                if let Some(m) = selected {
                    for (i, c) in m.columns.iter().enumerate() {
                        if u64::from(*c) == column {
                            spans[i] = Span {
                                start: begin,
                                end: reader.at,
                            };
                        }
                    }
                }
            }
            let row_start = reader.u64()?;
            let version_start = reader.at;
            reader.versions()?;
            let resume = reader.at;
            if let Some(m) = selected {
                if spans.iter().any(|s| s.start == s.end) {
                    return Err(reader.fail(118));
                }
                let mut cursors = std::array::from_fn::<_, 5, _>(|_| None);
                for i in 0..5 {
                    cursors[i] = Some(ColumnCursor::new(source, spans[i])?);
                }
                let mut versions = VersionCursor::new(source, version_start, resume)?;
                for row in 0..rows {
                    check()?;
                    let visible = versions.visible(row)?;
                    if !visible {
                        continue;
                    }
                    let mut fields = [None; 5];
                    if m.kind == NativeSelectedTable::Contains {
                        fields[1] = cursors[1].as_mut().unwrap().value(
                            row,
                            header,
                            &mut scratch.borrow_mut(),
                        )?;
                        let Some(kind) = fields[1] else {
                            return Err(reader.fail(119));
                        };
                        let logical = RowStringSource {
                            source,
                            locator: kind,
                            scratch: &scratch,
                        };
                        validate_logical_utf8(&logical, memory)?;
                        if !logical_matches(&logical, b"repository_source_root")?
                            && !logical_matches(&logical, b"source_root_file")?
                        {
                            continue;
                        }
                    }
                    for i in 0..5 {
                        if fields[i].is_none() {
                            fields[i] = cursors[i].as_mut().unwrap().value(
                                row,
                                header,
                                &mut scratch.borrow_mut(),
                            )?;
                        }
                    }
                    for locator in fields.iter().flatten() {
                        validate_logical_utf8(
                            &RowStringSource {
                                source,
                                locator: *locator,
                                scratch: &scratch,
                            },
                            memory,
                        )?;
                    }
                    let canonical_key = if let Some(locator) = fields[4] {
                        metadata_canonical_key(
                            &RowStringSource {
                                source,
                                locator,
                                scratch: &scratch,
                            },
                            memory,
                        )?
                    } else {
                        None
                    };
                    if canonical_key.is_none() {
                        return Err(reader.fail(120));
                    }
                    emit(NativeSelectedRow {
                        table: m.kind,
                        ordinal: row_start.checked_add(row).ok_or(NativeUnknown::Overflow)?,
                        fields,
                        canonical_key,
                    })?;
                    emitted = emitted.checked_add(1).ok_or(NativeUnknown::Overflow)?;
                }
            }
        }
        reader.u64()?;
        let stats = reader.u64()?;
        for _ in 0..stats {
            if reader.flag()? {
                reader.skip(64)?;
            }
        }
        let indexes = reader.u64()?;
        for _ in 0..indexes {
            reader.text(memory)?;
            reader.text(memory)?;
            reader.u64()?;
            let n = reader.u64()?;
            for _ in 0..n {
                reader.u32()?;
            }
            let n = reader.u64()?;
            reader.skip(n)?;
            reader.flag()?;
            reader.flag()?;
            let n = reader.u64()?;
            reader.skip(n)?;
        }
    }
    check()?;
    Ok(emitted)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn header() -> Vec<u8> {
        let mut bytes = vec![0; 3 * PAGE_BYTES as usize];
        bytes[..4].copy_from_slice(b"LBUG");
        bytes[4..12].copy_from_slice(&43u64.to_le_bytes());
        for (at, value) in [(12, 1u32), (16, 1), (20, 2), (24, 1), (45, 3)] {
            bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
        }
        bytes[28..44].copy_from_slice(&[0x55; 16]);
        bytes[44] = 2;
        bytes
    }
    #[test]
    fn dc_native_header43_exact_layout_ranges_and_account() {
        let memory = CleanupMemory::new(4096).unwrap();
        let floor = memory.used();
        let bytes = header();
        let facts = scan_native_header43(bytes.as_slice(), &memory).unwrap();
        assert_eq!(facts.catalog, NativePageRange { start: 1, count: 1 });
        assert_eq!(facts.metadata, NativePageRange { start: 2, count: 1 });
        assert_eq!(facts.database_id, [0x55; 16]);
        assert_eq!(facts.data_file_pages, 3);
        assert_eq!(memory.used(), floor);
        let small = CleanupMemory::new(64).unwrap();
        assert!(matches!(
            scan_native_header43(bytes.as_slice(), &small),
            Err(NativeUnknown::Source(ValidationIssue::Resource(_)))
        ));
    }
    #[test]
    fn dc_native_header43_truncation_sentinel_tail_and_range_rejections() {
        let memory = CleanupMemory::new(4096).unwrap();
        let bytes = header();
        for n in 0..HEADER_BYTES {
            assert_eq!(
                scan_native_header43(&bytes[..n], &memory),
                Err(NativeUnknown::Truncated)
            );
        }
        for (at, value) in [
            (12, u32::MAX),
            (16, 0),
            (16, u32::MAX),
            (20, 3),
            (24, 2),
            (45, 4),
        ] {
            let mut invalid = bytes.clone();
            invalid[at..at + 4].copy_from_slice(&value.to_le_bytes());
            assert_eq!(
                scan_native_header43(invalid.as_slice(), &memory),
                Err(NativeUnknown::InvalidRange)
            );
        }
        let mut empty = bytes.clone();
        for at in [12, 20] {
            empty[at..at + 4].copy_from_slice(&u32::MAX.to_le_bytes());
            empty[at + 4..at + 8].fill(0);
        }
        assert!(scan_native_header43(empty.as_slice(), &memory).is_ok());
        for at in [0, 4, 44] {
            let mut invalid = bytes.clone();
            invalid[at] ^= 1;
            assert_eq!(
                scan_native_header43(invalid.as_slice(), &memory),
                Err(NativeUnknown::UnsupportedLayout)
            );
        }
        assert_eq!(
            scan_native_header43(&bytes[..bytes.len() - 1], &memory),
            Err(NativeUnknown::UnsupportedLayout)
        );
    }
    #[test]
    fn dc_native_header43_short_reads_and_inconsistent_sources() {
        struct Short<'a> {
            bytes: &'a [u8],
            stop: usize,
        }
        impl ByteSource for Short<'_> {
            fn len(&self) -> u64 {
                self.bytes.len() as u64
            }
            fn read_at(&self, offset: u64, out: &mut [u8]) -> Result<usize, ValidationIssue> {
                let at = offset as usize;
                if at >= self.stop {
                    return Ok(0);
                }
                let n = out.len().min(3).min(self.stop - at);
                out[..n].copy_from_slice(&self.bytes[at..at + n]);
                Ok(n)
            }
        }
        let memory = CleanupMemory::new(4096).unwrap();
        let bytes = header();
        assert!(scan_native_header43(
            &Short {
                bytes: &bytes,
                stop: 49
            },
            &memory
        )
        .is_ok());
        for stop in 0..49 {
            assert_eq!(
                scan_native_header43(
                    &Short {
                        bytes: &bytes,
                        stop
                    },
                    &memory
                ),
                Err(NativeUnknown::Truncated)
            );
        }
    }
    fn put64(bytes: &mut Vec<u8>, value: u64) {
        bytes.extend(value.to_le_bytes());
    }
    fn put32(bytes: &mut Vec<u8>, value: u32) {
        bytes.extend(value.to_le_bytes());
    }
    fn put_text(bytes: &mut Vec<u8>, text: &str) {
        put64(bytes, text.len() as u64);
        bytes.extend(text.as_bytes());
    }
    fn null_literal(bytes: &mut Vec<u8>, logical: u8, physical: u8) {
        bytes.push(70);
        put_text(bytes, "");
        put_text(bytes, "NULL");
        put64(bytes, 0);
        bytes.extend([logical, physical, 0, 1]);
        put32(bytes, 0);
        match physical {
            0 => {}
            1 | 9 => bytes.push(0),
            7 => bytes.extend([0x7f; 4]),
            2 | 6 | 11 => bytes.extend([0x7f; 8]),
            14 => bytes.extend([0x7f; 16]),
            20 | 21 => put_text(bytes, ""),
            _ => unreachable!(),
        }
    }
    fn catalog_fixture() -> Vec<u8> {
        let mut bytes = Vec::new();
        put64(&mut bytes, PROFILE_TABLES as u64);
        put64(&mut bytes, PROFILE_TABLES as u64);
        for id in 0..PROFILE_TABLES as u64 {
            bytes.push(0);
            put_text(&mut bytes, &format!("Table{id}"));
            put64(&mut bytes, id);
            bytes.push(0);
            put_text(&mut bytes, "");
            put32(&mut bytes, 1);
            put32(&mut bytes, 1);
            put64(&mut bytes, 1);
            put32(&mut bytes, 0);
            put_text(&mut bytes, "id");
            bytes.extend([50, 20, 0]);
            null_literal(&mut bytes, 50, 20);
            put64(&mut bytes, 1);
            put32(&mut bytes, 0);
            put32(&mut bytes, 0);
            put_text(&mut bytes, "id");
            put64(&mut bytes, 0);
            put_text(&mut bytes, "");
            bytes.push(0);
        }
        for set in 1..10 {
            if set == 3 {
                put64(&mut bytes, 1);
                put64(&mut bytes, 1);
                bytes.push(41);
                put_text(&mut bytes, "JSON");
                put64(&mut bytes, 0);
                bytes.push(0);
                bytes.extend([60, 21, 0]);
            } else {
                put64(&mut bytes, 0);
                put64(&mut bytes, 0);
            }
        }
        bytes
    }
    fn wrap_catalog(catalog: &[u8], metadata: &[u8]) -> Vec<u8> {
        let cp = (catalog.len() as u64).div_ceil(PAGE_BYTES) as u32;
        let mp = (metadata.len() as u64).div_ceil(PAGE_BYTES) as u32;
        let mut bytes = vec![0; (1 + cp + mp) as usize * PAGE_BYTES as usize];
        bytes[..4].copy_from_slice(b"LBUG");
        bytes[4..12].copy_from_slice(&43u64.to_le_bytes());
        for (at, value) in [
            (12, 1),
            (16, cp),
            (20, if mp == 0 { u32::MAX } else { 1 + cp }),
            (24, mp),
            (45, 1 + cp + mp),
        ] {
            bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
        }
        bytes[44] = 2;
        bytes[PAGE_BYTES as usize..PAGE_BYTES as usize + catalog.len()].copy_from_slice(catalog);
        if mp > 0 {
            let at = (1 + cp) as usize * PAGE_BYTES as usize;
            bytes[at..at + metadata.len()].copy_from_slice(metadata);
        }
        bytes
    }
    #[test]
    fn dc_native_catalog_typed_nulls_builtin_json_and_explicit_source_floor() {
        let memory = CleanupMemory::new(1024 * 1024).unwrap();
        let catalog = catalog_fixture();
        let bytes = wrap_catalog(&catalog, &[]);
        let _source_floor = memory.reserve(bytes.capacity()).unwrap();
        let baseline = memory.used();
        let mut properties = 0;
        let facts = trace_native_catalog43(bytes.as_slice(), &memory, &mut |event| {
            if let NativeCatalogEvent::Property { default, .. } = event {
                assert_eq!(default, NativeDefaultFacts::Null);
                properties += 1;
            }
            Ok(())
        })
        .unwrap();
        assert_eq!(facts.tables, 112);
        assert_eq!(properties, 112);
        assert_eq!(memory.used(), baseline);
        for (logical, physical) in [
            (0, 0),
            (22, 1),
            (23, 2),
            (27, 6),
            (28, 7),
            (30, 9),
            (32, 11),
            (42, 14),
            (50, 20),
            (60, 21),
        ] {
            let mut raw = Vec::new();
            null_literal(&mut raw, logical, physical);
            let mut reader = CatalogCursor::new(raw.as_slice(), 0, raw.len() as u64).unwrap();
            assert_eq!(
                reader.literal(&memory, (logical, physical)).unwrap(),
                NativeDefaultFacts::Null
            );
            assert_eq!(reader.at, raw.len() as u64);
        }
        let mut wrong = Vec::new();
        null_literal(&mut wrong, 50, 20);
        let mut reader = CatalogCursor::new(wrong.as_slice(), 0, wrong.len() as u64).unwrap();
        assert!(reader.literal(&memory, (23, 2)).is_err());
    }
    #[test]
    fn dc_native_catalog_rejects_nonbuiltin_type_duplicate_table_and_bad_suffix() {
        let memory = CleanupMemory::new(1024 * 1024).unwrap();
        let catalog = catalog_fixture();
        let bytes = wrap_catalog(&catalog, &[]);
        let mut name = None;
        trace_native_catalog43(bytes.as_slice(), &memory, &mut |event| {
            if let NativeCatalogEvent::Table {
                id: 1, name: span, ..
            } = event
            {
                name = Some(span)
            }
            Ok(())
        })
        .unwrap();
        let name = name.unwrap();
        let id_offset = name.end as usize;
        let mut duplicate = bytes.clone();
        duplicate[id_offset..id_offset + 8].copy_from_slice(&0u64.to_le_bytes());
        assert!(trace_native_catalog43(duplicate.as_slice(), &memory, &mut |_| Ok(())).is_err());
        let json_at = bytes.windows(4).position(|v| v == b"JSON").unwrap();
        let mut custom = bytes.clone();
        custom[json_at..json_at + 4].copy_from_slice(b"JUNK");
        assert!(trace_native_catalog43(custom.as_slice(), &memory, &mut |_| Ok(())).is_err());
        let mut suffix = bytes.clone();
        let end = PAGE_BYTES as usize + catalog.len();
        suffix[end - 8..end].copy_from_slice(&1u64.to_le_bytes());
        assert!(trace_native_catalog43(suffix.as_slice(), &memory, &mut |_| Ok(())).is_err());
    }
    fn metadata_descriptor(bytes: &mut Vec<u8>, values: u64, tag: u8) {
        put32(bytes, u32::MAX);
        put32(bytes, 0);
        put64(bytes, values);
        bytes.extend([0; 32]);
        bytes.push(tag);
    }
    fn primitive_chunk(bytes: &mut Vec<u8>, logical: u8, physical: u8, values: u64) {
        bytes.extend([logical, physical, 0]);
        metadata_descriptor(bytes, values, 3);
        bytes.extend([1, 0]);
    }
    #[test]
    fn dc_native_metadata_empty_dictionary_and_implicit_visible_version_tail() {
        let memory = CleanupMemory::new(1024 * 1024).unwrap();
        let mut metadata = Vec::new();
        put64(&mut metadata, 112);
        for table in 0..112 {
            put64(&mut metadata, table);
            put64(&mut metadata, if table == 0 { 1 } else { 0 });
            if table == 0 {
                put64(&mut metadata, 0);
                metadata.extend([1, 0, 1]);
                put64(&mut metadata, 1);
                metadata.push(1);
                put64(&mut metadata, 1);
                metadata.extend([50, 20, 0]);
                metadata_descriptor(&mut metadata, 4097, 0);
                metadata.extend([1, 1]);
                metadata_descriptor(&mut metadata, 4097, 3);
                primitive_chunk(&mut metadata, 28, 7, 4097);
                primitive_chunk(&mut metadata, 27, 6, 1);
                primitive_chunk(&mut metadata, 30, 9, 0);
                put64(&mut metadata, 0);
                metadata.push(1);
                put64(&mut metadata, 1);
                metadata.extend([1, 2, 1]);
                put64(&mut metadata, 1);
            }
            put64(&mut metadata, if table == 0 { 4097 } else { 0 });
            put64(&mut metadata, 1);
            metadata.push(0);
            put64(&mut metadata, 0);
        }
        let bytes = wrap_catalog(&catalog_fixture(), &metadata);
        let mut group_rows = 0;
        let facts = trace_native_metadata43(bytes.as_slice(), &memory, &mut |event| {
            if let NativeMetadataEvent::Group { rows, .. } = event {
                group_rows += rows;
            }
            Ok(())
        })
        .unwrap();
        assert_eq!(facts.tables, 112);
        assert_eq!(group_rows, 4097);
        assert_eq!(facts.chunks, 1);
        let truncated = &bytes[..bytes.len() - PAGE_BYTES as usize];
        assert!(trace_native_metadata43(truncated, &memory, &mut |_| Ok(())).is_err());
    }
    #[test]
    #[ignore = "requires separately generated synthetic public-producer fixture directory"]
    fn dc_native_actual_producer187_catalog_metadata_checkpoint() {
        use super::super::maintenance_validation::{BorrowedFileSource, ByteSource};
        use std::collections::BTreeMap;
        let root = std::path::PathBuf::from(
            std::env::var_os("DC_S3_NATIVE_FIXTURES")
                .expect("set explicit synthetic fixture directory"),
        );
        let oracle: serde_json::Value =
            serde_json::from_slice(&std::fs::read(root.join("oracle-results.json")).unwrap())
                .unwrap();
        for case in oracle["cases"].as_array().unwrap() {
            let name = case["fixture"].as_str().unwrap();
            assert!(name.starts_with("synthetic_native_"));
            let path = root
                .join(name)
                .join(".codebaseGraph")
                .join(format!("{name}_graph.ldb"));
            let mut file = std::fs::File::open(path).unwrap();
            let memory = CleanupMemory::new(1024 * 1024).unwrap();
            let source = BorrowedFileSource::new(&mut file, &memory).unwrap();
            let mut tables = BTreeMap::new();
            let table_name = |span: super::super::maintenance_validation::Span| {
                let mut bytes = vec![0; (span.end - span.start) as usize];
                let mut at = 0;
                while at < bytes.len() {
                    let n = source
                        .read_at(span.start + at as u64, &mut bytes[at..])
                        .unwrap();
                    assert!(n > 0);
                    at += n;
                }
                String::from_utf8(bytes).unwrap()
            };
            let catalog = trace_native_catalog43(&source, &memory, &mut |event| {
                if let NativeCatalogEvent::Table { id, name, node } = event {
                    tables.insert(id, (table_name(name), node));
                }
                Ok(())
            })
            .unwrap();
            let mut expected = BTreeMap::new();
            for row in case["native_oracles"].as_array().unwrap().last().unwrap()["rows"]
                .as_array()
                .unwrap()
            {
                let id = row[0]
                    .as_str()
                    .unwrap()
                    .strip_prefix("UInt64(")
                    .unwrap()
                    .strip_suffix(')')
                    .unwrap()
                    .parse::<u64>()
                    .unwrap();
                let native_name = row[1]
                    .as_str()
                    .unwrap()
                    .strip_prefix("String(")
                    .unwrap()
                    .strip_suffix(')')
                    .unwrap();
                let name = serde_json::from_str::<String>(native_name).unwrap();
                expected.insert(id, (name, row[2].as_str().unwrap() == "String(\"NODE\")"));
            }
            assert_eq!(tables, expected);
            assert_eq!(catalog.tables, 112);
            assert_eq!(catalog.indexes, 20);
            let mut rows = 0;
            let metadata = trace_native_metadata43(&source, &memory, &mut |event| {
                if let NativeMetadataEvent::Group { rows: n, .. } = event {
                    rows += n;
                }
                Ok(())
            })
            .unwrap();
            assert_eq!(metadata.tables, 62);
            assert_eq!(metadata.indexes, 20);
            let response: serde_json::Value = serde_json::from_slice(
                &std::fs::read(root.join(name).join("response.json")).unwrap(),
            )
            .unwrap();
            assert_eq!(
                rows,
                response["node_rows"].as_u64().unwrap() + response["edge_rows"].as_u64().unwrap()
            );
        }
    }
}

#[cfg(test)]
mod selected_tests {
    use super::super::maintenance_validation::{key_stream_hash, KeyEncoding, Span};
    use super::*;
    fn meta(
        page: u32,
        pages: u32,
        values: u64,
        min: u64,
        max: u64,
        compression: u8,
    ) -> NativePrimitiveMetadata {
        let mut lo = [0; 16];
        let mut hi = [0; 16];
        lo[..8].copy_from_slice(&min.to_le_bytes());
        hi[..8].copy_from_slice(&max.to_le_bytes());
        NativePrimitiveMetadata {
            page,
            pages,
            values,
            min: lo,
            max: hi,
            compression,
        }
    }
    fn packed(
        width: u8,
        offset: u64,
        count: usize,
    ) -> (Vec<u8>, NativePrimitiveMetadata, Vec<u64>) {
        let per = 32768 / usize::from(width);
        let pages = count.div_ceil(per);
        let mut bytes = vec![0; (pages + 1) * 4096];
        let mask = if width == 64 {
            u64::MAX
        } else {
            (1u64 << width) - 1
        };
        let mut expected = Vec::new();
        for row in 0..count {
            let encoded = (row as u64).wrapping_mul(0x123456789abcdef) & mask;
            let value = encoded.checked_add(offset).unwrap();
            expected.push(value);
            let bit = (row % per) * usize::from(width);
            let base = (1 + row / per) * 4096;
            for k in 0..usize::from(width) {
                if encoded & (1u64 << k) != 0 {
                    bytes[base + (bit + k) / 8] |= 1 << ((bit + k) % 8);
                }
            }
        }
        (
            bytes,
            meta(1, pages as u32, count as u64, offset, mask + offset, 1),
            expected,
        )
    }
    #[test]
    fn dc_native_scalar_unsigned_every_width_page_word_and_frame_crossing() {
        for width in 1..=64 {
            for offset in [0, if width < 63 { 1u64 << 63 } else { 0 }] {
                let count = 32768 / width as usize + 39;
                let (bytes, m, expected) = packed(width, offset, count);
                let mut scratch = NativeDataScratch::new();
                for (i, want) in expected.iter().enumerate() {
                    assert_eq!(
                        scalar(bytes.as_slice(), &mut scratch, m, 64, i as u64).unwrap(),
                        *want,
                        "width={width} row={i}"
                    );
                }
            }
        }
        for bits in [8, 32, 64] {
            let mut bytes = vec![0; 8192];
            let n = usize::from(bits) / 8;
            let value = if bits == 64 {
                u64::MAX
            } else {
                (1u64 << bits) - 1
            };
            bytes[4096..4096 + n].copy_from_slice(&value.to_le_bytes()[..n]);
            let mut scratch = NativeDataScratch::new();
            assert_eq!(
                scalar(
                    bytes.as_slice(),
                    &mut scratch,
                    meta(1, 1, 1, 0, 0, 0),
                    bits,
                    0
                )
                .unwrap(),
                value
            );
            assert_eq!(
                scalar(
                    bytes.as_slice(),
                    &mut scratch,
                    meta(INVALID_PAGE, 0, 1, value, value, 3),
                    bits,
                    0
                )
                .unwrap(),
                value
            );
        }
    }
    #[test]
    fn dc_native_scalar_null_mask_and_corrupt_ranges_are_explicit() {
        let mut bytes = vec![0; 3 * 4096];
        bytes[4096] = 0b10000001;
        bytes[8192] = 1;
        let mut scratch = NativeDataScratch::new();
        let m = meta(1, 2, 32769, 0, 1, 2);
        for row in [0, 7, 32768] {
            assert_eq!(
                scalar(bytes.as_slice(), &mut scratch, m, 1, row).unwrap(),
                1
            );
        }
        assert_eq!(scalar(bytes.as_slice(), &mut scratch, m, 1, 1).unwrap(), 0);
        for bad in [
            meta(0, 1, 1, 0, 1, 1),
            meta(1, 0, 1, 0, 1, 1),
            meta(1, 1, 1, 2, 1, 1),
            meta(1, 1, 1, 0, 0, 1),
            meta(1, 1, 1, 0, 1, 4),
        ] {
            assert!(scalar(bytes.as_slice(), &mut scratch, bad, 64, 0).is_err());
        }
        assert!(scalar(bytes.as_slice(), &mut scratch, m, 1, m.values).is_err());
        let mut bad = meta(1, 1, 1, 0, 1, 1);
        bad.max[8] = 1;
        assert!(scalar(bytes.as_slice(), &mut scratch, bad, 64, 0).is_err());
    }
    fn locator(bytes: &[u8]) -> (Vec<u8>, NativeStringLocator) {
        let pages = bytes.len().div_ceil(4096);
        let mut source = vec![0; (pages + 1) * 4096];
        source[4096..4096 + bytes.len()].copy_from_slice(bytes);
        (
            source,
            NativeStringLocator {
                source: 0,
                compression: 0,
                bits: 8,
                reserved: [0; 5],
                page: 1,
                pages: pages as u32,
                values: bytes.len() as u64,
                min: 0,
                max: 0,
                start: 0,
                length: bytes.len() as u64,
            },
        )
    }
    #[test]
    fn dc_native_logical_long_utf8_metadata_empty_and_late_errors() {
        let memory = CleanupMemory::new(100_000).unwrap();
        let floor = memory.used();
        let text = format!("{}é🙂\0{}", "a".repeat(4095), "b".repeat(100_000));
        let (bytes, l) = locator(text.as_bytes());
        {
            let source = NativeStringSource::new(bytes.as_slice(), l, &memory).unwrap();
            assert_eq!(
                key_stream_hash(
                    &source,
                    Span {
                        start: 0,
                        end: l.length
                    },
                    KeyEncoding::NativeUtf8,
                    8,
                    &memory
                )
                .unwrap(),
                key_stream_hash(
                    text.as_bytes(),
                    Span {
                        start: 0,
                        end: text.len() as u64
                    },
                    KeyEncoding::NativeUtf8,
                    8,
                    &memory
                )
                .unwrap()
            );
        }
        assert_eq!(memory.used(), floor);
        let empty = NativeStringLocator {
            page: INVALID_PAGE,
            pages: 0,
            values: 0,
            start: 0,
            length: 0,
            ..l
        };
        let source = NativeStringSource::new([].as_slice(), empty, &memory).unwrap();
        assert!(validate_logical_utf8(&source, &memory).is_ok());
        drop(source);
        let (mut bad, l) = locator(text.as_bytes());
        bad[4096 + text.len() - 1] = 0xff;
        let source = NativeStringSource::new(bad.as_slice(), l, &memory).unwrap();
        assert!(validate_logical_utf8(&source, &memory).is_err());
        drop(source);
        let json = br#"{"canonical_key":"p\u0061th","extra":{"x":[1,null]}}"#;
        let (bytes, l) = locator(json);
        let source = NativeStringSource::new(bytes.as_slice(), l, &memory).unwrap();
        let span = metadata_canonical_key(&source, &memory).unwrap().unwrap();
        assert_eq!(
            super::super::maintenance_validation::compare_native_json(
                b"path".as_slice(),
                Span { start: 0, end: 4 },
                &source,
                span,
                &memory
            )
            .unwrap(),
            std::cmp::Ordering::Equal
        );
        let (bytes, l) = locator(br#"{"canonical_key":"a","canonical_key":"b"}"#);
        let source = NativeStringSource::new(bytes.as_slice(), l, &memory).unwrap();
        assert!(metadata_canonical_key(&source, &memory).is_err());
        let small = CleanupMemory::new(64).unwrap();
        assert!(NativeStringSource::new(bytes.as_slice(), l, &small).is_err());
    }
    #[test]
    fn dc_native_visibility_deleted_early_vector_and_implicit_tail() {
        let mut bytes = vec![1];
        bytes.extend(1u64.to_le_bytes());
        bytes.extend([1, 2, 1]);
        bytes.extend(u64::MAX.to_le_bytes());
        for i in 0..2048 {
            bytes.extend(if i == 4 { 42u64 } else { u64::MAX }.to_le_bytes());
        }
        let mut version = VersionCursor::new(bytes.as_slice(), 0, bytes.len() as u64).unwrap();
        for row in 0..4097 {
            assert_eq!(version.visible(row).unwrap(), row != 4);
        }
        bytes.truncate(20);
        let mut version = VersionCursor::new(bytes.as_slice(), 0, bytes.len() as u64).unwrap();
        assert!(version.visible(0).is_err());
    }
    #[test]
    fn dc_native_scan_cancellation_precedes_any_source_read() {
        struct NoRead;
        impl ByteSource for NoRead {
            fn len(&self) -> u64 {
                0
            }
            fn read_at(&self, _: u64, _: &mut [u8]) -> Result<usize, ValidationIssue> {
                panic!("cancelled source must not read")
            }
        }
        let memory = CleanupMemory::new(100_000).unwrap();
        let mut emits = 0;
        assert_eq!(
            scan_native_selected43(
                &NoRead,
                &memory,
                &mut || Err(NativeUnknown::Source(ValidationIssue::Io(
                    std::io::ErrorKind::Interrupted
                ))),
                &mut |_| {
                    emits += 1;
                    Ok(())
                }
            ),
            Err(NativeUnknown::Source(ValidationIssue::Io(
                std::io::ErrorKind::Interrupted
            )))
        );
        assert_eq!(emits, 0);
    }
}

#[cfg(test)]
mod selected_oracle_tests {
    use super::super::maintenance_validation::{compare_native_json, BorrowedFileSource, Span};
    use super::*;
    #[test]
    #[ignore = "requires separately generated actual public-producer187 synthetic fixtures"]
    fn dc_native_actual_producer187_selected_strings_and_support_rows() {
        let root = std::path::PathBuf::from(
            std::env::var_os("DC_S3_NATIVE_FIXTURES")
                .expect("explicit synthetic fixture directory"),
        );
        let oracle: serde_json::Value =
            serde_json::from_slice(&std::fs::read(root.join("oracle-results.json")).unwrap())
                .unwrap();
        let string = |value: &serde_json::Value| -> String {
            let s = value.as_str().unwrap();
            serde_json::from_str(
                s.strip_prefix("String(")
                    .unwrap()
                    .strip_suffix(')')
                    .unwrap(),
            )
            .unwrap()
        };
        for case in oracle["cases"].as_array().unwrap() {
            let name = case["fixture"].as_str().unwrap();
            assert!(name.starts_with("synthetic_native_"));
            let mut expected = std::collections::BTreeMap::new();
            for (index, query) in case["native_oracles"].as_array().unwrap()[..4]
                .iter()
                .enumerate()
            {
                for row in query["rows"].as_array().unwrap() {
                    let id = string(&row[0]);
                    let fields = match index {
                        0 => [
                            id.clone(),
                            string(&row[1]),
                            "repository".into(),
                            string(&row[2]),
                            string(&row[1]),
                        ],
                        1 => [
                            id.clone(),
                            string(&row[1]),
                            "sourceroot".into(),
                            string(&row[2]),
                            string(&row[2]),
                        ],
                        2 => [
                            id.clone(),
                            "file".into(),
                            string(&row[1]),
                            string(&row[2]),
                            string(&row[1]),
                        ],
                        3 => {
                            let kind = string(&row[1]);
                            let source = string(&row[2]);
                            let target = string(&row[3]);
                            [
                                id.clone(),
                                kind.clone(),
                                source.clone(),
                                target.clone(),
                                format!("Contains|{source}|{target}|{kind}"),
                            ]
                        }
                        _ => unreachable!(),
                    };
                    assert!(expected.insert(id, fields).is_none());
                }
            }
            let path = root
                .join(name)
                .join(".codebaseGraph")
                .join(format!("{name}_graph.ldb"));
            let mut file = std::fs::File::open(path).unwrap();
            let memory = CleanupMemory::new(1024 * 1024).unwrap();
            let source = BorrowedFileSource::new(&mut file, &memory).unwrap();
            let mut actual = 0;
            scan_native_selected43(&source, &memory, &mut || Ok(()), &mut |row| {
                let mut fields = Vec::new();
                for locator in row.fields[..4].iter() {
                    let l = locator.unwrap();
                    let s = NativeStringSource::new(&source, l, &memory)?;
                    let mut bytes = vec![0; s.len() as usize];
                    s.read_at(0, &mut bytes)?;
                    fields.push(String::from_utf8(bytes).unwrap());
                }
                let want = expected.remove(&fields[0]).unwrap();
                assert_eq!(&fields[..], &want[..4]);
                let metadata = NativeStringSource::new(&source, row.fields[4].unwrap(), &memory)?;
                assert_eq!(
                    compare_native_json(
                        want[4].as_bytes(),
                        Span {
                            start: 0,
                            end: want[4].len() as u64
                        },
                        &metadata,
                        row.canonical_key.unwrap(),
                        &memory
                    )?,
                    std::cmp::Ordering::Equal
                );
                actual += 1;
                Ok(())
            })
            .unwrap();
            assert_eq!(actual, 7);
            assert!(expected.is_empty());
            assert!(memory.peak() < 64 * 1024);
            eprintln!(
                "{name}: exact7selected rows+5fields+canonical metadata; requestedpeak={}",
                memory.peak()
            );
            // A changed incarnation/cancellation invalidates the entire pass,
            // including rows already observed by the provisional callback.
            let mut checks = 0;
            let result = scan_native_selected43(
                &source,
                &memory,
                &mut || {
                    checks += 1;
                    if checks > 3 {
                        Err(NativeUnknown::Source(ValidationIssue::Io(
                            std::io::ErrorKind::Interrupted,
                        )))
                    } else {
                        Ok(())
                    }
                },
                &mut |_| Ok(()),
            );
            assert!(result.is_err());
            let epoch = std::cell::Cell::new(0);
            let mut provisional = 0;
            let changed = scan_native_selected43(
                &source,
                &memory,
                &mut || {
                    if epoch.get() == 0 {
                        Ok(())
                    } else {
                        Err(NativeUnknown::SourceChanged)
                    }
                },
                &mut |_| {
                    provisional += 1;
                    epoch.set(1);
                    Ok(())
                },
            );
            assert_eq!(changed, Err(NativeUnknown::SourceChanged));
            assert_eq!(provisional, 1);
        }
    }
}

#[cfg(test)]
mod dictionary_tests {
    use super::*;
    fn m(page: u32, values: u64, value: u64, compression: u8) -> NativePrimitiveMetadata {
        let mut min = [0; 16];
        min[..8].copy_from_slice(&value.to_le_bytes());
        NativePrimitiveMetadata {
            page,
            pages: if page == INVALID_PAGE { 0 } else { 1 },
            values,
            min,
            max: min,
            compression,
        }
    }
    fn put(out: &mut Vec<u8>, m: NativePrimitiveMetadata) {
        out.extend(m.page.to_le_bytes());
        out.extend(m.pages.to_le_bytes());
        out.extend(m.values.to_le_bytes());
        out.extend(m.min);
        out.extend(m.max);
        out.push(m.compression);
    }
    fn child(out: &mut Vec<u8>, logical: u8, physical: u8, m: NativePrimitiveMetadata) {
        out.extend([logical, physical, 0]);
        put(out, m);
        out.extend([1, 0]);
    }
    fn segment(out: &mut Vec<u8>, page: u32, len: u64, null: bool, index: u64) {
        out.extend([50, 20, 0]);
        put(out, m(INVALID_PAGE, 1, 0, 0));
        out.extend([1, 1]);
        put(out, m(INVALID_PAGE, 1, u64::from(null), 3));
        child(out, 28, 7, m(INVALID_PAGE, 1, index, 3));
        child(out, 27, 6, m(INVALID_PAGE, 1, 0, 3));
        child(out, 30, 9, m(page, len, 0, 0));
    }
    #[test]
    fn dc_native_dictionary_segment_advance_retained_locator_null_empty_and_invalid_index() {
        let memory = CleanupMemory::new(100_000).unwrap();
        let mut metadata = vec![1];
        metadata.extend(4u64.to_le_bytes());
        segment(&mut metadata, 1, 3, false, 0);
        segment(&mut metadata, 2, 3, false, 0);
        segment(&mut metadata, INVALID_PAGE, 0, false, 0);
        segment(&mut metadata, INVALID_PAGE, 0, true, 99);
        let mut bytes = vec![0; 30 * 4096];
        bytes[..metadata.len()].copy_from_slice(&metadata);
        bytes[4096..4099].copy_from_slice(b"one");
        bytes[8192..8195].copy_from_slice(b"two");
        let header = NativeHeader43Facts {
            catalog: NativePageRange {
                start: 20,
                count: 1,
            },
            metadata: NativePageRange {
                start: 21,
                count: 1,
            },
            database_id: [0; 16],
            data_file_pages: 30,
            available_pages: 30,
        };
        let mut cursor = ColumnCursor::new(
            bytes.as_slice(),
            super::super::maintenance_validation::Span {
                start: 0,
                end: metadata.len() as u64,
            },
        )
        .unwrap();
        let mut scratch = NativeDataScratch::new();
        let retained = cursor.value(0, header, &mut scratch).unwrap().unwrap();
        let second = cursor.value(1, header, &mut scratch).unwrap().unwrap();
        for (l, text) in [(retained, b"one"), (second, b"two")] {
            let source = NativeStringSource::new(bytes.as_slice(), l, &memory).unwrap();
            let mut actual = [0; 3];
            source.read_at(0, &mut actual).unwrap();
            assert_eq!(&actual, text);
        }
        assert_eq!(
            cursor
                .value(2, header, &mut scratch)
                .unwrap()
                .unwrap()
                .length,
            0
        );
        assert_eq!(cursor.value(3, header, &mut scratch).unwrap(), None);
        let mut metadata = vec![1];
        metadata.extend(1u64.to_le_bytes());
        segment(&mut metadata, 1, 3, false, 2);
        let mut cursor = ColumnCursor::new(
            metadata.as_slice(),
            super::super::maintenance_validation::Span {
                start: 0,
                end: metadata.len() as u64,
            },
        )
        .unwrap();
        assert!(cursor.value(0, header, &mut scratch).is_err());
    }
    #[test]
    fn dc_native_metadata_preserves_terminal_source_cancellation_instead_of_retry() {
        struct Cancel;
        impl ByteSource for Cancel {
            fn len(&self) -> u64 {
                5000
            }
            fn read_at(&self, _: u64, _: &mut [u8]) -> Result<usize, ValidationIssue> {
                Err(ValidationIssue::Io(std::io::ErrorKind::Interrupted))
            }
        }
        let memory = CleanupMemory::new(100_000).unwrap();
        assert_eq!(
            metadata_canonical_key(&Cancel, &memory),
            Err(NativeUnknown::Source(ValidationIssue::Io(
                std::io::ErrorKind::Interrupted
            )))
        );
    }
}
