//! Minimal FlyBy pipeline example.
//!
//! Demonstrates the target builder style from the specification:
//!
//! ```text
//! FlyBy::builder()
//!     .source(...)
//!     .decoder(MyDecoder::new())
//!     .placement()
//!     .memory()
//!     .run::<MarketTick>()?;
//! ```
//!
//! Run with:
//!
//! ```sh
//! cargo run -p flyby-examples --bin hello_pipeline
//! ```

use flyby::prelude::*;

// ---------------------------------------------------------------------------
// Supplier-defined message type — the framework never inspects these fields.
// ---------------------------------------------------------------------------

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
        20 // seq(8) + price(8) + qty(4)
    }

    fn encode_into(&self, dst: &mut [u8]) -> Result<usize> {
        dst[0..8].copy_from_slice(&self.sequence.to_be_bytes());
        dst[8..16].copy_from_slice(&self.price.to_be_bytes());
        dst[16..20].copy_from_slice(&self.quantity.to_be_bytes());
        Ok(20)
    }
}

// ---------------------------------------------------------------------------
// Supplier-defined decoder — knows the wire format, nothing else does.
// ---------------------------------------------------------------------------

/// Decodes a 20-byte big-endian tick: [seq: u64][price: u64][qty: u32].
struct TickDecoder;

impl Decoder for TickDecoder {
    type Output = MarketTick;

    fn decode(&mut self, raw: &[u8]) -> Result<Option<MarketTick>> {
        if raw.len() < 20 {
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

// ---------------------------------------------------------------------------

fn main() -> Result<()> {
    FlyBy::builder()
        .source()
        .decoder(TickDecoder)
        .memory()
        .placement()
        .run::<MarketTick>()?;

    println!("flyby hello_pipeline: builder accepted the configuration");
    Ok(())
}
