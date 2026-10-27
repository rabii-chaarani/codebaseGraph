use super::files::{create_staging_directory, finish_output, StagingOutputFile};
use crate::error::NativeError;
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::cmp::Ordering;
use std::collections::BinaryHeap;
use std::fs::{self, File};
use std::io::{self, BufReader, BufWriter, Read, Write};
use std::marker::PhantomData;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering as AtomicOrdering};
use std::sync::Arc;

const MAX_MERGE_FAN_IN: usize = 32;
const LENGTH_PREFIX_BYTES: usize = std::mem::size_of::<u32>();

pub(super) trait SpillRecord: Serialize + DeserializeOwned {
    type Key: Clone + Ord;

    fn sort_key(&self) -> Self::Key;
    fn key_bytes(key: &Self::Key) -> usize;
}

#[derive(Clone, Debug, Default)]
pub(super) struct SpillMetrics {
    inner: Arc<SpillMetricsInner>,
}

#[derive(Debug, Default)]
struct SpillMetricsInner {
    bytes_written: AtomicU64,
    current_bytes: AtomicUsize,
    high_water_bytes: AtomicUsize,
}

impl SpillMetrics {
    fn add_live(&self, bytes: usize) {
        // Keep the saturating update compatible with the Rust 1.82 MSRV;
        // newer toolchains deprecate fetch_update in favor of try_update.
        let mut previous = self.inner.current_bytes.load(AtomicOrdering::Relaxed);
        loop {
            match self.inner.current_bytes.compare_exchange_weak(
                previous,
                previous.saturating_add(bytes),
                AtomicOrdering::Relaxed,
                AtomicOrdering::Relaxed,
            ) {
                Ok(_) => break,
                Err(current) => previous = current,
            }
        }
        self.inner
            .high_water_bytes
            .fetch_max(previous.saturating_add(bytes), AtomicOrdering::Relaxed);
    }

    pub(super) fn charge(&self, bytes: usize) -> LiveMemoryCharge {
        self.add_live(bytes);
        LiveMemoryCharge {
            metrics: self.clone(),
            bytes,
        }
    }

    fn release_live(&self, bytes: usize) {
        let mut current = self.inner.current_bytes.load(AtomicOrdering::Relaxed);
        while let Err(observed) = self.inner.current_bytes.compare_exchange_weak(
            current,
            current.saturating_sub(bytes),
            AtomicOrdering::Relaxed,
            AtomicOrdering::Relaxed,
        ) {
            current = observed;
        }
    }

    fn add_written(&self, bytes: u64) {
        let mut current = self.inner.bytes_written.load(AtomicOrdering::Relaxed);
        while let Err(observed) = self.inner.bytes_written.compare_exchange_weak(
            current,
            current.saturating_add(bytes),
            AtomicOrdering::Relaxed,
            AtomicOrdering::Relaxed,
        ) {
            current = observed;
        }
    }

    pub(super) fn snapshot(&self) -> (u64, usize) {
        (
            self.inner.bytes_written.load(AtomicOrdering::Relaxed),
            self.inner.high_water_bytes.load(AtomicOrdering::Relaxed),
        )
    }

    #[cfg(test)]
    fn current_bytes(&self) -> usize {
        self.inner.current_bytes.load(AtomicOrdering::Relaxed)
    }
}

pub(super) struct LiveMemoryCharge {
    metrics: SpillMetrics,
    bytes: usize,
}

impl Drop for LiveMemoryCharge {
    fn drop(&mut self) {
        self.metrics.release_live(self.bytes);
    }
}

struct BufferedRecord<K> {
    key: K,
    encoded: Vec<u8>,
}

#[derive(Clone, Debug)]
struct RunDescriptor {
    path: PathBuf,
}

