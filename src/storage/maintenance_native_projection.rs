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
