//! Integration tests: record framing strategies.
//!
//! Tests all four [`Frame`] implementations against realistic byte buffers.
//! Covers:
//! - Happy path: single record, multi-record, exact-boundary buffers
//! - Need-more-data: partial records
//! - Errors: payload exceeds max, delimiter not found within max_len
//! - Corner cases: empty buffer, back-to-back calls

use flyby_storage::{CustomFramer, Delimiter, FixedLength, Frame, LengthPrefixed, PrefixWidth};

// ---------------------------------------------------------------------------
// FixedLength
// ---------------------------------------------------------------------------

#[test]
fn fixed_empty_buffer_returns_none() {
    let mut f = FixedLength::new(8);
    assert_eq!(f.next_record_len(&[]).unwrap(), None);
}

#[test]
fn fixed_partial_record_returns_none() {
    let mut f = FixedLength::new(8);
    assert_eq!(f.next_record_len(&[0u8; 7]).unwrap(), None);
}

#[test]
fn fixed_exact_boundary_returns_len() {
    let mut f = FixedLength::new(8);
    assert_eq!(f.next_record_len(&[0u8; 8]).unwrap(), Some(8));
}

#[test]
fn fixed_excess_bytes_still_returns_record_len() {
    let mut f = FixedLength::new(8);
    assert_eq!(f.next_record_len(&[0u8; 100]).unwrap(), Some(8));
}

#[test]
fn fixed_sequential_calls_return_same_len() {
    let mut f = FixedLength::new(16);
    let buf = vec![0u8; 100];
    for _ in 0..5 {
        assert_eq!(f.next_record_len(&buf).unwrap(), Some(16));
    }
}

// ---------------------------------------------------------------------------
// LengthPrefixed
// ---------------------------------------------------------------------------

#[test]
fn lp_u8_empty_returns_none() {
    let mut f = LengthPrefixed::new(PrefixWidth::U8, 256);
    assert_eq!(f.next_record_len(&[]).unwrap(), None);
}

#[test]
fn lp_u8_zero_payload() {
    let mut f = LengthPrefixed::new(PrefixWidth::U8, 256);
    // Header = 0 means zero-byte payload; total = 1
    assert_eq!(f.next_record_len(&[0x00]).unwrap(), Some(1));
}

#[test]
fn lp_u8_full_record() {
    let mut f = LengthPrefixed::new(PrefixWidth::U8, 256);
    let mut buf = vec![5u8]; // payload len = 5
    buf.extend_from_slice(b"hello");
    assert_eq!(f.next_record_len(&buf).unwrap(), Some(6));
}

#[test]
fn lp_u8_partial_payload_returns_none() {
    let mut f = LengthPrefixed::new(PrefixWidth::U8, 256);
    // Header says 10 bytes but only 4 present
    let buf = vec![10u8, 0, 0, 0, 0];
    assert_eq!(f.next_record_len(&buf).unwrap(), None);
}

#[test]
fn lp_u8_payload_exceeds_max_returns_err() {
    let mut f = LengthPrefixed::new(PrefixWidth::U8, 3);
    let buf = vec![10u8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]; // claims 10 bytes
    assert!(f.next_record_len(&buf).is_err());
}

#[test]
fn lp_u16be_reads_two_byte_header() {
    let mut f = LengthPrefixed::new(PrefixWidth::U16Be, 65535);
    // payload len = 256 in big-endian
    let mut buf = vec![0x01u8, 0x00]; // 256
    buf.extend(vec![0u8; 256]);
    assert_eq!(f.next_record_len(&buf).unwrap(), Some(258));
}

#[test]
fn lp_u32be_reads_four_byte_header() {
    let mut f = LengthPrefixed::new(PrefixWidth::U32Be, 1024);
    let mut buf = vec![0x00u8, 0x00, 0x00, 0x0A]; // payload len = 10
    buf.extend(vec![0u8; 10]);
    assert_eq!(f.next_record_len(&buf).unwrap(), Some(14));
}

// ---------------------------------------------------------------------------
// Delimiter
// ---------------------------------------------------------------------------

#[test]
fn delim_empty_buffer_returns_none() {
    let mut f = Delimiter::new(b'\n', 1024);
    assert_eq!(f.next_record_len(&[]).unwrap(), None);
}

#[test]
fn delim_single_delimiter_byte() {
    let mut f = Delimiter::new(b'\n', 1024);
    assert_eq!(f.next_record_len(b"\n").unwrap(), Some(1));
}

#[test]
fn delim_delimiter_at_start() {
    let mut f = Delimiter::new(b'|', 1024);
    assert_eq!(f.next_record_len(b"|rest").unwrap(), Some(1));
}

#[test]
fn delim_typical_csv_line() {
    let mut f = Delimiter::new(b'\n', 4096);
    let line = b"2024-01-01,BTC/USD,42000.00,1.5\n";
    assert_eq!(f.next_record_len(line).unwrap(), Some(line.len()));
}

#[test]
fn delim_no_delimiter_short_buf_returns_none() {
    let mut f = Delimiter::new(b'\n', 100);
    // 50 bytes, no newline — buffer is shorter than max_len, so need more data
    let buf = vec![b'x'; 50];
    assert_eq!(f.next_record_len(&buf).unwrap(), None);
}

#[test]
fn delim_no_delimiter_at_max_len_returns_err() {
    let mut f = Delimiter::new(b'\n', 10);
    let buf = vec![b'x'; 10]; // exactly max_len bytes, no newline
    assert!(f.next_record_len(&buf).is_err());
}

#[test]
fn delim_delimiter_includes_byte_in_count() {
    let mut f = Delimiter::new(b'\0', 1024);
    // "abc\0" → length 4 (includes the null terminator)
    assert_eq!(f.next_record_len(b"abc\0more").unwrap(), Some(4));
}

// ---------------------------------------------------------------------------
// Custom
// ---------------------------------------------------------------------------

#[test]
fn custom_passthrough_always_returns_some() {
    let mut f = CustomFramer::new(|buf: &[u8]| Ok(Some(buf.len())));
    assert_eq!(f.next_record_len(b"anything").unwrap(), Some(8));
}

#[test]
fn custom_can_return_error() {
    use flyby_core::{Error, ErrorKind};
    let mut f = CustomFramer::new(|_buf: &[u8]| {
        Err(Error::new(ErrorKind::Decode, "corrupt magic"))
    });
    assert!(f.next_record_len(b"junk").is_err());
}

#[test]
fn custom_stateful_via_closure_captures() {
    // Alternates between two record sizes (odd/even)
    let mut toggle = false;
    let mut f = CustomFramer::new(move |buf: &[u8]| {
        let len = if toggle { 4 } else { 8 };
        toggle = !toggle;
        if buf.len() >= len { Ok(Some(len)) } else { Ok(None) }
    });
    let buf = vec![0u8; 16];
    assert_eq!(f.next_record_len(&buf).unwrap(), Some(8));
    assert_eq!(f.next_record_len(&buf).unwrap(), Some(4));
}