pub(super) struct SortedSpool<T: SpillRecord> {
    root: PathBuf,
    prefix: String,
    chunk_limit: usize,
    buffered: Vec<BufferedRecord<T::Key>>,
    buffered_bytes: usize,
    runs: Vec<RunDescriptor>,
    next_run: usize,
    metrics: SpillMetrics,
    marker: PhantomData<T>,
}

impl<T: SpillRecord> SortedSpool<T> {
    pub(super) fn new(
        root: &Path,
        prefix: &str,
        chunk_limit: usize,
        metrics: SpillMetrics,
    ) -> Result<Self, NativeError> {
        if chunk_limit == 0 {
            return Err(NativeError::InvalidInput(
                "spill chunk target must be positive".into(),
            ));
        }
        Ok(Self {
            root: root.to_path_buf(),
            prefix: prefix.to_string(),
            chunk_limit,
            buffered: Vec::new(),
            buffered_bytes: 0,
            runs: Vec::new(),
            next_run: 0,
            metrics,
            marker: PhantomData,
        })
    }

    pub(super) fn push(&mut self, record: &T) -> Result<(), NativeError> {
        let encoded = encode_record(record)?;
        let key = record.sort_key();
        let record_bytes = encoded
            .capacity()
            .checked_add(T::key_bytes(&key))
            .and_then(|bytes| bytes.checked_add(LENGTH_PREFIX_BYTES))
            .and_then(|bytes| bytes.checked_add(std::mem::size_of::<BufferedRecord<T::Key>>()))
            .ok_or_else(|| NativeError::InvalidInput("spill record size overflow".into()))?;
        let next_bytes = self
            .buffered_bytes
            .checked_add(record_bytes)
            .ok_or_else(|| NativeError::InvalidInput("spill record size overflow".into()))?;
        self.metrics.add_live(record_bytes);
        if !self.buffered.is_empty() && next_bytes > self.chunk_limit {
            if let Err(error) = self.flush() {
                self.metrics.release_live(record_bytes);
                return Err(error);
            }
        }
        if self.buffered.try_reserve_exact(1).is_err() {
            self.metrics.release_live(record_bytes);
            return Err(NativeError::Io(io::Error::other(
                "spill buffer allocation failed",
            )));
        }
        self.buffered.push(BufferedRecord { key, encoded });
        self.buffered_bytes = self
            .buffered_bytes
            .checked_add(record_bytes)
            .ok_or_else(|| NativeError::InvalidInput("spill record size overflow".into()))?;
        if self.buffered_bytes >= self.chunk_limit {
            self.flush()?;
        }
        Ok(())
    }

    pub(super) fn finish(mut self) -> Result<SortedStream<T>, NativeError> {
        self.flush()?;
        let mut runs = std::mem::take(&mut self.runs);
        while runs.len() > MAX_MERGE_FAN_IN {
            runs = self.merge_level(runs)?;
        }
        SortedStream::open(&runs, self.metrics.clone(), true)
    }

    fn flush(&mut self) -> Result<(), NativeError> {
        if self.buffered.is_empty() {
            return Ok(());
        }
        create_staging_directory(&self.root)?;
        self.buffered
            .sort_by(|left, right| left.key.cmp(&right.key));
        let path = self.next_path("run");
        let mut writer = BufWriter::new(StagingOutputFile::create(&path)?);
        for record in &self.buffered {
            write_encoded(&mut writer, &record.encoded, &self.metrics)?;
        }
        finish_output(writer)?;
        self.runs.push(RunDescriptor { path });
        let released_bytes = self.buffered_bytes;
        self.buffered = Vec::new();
        self.buffered_bytes = 0;
        self.metrics.release_live(released_bytes);
        Ok(())
    }

    fn merge_level(&mut self, runs: Vec<RunDescriptor>) -> Result<Vec<RunDescriptor>, NativeError> {
        let mut merged = Vec::new();
        for group in runs.chunks(MAX_MERGE_FAN_IN) {
            if group.len() == 1 {
                merged.push(group[0].clone());
            } else {
                let output = self.next_path("merge");
                merged.push(merge_run_group::<T>(group, &output, &self.metrics)?);
            }
        }
        Ok(merged)
    }

