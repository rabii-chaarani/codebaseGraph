//! Allocation-bounded JSON facts, not schema, provenance or usability proofs.
use super::maintenance_budget::{CleanupMemory, MemoryReservation, ReservedBytes, ResourceLimit};
use sha2::{Digest, Sha256};
use std::io::{self, Read};

const IO_BYTES: usize = 4096;
// serde_json 1.0.150 defaults to 128 and rejects when decrement reaches zero.
// The ordinary artifact loader does not disable that recursion limit.
const MAX_CONTAINERS: usize = 127;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum JsonFailure {
    Resource(ResourceLimit),
    Io(io::ErrorKind),
    Malformed,
    LoaderDepthExceeded,
    OffsetOverflow,
}
impl From<ResourceLimit> for JsonFailure {
    fn from(value: ResourceLimit) -> Self {
        Self::Resource(value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TokenKind {
    ObjectStart,
    ObjectEnd,
    ArrayStart,
    ArrayEnd,
    String {
        key: bool,
        decoded_len: u64,
        decoded_sha256: [u8; 32],
        producer_escapes: bool,
    },
    Number,
    Bool(bool),
    Null,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct JsonToken {
    pub(crate) start: u64,
    pub(crate) end: u64,
    pub(crate) kind: TokenKind,
}

#[derive(Clone, Copy)]
enum Frame {
    ObjectFirst,
    ObjectKey,
    Colon,
    ObjectValue,
    ObjectNext,
    ArrayFirst,
    ArrayValue,
    ArrayNext,
}

pub(crate) struct MaintenanceJsonReader<R> {
    input: R,
    buffer: ReservedBytes,
    cursor: usize,
    filled: usize,
    offset: u64,
    frames: [Frame; MAX_CONTAINERS],
    depth: usize,
    started: bool,
    failed: bool,
    raw_hash: Sha256,
    compact_hash: Sha256,
    // Covers this reader's fixed state and bounded stack scratch, including R's
    // inline layout. R must not conceal an uncharged heap buffer/path allocation.
    _floor: MemoryReservation,
}
impl<R: Read> MaintenanceJsonReader<R> {
    pub(crate) fn new(input: R, memory: &CleanupMemory) -> Result<Self, JsonFailure> {
        let floor = std::mem::size_of::<Self>()
            .checked_add(512)
            .ok_or(JsonFailure::Resource(ResourceLimit::ArithmeticOverflow))?;
        let reservation = memory.reserve(floor)?;
        let buffer = memory.reserve(IO_BYTES)?.bytes(IO_BYTES)?;
        Ok(Self {
            input,
            buffer,
            cursor: 0,
            filled: 0,
            offset: 0,
            frames: [Frame::ArrayFirst; MAX_CONTAINERS],
            depth: 0,
            started: false,
            failed: false,
            raw_hash: Sha256::new(),
            compact_hash: Sha256::new(),
            _floor: reservation,
        })
    }
    fn peek(&mut self) -> Result<Option<u8>, JsonFailure> {
        if self.cursor == self.filled {
            loop {
                match self.input.read(self.buffer.as_mut_slice()) {
                    Ok(n) => {
                        self.cursor = 0;
                        self.filled = n;
                        break;
                    }
                    Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                    Err(e) => return Err(JsonFailure::Io(e.kind())),
                }
            }
        }
        Ok((self.cursor < self.filled).then(|| self.buffer.as_slice()[self.cursor]))
    }
    fn byte(&mut self, compact: bool) -> Result<u8, JsonFailure> {
        let byte = self.peek()?.ok_or(JsonFailure::Malformed)?;
        self.offset = self
            .offset
            .checked_add(1)
            .ok_or(JsonFailure::OffsetOverflow)?;
        self.cursor += 1;
        self.raw_hash.update([byte]);
        if compact {
            self.compact_hash.update([byte]);
        }
        Ok(byte)
    }
    fn whitespace(&mut self) -> Result<(), JsonFailure> {
        while matches!(self.peek()?, Some(b' ' | b'\n' | b'\r' | b'\t')) {
            self.byte(false)?;
        }
        Ok(())
    }
    fn expected(&mut self, byte: u8) -> Result<(), JsonFailure> {
        if self.byte(true)? == byte {
            Ok(())
        } else {
            Err(JsonFailure::Malformed)
        }
    }
    fn hex4(&mut self) -> Result<u32, JsonFailure> {
        let mut value = 0;
        for _ in 0..4 {
            let digit = match self.byte(true)? {
                b @ b'0'..=b'9' => b - b'0',
                b @ b'a'..=b'f' => b - b'a' + 10,
                b @ b'A'..=b'F' => b - b'A' + 10,
                _ => return Err(JsonFailure::Malformed),
            };
            value = value * 16 + u32::from(digit);
        }
        Ok(value)
    }
    fn string(&mut self, key: bool) -> Result<TokenKind, JsonFailure> {
        self.expected(b'"')?;
        let mut hash = Sha256::new();
        let mut len = 0u64;
        let mut producer_escapes = true;
        loop {
            let byte = self.byte(true)?;
            let mut scalar = [0u8; 4];
            let count;
            match byte {
                b'"' => break,
                0..=31 => return Err(JsonFailure::Malformed),
                b'\\' => {
                    let escape = self.byte(true)?;
                    match escape {
                        b'"' | b'\\' => {
                            scalar[0] = escape;
                            count = 1;
                        }
                        b'/' => {
                            scalar[0] = b'/';
                            count = 1;
                            producer_escapes = false;
                        }
                        b'b' => {
                            scalar[0] = 8;
                            count = 1;
                        }
                        b'f' => {
                            scalar[0] = 12;
                            count = 1;
                        }
                        b'n' => {
                            scalar[0] = 10;
                            count = 1;
                        }
                        b'r' => {
                            scalar[0] = 13;
                            count = 1;
                        }
                        b't' => {
                            scalar[0] = 9;
                            count = 1;
                        }
                        b'u' => {
                            let first = self.hex4()?;
                            let code = if (0xd800..=0xdbff).contains(&first) {
                                self.expected(b'\\')?;
                                self.expected(b'u')?;
                                let low = self.hex4()?;
                                if !(0xdc00..=0xdfff).contains(&low) {
                                    return Err(JsonFailure::Malformed);
                                }
                                0x10000 + ((first - 0xd800) << 10) + low - 0xdc00
                            } else {
                                first
                            };
                            let character = char::from_u32(code).ok_or(JsonFailure::Malformed)?;
                            count = character.encode_utf8(&mut scalar).len();
                            // Conservative fact: escaped Unicode is never used
                            // as an exact producer-form proof by this tokenizer.
                            producer_escapes = false;
                        }
                        _ => return Err(JsonFailure::Malformed),
                    }
                }
                0x20..=0x7f => {
                    scalar[0] = byte;
                    count = 1;
                }
                _ => {
                    count = match byte {
                        0xc2..=0xdf => 2,
                        0xe0..=0xef => 3,
                        0xf0..=0xf4 => 4,
                        _ => return Err(JsonFailure::Malformed),
                    };
                    scalar[0] = byte;
                    for value in &mut scalar[1..count] {
                        *value = self.byte(true)?;
                    }
                    std::str::from_utf8(&scalar[..count]).map_err(|_| JsonFailure::Malformed)?;
                }
            }
            len = len
                .checked_add(count as u64)
                .ok_or(JsonFailure::OffsetOverflow)?;
            hash.update(&scalar[..count]);
        }
        Ok(TokenKind::String {
            key,
            decoded_len: len,
            decoded_sha256: hash.finalize().into(),
            producer_escapes,
        })
    }
    fn number(&mut self) -> Result<(), JsonFailure> {
        if self.peek()? == Some(b'-') {
            self.byte(true)?;
        }
        match self.byte(true)? {
            b'0' => {}
            b'1'..=b'9' => {
                while matches!(self.peek()?, Some(b'0'..=b'9')) {
                    self.byte(true)?;
                }
            }
            _ => return Err(JsonFailure::Malformed),
        }
        if self.peek()? == Some(b'.') {
            self.byte(true)?;
            if !matches!(self.peek()?, Some(b'0'..=b'9')) {
                return Err(JsonFailure::Malformed);
            }
            while matches!(self.peek()?, Some(b'0'..=b'9')) {
                self.byte(true)?;
            }
        }
        if matches!(self.peek()?, Some(b'e' | b'E')) {
            self.byte(true)?;
            if matches!(self.peek()?, Some(b'+' | b'-')) {
                self.byte(true)?;
            }
            if !matches!(self.peek()?, Some(b'0'..=b'9')) {
                return Err(JsonFailure::Malformed);
            }
            while matches!(self.peek()?, Some(b'0'..=b'9')) {
                self.byte(true)?;
            }
        }
        Ok(())
    }
    fn value(&mut self) -> Result<TokenKind, JsonFailure> {
        Ok(match self.peek()?.ok_or(JsonFailure::Malformed)? {
            b'{' | b'[' => {
                if self.depth == MAX_CONTAINERS {
                    return Err(JsonFailure::LoaderDepthExceeded);
                }
                let byte = self.byte(true)?;
                self.frames[self.depth] = if byte == b'{' {
                    Frame::ObjectFirst
                } else {
                    Frame::ArrayFirst
                };
                self.depth += 1;
                if byte == b'{' {
                    TokenKind::ObjectStart
                } else {
                    TokenKind::ArrayStart
                }
            }
            b'"' => self.string(false)?,
            b'-' | b'0'..=b'9' => {
                self.number()?;
                TokenKind::Number
            }
            b't' => {
                for byte in b"true" {
                    self.expected(*byte)?;
                }
                TokenKind::Bool(true)
            }
            b'f' => {
                for byte in b"false" {
                    self.expected(*byte)?;
                }
                TokenKind::Bool(false)
            }
            b'n' => {
                for byte in b"null" {
                    self.expected(*byte)?;
                }
                TokenKind::Null
            }
            _ => return Err(JsonFailure::Malformed),
        })
    }
    fn next_inner(&mut self) -> Result<Option<JsonToken>, JsonFailure> {
        loop {
            self.whitespace()?;
            let start = self.offset;
            let kind = if self.depth == 0 {
                if self.started {
                    return if self.peek()?.is_none() {
                        Ok(None)
                    } else {
                        Err(JsonFailure::Malformed)
                    };
                }
                self.started = true;
                self.value()?
            } else {
                let index = self.depth - 1;
                match self.frames[index] {
                    Frame::Colon => {
                        self.expected(b':')?;
                        self.frames[index] = Frame::ObjectValue;
                        continue;
                    }
                    Frame::ObjectFirst | Frame::ObjectKey => {
                        if matches!(self.frames[index], Frame::ObjectFirst)
                            && self.peek()? == Some(b'}')
                        {
                            self.byte(true)?;
                            self.depth -= 1;
                            TokenKind::ObjectEnd
                        } else {
                            let token = self.string(true)?;
                            self.frames[index] = Frame::Colon;
                            token
                        }
                    }
                    Frame::ObjectValue => {
                        self.frames[index] = Frame::ObjectNext;
                        self.value()?
                    }
                    Frame::ArrayFirst | Frame::ArrayValue => {
                        if matches!(self.frames[index], Frame::ArrayFirst)
                            && self.peek()? == Some(b']')
                        {
                            self.byte(true)?;
                            self.depth -= 1;
                            TokenKind::ArrayEnd
                        } else {
                            self.frames[index] = Frame::ArrayNext;
                            self.value()?
                        }
                    }
                    Frame::ObjectNext | Frame::ArrayNext => {
                        let object = matches!(self.frames[index], Frame::ObjectNext);
                        if self.peek()? == Some(b',') {
                            self.byte(true)?;
                            self.frames[index] = if object {
                                Frame::ObjectKey
                            } else {
                                Frame::ArrayValue
                            };
                            continue;
                        }
                        self.expected(if object { b'}' } else { b']' })?;
                        self.depth -= 1;
                        if object {
                            TokenKind::ObjectEnd
                        } else {
                            TokenKind::ArrayEnd
                        }
                    }
                }
            };
            return Ok(Some(JsonToken {
                start,
                end: self.offset,
                kind,
            }));
        }
    }
    pub(crate) fn next_token(&mut self) -> Result<Option<JsonToken>, JsonFailure> {
        if self.failed {
            return Err(JsonFailure::Malformed);
        }
        let result = self.next_inner();
        if result.is_err() {
            self.failed = true;
        }
        result
    }
    /// Logical consumed offset, excluding buffered read-ahead. Facts only.
    pub(crate) fn position(&self) -> u64 {
        self.offset
    }
    /// Available only after complete EOF validation. Compact means whitespace
    /// removal, NOT schema-specific or Serde canonicalization.
    pub(crate) fn finish(mut self) -> Result<([u8; 32], [u8; 32]), JsonFailure> {
        while self.next_token()?.is_some() {}
        Ok((
            self.raw_hash.finalize().into(),
            self.compact_hash.finalize().into(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn scan(bytes: &[u8]) -> Result<([u8; 32], [u8; 32]), JsonFailure> {
        let memory = CleanupMemory::new(16384).unwrap();
        MaintenanceJsonReader::new(bytes, &memory)?.finish()
    }
    #[test]
    fn dc_json_differential_producer_whitespace_hash() {
        let values = [
            serde_json::json!({"x": [null, true, -1, 0.5, "é\n\\\""], "metadata": {"z": 1, "a": 2}}),
            serde_json::json!([]),
        ];
        for value in values {
            let pretty = serde_json::to_vec_pretty(&value).unwrap();
            let compact = serde_json::to_vec(&value).unwrap();
            let (raw, stripped) = scan(&pretty).unwrap();
            assert_eq!(raw, <[u8; 32]>::from(Sha256::digest(&pretty)));
            assert_eq!(stripped, <[u8; 32]>::from(Sha256::digest(&compact)));
        }
    }
    #[test]
    fn dc_json_producer_defaults_and_nonfinite_are_only_facts() {
        #[derive(serde::Serialize, serde::Deserialize)]
        struct Typed {
            confidence: f64,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            optional: Option<String>,
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            rows: Vec<String>,
        }
        for confidence in [0.5, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let value = Typed {
                confidence,
                optional: None,
                rows: Vec::new(),
            };
            let pretty = serde_json::to_vec_pretty(&value).unwrap();
            let compact = serde_json::to_vec(&value).unwrap();
            assert_eq!(
                scan(&pretty).unwrap().1,
                <[u8; 32]>::from(Sha256::digest(&compact))
            );
            assert_eq!(
                serde_json::from_slice::<Typed>(&pretty).is_ok(),
                confidence.is_finite()
            );
            // Token acceptance and a matching compact hash do NOT certify that
            // typed deserialization (including GraphEdgeRow) can read it.
        }
    }
    #[test]
    fn dc_json_long_string_dense_values_fixed_account() {
        let input = format!("[\"{}\",{}0]", "é".repeat(100_000), "0,".repeat(100_000));
        let memory = CleanupMemory::new(16384).unwrap();
        let floor = memory.used();
        MaintenanceJsonReader::new(input.as_bytes(), &memory)
            .unwrap()
            .finish()
            .unwrap();
        assert_eq!(memory.used(), floor);
        assert!(memory.peak() < 16384);
    }
    #[test]
    fn dc_json_unicode_duplicate_keys_and_lexical_facts() {
        let input = r#"{"x":"\uD83D\uDE00","x":"😀"}"#.as_bytes();
        let memory = CleanupMemory::new(16384).unwrap();
        let mut reader = MaintenanceJsonReader::new(&input[..], &memory).unwrap();
        let mut strings = Vec::new();
        while let Some(token) = reader.next_token().unwrap() {
            if let TokenKind::String {
                key: false,
                decoded_len,
                decoded_sha256,
                producer_escapes,
            } = token.kind
            {
                strings.push((decoded_len, decoded_sha256, producer_escapes));
            }
        }
        assert_eq!(strings[0].0, 4);
        assert_eq!(strings[0].1, strings[1].1);
        assert!(!strings[0].2);
        assert!(strings[1].2);
    }
    #[test]
    fn dc_json_rejects_malformed_trailing_utf8_and_matches_loader_depth() {
        for input in [
            b"".as_slice(),
            b"[1,]",
            b"{\"x\":1,}",
            b"01",
            b"1.",
            b"1e+",
            b"null true",
            br#""\uD800""#,
            b"\"\xff\"",
            b"[",
            b"{1:2}",
        ] {
            assert!(scan(input).is_err(), "{input:?}");
        }
        for depth in [126, 127, 128, 129] {
            let input = format!("{}0{}", "[".repeat(depth), "]".repeat(depth));
            assert_eq!(
                scan(input.as_bytes()).is_ok(),
                serde_json::from_str::<serde_json::Value>(&input).is_ok(),
                "depth={depth}"
            );
        }
        // Lexically valid JSON may exceed a loader's numeric range: facts only.
        assert!(scan(b"1e999999").is_ok());
        assert!(serde_json::from_slice::<serde_json::Value>(b"1e999999").is_err());
    }
    #[test]
    fn dc_json_low_ceiling_releases_failed_construction() {
        let memory = CleanupMemory::new(4096).unwrap();
        let floor = memory.used();
        assert!(MaintenanceJsonReader::new(&b"null"[..], &memory).is_err());
        assert_eq!(memory.used(), floor);
    }
}
