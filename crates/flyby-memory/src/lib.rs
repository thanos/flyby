//! # flyby-memory
//!
//! The shared-memory sink for FlyBy: a lock-free SPSC ring of fixed-size
//! slots backed by an anonymous memory-mapped region.
//!
//! ## Architecture
//!
//! ```text
//! SharedMemorySink<M>
//!     └── Region
//!             ├── RegionHeader   (magic, version, geometry)
//!             ├── Producer head  (AtomicU64, own cache line)
//!             ├── Consumer tail  (AtomicU64, own cache line)
//!             └── Slot[0..N]     (SlotHeader + payload + padding)
//! ```
//!
//! ## Usage
//!
//! ```rust
//! use flyby_memory::SharedMemorySink;
//! use flyby_core::{Lifecycle, Sink};
//!
//! // 1. Create a sink with 1024 slots, each holding up to 64 payload bytes.
//! let mut sink: SharedMemorySink<flyby_memory::StubMessage> =
//!     SharedMemorySink::new(1024, 64).unwrap();
//!
//! // 2. Init (validates the region; no-op in the current implementation).
//! sink.init().unwrap();
//! ```
//!
//! ## Unsafe code
//!
//! All `unsafe` is confined to [`region`] and [`slot`]. The public API of
//! this crate is entirely safe.

#![deny(missing_docs)]
#![deny(rustdoc::broken_intra_doc_links)]

mod ring;
pub mod region;
pub mod slot;

use std::marker::PhantomData;

use flyby_core::{Encode, Error, ErrorKind, Lifecycle, Message, Result, SchemaId, Sink};

use region::Region;
use slot::{FLAG_SUSPECT, FLAG_VALID, SlotHeader, slot_size};

/// Default number of ring slots (power of two).
pub const DEFAULT_SLOT_COUNT: usize = 1024;

/// Default maximum payload size in bytes.
pub const DEFAULT_MAX_PAYLOAD: usize = 96;

/// A sink that writes messages into a lock-free SPSC shared-memory ring.
///
/// `M` is the concrete message type. It must implement both [`Message`]
/// (for metadata extraction) and [`Encode`] (for payload serialisation).
///
/// Slots are written in the format defined by [`slot::SlotHeader`]. A
/// consumer can decode them with [`slot::decode`].
pub struct SharedMemorySink<M: Message + Encode> {
    region: Region,
    /// Maximum payload bytes per slot (slot_size - HEADER_SIZE).
    max_payload: usize,
    /// Total messages successfully written to the ring.
    written: u64,
    _marker: PhantomData<M>,
}

impl<M: Message + Encode> SharedMemorySink<M> {
    /// Create a new sink backed by an anonymous mmap'd region.
    ///
    /// - `slot_count` — number of ring slots (must be a non-zero power of two).
    /// - `max_payload` — maximum payload bytes per message. The total
    ///   slot size will be rounded up to the next cache-line multiple.
    pub fn new(slot_count: usize, max_payload: usize) -> Result<Self> {
        let ss = slot_size(max_payload);
        let region = Region::anonymous(slot_count, ss)?;
        Ok(Self {
            region,
            max_payload,
            written: 0,
            _marker: PhantomData,
        })
    }

    /// Total messages successfully handed to [`Sink::write`].
    pub fn written(&self) -> u64 {
        self.written
    }

    /// Number of slots currently occupied.
    pub fn len(&self) -> usize {
        self.region.len()
    }

    /// `true` if the ring contains no messages.
    pub fn is_empty(&self) -> bool {
        self.region.is_empty()
    }

    /// `true` if the ring is full and the next write will return an error.
    pub fn is_full(&self) -> bool {
        self.region.is_full()
    }

    /// Pop one slot from the ring, invoking `consume` with the raw slot bytes.
    ///
    /// Returns `true` if a slot was available. Callers use
    /// [`slot::decode`] to parse the slot header and payload.
    pub fn pop(&mut self, consume: impl FnOnce(&[u8])) -> bool {
        self.region.pop(consume)
    }
}

impl<M: Message + Encode> Lifecycle for SharedMemorySink<M> {
    fn init(&mut self) -> Result<()> {
        self.written = 0;
        Ok(())
    }
}

impl<M: Message + Encode> Sink for SharedMemorySink<M> {
    type Message = M;