    fn next_path(&mut self, kind: &str) -> PathBuf {
        let index = self.next_run;
        self.next_run = self.next_run.saturating_add(1);
        self.root
            .join(format!("{}-{kind}-{index:08}.bin", self.prefix))
    }
}

impl<T: SpillRecord> Drop for SortedSpool<T> {
    fn drop(&mut self) {
        if self.buffered_bytes > 0 {
            self.metrics.release_live(self.buffered_bytes);
            self.buffered_bytes = 0;
        }
    }
}

pub(super) struct SortedStream<T: SpillRecord> {
    readers: Vec<RunReader>,
    heap: BinaryHeap<HeapItem<T>>,
    heap_bytes: usize,
    outstanding_bytes: usize,
    pending_reader: Option<usize>,
    metrics: SpillMetrics,
    input_paths: Vec<PathBuf>,
    cleanup_when_exhausted: bool,
}

pub(super) struct RecordFileWriter<T: Serialize> {
    writer: BufWriter<StagingOutputFile>,
    metrics: SpillMetrics,
    marker: PhantomData<T>,
}

impl<T: Serialize> RecordFileWriter<T> {
    pub(super) fn create(path: &Path, metrics: SpillMetrics) -> Result<Self, NativeError> {
        if let Some(parent) = path.parent() {
            create_staging_directory(parent)?;
        }
        Ok(Self {
            writer: BufWriter::new(StagingOutputFile::create(path)?),
            metrics,
            marker: PhantomData,
        })
    }

    pub(super) fn push(&mut self, record: &T) -> Result<(), NativeError> {
        let encoded = encode_record(record)?;
        write_encoded(&mut self.writer, &encoded, &self.metrics)
    }

    pub(super) fn finish(self) -> Result<(), NativeError> {
        finish_output(self.writer)
    }
}

pub(super) struct RecordFileReader<T: DeserializeOwned> {
    reader: RunReader,
    marker: PhantomData<T>,
}

impl<T: DeserializeOwned> RecordFileReader<T> {
    pub(super) fn open(path: &Path) -> Result<Self, NativeError> {
        Ok(Self {
            reader: RunReader::open(path)?,
            marker: PhantomData,
        })
    }

    pub(super) fn next(&mut self) -> Result<Option<T>, NativeError> {
        self.reader
            .read_record()?
            .map(|encoded| serde_json::from_slice(&encoded).map_err(NativeError::from))
            .transpose()
    }
}

impl<T: SpillRecord> SortedStream<T> {
    fn open(
        descriptors: &[RunDescriptor],
        metrics: SpillMetrics,
        cleanup_when_exhausted: bool,
    ) -> Result<Self, NativeError> {
        let mut live_charge = LiveChargeGuard::new(metrics.clone());
        let mut readers = Vec::new();
        readers
            .try_reserve_exact(descriptors.len())
            .map_err(|_| NativeError::Io(io::Error::other("merge reader allocation failed")))?;
        let mut heap = BinaryHeap::new();
        let mut heap_bytes = 0_usize;
        for (reader_index, descriptor) in descriptors.iter().enumerate() {
            let mut reader = RunReader::open(&descriptor.path)?;
            if let Some(encoded) = reader.read_record()? {
                let record = serde_json::from_slice::<T>(&encoded)?;
                let key = record.sort_key();
                let accounted_bytes = heap_item_bytes::<T>(&key, &encoded)?;
                let next_heap_bytes = heap_bytes
                    .checked_add(accounted_bytes)
                    .ok_or_else(|| NativeError::InvalidInput("merge accounting overflow".into()))?;
                heap.try_reserve(1).map_err(|_| {
                    NativeError::Io(io::Error::other("merge heap allocation failed"))
                })?;
                metrics.add_live(accounted_bytes);
                live_charge.add(accounted_bytes);
                heap.push(HeapItem {
                    key,
                    reader_index,
                    encoded,
                    record,
                    accounted_bytes,
                });
                heap_bytes = next_heap_bytes;
            }
            readers.push(reader);
        }
        live_charge.disarm();
        Ok(Self {
            readers,
            heap,
            heap_bytes,
            outstanding_bytes: 0,
            pending_reader: None,
            metrics,
            input_paths: descriptors
                .iter()
                .map(|descriptor| descriptor.path.clone())
                .collect(),
            cleanup_when_exhausted,
        })
    }

