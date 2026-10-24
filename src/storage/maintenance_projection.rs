//! Advisory bounded control projections. Facts retain source stamps and spans;
//! they never grant authority or materialize an ignored JSON map/string.
use super::locks::{FixedControlKind, FixedFileStamp};
use super::maintenance_budget::{CleanupMemory, MemoryReservation, ReservedBytes, ResourceLimit};
use super::maintenance_json::{JsonFailure, JsonToken, MaintenanceJsonReader, TokenKind};
use std::fs::File;
use std::io::{self, Read};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProjectionFailure {
    Resource(ResourceLimit),
    Json(JsonFailure),
    Io(io::ErrorKind),
    Changed,
    InvalidObject,
    InvalidString,
    InvalidNumber,
    UnsupportedNumericMode,
}
impl From<ResourceLimit> for ProjectionFailure {
    fn from(value: ResourceLimit) -> Self {
        Self::Resource(value)
    }
}
impl From<JsonFailure> for ProjectionFailure {
    fn from(value: JsonFailure) -> Self {
        Self::Json(value)
    }
}
impl From<io::Error> for ProjectionFailure {
    fn from(value: io::Error) -> Self {
        Self::Io(value.kind())
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct StringSpan {
    pub(crate) start: u64,
    pub(crate) end: u64,
    pub(crate) decoded_bytes: u64,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ScalarProjection {
    Missing,
    Null,
    String(StringSpan),
    Number { unsigned: Option<u64> },
    Other,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ConfigField {
    SchemaVersion,
    Repository,
    Storage,
    Database,
    Manifest,
}
impl ConfigField {
    fn index(self) -> usize {
        self as usize
    }
    fn key(bytes: &[u8]) -> Option<Self> {
        match bytes {
            b"schema_version" => Some(Self::SchemaVersion),
            b"repo_root" => Some(Self::Repository),
            b"storage_root" => Some(Self::Storage),
            b"database_path" => Some(Self::Database),
            b"manifest_path" => Some(Self::Manifest),
            _ => None,
        }
    }
}
pub(crate) struct ConfigProjection {
    fields: [ScalarProjection; 5],
    source: FixedFileStamp,
    // Includes this fixed retained result, numeric parser and string decoder
    // stack scratch; scanner retains its separate floor in the SAME account.
    _floor: MemoryReservation,
}
impl ConfigProjection {
    pub(crate) fn field(&self, field: ConfigField) -> ScalarProjection {
        self.fields[field.index()]
    }
    pub(crate) fn source(&self) -> &FixedFileStamp {
        &self.source
    }
    pub(crate) fn string(
        &self,
        file: &File,
        field: ConfigField,
        memory: &CleanupMemory,
    ) -> Result<Option<ReservedBytes>, ProjectionFailure> {
        match self.field(field) {
            ScalarProjection::Missing | ScalarProjection::Null => Ok(None),
            ScalarProjection::String(span) => {
                decode_projected_string(file, &self.source, span, memory).map(Some)
            }
            _ => Err(ProjectionFailure::InvalidString),
        }
    }
}

/// Positional reads do not disturb the scanner's read-ahead/cursor. Duplicated
/// Unix FDs would share offsets, so File::try_clone is deliberately not used.
struct SpanReader<'a> {
    file: &'a File,
    #[cfg(windows)]
    independent: File,
    offset: u64,
    end: u64,
}
impl<'a> SpanReader<'a> {
    fn new(file: &'a File, offset: u64, end: u64) -> io::Result<Self> {
        #[cfg(windows)]
        let independent = {
            use std::os::windows::io::{AsRawHandle, FromRawHandle};
            use windows_sys::Win32::{
                Foundation::{GENERIC_READ, INVALID_HANDLE_VALUE},
                Storage::FileSystem::{
                    ReOpenFile, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
                },
            };
            // A duplicated handle shares its cursor. ReOpenFile creates a new
            // file object for this exact inode without resolving a pathname.
            let handle = unsafe {
                ReOpenFile(
                    file.as_raw_handle(),
                    GENERIC_READ,
                    FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                    0,
                )
            };
            if handle == INVALID_HANDLE_VALUE {
                return Err(io::Error::last_os_error());
            }
            // SAFETY: the successful reopen returned one uniquely owned handle.
            let reopened = unsafe { File::from_raw_handle(handle) };
            if super::locks::file_identity(file).map_err(|_| io::ErrorKind::InvalidData)?
                != super::locks::file_identity(&reopened).map_err(|_| io::ErrorKind::InvalidData)?
            {
                return Err(io::ErrorKind::InvalidData.into());
            }
            reopened
        };
        Ok(Self {
            file,
            #[cfg(windows)]
            independent,
            offset,
            end,
        })
    }
}
impl Read for SpanReader<'_> {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        let len = usize::try_from((self.end - self.offset).min(output.len() as u64))
            .map_err(|_| io::ErrorKind::InvalidInput)?;
        if len == 0 {
            return Ok(0);
        }
        #[cfg(unix)]
        let read = {
            use std::os::unix::fs::FileExt;
            self.file.read_at(&mut output[..len], self.offset)?
        };
        #[cfg(windows)]
        let read = {
            use std::os::windows::fs::FileExt;
            let _ = self.file;
            self.independent
                .seek_read(&mut output[..len], self.offset)?
        };
        #[cfg(not(any(unix, windows)))]
        return Err(io::ErrorKind::Unsupported.into());
        self.offset += read as u64;
        Ok(read)
    }
}
struct StringReader<'a> {
    input: SpanReader<'a>,
    buffer: [u8; 512],
    cursor: usize,
    filled: usize,
}
impl StringReader<'_> {
    fn byte(&mut self) -> Result<u8, ProjectionFailure> {
        if self.cursor == self.filled {
            self.filled = self.input.read(&mut self.buffer)?;
            self.cursor = 0;
            if self.filled == 0 {
                return Err(ProjectionFailure::InvalidString);
            }
        }
        let byte = self.buffer[self.cursor];
        self.cursor += 1;
        Ok(byte)
    }
    fn hex4(&mut self) -> Result<u32, ProjectionFailure> {
        let mut value = 0;
        for _ in 0..4 {
            let digit = match self.byte()? {
                b'0'..=b'9' => self.buffer[self.cursor - 1] - b'0',
                b'a'..=b'f' => self.buffer[self.cursor - 1] - b'a' + 10,
                b'A'..=b'F' => self.buffer[self.cursor - 1] - b'A' + 10,
                _ => return Err(ProjectionFailure::InvalidString),
            };
            value = (value << 4) | u32::from(digit);
        }
        Ok(value)
    }
}
fn decode_string_into(
    file: &File,
    span: StringSpan,
    output: &mut [u8],
) -> Result<(), ProjectionFailure> {
    if span.end < span.start || span.decoded_bytes != output.len() as u64 {
        return Err(ProjectionFailure::InvalidString);
    }
    let mut reader = StringReader {
        input: SpanReader::new(file, span.start, span.end)?,
        buffer: [0; 512],
        cursor: 0,
        filled: 0,
    };
    if reader.byte()? != b'"' {
        return Err(ProjectionFailure::InvalidString);
    }
    let mut written = 0usize;
    loop {
        let byte = reader.byte()?;
        if byte == b'"' {
            break;
        }
        let mut encoded = [0; 4];
        let bytes: &[u8] = if byte == b'\\' {
            match reader.byte()? {
                b'"' => b"\"",
                b'\\' => b"\\",
                b'/' => b"/",
                b'b' => b"\x08",
                b'f' => b"\x0c",
                b'n' => b"\n",
                b'r' => b"\r",
                b't' => b"\t",
                b'u' => {
                    let mut point = reader.hex4()?;
                    if (0xd800..=0xdbff).contains(&point) {
                        if reader.byte()? != b'\\' || reader.byte()? != b'u' {
                            return Err(ProjectionFailure::InvalidString);
                        }
                        let low = reader.hex4()?;
                        if !(0xdc00..=0xdfff).contains(&low) {
                            return Err(ProjectionFailure::InvalidString);
                        }
                        point = 0x10000 + ((point - 0xd800) << 10) + (low - 0xdc00);
                    }
                    let ch = char::from_u32(point).ok_or(ProjectionFailure::InvalidString)?;
                    ch.encode_utf8(&mut encoded).as_bytes()
                }
                _ => return Err(ProjectionFailure::InvalidString),
            }
        } else {
            encoded[0] = byte;
            &encoded[..1]
        };
        let end = written
            .checked_add(bytes.len())
            .ok_or(ProjectionFailure::InvalidString)?;
        if end > output.len() {
            return Err(ProjectionFailure::InvalidString);
        }
        output[written..end].copy_from_slice(bytes);
        written = end;
    }
    let consumed = reader.input.offset - (reader.filled - reader.cursor) as u64;
    if written != output.len() || consumed != span.end || std::str::from_utf8(output).is_err() {
        return Err(ProjectionFailure::InvalidString);
    }
    Ok(())
}
/// Reserve exact decoded storage BEFORE allocating; long relevant values return
/// the account's typed limit, never truncation or a global control-file cutoff.
fn decode_projected_string(
    file: &File,
    source: &FixedFileStamp,
    span: StringSpan,
    memory: &CleanupMemory,
) -> Result<ReservedBytes, ProjectionFailure> {
    let _floor = memory.reserve(1024)?;
    source
        .recheck(file)
        .map_err(|_| ProjectionFailure::Changed)?;
    let len = usize::try_from(span.decoded_bytes).map_err(|_| ResourceLimit::ArithmeticOverflow)?;
    let mut bytes = memory.reserve(len)?.bytes(len)?;
    decode_string_into(file, span, bytes.as_mut_slice())?;
    source
        .recheck(file)
        .map_err(|_| ProjectionFailure::Changed)?;
    Ok(bytes)
}
fn span(token: JsonToken, decoded_bytes: u64) -> StringSpan {
    StringSpan {
        start: token.start,
        end: token.end,
        decoded_bytes,
    }
}
/// Validate ALL number values against the pinned ordinary loader's Number
/// implementation, including ignored/nested/overwritten numeric fields. There
/// is no64-byte producer-number cutoff here. Default-runtime Number parsing is
/// incremental; this fixed same-account floor covers its scalar/error scratch.
fn number(file: &File, token: JsonToken) -> Result<serde_json::Number, ProjectionFailure> {
    serde_json::from_reader(SpanReader::new(file, token.start, token.end)?)
        .map_err(|_| ProjectionFailure::InvalidNumber)
}
pub(crate) fn project_config(
    file: &File,
    memory: &CleanupMemory,
) -> Result<ConfigProjection, ProjectionFailure> {
    let floor = memory.reserve(std::mem::size_of::<ConfigProjection>() + 2048)?;
    // The pinned default-runtime Number parser is incremental. An unexpected
    // arbitrary-precision feature would allocate complete unbounded literals;
    // detect that mode using only this fixed, already-accounted probe before
    // accepting any source-controlled token. It disables advisory proof only.
    if serde_json::from_str::<serde_json::Number>("1e9999").is_ok() {
        return Err(ProjectionFailure::UnsupportedNumericMode);
    }
    let source = FixedFileStamp::capture(file, FixedControlKind::File)
        .map_err(|_| ProjectionFailure::Changed)?;
    let end = file.metadata()?.len();
    let mut scanner = MaintenanceJsonReader::new(SpanReader::new(file, 0, end)?, memory)?;
    let mut fields = [ScalarProjection::Missing; 5];
    let mut depth = 0usize;
    let mut key = None;
    let mut root = false;
    while let Some(token) = scanner.next_token()? {
        if !root {
            if !matches!(token.kind, TokenKind::ObjectStart) {
                return Err(ProjectionFailure::InvalidObject);
            }
            root = true;
        }
        let numeric = if matches!(token.kind, TokenKind::Number) {
            Some(number(file, token)?)
        } else {
            None
        };
        if depth == 1 {
            if let TokenKind::String {
                key: true,
                decoded_len,
                ..
            } = token.kind
            {
                key = if decoded_len <= 16 {
                    let mut bytes = [0; 16];
                    let len = decoded_len as usize;
                    decode_string_into(file, span(token, decoded_len), &mut bytes[..len])?;
                    ConfigField::key(&bytes[..len])
                } else {
                    None
                };
            } else if let Some(field) = key.take() {
                fields[field.index()] = match token.kind {
                    TokenKind::Null => ScalarProjection::Null,
                    TokenKind::String {
                        key: false,
                        decoded_len,
                        ..
                    } => ScalarProjection::String(span(token, decoded_len)),
                    TokenKind::Number => ScalarProjection::Number {
                        unsigned: numeric.as_ref().and_then(serde_json::Number::as_u64),
                    },
                    _ => ScalarProjection::Other,
                };
            }
        }
        match token.kind {
            TokenKind::ObjectStart | TokenKind::ArrayStart => depth += 1,
            TokenKind::ObjectEnd | TokenKind::ArrayEnd => {
                depth = depth
                    .checked_sub(1)
                    .ok_or(ProjectionFailure::InvalidObject)?
            }
            _ => (),
        }
    }
    scanner.finish()?;
    if !root || depth != 0 {
        return Err(ProjectionFailure::InvalidObject);
    }
    source
        .recheck(file)
        .map_err(|_| ProjectionFailure::Changed)?;
    Ok(ConfigProjection {
        fields,
        source,
        _floor: floor,
    })
}

