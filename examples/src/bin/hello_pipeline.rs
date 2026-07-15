//! Minimal FlyBy pipeline example.
//!
//! Validates the builder configuration and, when the `memory` feature is
//! on (default), runs a short demo path through the network simulator and
//! shared-memory sink.
//!
//! ```sh
//! cargo run -p flyby-examples --bin hello_pipeline
//! ```

use flyby::prelude::*;

#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
struct MarketTick {
    sequence: u64,
    price: u64,
    quantity: u32,
}

impl Message for MarketTick {
    type Schema = DefaultSchemaId;

    fn schema_id(&self) -> DefaultSchemaId {
        DefaultSchemaId(1)
    }

    fn timestamp(&self) -> Timestamp {
        Timestamp::from_nanos(0)
    }

    fn metadata(&self) -> Metadata {
        Metadata {
            sequence: self.sequence,
            suspect: false,
        }
    }
}

impl Encode for MarketTick {
    fn encoded_len(&self) -> usize {
        20
    }

    fn encode_into(&self, dst: &mut [u8]) -> Result<usize> {
        if dst.len() < 20 {
            return Err(Error::encode("buffer too small for MarketTick"));
        }
        dst[0..8].copy_from_slice(&self.sequence.to_be_bytes());
        dst[8..16].copy_from_slice(&self.price.to_be_bytes());
        dst[16..20].copy_from_slice(&self.quantity.to_be_bytes());
        Ok(20)
    }
}

/// Decodes a 20-byte big-endian tick: [seq: u64][price: u64][qty: u32].
///
/// Simulator frames are larger Ethernet/IP/UDP packets; this decoder returns
/// `Ok(None)` for those frames (filter), which is expected for the demo.
struct TickDecoder;

impl Decoder for TickDecoder {
    type Output = MarketTick;

    fn decode(&mut self, raw: &[u8]) -> Result<Option<MarketTick>> {
        if raw.len() < 20 {
            return Ok(None);
        }
        // Only accept exact 20-byte payloads (not full Ethernet frames).
        if raw.len() != 20 {
            return Ok(None);
        }
        let sequence = u64::from_be_bytes(raw[0..8].try_into().unwrap());
        let price = u64::from_be_bytes(raw[8..16].try_into().unwrap());
        let quantity = u32::from_be_bytes(raw[16..20].try_into().unwrap());
        Ok(Some(MarketTick {
            sequence,
            price,
            quantity,
        }))
    }
}

fn main() -> Result<()> {
    // Configuration skeleton: validates selectors without claiming a full
    // multi-stage pipeline.
    FlyBy::builder()
        .source()
        .decoder(TickDecoder)
        .memory()
        .placement()
        .run::<MarketTick>()?;

    println!("flyby hello_pipeline: builder configuration accepted");

    // Executable smoke path: simulator → decoder → shared-memory sink.
    // TickDecoder filters sim Ethernet frames (Ok(None)); the path still
    // exercises source init/poll and sink lifecycle.
    let written = FlyBy::builder()
        .source()
        .memory()
        .run_demo(TickDecoder, 4)?;
    println!("flyby hello_pipeline: demo completed (messages written via decoder: {written})");

    Ok(())
}