    pub(super) fn next(&mut self) -> Result<Option<T>, NativeError> {
        Ok(self.next_encoded()?.map(|item| item.record))
    }

    fn next_encoded(&mut self) -> Result<Option<HeapItem<T>>, NativeError> {
        if self.outstanding_bytes > 0 {
            self.metrics.release_live(self.outstanding_bytes);
            self.outstanding_bytes = 0;
        }
        if let Some(reader_index) = self.pending_reader.take() {
            if let Some(encoded) = self.readers[reader_index].read_record()? {
                let record = serde_json::from_slice::<T>(&encoded)?;
                let key = record.sort_key();
                let accounted_bytes = heap_item_bytes::<T>(&key, &encoded)?;
                let next_heap_bytes = self
                    .heap_bytes
                    .checked_add(accounted_bytes)
                    .ok_or_else(|| NativeError::InvalidInput("merge accounting overflow".into()))?;
                self.heap.try_reserve(1).map_err(|_| {
                    NativeError::Io(io::Error::other("merge heap allocation failed"))
                })?;
                self.metrics.add_live(accounted_bytes);
                self.heap.push(HeapItem {
                    key,
                    reader_index,
                    encoded,
                    record,
                    accounted_bytes,
                });
                self.heap_bytes = next_heap_bytes;
            }
        }
        let Some(item) = self.heap.pop() else {
            if self.cleanup_when_exhausted {
                self.cleanup_inputs()?;
            }
            return Ok(None);
        };
        self.heap_bytes = self.heap_bytes.saturating_sub(item.accounted_bytes);
        self.outstanding_bytes = item.accounted_bytes;
        self.pending_reader = Some(item.reader_index);
        Ok(Some(item))
    }

    fn cleanup_inputs(&mut self) -> Result<(), NativeError> {
        self.readers.clear();
        for path in std::mem::take(&mut self.input_paths) {
            if path.exists() {
                fs::remove_file(path)?;
            }
        }
        Ok(())
    }
}

struct LiveChargeGuard {
    metrics: SpillMetrics,
    bytes: usize,
    armed: bool,
}

impl LiveChargeGuard {
    fn new(metrics: SpillMetrics) -> Self {
        Self {
            metrics,
            bytes: 0,
            armed: true,
        }
    }

    fn add(&mut self, bytes: usize) {
        self.bytes = self.bytes.saturating_add(bytes);
    }

    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for LiveChargeGuard {
    fn drop(&mut self) {
        if self.armed {
            self.metrics.release_live(self.bytes);
        }
    }
}

impl<T: SpillRecord> Drop for SortedStream<T> {
    fn drop(&mut self) {
        self.metrics
            .release_live(self.heap_bytes.saturating_add(self.outstanding_bytes));
        self.heap_bytes = 0;
        self.outstanding_bytes = 0;
        if self.cleanup_when_exhausted && !self.input_paths.is_empty() {
            self.readers.clear();
            for path in std::mem::take(&mut self.input_paths) {
                if path.is_file() {
                    let _ = fs::remove_file(path);
                }
            }
        }
    }
}

struct HeapItem<T: SpillRecord> {
    key: T::Key,
    reader_index: usize,
    encoded: Vec<u8>,
    record: T,
    accounted_bytes: usize,
}

impl<T: SpillRecord> PartialEq for HeapItem<T> {
    fn eq(&self, other: &Self) -> bool {
        self.key == other.key && self.reader_index == other.reader_index
    }
}

impl<T: SpillRecord> Eq for HeapItem<T> {}

impl<T: SpillRecord> PartialOrd for HeapItem<T> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<T: SpillRecord> Ord for HeapItem<T> {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .key
            .cmp(&self.key)
            .then_with(|| other.reader_index.cmp(&self.reader_index))
    }
}

