//! Integration tests: shared-memory sink end-to-end.

use flyby_core::{ErrorKind, Lifecycle, Sink};
use flyby_memory::{DEFAULT_MAX_PAYLOAD, DEFAULT_SLOT_COUNT, SharedMemorySink, StubMessage, slot};

fn make_sink() -> SharedMemorySink<StubMessage> {
    SharedMemorySink::new(DEFAULT_SLOT_COUNT, DEFAULT_MAX_PAYLOAD).expect("sink creation")
}

#[test]
fn write_single_message_increments_count() {
    let mut sink = make_sink();
    sink.init().unwrap();
    sink.write(&StubMessage { seq: 1 }).unwrap();
    assert_eq!(sink.written(), 1);
}

#[test]
fn write_and_read_back_payload() {
    let mut sink = make_sink();
    sink.init().unwrap();
    sink.write(&StubMessage { seq: 42 }).unwrap();

    let mut recovered = None;
    assert!(sink.pop(|buf| {
        let (hdr, payload) = slot::decode(buf).unwrap();
        assert_eq!(hdr.sequence, 42);
        recovered = Some(u64::from_be_bytes(payload.try_into().unwrap()));
    }));
    assert_eq!(recovered, Some(42));
}

#[test]
fn sequence_numbers_are_monotonically_increasing() {
    let mut sink = make_sink();
    sink.init().unwrap();
    for i in 0..5u64 {
        sink.write(&StubMessage { seq: i }).unwrap();
    }
    let mut seqs = Vec::new();
    while sink.pop(|buf| {
        let (hdr, _) = slot::decode(buf).unwrap();
        seqs.push(hdr.sequence);
    }) {}
    assert_eq!(seqs, vec![0, 1, 2, 3, 4]);
}

#[test]
fn full_ring_returns_back_pressure() {
    let mut sink = SharedMemorySink::<StubMessage>::new(4, DEFAULT_MAX_PAYLOAD).expect("sink");
    sink.init().unwrap();
    for i in 0..4u64 {
        sink.write(&StubMessage { seq: i }).unwrap();
    }
    let err = sink.write(&StubMessage { seq: 99 }).unwrap_err();
    assert_eq!(err.kind(), ErrorKind::BackPressure);
}

#[test]
fn oversized_payload_returns_error() {
    let mut sink = SharedMemorySink::<StubMessage>::new(16, 4).expect("sink");
    sink.init().unwrap();
    assert!(sink.write(&StubMessage { seq: 1 }).is_err());
}

#[test]
fn reinit_clears_ring_and_counter() {
    let mut sink = make_sink();
    sink.init().unwrap();
    sink.write(&StubMessage { seq: 1 }).unwrap();
    assert_eq!(sink.written(), 1);
    assert_eq!(sink.len(), 1);

    sink.shutdown().unwrap();
    sink.init().unwrap();
    assert_eq!(sink.written(), 0);
    assert!(sink.is_empty());
}
