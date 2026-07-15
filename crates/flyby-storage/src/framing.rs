//! Record framing strategies.
//!
//! A *framing* strategy determines how individual records are extracted from a
//! raw byte stream.  Four strategies are supported:
//!
//! | Strategy | Description |
//! |---|---|
//! | [`FixedLength`] | Every record is exactly N bytes |
//! | [`LengthPrefixed`] | A header carries the payload length |
//! | [`Delimiter`] | Records end with a sentinel byte (e.g. `\n`) |
//! | [`Custom`] | Caller-supplied closure |
//!
//! All strategies share the [`Frame`] trait: given a byte buffer they return
//! the length of the next record (or `None` if more data is needed).  The
//! [`FileSource`][crate::file::FileSource] uses the strategy to split its
//! read buffer into records before populating a [`crate::batch::RawRecordBatch`].
//!
//! ## Framing vs parsing
//!
//! The framer only finds record *boundaries*; it never interprets the content.
//! Parsing (decoding bytes into a typed [`Message`][flyby_core::Message])
//! happens downstream via a [`Decoder`][flyby_core::Decoder].

use flyby_core::{Error, Result};

// ---------------------------------------------------------------------------
// Trait
// ---------------------------------------------------------------------------

/// Determine where the next record ends in a byte buffer.
///
/// Built-in framers are effectively pure. Custom framers may be stateful
/// (e.g. re-sync scanners). Zero-length records (`Some(0)`) are rejected by
/// [`FileSource`][crate::file::FileSource]; framers must not return them.
pub trait Frame: Send + Sync + 'static {
    /// Return the total byte length of the next record found in `buf`.
    ///
    /// - `Ok(Some(n))` — the first `n` bytes form a complete record (`n > 0`
    ///   and `n <= buf.len()`).
    /// - `Ok(None)` — more data is needed; retry after appending bytes.
    /// - `Err(_)` — the buffer is unrecoverable (corrupt framing header).
    fn next_record_len(&mut self, buf: &[u8]) -> Result<Option<usize>>;
}

// ---------------------------------------------------------------------------
// Fixed-length framing
// ---------------------------------------------------------------------------

/// Every record is exactly `record_len` bytes.
///
/// This is the most efficient strategy: no header parsing, no scanning.
/// Used for binary formats with a known, constant record size (e.g. market-
/// tick blobs).
pub struct FixedLength {
    record_len: usize,
}

impl FixedLength {
    /// Create a fixed-length framer.
    ///
    /// # Panics
    ///
    /// Panics if `record_len` is zero.
    pub fn new(record_len: usize) -> Self {
        assert!(record_len > 0, "record_len must be > 0");
        Self { record_len }
    }
}

impl Frame for FixedLength {
    fn next_record_len(&mut self, buf: &[u8]) -> Result<Option<usize>> {
        if buf.len() >= self.record_len {
            Ok(Some(self.record_len))
        } else {
            Ok(None)
        }
    }
}

// ---------------------------------------------------------------------------
// Length-prefixed framing
// ---------------------------------------------------------------------------

/// Length prefix width.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrefixWidth {
    /// 1-byte unsigned prefix (max payload 255 bytes).
    U8,
    /// 2-byte big-endian unsigned prefix (max payload 65 535 bytes).
    U16Be,
    /// 4-byte big-endian unsigned prefix (max payload ~4 GiB).
    U32Be,
}

impl PrefixWidth {
    fn byte_count(self) -> usize {
        match self {
            Self::U8 => 1,
            Self::U16Be => 2,
            Self::U32Be => 4,
        }
    }

    fn read(self, buf: &[u8]) -> u64 {
        match self {
            Self::U8 => buf[0] as u64,
            Self::U16Be => u16::from_be_bytes([buf[0], buf[1]]) as u64,
            Self::U32Be => u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]) as u64,
        }
    }
}

/// A header at the start of each record carries its payload length.
///
/// Total record size seen by the pipeline is `prefix_bytes + payload_bytes`.
/// The [`RawRecordBatch`][crate::batch::RawRecordBatch] slot receives the
/// *full* record (prefix + payload) so the decoder can strip the header.
pub struct LengthPrefixed {
    width: PrefixWidth,
    /// Maximum allowed payload length.  Records exceeding this are rejected.
    max_payload: usize,
}

impl LengthPrefixed {
    /// Create a length-prefixed framer.
    pub fn new(width: PrefixWidth, max_payload: usize) -> Self {
        Self { width, max_payload }
    }
}

impl Frame for LengthPrefixed {
    fn next_record_len(&mut self, buf: &[u8]) -> Result<Option<usize>> {
        let header = self.width.byte_count();
        if buf.len() < header {
            return Ok(None);
        }
        let payload_len = self.width.read(&buf[..header]) as usize;
        if payload_len > self.max_payload {
            return Err(Error::new(
                flyby_core::ErrorKind::Decode,
                format!(
                    "framing: payload length {payload_len} exceeds max {}",
                    self.max_payload
                ),
            ));
        }
        let total = header + payload_len;
        if buf.len() >= total {
            Ok(Some(total))
        } else {
            Ok(None)
        }
    }
}