struct RunReader {
    reader: BufReader<File>,
}

impl RunReader {
    fn open(path: &Path) -> Result<Self, NativeError> {
        Ok(Self {
            reader: BufReader::new(File::open(path)?),
        })
    }

    fn read_record(&mut self) -> Result<Option<Vec<u8>>, NativeError> {
        let mut prefix = [0_u8; LENGTH_PREFIX_BYTES];
        let first = self.reader.read(&mut prefix[..1])?;
        if first == 0 {
            return Ok(None);
        }
        self.reader.read_exact(&mut prefix[1..])?;
        let length = u32::from_le_bytes(prefix) as usize;
        let mut encoded = Vec::new();
        encoded
            .try_reserve_exact(length)
            .map_err(|_| NativeError::Io(io::Error::other("spill record allocation failed")))?;
        encoded.resize(length, 0);
        self.reader.read_exact(&mut encoded)?;
        Ok(Some(encoded))
    }
}

fn merge_run_group<T: SpillRecord>(
    inputs: &[RunDescriptor],
    output: &Path,
    metrics: &SpillMetrics,
) -> Result<RunDescriptor, NativeError> {
    let mut stream = SortedStream::<T>::open(inputs, metrics.clone(), false)?;
    let mut writer = BufWriter::new(StagingOutputFile::create(output)?);
    while let Some(item) = stream.next_encoded()? {
        write_encoded(&mut writer, &item.encoded, metrics)?;
    }
    finish_output(writer)?;
    drop(stream);
    for input in inputs {
        fs::remove_file(&input.path)?;
    }
    Ok(RunDescriptor {
        path: output.to_path_buf(),
    })
}

fn heap_item_bytes<T: SpillRecord>(key: &T::Key, encoded: &[u8]) -> Result<usize, NativeError> {
    encoded
        .len()
        .checked_mul(2)
        .and_then(|bytes| bytes.checked_add(T::key_bytes(key)))
        .and_then(|bytes| bytes.checked_add(std::mem::size_of::<HeapItem<T>>()))
        .ok_or_else(|| NativeError::InvalidInput("merge record size overflow".into()))
}

fn write_encoded(
    writer: &mut impl Write,
    encoded: &[u8],
    metrics: &SpillMetrics,
) -> Result<(), NativeError> {
    let length = u32::try_from(encoded.len()).map_err(|_| {
        NativeError::InvalidInput("spill record exceeds u32 encoding length".into())
    })?;
    writer.write_all(&length.to_le_bytes())?;
    writer.write_all(encoded)?;
    metrics.add_written((LENGTH_PREFIX_BYTES + encoded.len()) as u64);
    Ok(())
}

pub(super) fn encode_record<T: Serialize>(value: &T) -> Result<Vec<u8>, NativeError> {
    let mut writer = FallibleBuffer::default();
    serde_json::to_writer(&mut writer, value)?;
    Ok(writer.bytes)
}

pub(super) fn encode_output(
    encode: impl FnOnce(&mut dyn Write) -> io::Result<()>,
) -> Result<Vec<u8>, NativeError> {
    let mut writer = FallibleBuffer::default();
    encode(&mut writer)?;
    Ok(writer.bytes)
}

#[derive(Default)]
struct FallibleBuffer {
    bytes: Vec<u8>,
}