/// Fully resolved advisory selectors, never authority. Every owned native path
/// is covered by this shared-account envelope before any path allocation.
pub(crate) struct BoundConfiguration {
    pub(crate) storage: std::path::PathBuf,
    pub(crate) direct: Option<super::inventory::DirectPair>,
    pub(crate) config_path: std::path::PathBuf,
    pub(crate) config_stamp: Option<FixedFileStamp>,
    pub(crate) repository: super::locks::FileIdentity,
    pub(crate) state: super::locks::FileIdentity,
    _paths: MemoryReservation,
    _projected_paths: Option<MemoryReservation>,
}

/// Reserve a conservative envelope for actual native path allocations and
/// transient joins. Long-path support does not require charging every short
/// selector as a maximum-length Windows path, repeatedly, in the same account.
pub(super) fn path_charge<'a>(
    paths: impl IntoIterator<Item = &'a std::path::Path>,
    copies: usize,
    owner: usize,
) -> Result<usize, ResourceLimit> {
    paths.into_iter().try_fold(
        owner
            .checked_add(4096)
            .ok_or(ResourceLimit::ArithmeticOverflow)?,
        |total, path| {
            path.as_os_str()
                .as_encoded_bytes()
                .len()
                .checked_add(512)
                .and_then(|bytes| bytes.checked_mul(2))
                .and_then(|bytes| bytes.checked_mul(copies))
                .and_then(|bytes| total.checked_add(bytes))
                .ok_or(ResourceLimit::ArithmeticOverflow)
        },
    )
}