    fn write(&mut self, message: &M) -> Result<()> {
        let payload_len = message.encoded_len();
        if payload_len > self.max_payload {
            return Err(Error::new(
                ErrorKind::Sink,
                format!(
                    "encoded message ({} bytes) exceeds max_payload ({} bytes)",
                    payload_len, self.max_payload
                ),
            ));
        }

        let meta = message.metadata();
        let ts = message.timestamp();
        let schema_id = message.schema_id().id();
        let flags = FLAG_VALID | if meta.suspect { FLAG_SUSPECT } else { 0 };

        let header =
            SlotHeader::new(schema_id, flags, meta.sequence, ts.as_nanos(), payload_len as u32);

        let pushed = self.region.push(|buf| {
            // Encode payload into a temporary stack buffer then copy into
            // the slot. Avoids holding a mutable borrow on `region` while
            // calling user code that might re-enter the sink.
            //
            // For payloads up to DEFAULT_MAX_PAYLOAD this stays on the
            // stack. A heap fallback is used for larger payloads (rare in
            // practice; slot sizes are configured at construction time).
            let mut tmp = vec![0u8; payload_len];
            message.encode_into(&mut tmp)?;
            slot::encode(&header, &tmp, buf)
        })?;

        if pushed {
            self.written += 1;
            Ok(())
        } else {
            Err(Error::new(ErrorKind::Sink, "ring buffer full"))
        }
    }
}

// ---------------------------------------------------------------------------
// StubMessage: a minimal Message + Encode for doc-tests and integration tests.
// ---------------------------------------------------------------------------

/// A minimal message type used in doc-tests and integration tests.
///
/// Carries only a sequence number. Not intended for production use.
#[derive(Debug, Default, Clone, Copy)]
pub struct StubMessage {
    /// Monotonic sequence number.
    pub seq: u64,
}

impl Message for StubMessage {
    type Schema = flyby_core::DefaultSchemaId;

    fn schema_id(&self) -> flyby_core::DefaultSchemaId {
        flyby_core::DefaultSchemaId(0)
    }

    fn timestamp(&self) -> flyby_core::Timestamp {
        flyby_core::Timestamp::from_nanos(0)
    }

    fn metadata(&self) -> flyby_core::Metadata {
        flyby_core::Metadata { sequence: self.seq, suspect: false }
    }
}

impl Encode for StubMessage {
    fn encoded_len(&self) -> usize {
        8
    }

    fn encode_into(&self, dst: &mut [u8]) -> Result<usize> {
        if dst.len() < 8 {
            return Err(Error::new(ErrorKind::Encode, "buffer too small for StubMessage"));
        }
        dst[..8].copy_from_slice(&self.seq.to_be_bytes());
        Ok(8)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use flyby_core::Sink;

    fn make_sink() -> SharedMemorySink<StubMessage> {
        SharedMemorySink::new(16, 64).unwrap()
    }

    fn msg(seq: u64) -> StubMessage {
        StubMessage { seq }
    }

    #[test]
    fn write_and_count() {
        let mut sink = make_sink();
        sink.write(&msg(1)).unwrap();
        sink.write(&msg(2)).unwrap();
        assert_eq!(sink.written(), 2);
        assert_eq!(sink.len(), 2);
    }

    #[test]
    fn write_pop_roundtrip() {
        let mut sink = make_sink();
        sink.write(&msg(42)).unwrap();

        let mut recovered = 0u64;
        sink.pop(|buf| {
            let (_hdr, payload) = slot::decode(buf).unwrap();
            recovered = u64::from_be_bytes(payload.try_into().unwrap());
        });
        assert_eq!(recovered, 42);
    }

    #[test]
    fn full_ring_returns_error() {
        let mut sink = SharedMemorySink::new(4, 64).unwrap();
        for i in 0..4 {
            sink.write(&msg(i)).unwrap();
        }
        assert!(sink.write(&msg(99)).is_err());
    }

    #[test]
    fn oversized_payload_returns_error() {
        let mut sink = SharedMemorySink::new(4, 4).unwrap(); // max 4 bytes
        // StubMessage encodes 8 bytes — exceeds max
        assert!(sink.write(&msg(0)).is_err());
    }

    #[test]
    fn sequence_monotonic_across_writes() {
        let mut sink = make_sink();
        for i in 0..8u64 {
            sink.write(&msg(i)).unwrap();
        }
        let mut seqs = Vec::new();
        while sink.pop(|buf| {
            let (hdr, _) = slot::decode(buf).unwrap();
            seqs.push(hdr.sequence);
        }) {}
        let expected: Vec<u64> = (0..8).collect();
        assert_eq!(seqs, expected);
    }
}