impl Write for FallibleBuffer {
    fn write(&mut self, source: &[u8]) -> io::Result<usize> {
        self.bytes
            .try_reserve(source.len())
            .map_err(|_| io::Error::other("record allocation failed"))?;
        self.bytes.extend_from_slice(source);
        Ok(source.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};
    use std::time::{SystemTime, UNIX_EPOCH};

    #[derive(Debug, Deserialize, Serialize)]
    struct TestRecord {
        key: u32,
        payload: String,
    }

    impl SpillRecord for TestRecord {
        type Key = u32;

        fn sort_key(&self) -> Self::Key {
            self.key
        }

        fn key_bytes(_key: &Self::Key) -> usize {
            std::mem::size_of::<u32>()
        }
    }

    #[test]
    fn metrics_saturate_without_overflow_or_underflow() {
        let metrics = SpillMetrics::default();
        metrics.add_live(usize::MAX);
        metrics.add_live(1);
        assert_eq!(metrics.current_bytes(), usize::MAX);
        assert_eq!(metrics.snapshot().1, usize::MAX);
        metrics.release_live(usize::MAX);
        metrics.release_live(1);
        assert_eq!(metrics.current_bytes(), 0);
        metrics.add_written(u64::MAX);
        metrics.add_written(1);
        assert_eq!(metrics.snapshot().0, u64::MAX);
    }

    #[test]
    fn metrics_preserve_concurrent_charges_and_written_totals() {
        let metrics = SpillMetrics::default();
        let ready = Arc::new(std::sync::Barrier::new(5));
        let release = Arc::new(std::sync::Barrier::new(5));
        let threads = (0..4)
            .map(|_| {
                let metrics = metrics.clone();
                let ready = ready.clone();
                let release = release.clone();
                std::thread::spawn(move || {
                    let charge = metrics.charge(7);
                    ready.wait();
                    release.wait();
                    drop(charge);
                    for _ in 0..1_000 {
                        let _charge = metrics.charge(7);
                        metrics.add_written(3);
                    }
                })
            })
            .collect::<Vec<_>>();
        ready.wait();
        let charged = metrics.current_bytes();
        release.wait();
        for thread in threads {
            thread.join().unwrap();
        }
        assert_eq!(charged, 28);
        assert_eq!(metrics.current_bytes(), 0);
        assert_eq!(metrics.snapshot(), (12_000, 28));
    }

    #[test]
    fn metrics_track_aggregate_live_buffers_and_release_on_drop() {
        let root = temp_root("aggregate-hwm");
        let metrics = SpillMetrics::default();
        let mut left =
            SortedSpool::<TestRecord>::new(&root, "left", 8_192, metrics.clone()).unwrap();
        let mut right =
            SortedSpool::<TestRecord>::new(&root, "right", 8_192, metrics.clone()).unwrap();
        let record = |key| TestRecord {
            key,
            payload: "x".repeat(512),
        };

        left.push(&record(1)).unwrap();
        let after_left = metrics.current_bytes();
        let (_, left_high_water) = metrics.snapshot();
        assert!(after_left > 0);
        assert_eq!(left_high_water, after_left);

        right.push(&record(2)).unwrap();
        let after_both = metrics.current_bytes();
        let (_, aggregate_high_water) = metrics.snapshot();
        assert!(after_both > after_left);
        assert_eq!(aggregate_high_water, after_both);

        drop(left);
        assert!(metrics.current_bytes() < after_both);
        drop(right);
        assert_eq!(metrics.current_bytes(), 0);
    }

    #[test]
    fn multi_level_merge_removes_consumed_runs_and_stays_heap_bounded() {
        let root = temp_root("merge-cleanup");
        let metrics = SpillMetrics::default();
        let chunk_limit = 4_096;
        let mut spool =
            SortedSpool::<TestRecord>::new(&root, "records", chunk_limit, metrics.clone()).unwrap();
        for key in (0..2_048).rev() {
            spool
                .push(&TestRecord {
                    key,
                    payload: "payload".repeat(12),
                })
                .unwrap();
        }

        let mut stream = spool.finish().unwrap();
        let surviving_runs = run_file_count(&root);
        assert!(surviving_runs > 0);
        assert!(surviving_runs <= MAX_MERGE_FAN_IN);

        let mut keys = Vec::new();
        while let Some(record) = stream.next().unwrap() {
            keys.push(record.key);
        }
        assert_eq!(keys, (0..2_048).collect::<Vec<_>>());
        assert_eq!(run_file_count(&root), 0);
        assert_eq!(metrics.current_bytes(), 0);
        let (_, high_water) = metrics.snapshot();
        assert!(high_water > 0);
        assert!(high_water <= chunk_limit.saturating_mul(MAX_MERGE_FAN_IN));
    }

    #[test]
    fn jumbo_records_merge_across_multiple_levels_without_a_budget() {
        let root = temp_root("jumbo-merge");
        let metrics = SpillMetrics::default();
        let mut spool =
            SortedSpool::<TestRecord>::new(&root, "jumbo", 64, metrics.clone()).unwrap();
        let count = (MAX_MERGE_FAN_IN * MAX_MERGE_FAN_IN + 3) as u32;
        for key in (0..count).rev() {
            spool
                .push(&TestRecord {
                    key,
                    payload: "jumbo".repeat(80),
                })
                .unwrap();
        }
        assert_eq!(run_file_count(&root), count as usize);
        let mut stream = spool.finish().unwrap();
        assert!(run_file_count(&root) <= MAX_MERGE_FAN_IN);
        for key in 0..count {
            let record = stream.next().unwrap().unwrap();
            assert_eq!(record.key, key);
            assert_eq!(record.payload, "jumbo".repeat(80));
        }
        assert!(stream.next().unwrap().is_none());
        assert_eq!(run_file_count(&root), 0);
        assert_eq!(metrics.current_bytes(), 0);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn mixed_small_and_jumbo_records_keep_sorted_values() {
        let root = temp_root("mixed-jumbo");
        let mut spool =
            SortedSpool::<TestRecord>::new(&root, "mixed", 256, SpillMetrics::default()).unwrap();
        for key in (0..75).rev() {
            spool
                .push(&TestRecord {
                    key,
                    payload: if key % 3 == 0 {
                        "large".repeat(400)
                    } else {
                        "small".into()
                    },
                })
                .unwrap();
        }
        let mut stream = spool.finish().unwrap();
        for key in 0..75 {
            let row = stream.next().unwrap().unwrap();
            assert_eq!(row.key, key);
            assert_eq!(
                row.payload,
                if key % 3 == 0 {
                    "large".repeat(400)
                } else {
                    "small".into()
                }
            );
        }
        assert!(stream.next().unwrap().is_none());
        assert_eq!(run_file_count(&root), 0);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn dropping_final_stream_removes_owned_run_files() {
        let root = temp_root("drop-cleanup");
        let metrics = SpillMetrics::default();
        let mut spool = SortedSpool::<TestRecord>::new(&root, "records", 4_096, metrics).unwrap();
        for key in 0..64 {
            spool
                .push(&TestRecord {
                    key,
                    payload: "drop".repeat(24),
                })
                .unwrap();
        }

        let stream = spool.finish().unwrap();
        assert!(run_file_count(&root) > 0);
        drop(stream);
        assert_eq!(run_file_count(&root), 0);
    }

    fn run_file_count(root: &Path) -> usize {
        fs::read_dir(root)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| {
                entry
                    .path()
                    .extension()
                    .is_some_and(|extension| extension == "bin")
            })
            .count()
    }

    fn temp_root(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "codebase_graph_spill_{name}_{}_{nanos}",
            std::process::id()
        ));
        fs::create_dir_all(&root).unwrap();
        root
    }
}