// ---------------------------------------------------------------------------
// Delimiter framing
// ---------------------------------------------------------------------------

/// Records end with a sentinel byte (the delimiter).
///
/// The delimiter byte is **included** in the returned record so the decoder can
/// choose to strip it or treat it as a field separator.  For newline-delimited
/// formats set `delimiter = b'\n'`.
pub struct Delimiter {
    byte: u8,
    /// Maximum scan length before the framer gives up and returns an error.
    max_len: usize,
}

impl Delimiter {
    /// Create a delimiter-based framer.
    ///
    /// `max_len` is the maximum record length including the delimiter byte.
    /// If no delimiter is found within `max_len` bytes the framer returns an
    /// error.
    pub fn new(byte: u8, max_len: usize) -> Self {
        assert!(max_len > 0, "max_len must be > 0");
        Self { byte, max_len }
    }
}

impl Frame for Delimiter {
    fn next_record_len(&mut self, buf: &[u8]) -> Result<Option<usize>> {
        let scan_len = buf.len().min(self.max_len);
        match buf[..scan_len].iter().position(|&b| b == self.byte) {
            Some(pos) => Ok(Some(pos + 1)), // include the delimiter
            None if buf.len() >= self.max_len => Err(Error::new(
                flyby_core::ErrorKind::Decode,
                format!("framing: no delimiter found within {} bytes", self.max_len),
            )),
            None => Ok(None), // need more data
        }
    }
}

// ---------------------------------------------------------------------------
// Custom framing
// ---------------------------------------------------------------------------

/// Caller-supplied framing closure.
///
/// The closure has the same contract as [`Frame::next_record_len`]:
/// `Ok(Some(n))` = record of `n` bytes, `Ok(None)` = need more data,
/// `Err(_)` = unrecoverable.
pub struct Custom<F>
where
    F: FnMut(&[u8]) -> Result<Option<usize>> + Send + Sync + 'static,
{
    f: F,
}

impl<F> Custom<F>
where
    F: FnMut(&[u8]) -> Result<Option<usize>> + Send + Sync + 'static,
{
    /// Wrap a closure as a [`Frame`] implementation.
    pub fn new(f: F) -> Self {
        Self { f }
    }
}

impl<F> Frame for Custom<F>
where
    F: FnMut(&[u8]) -> Result<Option<usize>> + Send + Sync + 'static,
{
    fn next_record_len(&mut self, buf: &[u8]) -> Result<Option<usize>> {
        (self.f)(buf)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_needs_full_record() {
        let mut f = FixedLength::new(8);
        assert_eq!(f.next_record_len(b"hello").unwrap(), None);
        assert_eq!(f.next_record_len(b"hello!!!").unwrap(), Some(8));
        assert_eq!(f.next_record_len(b"hello!!!!extra").unwrap(), Some(8));
    }

    #[test]
    fn length_prefixed_u8() {
        let mut f = LengthPrefixed::new(PrefixWidth::U8, 256);
        // 1 byte header saying payload = 3
        let buf: &[u8] = &[3, b'a', b'b', b'c'];
        assert_eq!(f.next_record_len(buf).unwrap(), Some(4));
    }

    #[test]
    fn length_prefixed_too_short() {
        let mut f = LengthPrefixed::new(PrefixWidth::U16Be, 1024);
        // Only one byte — need 2 for the header
        assert_eq!(f.next_record_len(&[0x00]).unwrap(), None);
    }

    #[test]
    fn length_prefixed_exceeds_max() {
        let mut f = LengthPrefixed::new(PrefixWidth::U8, 4);
        let buf: &[u8] = &[10, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]; // says 10 bytes
        assert!(f.next_record_len(buf).is_err());
    }

    #[test]
    fn delimiter_finds_newline() {
        let mut f = Delimiter::new(b'\n', 1024);
        assert_eq!(f.next_record_len(b"hello\nworld").unwrap(), Some(6));
    }

    #[test]
    fn delimiter_needs_more_data() {
        let mut f = Delimiter::new(b'\n', 1024);
        assert_eq!(f.next_record_len(b"no newline here").unwrap(), None);
    }

    #[test]
    fn delimiter_exceeds_max() {
        let mut f = Delimiter::new(b'\n', 5);
        // 6 bytes with no newline — should error
        assert!(f.next_record_len(b"abcdef").is_err());
    }

    #[test]
    fn custom_framer() {
        // Always returns records of 3 bytes
        let mut f = Custom::new(|buf: &[u8]| {
            if buf.len() >= 3 {
                Ok(Some(3))
            } else {
                Ok(None)
            }
        });
        assert_eq!(f.next_record_len(b"ab").unwrap(), None);
        assert_eq!(f.next_record_len(b"abc").unwrap(), Some(3));
    }
}
