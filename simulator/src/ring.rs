//! In-process virtual shared-memory ring for the simulator.
//!
//! This is a simplified SPSC byte-slot ring used to model the production
//! shared-memory sink without requiring mmap or message encode traits.
//! Production code should use `flyby-memory::SharedMemorySink`; the
//! simulator ring exists so scenarios can exercise producer/consumer
//! back-pressure and queue occupancy on any platform.

/// Error returned when a push cannot complete.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RingError {
    /// The ring has no free slots.
    Full,
    /// The payload exceeds the configured slot size.
    Oversized,
}

/// A fixed-capacity byte-slot ring.
#[derive(Debug)]
pub struct VirtualSharedMemory {
    name: &'static str,
    slots: Vec<Vec<u8>>,
    lens: Vec<usize>,
    capacity: usize,
    max_payload: usize,
    head: u64,
    tail: u64,
    /// Sequence number of the next successful push.
    next_seq: u64,
    /// Cumulative successful pushes.
    pub writes: u64,
    /// Cumulative failed pushes due to fullness.
    pub overflows: u64,
}

impl VirtualSharedMemory {
    /// Create a ring with `capacity` slots of `max_payload` bytes each.
    ///
    /// # Panics
    ///
    /// Panics if `capacity` or `max_payload` is zero.
    pub fn new(name: &'static str, capacity: usize, max_payload: usize) -> Self {
        assert!(capacity > 0, "ring capacity must be > 0");
        assert!(max_payload > 0, "ring max_payload must be > 0");
        Self {
            name,
            slots: (0..capacity).map(|_| vec![0u8; max_payload]).collect(),
            lens: vec![0; capacity],
            capacity,
            max_payload,
            head: 0,
            tail: 0,
            next_seq: 0,
            writes: 0,
            overflows: 0,
        }
    }

    /// Ring name (used in events).
    pub fn name(&self) -> &'static str {
        self.name
    }

    /// Slot capacity.
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Number of occupied slots.
    pub fn len(&self) -> usize {
        self.head.wrapping_sub(self.tail) as usize
    }

    /// `true` if no slots are occupied.
    pub fn is_empty(&self) -> bool {
        self.head == self.tail
    }

    /// Occupancy as a fraction of capacity in `[0.0, 1.0]`.
    pub fn occupancy(&self) -> f64 {
        self.len() as f64 / self.capacity as f64
    }

    /// Push a payload into the next free slot.
    ///
    /// Returns the sequence number on success.
    pub fn try_push(&mut self, data: &[u8]) -> Result<u64, RingError> {
        if data.len() > self.max_payload {
            return Err(RingError::Oversized);
        }
        if self.len() == self.capacity {
            self.overflows += 1;
            return Err(RingError::Full);
        }
        let idx = (self.head as usize) % self.capacity;
        self.slots[idx][..data.len()].copy_from_slice(data);
        self.lens[idx] = data.len();
        self.head = self.head.wrapping_add(1);
        let seq = self.next_seq;
        self.next_seq = self.next_seq.wrapping_add(1);
        self.writes += 1;
        Ok(seq)
    }

    /// Pop the oldest payload into `out`, returning its length.
    ///
    /// Returns `None` when the ring is empty.
    pub fn try_pop(&mut self, out: &mut Vec<u8>) -> Option<usize> {
        if self.is_empty() {
            return None;
        }
        let idx = (self.tail as usize) % self.capacity;
        let len = self.lens[idx];
        out.clear();
        out.extend_from_slice(&self.slots[idx][..len]);
        self.tail = self.tail.wrapping_add(1);
        Some(len)
    }

    /// Inspect slot contents without consuming (educational mode).
    ///
    /// `offset` is relative to the oldest occupied slot (`0` = next to pop).
    pub fn peek(&self, offset: usize) -> Option<&[u8]> {
        if offset >= self.len() {
            return None;
        }
        let idx = (self.tail.wrapping_add(offset as u64) as usize) % self.capacity;
        Some(&self.slots[idx][..self.lens[idx]])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_pop_roundtrip() {
        let mut ring = VirtualSharedMemory::new("r0", 4, 32);
        assert_eq!(ring.try_push(b"hello").unwrap(), 0);
        let mut out = Vec::new();
        assert_eq!(ring.try_pop(&mut out), Some(5));
        assert_eq!(&out, b"hello");
        assert!(ring.is_empty());
    }

    #[test]
    fn full_ring_returns_full() {
        let mut ring = VirtualSharedMemory::new("r0", 2, 8);
        assert!(ring.try_push(b"a").is_ok());
        assert!(ring.try_push(b"b").is_ok());
        assert_eq!(ring.try_push(b"c"), Err(RingError::Full));
        assert_eq!(ring.overflows, 1);
    }

    #[test]
    fn occupancy_tracks_fill() {
        let mut ring = VirtualSharedMemory::new("r0", 4, 8);
        assert_eq!(ring.occupancy(), 0.0);
        ring.try_push(b"x").unwrap();
        ring.try_push(b"y").unwrap();
        assert_eq!(ring.occupancy(), 0.5);
    }

    #[test]
    fn peek_does_not_consume() {
        let mut ring = VirtualSharedMemory::new("r0", 4, 8);
        ring.try_push(b"ab").unwrap();
        assert_eq!(ring.peek(0), Some(b"ab".as_ref()));
        assert_eq!(ring.len(), 1);
    }
}
