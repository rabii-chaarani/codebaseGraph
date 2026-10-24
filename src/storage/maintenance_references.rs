//! Bounded current-manifest reachability facts. No ownership or deletion authority.
use super::inventory::MaintenanceTarget;
use super::locks::{
    open_fixed_control, FixedControlKind, FixedControlScratch, FixedFileStamp, RootedDirectory,
};
use super::maintenance_budget::{CleanupMemory, ResourceLimit};
use super::maintenance_json::{JsonFailure, JsonToken, MaintenanceJsonReader, TokenKind};
use super::maintenance_ownership::{ReferenceDomain, ReferenceEpoch};
use super::maintenance_validation::{compare_decoded_spans, ByteSource, Span, ValidationIssue};
use std::cmp::Ordering;
use std::fs::File;
use std::io::{self, Read};
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReferenceIssue {
    ResourceLimit(ResourceLimit),
    Io(io::ErrorKind),
    Unknown,
    Stale,
    Cancelled,
}
impl From<ResourceLimit> for ReferenceIssue {
    fn from(e: ResourceLimit) -> Self {
        Self::ResourceLimit(e)
    }
}
impl From<io::Error> for ReferenceIssue {
    fn from(e: io::Error) -> Self {
        Self::Io(e.kind())
    }
}
impl From<JsonFailure> for ReferenceIssue {
    fn from(e: JsonFailure) -> Self {
        match e {
            JsonFailure::Resource(r) => Self::ResourceLimit(r),
            JsonFailure::Io(k) => Self::Io(k),
            _ => Self::Unknown,
        }
    }
}
impl From<ValidationIssue> for ReferenceIssue {
    fn from(e: ValidationIssue) -> Self {
        match e {
            ValidationIssue::Resource(r) => Self::ResourceLimit(r),
            ValidationIssue::Io(k) => Self::Io(k),
            _ => Self::Unknown,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ArtifactReferences {
    pub(crate) referenced: bool,
    pub(crate) complete: bool,
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
        #[cfg(unix)]
        {
            use std::os::unix::fs::FileExt;
            self.file
                .read_at(out, offset)
                .map_err(|e| ValidationIssue::Io(e.kind()))
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::FileExt;
            self.file
                .seek_read(out, offset)
                .map_err(|e| ValidationIssue::Io(e.kind()))
        }
        #[cfg(not(any(unix, windows)))]
        {
            let _ = (offset, out);
            Err(ValidationIssue::Schema)
        }
    }
}
struct Input<'a> {
    source: &'a Source<'a>,
    offset: u64,
    end: u64,
}
impl Read for Input<'_> {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        let n = usize::try_from((self.end - self.offset).min(out.len() as u64))
            .map_err(|_| io::ErrorKind::InvalidData)?;
        let n = self
            .source
            .read_at(self.offset, &mut out[..n])
            .map_err(|_| io::ErrorKind::InvalidData)?;
        self.offset += n as u64;
        Ok(n)
    }
}
/// Exact decoded-token spans in one fully validated, unchanged manifest.
/// The caller retains/rechecks the actual bound manifest FD/name and original E.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ManifestReferenceContext {
    pub(crate) stamp: FixedFileStamp,
    pub(crate) path: Span,
    pub(crate) content_hash: Span,
    pub(crate) language: Span,
    pub(crate) partition_id: Span,
}
struct Parser<'a> {
    source: &'a Source<'a>,
    reader: MaintenanceJsonReader<Input<'a>>,
    memory: &'a CleanupMemory,
    cancel: &'a dyn Fn() -> bool,
    key: [u8; 66],
    hit: bool,
    context: Option<[Span; 4]>,
}
fn span(t: JsonToken) -> Span {
    Span {
        start: t.start,
        end: t.end,
    }
}
impl Parser<'_> {
    fn token(&mut self) -> Result<JsonToken, ReferenceIssue> {
        if (self.cancel)() {
            return Err(ReferenceIssue::Cancelled);
        }
        let token = self.reader.next_token()?.ok_or(ReferenceIssue::Unknown)?;
        if matches!(token.kind, TokenKind::Number) {
            if serde_json::from_str::<serde_json::Number>("1e9999").is_ok() {
                return Err(ReferenceIssue::Unknown);
            }
            let _: serde_json::Number = serde_json::from_reader(Input {
                source: self.source,
                offset: token.start,
                end: token.end,
            })
            .map_err(|_| ReferenceIssue::Unknown)?;
        }
        Ok(token)
    }
    fn equal(&self, t: JsonToken, literal: &[u8]) -> Result<bool, ReferenceIssue> {
        Ok(compare_decoded_spans(
            self.source,
            span(t),
            literal,
            Span {
                start: 0,
                end: literal.len() as u64,
            },
            self.memory,
        )? == Ordering::Equal)
    }
    fn field(&self, t: JsonToken, fields: &[&[u8]]) -> Result<Option<usize>, ReferenceIssue> {
        if !matches!(t.kind, TokenKind::String { key: true, .. }) {
            return Err(ReferenceIssue::Unknown);
        }
        for (i, f) in fields.iter().enumerate() {
            if self.equal(t, f)? {
                return Ok(Some(i));
            }
        }
        Ok(None)
    }
    fn string(&self, t: JsonToken) -> Result<(), ReferenceIssue> {
        if matches!(t.kind, TokenKind::String { key: false, .. }) {
            Ok(())
        } else {
            Err(ReferenceIssue::Unknown)
        }
    }
    fn unsigned(&self, t: JsonToken) -> Result<(), ReferenceIssue> {
        if !matches!(t.kind, TokenKind::Number)
            || serde_json::from_str::<serde_json::Number>("1e9999").is_ok()
        {
            return Err(ReferenceIssue::Unknown);
        }
        let n: serde_json::Number = serde_json::from_reader(Input {
            source: self.source,
            offset: t.start,
            end: t.end,
        })
        .map_err(|_| ReferenceIssue::Unknown)?;
        n.as_u64().map(|_| ()).ok_or(ReferenceIssue::Unknown)
    }
    fn skip(&mut self, t: JsonToken) -> Result<(), ReferenceIssue> {
        let mut depth = usize::from(matches!(
            t.kind,
            TokenKind::ObjectStart | TokenKind::ArrayStart
        ));
        while depth > 0 {
            match self.token()?.kind {
                TokenKind::ObjectStart | TokenKind::ArrayStart => depth += 1,
                TokenKind::ObjectEnd | TokenKind::ArrayEnd => depth -= 1,
                _ => {}
            }
        }
        Ok(())
    }
    fn strings(&mut self, t: JsonToken, map: bool) -> Result<(), ReferenceIssue> {
        if t.kind
            != if map {
                TokenKind::ObjectStart
            } else {
                TokenKind::ArrayStart
            }
        {
            return Err(ReferenceIssue::Unknown);
        }
        let mut previous = None;
        loop {
            let t = self.token()?;
            if t.kind
                == if map {
                    TokenKind::ObjectEnd
                } else {
                    TokenKind::ArrayEnd
                }
            {
                break;
            }
            if map {
                self.ordered(t, &mut previous)?;
                let v = self.token()?;
                self.string(v)?;
            } else {
                self.string(t)?;
            }
        }
        Ok(())
    }
    fn ordered(&self, t: JsonToken, previous: &mut Option<Span>) -> Result<(), ReferenceIssue> {
        if !matches!(t.kind, TokenKind::String { key: true, .. }) {
            return Err(ReferenceIssue::Unknown);
        }
        if let Some(p) = *previous {
            if compare_decoded_spans(self.source, p, self.source, span(t), self.memory)?
                != Ordering::Less
            {
                return Err(ReferenceIssue::Unknown);
            }
        }
        *previous = Some(span(t));
        Ok(())
    }
    fn entry(&mut self, t: JsonToken) -> Result<(), ReferenceIssue> {
        if t.kind != TokenKind::ObjectStart {
            return Err(ReferenceIssue::Unknown);
        }
        let fields: &[&[u8]] = &[
            br#""path""#,
            br#""content_hash""#,
            br#""language""#,
            br#""partition_id""#,
            br#""artifact_key""#,
            br#""node_ids""#,
            br#""edge_ids""#,
            br#""node_types""#,
            br#""edge_types""#,
            br#""node_count""#,
            br#""edge_count""#,
            br#""materialized_at""#,
        ];
        let mut seen = 0u16;
        let mut context = [Span { start: 0, end: 0 }; 4];
        let mut entry_hit = false;
        loop {
            let t = self.token()?;
            if t.kind == TokenKind::ObjectEnd {
                break;
            }
            let f = self.field(t, fields)?;
            let v = self.token()?;
            if let Some(i) = f {
                if seen & (1 << i) != 0 {
                    return Err(ReferenceIssue::Unknown);
                }
                seen |= 1 << i;
                match i {
                    0..=3 => {
                        self.string(v)?;
                        context[i] = span(v);
                    }
                    11 => self.string(v)?,
                    4 => {
                        if v.kind != TokenKind::Null {
                            self.string(v)?;
                            if self.equal(v, &self.key)? {
                                self.hit = true;
                                entry_hit = true
                            }
                        }
                    }
                    5 | 6 => self.strings(v, false)?,
                    7 | 8 => self.strings(v, true)?,
                    9 | 10 => self.unsigned(v)?,
                    _ => unreachable!(),
                }
            } else {
                self.skip(v)?
            }
        }
        if seen & 15 != 15 {
            return Err(ReferenceIssue::Unknown);
        }
        if entry_hit {
            if let Some(previous) = self.context {
                for (left, right) in previous.into_iter().zip(context) {
                    if compare_decoded_spans(self.source, left, self.source, right, self.memory)?
                        != Ordering::Equal
                    {
                        return Err(ReferenceIssue::Unknown);
                    }
                }
            }
            self.context = Some(context);
        }
        Ok(())
    }
    fn files(&mut self, t: JsonToken) -> Result<(), ReferenceIssue> {
        // Historical array manifests deliberately remain unknown.
        if t.kind != TokenKind::ObjectStart {
            return Err(ReferenceIssue::Unknown);
        }
        let mut previous = None;
        loop {
            let k = self.token()?;
            if k.kind == TokenKind::ObjectEnd {
                break;
            }
            self.ordered(k, &mut previous)?;
            let v = self.token()?;
            self.entry(v)?;
        }
        Ok(())
    }
    fn search(&mut self, t: JsonToken) -> Result<(), ReferenceIssue> {
        if t.kind == TokenKind::Null {
            return Ok(());
        }
        if t.kind != TokenKind::ObjectStart {
            return Err(ReferenceIssue::Unknown);
        }
        let mut seen = 0u8;
        let fields: &[&[u8]] = &[
            br#""backend""#,
            br#""format_version""#,
            br#""files""#,
            br#""document_count""#,
            br#""term_count""#,
            br#""total_tokens""#,
        ];
        loop {
            let k = self.token()?;
            if k.kind == TokenKind::ObjectEnd {
                break;
            }
            let f = self.field(k, fields)?;
            let v = self.token()?;
            if let Some(i) = f {
                if seen & (1 << i) != 0 {
                    return Err(ReferenceIssue::Unknown);
                }
                seen |= 1 << i;
                match i {
                    0 => self.string(v)?,
                    2 => self.strings(v, true)?,
                    _ => self.unsigned(v)?,
                }
            } else {
                self.skip(v)?
            }
        }
        if seen & 3 != 3 {
            return Err(ReferenceIssue::Unknown);
        }
        Ok(())
    }
    fn manifest(&mut self) -> Result<bool, ReferenceIssue> {
        if self.token()?.kind != TokenKind::ObjectStart {
            return Err(ReferenceIssue::Unknown);
        }
        let mut seen = 0u8;
        let fields: &[&[u8]] = &[
            br#""schema_version""#,
            br#""ontology""#,
            br#""parser_version""#,
            br#""graph_build_digest""#,
            br#""search_backend""#,
            br#""files""#,
        ];
        loop {
            let k = self.token()?;
            if k.kind == TokenKind::ObjectEnd {
                break;
            }
            let f = self.field(k, fields)?;
            let v = self.token()?;
            if let Some(i) = f {
                if seen & (1 << i) != 0 {
                    return Err(ReferenceIssue::Unknown);
                }
                seen |= 1 << i;
                match i {
                    0 => self.unsigned(v)?,
                    1 | 2 => self.string(v)?,
                    3 => {
                        if v.kind != TokenKind::Null {
                            self.string(v)?
                        }
                    }
                    4 => self.search(v)?,
                    5 => self.files(v)?,
                    _ => unreachable!(),
                }
            } else {
                self.skip(v)?
            }
        }
        if seen & 7 != 7 || self.reader.next_token()?.is_some() {
            return Err(ReferenceIssue::Unknown);
        }
        Ok(self.hit)
    }
}
pub(crate) fn manifest_reference_context(
    file: &File,
    key: [u8; 32],
    memory: &CleanupMemory,
    cancel: &dyn Fn() -> bool,
) -> Result<Option<ManifestReferenceContext>, ReferenceIssue> {
    let _floor = memory.reserve(std::mem::size_of::<Parser<'_>>() + 4096)?;
    let stamp = FixedFileStamp::capture(file, FixedControlKind::File)
        .map_err(|_| ReferenceIssue::Unknown)?;
    let source = Source {
        file,
        len: stamp.len(),
    };
    let mut literal = [b'"'; 66];
    let hex = b"0123456789abcdef";
    for (i, b) in key.iter().enumerate() {
        literal[1 + 2 * i] = hex[(b >> 4) as usize];
        literal[2 + 2 * i] = hex[(b & 15) as usize];
    }
    let reader = MaintenanceJsonReader::new(
        Input {
            source: &source,
            offset: 0,
            end: source.len,
        },
        memory,
    )?;
    let mut parser = Parser {
        source: &source,
        reader,
        memory,
        cancel,
        key: literal,
        hit: false,
        context: None,
    };
    parser.manifest()?;
    stamp.recheck(file).map_err(|_| ReferenceIssue::Stale)?;
    Ok(parser.context.map(|context| ManifestReferenceContext {
        stamp,
        path: context[0],
        content_hash: context[1],
        language: context[2],
        partition_id: context[3],
    }))
}
#[cfg(test)]
fn manifest(
    file: &File,
    key: [u8; 32],
    memory: &CleanupMemory,
    cancel: &dyn Fn() -> bool,
) -> Result<bool, ReferenceIssue> {
    Ok(manifest_reference_context(file, key, memory, cancel)?.is_some())
}
fn control(error: super::locks::FixedControlError) -> ReferenceIssue {
    match error {
        super::locks::FixedControlError::Resource(r) => ReferenceIssue::ResourceLimit(r),
        super::locks::FixedControlError::Io(e) => ReferenceIssue::Io(e.kind()),
        _ => ReferenceIssue::Stale,
    }
}
fn directory(
    path: &Path,
    scratch: &mut FixedControlScratch,
) -> Result<RootedDirectory, ReferenceIssue> {
    let file =
        open_fixed_control(path, FixedControlKind::Directory, false, scratch).map_err(control)?;
    let stamp = FixedFileStamp::capture(&file, FixedControlKind::Directory).map_err(control)?;
    RootedDirectory::open_fixed(path, stamp.identity(), false, scratch).map_err(control)
}
fn absent(path: &Path, scratch: &mut FixedControlScratch) -> Result<bool, ReferenceIssue> {
    match open_fixed_control(path, FixedControlKind::File, false, scratch) {
        Ok(_) => Ok(false),
        Err(super::locks::FixedControlError::Io(e)) if e.kind() == io::ErrorKind::NotFound => {
            Ok(true)
        }
        Err(_) => Err(ReferenceIssue::Unknown),
    }
}
/// Observe every relevant current manifest under the ORIGINAL expected epoch.
/// All generations are retained as references (including leased and recoverable
/// predecessors). Runs or Direct publication evidence defer absence until their
/// real owner reconciles them. Before/after exclusive activity observations
/// reject a producer already live at capture as well as a producer starting
/// during this scan; no lock survives either observation.
pub(crate) fn prepare_artifact_references(
    target: &MaintenanceTarget,
    domain: &ReferenceDomain,
    expected: &ReferenceEpoch,
    key: [u8; 32],
    memory: &CleanupMemory,
    cancel: &dyn Fn() -> bool,
) -> Result<ArtifactReferences, ReferenceIssue> {
    prepare_artifact_references_with_context(
        target,
        domain,
        expected,
        key,
        memory,
        cancel,
        &mut |_, _| Ok(()),
    )
}
/// The visitor borrows a dedicated, bound source FD only while its complete
/// context is observed. No context is usable unless the returned complete flag
/// and ORIGINAL-E final seal both succeed. No FD/catalog union is retained.
pub(crate) fn prepare_artifact_references_with_context(
    target: &MaintenanceTarget,
    domain: &ReferenceDomain,
    expected: &ReferenceEpoch,
    key: [u8; 32],
    memory: &CleanupMemory,
    cancel: &dyn Fn() -> bool,
    visit: &mut dyn FnMut(&File, ManifestReferenceContext) -> Result<(), ReferenceIssue>,
) -> Result<ArtifactReferences, ReferenceIssue> {
    if cancel() {
        return Err(ReferenceIssue::Cancelled);
    }
    let mut charge = 32768usize;
    for path in [
        Some(target.repository_root.as_path()),
        Some(target.state_root.as_path()),
        target.storage_root.as_deref(),
        target.direct_pair.as_ref().map(|p| p.database.as_path()),
        target.direct_pair.as_ref().map(|p| p.manifest.as_path()),
        Some(domain.artifact_root()),
    ]
    .into_iter()
    .flatten()
    {
        charge = charge
            .checked_add(
                path.as_os_str()
                    .as_encoded_bytes()
                    .len()
                    .checked_mul(32)
                    .ok_or(ResourceLimit::ArithmeticOverflow)?,
            )
            .ok_or(ResourceLimit::ArithmeticOverflow)?;
    }
    let _floor = memory.reserve(charge)?;
    let mut scratch =
        FixedControlScratch::new(memory.reserve(FixedControlScratch::required_bytes())?)
            .map_err(control)?;
    if !super::maintenance_ownership::expected_reference_current(domain, expected, memory)
        .map_err(control)?
    {
        return Err(ReferenceIssue::Stale);
    }
    let mut result = ArtifactReferences {
        referenced: false,
        complete: true,
    };
    let managed = target
        .storage_root
        .as_ref()
        .filter(|p| p.join("artifacts") == domain.artifact_root());
    if let Some(storage) = managed {
        let generations_path = storage.join("generations");
        let runs_path = storage.join("runs");
        let generations_source = open_fixed_control(
            &generations_path,
            FixedControlKind::Directory,
            false,
            &mut scratch,
        )
        .map_err(control)?;
        let generations_stamp =
            FixedFileStamp::capture(&generations_source, FixedControlKind::Directory)
                .map_err(control)?;
        let runs_source =
            open_fixed_control(&runs_path, FixedControlKind::Directory, false, &mut scratch)
                .map_err(control)?;
        let runs_stamp =
            FixedFileStamp::capture(&runs_source, FixedControlKind::Directory).map_err(control)?;
        let generations = directory(&generations_path, &mut scratch)?;
        let runs = directory(&runs_path, &mut scratch)?;
        let mut buffer = [0u8; 4096];
        let mut failure = None;
        generations
            .visit_entries(&mut buffer, &mut |name| {
                if cancel() {
                    failure = Some(ReferenceIssue::Cancelled);
                    return Err(io::ErrorKind::Interrupted.into());
                }
                let observed = (|| {
                    // Unrecognized generation leaves are retained, never interpreted as empty.
                    if !name.starts_with(b"gen-") {
                        return Err(ReferenceIssue::Unknown);
                    }
                    let generation = generations.open_directory_readonly(name)?;
                    let file = generation.open_readonly_file(b"manifest.json")?;
                    let stamp =
                        FixedFileStamp::capture(&file, FixedControlKind::File).map_err(control)?;
                    let context = manifest_reference_context(&file, key, memory, cancel)?;
                    if let Some(context) = context {
                        visit(&file, context)?;
                    }
                    let hit = context.is_some();
                    let current = generation.open_readonly_file(b"manifest.json")?;
                    stamp.recheck(&current).map_err(control)?;
                    if generations.open_directory_readonly(name)?.identity()
                        != generation.identity()
                    {
                        return Err(ReferenceIssue::Stale);
                    }
                    Ok(hit)
                })();
                match observed {
                    Ok(hit) => result.referenced |= hit,
                    Err(ReferenceIssue::Cancelled) => {
                        failure = Some(ReferenceIssue::Cancelled);
                        return Err(io::ErrorKind::Interrupted.into());
                    }
                    Err(ReferenceIssue::ResourceLimit(r)) => {
                        failure = Some(ReferenceIssue::ResourceLimit(r));
                        return Err(io::ErrorKind::OutOfMemory.into());
                    }
                    Err(_) => result.complete = false,
                };
                Ok(())
            })
            .map_err(|e| failure.unwrap_or_else(|| e.into()))?;
        runs.visit_entries(&mut buffer, &mut |_| {
            if cancel() {
                return Err(io::ErrorKind::Interrupted.into());
            }
            result.complete = false;
            Ok(())
        })
        .map_err(|e| {
            if cancel() {
                ReferenceIssue::Cancelled
            } else {
                e.into()
            }
        })?;
        for (path, source, stamp) in [
            (&generations_path, &generations_source, generations_stamp),
            (&runs_path, &runs_source, runs_stamp),
        ] {
            stamp.recheck(source).map_err(control)?;
            let current =
                open_fixed_control(path, FixedControlKind::Directory, false, &mut scratch)
                    .map_err(control)?;
            stamp.recheck(&current).map_err(control)?;
        }
    } else {
        let pair = target
            .direct_pair
            .clone()
            .unwrap_or_else(|| super::inventory::default_direct(target));
        let layout = super::layout::DirectLayout::new(&pair.database, &pair.manifest);
        if layout.artifact_root_path() != domain.artifact_root() {
            return Err(ReferenceIssue::Unknown);
        }
        // Recovery/candidate/shadow manifests can carry keys absent from current.
        // Do not infer their irrelevance or silently reduce them to an empty set.
        let mut shadow = pair.manifest.as_os_str().to_os_string();
        shadow.push(".shadow");
        for path in [
            layout.journal_path(),
            layout.manifest_candidate_path(),
            std::path::PathBuf::from(shadow),
        ] {
            if !absent(&path, &mut scratch)? {
                result.complete = false;
            }
        }
        let file = open_fixed_control(&pair.manifest, FixedControlKind::File, false, &mut scratch)
            .map_err(control)?;
        let source_stamp =
            FixedFileStamp::capture(&file, FixedControlKind::File).map_err(control)?;
        match manifest_reference_context(&file, key, memory, cancel) {
            Ok(context) => {
                if let Some(context) = context {
                    visit(&file, context)?;
                }
                result.referenced = context.is_some();
            }
            Err(ReferenceIssue::Unknown) => result.complete = false,
            Err(e) => return Err(e),
        }
        let current =
            open_fixed_control(&pair.manifest, FixedControlKind::File, false, &mut scratch)
                .map_err(control)?;
        source_stamp.recheck(&current).map_err(control)?;
    }
    if cancel() {
        return Err(ReferenceIssue::Cancelled);
    }
    if !super::maintenance_ownership::expected_reference_current(domain, expected, memory)
        .map_err(control)?
    {
        return Err(ReferenceIssue::Stale);
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{ManifestEntry, NativeManifest};
    use std::collections::BTreeMap;
    struct TestRoot(std::path::PathBuf);
    impl TestRoot {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "cleanup-refs-{}",
                super::super::layout::managed_generation_id()
            ));
            std::fs::create_dir(&path).unwrap();
            Self(std::fs::canonicalize(path).unwrap())
        }
    }
    impl Drop for TestRoot {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    fn producer(key: Option<String>) -> Vec<u8> {
        serde_json::to_vec(&NativeManifest {
            schema_version: 2,
            ontology: "code".into(),
            parser_version: "native".into(),
            graph_build_digest: None,
            search_backend: None,
            files: BTreeMap::from([(
                "src/a.rs".into(),
                ManifestEntry {
                    path: "src/a.rs".into(),
                    content_hash: "hash".into(),
                    language: "rust".into(),
                    partition_id: "part".into(),
                    artifact_key: key,
                    node_ids: vec![],
                    edge_ids: vec![],
                    node_types: BTreeMap::new(),
                    edge_types: BTreeMap::new(),
                    node_count: 0,
                    edge_count: 0,
                    materialized_at: String::new(),
                },
            )]),
        })
        .unwrap()
    }
    fn read(
        bytes: &[u8],
        key: [u8; 32],
        limit: usize,
        cancel: &dyn Fn() -> bool,
    ) -> Result<bool, ReferenceIssue> {
        let temp = TestRoot::new();
        let path = temp.0.join("manifest.json");
        std::fs::write(&path, bytes).unwrap();
        let file = File::open(path).unwrap();
        let memory = CleanupMemory::new(limit).unwrap();
        let before = memory.used();
        let result = manifest(&file, key, &memory, cancel);
        assert_eq!(memory.used(), before);
        assert!(memory.peak() <= limit);
        result
    }
    #[test]
    fn dc_references_actual_current_manifest_exact_key() {
        let bytes = producer(Some("ab".repeat(32)));
        assert_eq!(read(&bytes, [0xab; 32], 128 * 1024, &|| false), Ok(true));
        assert_eq!(read(&bytes, [0xac; 32], 128 * 1024, &|| false), Ok(false));
        assert_eq!(
            read(&producer(None), [0xab; 32], 128 * 1024, &|| false),
            Ok(false)
        );
    }
    #[test]
    fn dc_references_escaped_keys_match_decoded_exact_bytes() {
        let bytes = producer(Some("ab".repeat(32)));
        let text = String::from_utf8(bytes)
            .unwrap()
            .replace("\"artifact_key\"", "\"artifact_\\u006bey\"")
            .replace(&"ab".repeat(32), &"\\u0061b".repeat(32));
        assert_eq!(
            read(text.as_bytes(), [0xab; 32], 128 * 1024, &|| false),
            Ok(true)
        );
    }
    #[test]
    fn dc_references_duplicate_schema_fields_are_unknown() {
        let text = String::from_utf8(producer(Some("ab".repeat(32)))).unwrap();
        let text = text.replace(
            "\"artifact_key\":",
            "\"artifact_key\":null,\"artifact_key\":",
        );
        assert_eq!(
            read(text.as_bytes(), [0xab; 32], 128 * 1024, &|| false),
            Err(ReferenceIssue::Unknown)
        );
    }
    #[test]
    fn dc_references_duplicate_map_keys_fail_closed_after_last_value() {
        let text = String::from_utf8(producer(None)).unwrap();
        let value: serde_json::Value = serde_json::from_str(&text).unwrap();
        let entry = serde_json::to_string(&value["files"]["src/a.rs"]).unwrap();
        let text = format!(
            r#"{{"schema_version":2,"ontology":"code","parser_version":"native","files":{{"src/a.rs":{entry},"src/\u0061.rs":{entry}}}}}"#
        );
        assert_eq!(
            read(text.as_bytes(), [0; 32], 128 * 1024, &|| false),
            Err(ReferenceIssue::Unknown)
        );
    }
    #[test]
    fn dc_references_malformed_tail_and_required_entry_fields_fail_closed() {
        let bytes = producer(Some("ab".repeat(32)));
        let mut malformed = bytes.clone();
        malformed.extend_from_slice(b"false");
        assert_eq!(
            read(&malformed, [0xab; 32], 128 * 1024, &|| false),
            Err(ReferenceIssue::Unknown)
        );
        let text = String::from_utf8(bytes)
            .unwrap()
            .replace("\"language\":\"rust\",", "");
        assert_eq!(
            read(text.as_bytes(), [0xab; 32], 128 * 1024, &|| false),
            Err(ReferenceIssue::Unknown)
        );
    }
    #[test]
    fn dc_references_historical_arrays_and_invalid_map_values_are_unknown() {
        for files in ["[]", "null", "{\"a\":false}"] {
            let text = format!(
                r#"{{"schema_version":2,"ontology":"code","parser_version":"native","files":{files}}}"#
            );
            assert_eq!(
                read(text.as_bytes(), [0; 32], 128 * 1024, &|| false),
                Err(ReferenceIssue::Unknown)
            );
        }
    }
    #[test]
    fn dc_references_large_irrelevant_string_streams_below_cap() {
        let text = String::from_utf8(producer(None)).unwrap().replacen(
            '{',
            &format!("{{\"extension\":\"{}\",", "x".repeat(1024 * 1024)),
            1,
        );
        assert_eq!(
            read(text.as_bytes(), [0; 32], 32 * 1024, &|| false),
            Ok(false)
        );
    }
    #[test]
    fn dc_references_memory_and_cancellation_are_typed() {
        assert!(matches!(
            read(&producer(None), [0; 32], 1024, &|| false),
            Err(ReferenceIssue::ResourceLimit(_))
        ));
        assert_eq!(
            read(&producer(None), [0; 32], 128 * 1024, &|| true),
            Err(ReferenceIssue::Cancelled)
        );
    }
    #[test]
    fn dc_references_short_prefix_and_long_key_never_match() {
        for key in ["ab".repeat(31), "ab".repeat(33)] {
            assert_eq!(
                read(&producer(Some(key)), [0xab; 32], 128 * 1024, &|| false),
                Ok(false)
            );
        }
    }
    struct ManagedFixture {
        _root: TestRoot,
        target: MaintenanceTarget,
        layout: super::super::layout::ManagedLayout,
        domain: ReferenceDomain,
    }
    impl ManagedFixture {
        fn new() -> Self {
            use super::super::locks::{open_locked, LockMode};
            use super::super::maintenance_ownership::initialize_fresh_reference_domain;
            let root = TestRoot::new();
            let layout = super::super::layout::ManagedLayout::new(root.0.join("storage"));
            let fresh = super::super::layout::create_fresh_reference_root(layout.storage_root())
                .unwrap()
                .unwrap();
            layout.ensure_roots().unwrap();
            let writer = open_locked(layout.writer_lock_path(), LockMode::Exclusive).unwrap();
            let domain = ReferenceDomain::managed(&layout).unwrap();
            assert!(initialize_fresh_reference_domain(&domain, &fresh, &writer).unwrap());
            drop(writer);
            let mutation =
                super::super::maintenance_ownership::ReferenceMutation::begin_in_domain(&domain)
                    .unwrap();
            let generation = layout.generations_root().join("gen-current");
            std::fs::create_dir(&generation).unwrap();
            std::fs::write(
                generation.join("manifest.json"),
                producer(Some("ab".repeat(32))),
            )
            .unwrap();
            drop(mutation);
            let target = MaintenanceTarget {
                repository_root: root.0.clone(),
                state_root: root.0.clone(),
                storage_root: Some(layout.storage_root().to_owned()),
                direct_pair: None,
                config_path: None,
            };
            Self {
                _root: root,
                target,
                layout,
                domain,
            }
        }
        fn observe(&self) -> ReferenceEpoch {
            match super::super::maintenance_ownership::observe_reference_epoch(&self.domain)
                .unwrap()
            {
                super::super::maintenance_ownership::EpochObservation::Verified(e) => e,
                other => panic!("{other:?}"),
            }
        }
    }
    #[test]
    fn dc_references_managed_current_and_predecessor_manifests_are_covered() {
        let f = ManagedFixture::new();
        let memory = CleanupMemory::new(256 * 1024).unwrap();
        let e = f.observe();
        assert_eq!(
            prepare_artifact_references(&f.target, &f.domain, &e, [0xab; 32], &memory, &|| false)
                .unwrap(),
            ArtifactReferences {
                referenced: true,
                complete: true
            }
        );
        assert_eq!(
            prepare_artifact_references(&f.target, &f.domain, &e, [0xac; 32], &memory, &|| false)
                .unwrap(),
            ArtifactReferences {
                referenced: false,
                complete: true
            }
        );
        let mutation =
            super::super::maintenance_ownership::ReferenceMutation::begin_in_domain(&f.domain)
                .unwrap();
        let generation = f.layout.generations_root().join("gen-leased-or-recovery");
        std::fs::create_dir(&generation).unwrap();
        std::fs::write(
            generation.join("manifest.json"),
            producer(Some("ac".repeat(32))),
        )
        .unwrap();
        drop(mutation);
        assert_eq!(
            prepare_artifact_references(&f.target, &f.domain, &e, [0xac; 32], &memory, &|| false),
            Err(ReferenceIssue::Stale)
        );
        let e = f.observe();
        assert_eq!(
            prepare_artifact_references(&f.target, &f.domain, &e, [0xac; 32], &memory, &|| false)
                .unwrap(),
            ArtifactReferences {
                referenced: true,
                complete: true
            }
        );
    }
    #[test]
    fn dc_references_live_producer_and_changed_epoch_never_prove_absence() {
        let f = ManagedFixture::new();
        let memory = CleanupMemory::new(256 * 1024).unwrap();
        let e = f.observe();
        let mutation =
            super::super::maintenance_ownership::ReferenceMutation::begin_in_domain(&f.domain)
                .unwrap();
        assert!(prepare_artifact_references(
            &f.target,
            &f.domain,
            &e,
            [0xac; 32],
            &memory,
            &|| false
        )
        .is_err());
        drop(mutation);
        assert_eq!(
            prepare_artifact_references(&f.target, &f.domain, &e, [0xac; 32], &memory, &|| false),
            Err(ReferenceIssue::Stale)
        );
    }
    #[test]
    fn dc_references_recovery_runs_and_unknown_generations_remain_incomplete() {
        let f = ManagedFixture::new();
        let memory = CleanupMemory::new(256 * 1024).unwrap();
        let mutation =
            super::super::maintenance_ownership::ReferenceMutation::begin_in_domain(&f.domain)
                .unwrap();
        std::fs::create_dir(f.layout.runs_root().join("run-recovery")).unwrap();
        drop(mutation);
        let e = f.observe();
        assert!(
            !prepare_artifact_references(&f.target, &f.domain, &e, [0xac; 32], &memory, &|| false)
                .unwrap()
                .complete
        );
        let mutation =
            super::super::maintenance_ownership::ReferenceMutation::begin_in_domain(&f.domain)
                .unwrap();
        std::fs::remove_dir(f.layout.runs_root().join("run-recovery")).unwrap();
        std::fs::write(f.layout.generations_root().join("foreign"), b"unknown").unwrap();
        drop(mutation);
        let e = f.observe();
        assert!(
            !prepare_artifact_references(&f.target, &f.domain, &e, [0xac; 32], &memory, &|| false)
                .unwrap()
                .complete
        );
    }
    #[test]
    fn dc_references_source_change_during_scan_cannot_prove_absence() {
        let root = TestRoot::new();
        let path = root.0.join("manifest.json");
        let bytes = producer(None);
        std::fs::write(&path, &bytes).unwrap();
        let file = File::open(&path).unwrap();
        let memory = CleanupMemory::new(128 * 1024).unwrap();
        let calls = std::cell::Cell::new(0);
        let observe = || {
            let n = calls.get() + 1;
            calls.set(n);
            if n == 5 {
                let mut changed = bytes.clone();
                changed.extend_from_slice(b" ");
                std::fs::write(&path, changed).unwrap();
            }
            false
        };
        assert_eq!(
            manifest(&file, [0xab; 32], &memory, &observe),
            Err(ReferenceIssue::Stale)
        );
    }
    #[test]
    fn dc_references_direct_current_and_recovery_manifest_preservation() {
        use super::super::inventory::DirectPair;
        use super::super::layout::{create_fresh_reference_root, DirectLayout};
        use super::super::locks::{open_locked, LockMode};
        use super::super::maintenance_ownership::{
            initialize_fresh_reference_domain, observe_reference_epoch, ArtifactUseLease,
            EpochObservation,
        };
        let root = TestRoot::new();
        let destination = root.0.join("direct");
        let fresh = create_fresh_reference_root(&destination).unwrap().unwrap();
        let pair = DirectPair {
            database: destination.join("graph.ldb"),
            manifest: destination.join("manifest.json"),
        };
        let layout = DirectLayout::new(&pair.database, &pair.manifest);
        std::fs::write(&pair.database, b"fixture database, never opened").unwrap();
        std::fs::write(&pair.manifest, producer(Some("ab".repeat(32)))).unwrap();
        drop(ArtifactUseLease::acquire_shared(&layout.artifact_root_path()).unwrap());
        let writer = open_locked(layout.writer_lock_path(), LockMode::Exclusive).unwrap();
        let domain = ReferenceDomain::direct(&layout).unwrap();
        assert!(initialize_fresh_reference_domain(&domain, &fresh, &writer).unwrap());
        drop(writer);
        let target = MaintenanceTarget {
            repository_root: root.0.clone(),
            state_root: root.0.clone(),
            storage_root: None,
            direct_pair: Some(pair.clone()),
            config_path: None,
        };
        let memory = CleanupMemory::new(256 * 1024).unwrap();
        let observe = || match observe_reference_epoch(&domain).unwrap() {
            EpochObservation::Verified(e) => e,
            other => panic!("{other:?}"),
        };
        let e = observe();
        assert_eq!(
            prepare_artifact_references(&target, &domain, &e, [0xab; 32], &memory, &|| false)
                .unwrap(),
            ArtifactReferences {
                referenced: true,
                complete: true
            }
        );
        let mutation =
            super::super::maintenance_ownership::ReferenceMutation::begin_in_domain(&domain)
                .unwrap();
        std::fs::write(
            layout.manifest_candidate_path(),
            producer(Some("ac".repeat(32))),
        )
        .unwrap();
        drop(mutation);
        let e = observe();
        assert!(
            !prepare_artifact_references(&target, &domain, &e, [0xac; 32], &memory, &|| false)
                .unwrap()
                .complete
        );
        let mutation =
            super::super::maintenance_ownership::ReferenceMutation::begin_in_domain(&domain)
                .unwrap();
        std::fs::remove_file(layout.manifest_candidate_path()).unwrap();
        std::fs::write(layout.journal_path(), b"unknown recovery").unwrap();
        drop(mutation);
        let e = observe();
        assert!(
            !prepare_artifact_references(&target, &domain, &e, [0xac; 32], &memory, &|| false)
                .unwrap()
                .complete
        );
    }
}