pub(crate) fn project_binding(
    target: &super::inventory::MaintenanceTarget,
    memory: &CleanupMemory,
) -> Result<BoundConfiguration, ProjectionFailure> {
    use super::inventory::{default_direct, real_path, same_physical_path};
    use super::locks::{open_fixed_control, path_identity, FixedControlScratch};
    use std::path::{Component, Path, PathBuf};
    #[cfg(unix)]
    const PATH_BYTES: usize = libc::PATH_MAX as usize;
    #[cfg(windows)]
    const PATH_BYTES: usize = 131072;
    #[cfg(not(any(unix, windows)))]
    const PATH_BYTES: usize = 4096;
    for path in [
        Some(&target.repository_root),
        Some(&target.state_root),
        target.storage_root.as_ref(),
        target.config_path.as_ref(),
        target.direct_pair.as_ref().map(|p| &p.database),
        target.direct_pair.as_ref().map(|p| &p.manifest),
    ]
    .into_iter()
    .flatten()
    {
        #[cfg(windows)]
        {
            use std::os::windows::ffi::OsStrExt;
            if path.as_os_str().encode_wide().count() >= 32768 {
                return Err(ProjectionFailure::InvalidString);
            }
        }
        #[cfg(not(windows))]
        if path.as_os_str().as_encoded_bytes().len() >= PATH_BYTES {
            return Err(ProjectionFailure::InvalidString);
        }
    }
    // Native paths have a fixed addressing domain. This envelope covers all
    // simultaneously retained selector clones, joins and decoded path images;
    // strings themselves are separately reserved exactly before decoding.
    let paths = memory.reserve(path_charge(
        [
            Some(target.repository_root.as_path()),
            Some(target.state_root.as_path()),
            target.storage_root.as_deref(),
            target.config_path.as_deref(),
            target.direct_pair.as_ref().map(|p| p.database.as_path()),
            target.direct_pair.as_ref().map(|p| p.manifest.as_path()),
        ]
        .into_iter()
        .flatten(),
        24,
        std::mem::size_of::<BoundConfiguration>(),
    )?)?;
    if !target.repository_root.is_absolute() || !real_path(&target.repository_root) {
        return Err(ProjectionFailure::InvalidObject);
    }
    let conventional = target.repository_root.join(".codebaseGraph");
    if target.state_root != conventional || !real_path(&conventional) {
        return Err(ProjectionFailure::InvalidObject);
    }
    let default_config = conventional.join("config.json");
    let config_path = target
        .config_path
        .as_ref()
        .unwrap_or(&default_config)
        .clone();
    let mut scratch =
        FixedControlScratch::new(memory.reserve(FixedControlScratch::required_bytes())?)
            .map_err(|_| ProjectionFailure::Changed)?;
    let file = match open_fixed_control(&config_path, FixedControlKind::File, false, &mut scratch) {
        Ok(file) => Some(file),
        Err(super::locks::FixedControlError::Io(error))
            if error.kind() == io::ErrorKind::NotFound && target.config_path.is_none() =>
        {
            None
        }
        _ => return Err(ProjectionFailure::Changed),
    };
    let mut managed = false;
    let mut storage = conventional.join("storage");
    let mut configured = default_direct(target);
    let mut config_stamp = None;
    let mut projected_paths = None;
    if let Some(file) = file {
        let projection = project_config(&file, memory)?;
        let projected_bytes = [
            ConfigField::Repository,
            ConfigField::Storage,
            ConfigField::Database,
            ConfigField::Manifest,
        ]
        .into_iter()
        .try_fold(0usize, |total, field| {
            let bytes = match projection.field(field) {
                ScalarProjection::String(span) => usize::try_from(span.decoded_bytes)
                    .map_err(|_| ResourceLimit::ArithmeticOverflow)?,
                _ => 0,
            };
            bytes
                .checked_mul(48)
                .and_then(|bytes| total.checked_add(bytes))
                .ok_or(ResourceLimit::ArithmeticOverflow)
        })?;
        projected_paths = Some(memory.reserve(projected_bytes)?);
        config_stamp = Some(*projection.source());
        let path = |field: ConfigField| -> Result<Option<PathBuf>, ProjectionFailure> {
            let Some(bytes) = projection.string(&file, field, memory)? else {
                return Ok(None);
            };
            let string = std::str::from_utf8(bytes.as_slice())
                .map_err(|_| ProjectionFailure::InvalidString)?;
            if string.len() >= PATH_BYTES || string.as_bytes().contains(&0) {
                return Err(ProjectionFailure::InvalidString);
            }
            let value = Path::new(string);
            let result = if value.is_absolute() {
                value.to_owned()
            } else {
                config_path
                    .parent()
                    .ok_or(ProjectionFailure::InvalidString)?
                    .join(value)
            };
            if result.as_os_str().as_encoded_bytes().len() >= PATH_BYTES {
                return Err(ProjectionFailure::InvalidString);
            }
            Ok(Some(result))
        };
        match path(ConfigField::Repository)? {
            Some(repository)
                if real_path(&repository)
                    && same_physical_path(&repository, &target.repository_root) => {}
            None if config_path == default_config => (),
            _ => return Err(ProjectionFailure::InvalidObject),
        }
        match projection.field(ConfigField::SchemaVersion) {
            ScalarProjection::Missing | ScalarProjection::Null => (),
            ScalarProjection::Number { unsigned: Some(1) } => (),
            ScalarProjection::Number {
                unsigned: Some(2 | 3),
            } => managed = true,
            _ => return Err(ProjectionFailure::InvalidNumber),
        }
        if managed {
            if let Some(value) = path(ConfigField::Storage)? {
                if !real_path(&value) {
                    return Err(ProjectionFailure::Changed);
                }
                storage = value;
            }
        }
        if let Some(value) = path(ConfigField::Database)? {
            configured.database = value;
        }
        if let Some(value) = path(ConfigField::Manifest)? {
            configured.manifest = value;
        }
        let current = open_fixed_control(&config_path, FixedControlKind::File, false, &mut scratch)
            .map_err(|_| ProjectionFailure::Changed)?;
        projection
            .source()
            .recheck(&current)
            .map_err(|_| ProjectionFailure::Changed)?;
    }
    let selected_storage = target.storage_root.as_ref().unwrap_or(&storage);
    if selected_storage != &storage
        && !(real_path(&storage)
            && real_path(selected_storage)
            && same_physical_path(selected_storage, &storage))
    {
        return Err(ProjectionFailure::InvalidObject);
    }
    // Selecting an ancestor of repository/state could collect its own controls.
    for protected in [&target.repository_root, &conventional] {
        if protected
            .ancestors()
            .any(|p| same_physical_path(p, selected_storage))
        {
            return Err(ProjectionFailure::InvalidObject);
        }
    }
    let direct = target
        .direct_pair
        .clone()
        .or_else(|| (!managed).then_some(configured.clone()));
    if let Some(pair) = &direct {
        let local = pair.database.starts_with(&conventional)
            && pair.manifest.starts_with(&conventional)
            && !pair
                .database
                .components()
                .chain(pair.manifest.components())
                .any(|c| matches!(c, Component::ParentDir));
        if (!local && (config_stamp.is_none() || pair != &configured))
            || pair.database == pair.manifest
        {
            return Err(ProjectionFailure::InvalidObject);
        }
    }
    if super::inventory::bound_roles_overlap_paths(target, selected_storage, direct.as_ref()) {
        return Err(ProjectionFailure::InvalidObject);
    }
    Ok(BoundConfiguration {
        storage: selected_storage.clone(),
        direct,
        config_path,
        config_stamp,
        repository: path_identity(&target.repository_root)
            .map_err(|_| ProjectionFailure::Changed)?,
        state: path_identity(&target.state_root).map_err(|_| ProjectionFailure::Changed)?,
        _paths: paths,
        _projected_paths: projected_paths,
    })
}

