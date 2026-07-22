//! Virtual consumers that drain a [`VirtualSharedMemory`] ring.
//!
//! Models the production shared-memory consumer side inside the simulator
//! without requiring IPC.  Consumers can be slowed (drain fewer slots per
//! tick) to exercise back-pressure and queue occupancy scenarios.

use crate::ring::VirtualSharedMemory;

/// A simulated consumer that pops slots from a virtual shared-memory ring.
#[derive(Debug)]
pub struct VirtualConsumer {
    name: &'static str,
    /// Maximum slots drained per [`drain`][Self::drain] call.
    pub max_per_drain: usize,
    /// Cumulative slots successfully read.
    reads: u64,
    scratch: Vec<u8>,
}

impl VirtualConsumer {
    /// Create a consumer that drains as fast as possible.
    pub fn new(name: &'static str) -> Self {
        Self {
            name,
            max_per_drain: usize::MAX,
            reads: 0,
            scratch: Vec::new(),
        }
    }

    /// Create a slow consumer that drains at most `max_per_drain` slots.
    pub fn slow(name: &'static str, max_per_drain: usize) -> Self {
        Self {
            name,
            max_per_drain: max_per_drain.max(1),
            reads: 0,
            scratch: Vec::new(),
        }
    }

    /// Consumer name.
    pub fn name(&self) -> &'static str {
        self.name
    }

    /// Cumulative reads since creation.
    pub fn reads(&self) -> u64 {
        self.reads
    }

    /// Drain up to `limit.min(max_per_drain)` slots from `ring`.
    ///
    /// Returns the number of slots consumed.
    pub fn drain(&mut self, ring: &mut VirtualSharedMemory, limit: usize) -> usize {
        let budget = limit.min(self.max_per_drain);
        let mut n = 0usize;
        while n < budget {
            if ring.try_pop(&mut self.scratch).is_none() {
                break;
            }
            self.reads += 1;
            n += 1;
        }
        n
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drains_all_available() {
        let mut ring = VirtualSharedMemory::new("r0", 8, 16);
        for i in 0..5u8 {
            ring.try_push(&[i]).unwrap();
        }
        let mut c = VirtualConsumer::new("c0");
        assert_eq!(c.drain(&mut ring, usize::MAX), 5);
        assert_eq!(c.reads(), 5);
        assert!(ring.is_empty());
    }

    #[test]
    fn slow_consumer_respects_budget() {
        let mut ring = VirtualSharedMemory::new("r0", 8, 16);
        for i in 0..5u8 {
            ring.try_push(&[i]).unwrap();
        }
        let mut c = VirtualConsumer::slow("c0", 2);
        assert_eq!(c.drain(&mut ring, usize::MAX), 2);
        assert_eq!(ring.len(), 3);
    }
}