impl BoundConfiguration {
    pub(super) fn duplicate(&self, memory: &CleanupMemory) -> Result<Self, ResourceLimit> {
        let paths = memory.reserve(self._paths.remaining())?;
        let projected_paths = self
            ._projected_paths
            .as_ref()
            .map(|charge| memory.reserve(charge.remaining()))
            .transpose()?;
        Ok(Self {
            storage: self.storage.clone(),
            direct: self.direct.clone(),
            config_path: self.config_path.clone(),
            config_stamp: self.config_stamp,
            repository: self.repository,
            state: self.state,
            _paths: paths,
            _projected_paths: projected_paths,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{self, OpenOptions};
    use std::path::PathBuf;

    #[test]
    fn numeric_projection_preserves_the_input_cursor() {
        use std::io::{Seek, SeekFrom};
        let fixture = Fixture::new(br#"{"schema_version":3,"ignored":[1,-2,3.5]}"#);
        let mut file = fixture.file();
        file.seek(SeekFrom::Start(2)).unwrap();
        let memory = CleanupMemory::new(32768).unwrap();
        let projection = project_config(&file, &memory).unwrap();
        assert_eq!(
            projection.field(ConfigField::SchemaVersion),
            ScalarProjection::Number { unsigned: Some(3) }
        );
        assert_eq!(file.stream_position().unwrap(), 2);
    }

    struct Fixture {
        path: PathBuf,
    }
    impl Fixture {
        fn new(bytes: &[u8]) -> Self {
            let path = std::env::temp_dir().join(format!(
                "cleanup-projection-{}",
                crate::storage::layout::managed_generation_id()
            ));
            let mut file = OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&path)
                .unwrap();
            std::io::Write::write_all(&mut file, bytes).unwrap();
            Self { path }
        }
        fn file(&self) -> File {
            File::open(&self.path).unwrap()
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.path);
        }
    }
    fn projected(
        bytes: &[u8],
    ) -> Result<(Fixture, ConfigProjection, CleanupMemory), ProjectionFailure> {
        let fixture = Fixture::new(bytes);
        let memory = CleanupMemory::new(32768).unwrap();
        let projection = project_config(&fixture.file(), &memory)?;
        Ok((fixture, projection, memory))
    }
    #[test]
    fn dc_projection_matches_current_config_scalars_duplicates_and_escaped_keys() {
        for bytes in [
            br#"{}"#.as_slice(),
            br#"{"schema_version":1,"repo_root":null,"database_path":"raw/../db.ldb"}"#,
            br#"{"schema_version":2,"schema_version":3,"repo_root":"old","repo_root":"new"}"#,
            br#"{"schema_version":2.0,"storage_root":{},"manifest_path":false}"#,
            br#"{"schema_version":-0,"repo_root":["x"],"ignored":{"schema_version":3}}"#,
            br#"{"schema\u005fversion":3,"database\u005fpath":"a\\b\"c"}"#,
        ] {
            let ordinary: serde_json::Value = serde_json::from_slice(bytes).unwrap();
            let (fixture, projection, memory) = projected(bytes).unwrap();
            for (field, key) in [
                (ConfigField::SchemaVersion, "schema_version"),
                (ConfigField::Repository, "repo_root"),
                (ConfigField::Storage, "storage_root"),
                (ConfigField::Database, "database_path"),
                (ConfigField::Manifest, "manifest_path"),
            ] {
                let expected = match ordinary.get(key) {
                    None => ScalarProjection::Missing,
                    Some(serde_json::Value::Null) => ScalarProjection::Null,
                    Some(serde_json::Value::Number(n)) => ScalarProjection::Number {
                        unsigned: n.as_u64(),
                    },
                    Some(serde_json::Value::String(s)) => {
                        let actual = projection
                            .string(&fixture.file(), field, &memory)
                            .unwrap()
                            .unwrap();
                        assert_eq!(actual.as_slice(), s.as_bytes());
                        assert!(matches!(
                            projection.field(field),
                            ScalarProjection::String(_)
                        ));
                        continue;
                    }
                    _ => ScalarProjection::Other,
                };
                assert_eq!(projection.field(field), expected, "{key}");
            }
        }
    }
    #[test]
    fn dc_projection_decodes_utf8_surrogates_and_all_json_escapes_exactly() {
        let bytes = br#"{"repo_root":"\uD83D\uDE00/\u00e9/\b\f\n\r\t\/\\\""}"#;
        let ordinary: serde_json::Value = serde_json::from_slice(bytes).unwrap();
        let (fixture, projection, memory) = projected(bytes).unwrap();
        let decoded = projection
            .string(&fixture.file(), ConfigField::Repository, &memory)
            .unwrap()
            .unwrap();
        assert_eq!(
            decoded.as_slice(),
            ordinary["repo_root"].as_str().unwrap().as_bytes()
        );
        assert_eq!(
            std::str::from_utf8(decoded.as_slice())
                .unwrap()
                .chars()
                .next(),
            Some('😀')
        );
    }
    #[test]
    fn dc_projection_streams_large_ignored_keys_values_and_limits_only_decoded_storage() {
        let mut bytes = b"{\"".to_vec();
        bytes.extend(std::iter::repeat_n(b'x', 100_000));
        bytes.extend_from_slice(b"\":\"");
        bytes.extend(std::iter::repeat_n(b'y', 200_000));
        bytes.extend_from_slice(b"\",\"repo_root\":\"");
        bytes.extend(std::iter::repeat_n(b'z', 100_000));
        bytes.extend_from_slice(b"\",\"schema_version\":3}");
        let (fixture, projection, memory) = projected(&bytes).unwrap();
        assert_eq!(
            projection.field(ConfigField::SchemaVersion),
            ScalarProjection::Number { unsigned: Some(3) }
        );
        let before = memory.used();
        assert!(memory.peak() <= memory.limit());
        assert!(matches!(
            projection.string(&fixture.file(), ConfigField::Repository, &memory),
            Err(ProjectionFailure::Resource(ResourceLimit::CeilingExceeded))
        ));
        assert_eq!(memory.used(), before);
        drop(projection);
        assert!(memory.used() < before);
    }
    #[test]
    fn dc_projection_validates_long_ignored_numbers_like_ordinary_loader_without_cutoff() {
        let values = [
            "0".into(),
            "-0".into(),
            "2.0".into(),
            "2e0".into(),
            format!("0.{}", "0".repeat(10_000)),
            format!("1e{}", "0".repeat(10_000)),
            "1e309".into(),
            "-1e99999999999999999999".into(),
            "9".repeat(10_000),
        ];
        for value in values {
            let bytes = format!("{{\"ignored\":[{value}],\"schema_version\":2}}");
            let expected = serde_json::from_str::<serde_json::Value>(&bytes).is_ok();
            assert_eq!(
                projected(bytes.as_bytes()).is_ok(),
                expected,
                "numeric prefix {:?}",
                &value[..value.len().min(20)]
            );
        }
    }
    #[test]
    fn dc_projection_requires_complete_object_and_preserves_loader_depth_errors() {
        for bytes in [
            b"null".as_slice(),
            b"[]",
            b"{} {}",
            br#"{"repo_root":"\ud800"}"#,
            br#"{"x":01}"#,
            br#"{"x":true,}"#,
        ] {
            assert!(projected(bytes).is_err());
        }
        let bytes = format!("{{\"ignored\":{}0{}}}", "[".repeat(140), "]".repeat(140));
        assert!(serde_json::from_str::<serde_json::Value>(&bytes).is_err());
        assert!(projected(bytes.as_bytes()).is_err());
    }
    #[test]
    fn dc_projection_rechecks_current_source_before_decoding_and_does_not_move_file_cursor() {
        let (fixture, projection, memory) =
            projected(br#"{"repo_root":"old","schema_version":3}"#).unwrap();
        let mut file = fixture.file();
        use std::io::{Seek, SeekFrom};
        file.seek(SeekFrom::Start(2)).unwrap();
        let value = projection
            .string(&file, ConfigField::Repository, &memory)
            .unwrap()
            .unwrap();
        assert_eq!(value.as_slice(), b"old");
        assert_eq!(file.stream_position().unwrap(), 2);
        drop(value);
        fs::write(&fixture.path, br#"{"repo_root":"new","schema_version":3}"#).unwrap();
        assert!(matches!(
            projection.string(&file, ConfigField::Repository, &memory),
            Err(ProjectionFailure::Changed)
        ));
    }
}
