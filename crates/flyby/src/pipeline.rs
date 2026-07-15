//! Minimal concrete [`Pipeline`] implementation.
//!
//! [`SimplePipeline`] owns a batch-oriented raw source, a decoder, optional
//! preprocessor, placement, and a map of sinks. It is the composition path
//! used by demos and tests until the fluent builder grows full type-state
//! wiring.

use std::collections::HashMap;

use crate::api::{
    Decoder, Error, ErrorKind, Lifecycle, Message, MetricsCollector, NullCollector, Pipeline,
    Placement, PreProcessor, Result, Sink, SinkId, StepOutcome,
};

/// Pulls framed raw bytes into a caller-supplied buffer.
///
/// Adapters wrap [`flyby_net::NetworkSource`] / [`flyby_storage::StorageSource`]
/// (or any custom source) behind this trait so the pipeline stays free of
/// backend crates.
pub trait RawBatchSource: Lifecycle {
    /// Fill `out` with the next frame(s). Returns the number of frames written
    /// as separate slices into `out` (appended). Zero means idle.
    fn poll_frames(&mut self, out: &mut Vec<Vec<u8>>) -> Result<usize>;

    /// `true` when a finite source will never produce more data.
    fn is_exhausted(&self) -> bool {
        false
    }
}

/// Identity preprocessor: passes every message through unchanged.
#[derive(Debug)]
pub struct IdentityPreProcessor<M>(std::marker::PhantomData<fn() -> M>);

impl<M> Default for IdentityPreProcessor<M> {
    fn default() -> Self {
        Self(std::marker::PhantomData)
    }
}

impl<M: Message> PreProcessor for IdentityPreProcessor<M> {
    type Message = M;

    fn process(&mut self, message: M) -> Result<Option<M>> {
        Ok(Some(message))
    }
}

/// Routes every message to a fixed [`SinkId`].
#[derive(Debug, Clone, Copy)]
pub struct FixedPlacement<M> {
    id: SinkId,
    _marker: std::marker::PhantomData<fn() -> M>,
}

impl<M: Message> FixedPlacement<M> {
    /// Create a placement that always returns `id` (must not be [`SinkId::NONE`]).
    pub fn new(id: SinkId) -> Result<Self> {
        if id.is_none() {
            return Err(Error::config(
                "FixedPlacement cannot use SinkId::NONE; use DropAllPlacement",
            ));
        }
        Ok(Self {
            id,
            _marker: std::marker::PhantomData,
        })
    }
}

impl<M: Message> Placement for FixedPlacement<M> {
    type Message = M;

    fn route(&mut self, _message: &M) -> Result<SinkId> {
        Ok(self.id)
    }
}

/// Drops every message (`SinkId::NONE`).
#[derive(Debug, Clone, Copy)]
pub struct DropAllPlacement<M>(std::marker::PhantomData<fn() -> M>);

impl<M> Default for DropAllPlacement<M> {
    fn default() -> Self {
        Self(std::marker::PhantomData)
    }
}

impl<M: Message> Placement for DropAllPlacement<M> {
    type Message = M;

    fn route(&mut self, _message: &M) -> Result<SinkId> {
        Ok(SinkId::NONE)
    }
}

/// A single-threaded pipeline: source → decode → preprocess → place → sink.
pub struct SimplePipeline<M, S, D, P, Pl>
where
    M: Message,
    S: RawBatchSource,
    D: Decoder<Output = M>,
    P: PreProcessor<Message = M>,
    Pl: Placement<Message = M>,
{
    source: S,
    decoder: D,
    preprocessor: P,
    placement: Pl,
    sinks: HashMap<u32, Box<dyn Sink<Message = M>>>,
    metrics: Box<dyn MetricsCollector>,
    /// Pending frames from the last source poll, awaiting decode.
    pending: Vec<Vec<u8>>,
    pending_idx: usize,
    initialized: bool,
    messages_out: u64,
}

impl<M, S, D, P, Pl> SimplePipeline<M, S, D, P, Pl>
where
    M: Message,
    S: RawBatchSource,
    D: Decoder<Output = M>,
    P: PreProcessor<Message = M>,
    Pl: Placement<Message = M>,
{
    /// Build a pipeline from its stages. Register sinks before [`Lifecycle::init`].
    pub fn new(source: S, decoder: D, preprocessor: P, placement: Pl) -> Self {
        Self {
            source,
            decoder,
            preprocessor,
            placement,
            sinks: HashMap::new(),
            metrics: Box::new(NullCollector),
            pending: Vec::new(),
            pending_idx: 0,
            initialized: false,
            messages_out: 0,
        }
    }

    /// Attach a metrics collector (replaces the default null collector).
    pub fn with_metrics(mut self, metrics: impl MetricsCollector + 'static) -> Self {
        self.metrics = Box::new(metrics);
        self
    }

    /// Messages successfully written to a sink since construction / last re-init.
    pub fn messages_out(&self) -> u64 {
        self.messages_out
    }

    /// Borrow the source.
    pub fn source(&self) -> &S {
        &self.source
    }

    /// Mutably borrow the source.
    pub fn source_mut(&mut self) -> &mut S {
        &mut self.source
    }

    fn ensure_init(&self) -> Result<()> {
        if !self.initialized {
            return Err(Error::lifecycle(
                "SimplePipeline: call init() before step()",
            ));
        }
        Ok(())
    }

    fn refill_pending(&mut self) -> Result<bool> {
        self.pending.clear();
        self.pending_idx = 0;
        let n = self.source.poll_frames(&mut self.pending)?;
        Ok(n > 0)
    }

    fn process_one_frame(&mut self, frame: &[u8]) -> Result<StepOutcome> {
        let Some(msg) = self.decoder.decode(frame)? else {
            return Ok(StepOutcome::Idle);
        };
        let Some(msg) = self.preprocessor.process(msg)? else {
            return Ok(StepOutcome::Idle);
        };
        let sink_id = self.placement.route(&msg)?;
        if sink_id.is_none() {
            return Ok(StepOutcome::Idle);
        }
        let sink = self.sinks.get_mut(&sink_id.as_u32()).ok_or_else(|| {
            Error::placement(format!("no sink registered for id {}", sink_id.as_u32()))
        })?;
        match sink.write(&msg) {
            Ok(()) => {
                self.messages_out += 1;
                Ok(StepOutcome::Progress)
            }
            Err(e) if e.kind() == ErrorKind::BackPressure => Ok(StepOutcome::BackPressured),
            Err(e) => Err(e),
        }
    }
}

impl<M, S, D, P, Pl> Lifecycle for SimplePipeline<M, S, D, P, Pl>
where
    M: Message,
    S: RawBatchSource,
    D: Decoder<Output = M>,
    P: PreProcessor<Message = M>,
    Pl: Placement<Message = M>,
{
    fn init(&mut self) -> Result<()> {
        if self.sinks.is_empty() {
            return Err(Error::config(
                "SimplePipeline: register at least one sink before init",
            ));
        }
        self.source.init()?;
        for sink in self.sinks.values_mut() {
            sink.init()?;
        }
        self.pending.clear();
        self.pending_idx = 0;
        self.messages_out = 0;
        self.initialized = true;
        Ok(())
    }

    fn shutdown(&mut self) -> Result<()> {
        let mut first_err = None;
        for sink in self.sinks.values_mut() {
            if let Err(e) = sink.flush().and_then(|_| sink.shutdown()) {
                if first_err.is_none() {
                    first_err = Some(e);
                }
            }
        }
        if let Err(e) = self.source.shutdown() {
            if first_err.is_none() {
                first_err = Some(e);
            }
        }
        self.initialized = false;
        self.pending.clear();
        first_err.map_or(Ok(()), Err)
    }

    fn run(&mut self) -> Result<()> {
        while !matches!(self.step_outcome()?, StepOutcome::Exhausted) {}
        Ok(())
    }
}

impl<M, S, D, P, Pl> Pipeline for SimplePipeline<M, S, D, P, Pl>
where
    M: Message,
    S: RawBatchSource,
    D: Decoder<Output = M>,
    P: PreProcessor<Message = M>,
    Pl: Placement<Message = M>,
{
    type Message = M;

    fn step(&mut self) -> Result<bool> {
        match self.step_outcome()? {
            StepOutcome::Progress => Ok(true),
            _ => Ok(false),
        }
    }

    fn step_outcome(&mut self) -> Result<StepOutcome> {
        self.ensure_init()?;

        // Drain pending frames first.
        while self.pending_idx < self.pending.len() {
            let frame = self.pending[self.pending_idx].clone();
            self.pending_idx += 1;
            let outcome = self.process_one_frame(&frame)?;
            match outcome {
                StepOutcome::Progress | StepOutcome::BackPressured => return Ok(outcome),
                StepOutcome::Idle => continue,
                StepOutcome::Exhausted => return Ok(StepOutcome::Exhausted),
            }
        }

        if self.source.is_exhausted() {
            return Ok(StepOutcome::Exhausted);
        }

        let had_data = self.refill_pending()?;
        if !had_data {
            if self.source.is_exhausted() {
                return Ok(StepOutcome::Exhausted);
            }
            return Ok(StepOutcome::Idle);
        }

        // Process first available frame from the new batch.
        while self.pending_idx < self.pending.len() {
            let frame = self.pending[self.pending_idx].clone();
            self.pending_idx += 1;
            let outcome = self.process_one_frame(&frame)?;
            match outcome {
                StepOutcome::Progress | StepOutcome::BackPressured => return Ok(outcome),
                StepOutcome::Idle => continue,
                StepOutcome::Exhausted => return Ok(StepOutcome::Exhausted),
            }
        }
        Ok(StepOutcome::Idle)
    }

    fn register_sink(
        &mut self,
        id: SinkId,
        sink: Box<dyn Sink<Message = Self::Message>>,
    ) -> Result<()> {
        if self.initialized {
            return Err(Error::lifecycle("cannot register_sink after init"));
        }
        if id.is_none() {
            return Err(Error::config("cannot register sink under SinkId::NONE"));
        }
        if self.sinks.contains_key(&id.as_u32()) {
            return Err(Error::config(format!(
                "sink id {} already registered",
                id.as_u32()
            )));
        }
        self.sinks.insert(id.as_u32(), sink);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Backend adapters
// ---------------------------------------------------------------------------

/// Adapts a [`flyby_net::NetworkSource`] into a [`RawBatchSource`].
pub struct NetworkBatchSource<N> {
    inner: N,
    batch: flyby_net::RawBatch,
}

impl<N: flyby_net::NetworkSource> NetworkBatchSource<N> {
    /// Create an adapter with the given batch capacity and max frame size.
    pub fn new(inner: N, capacity: usize, max_frame_size: usize) -> Self {
        Self {
            inner,
            batch: flyby_net::RawBatch::new(capacity, max_frame_size),
        }
    }

    /// Borrow the inner network source.
    pub fn inner(&self) -> &N {
        &self.inner
    }

    /// Mutably borrow the inner network source.
    pub fn inner_mut(&mut self) -> &mut N {
        &mut self.inner
    }
}

impl<N: flyby_net::NetworkSource> Lifecycle for NetworkBatchSource<N> {
    fn init(&mut self) -> Result<()> {
        self.inner.init()
    }

    fn shutdown(&mut self) -> Result<()> {
        self.inner.shutdown()
    }
}

impl<N: flyby_net::NetworkSource> RawBatchSource for NetworkBatchSource<N> {
    fn poll_frames(&mut self, out: &mut Vec<Vec<u8>>) -> Result<usize> {
        self.batch.reset(self.batch.max_frame_size());
        let n = self.inner.poll_batch(&mut self.batch)?;
        for (data, _) in self.batch.packets() {
            out.push(data.to_vec());
        }
        Ok(n)
    }
}

/// Adapts a [`flyby_storage::StorageSource`] into a [`RawBatchSource`].
pub struct StorageBatchSource<S> {
    inner: S,
    batch: flyby_storage::RawRecordBatch,
}

impl<S: flyby_storage::StorageSource> StorageBatchSource<S> {
    /// Create an adapter with the given batch capacity and max record size.
    pub fn new(inner: S, capacity: usize, max_record_size: usize) -> Self {
        Self {
            inner,
            batch: flyby_storage::RawRecordBatch::new(capacity, max_record_size),
        }
    }

    /// Create from a [`flyby_storage::FileConfig`]'s sizing fields.
    pub fn from_file_config(inner: S, cfg: &flyby_storage::FileConfig) -> Self {
        Self::new(inner, cfg.batch_size.max(1), cfg.max_record_size.max(1))
    }

    /// Borrow the inner storage source.
    pub fn inner(&self) -> &S {
        &self.inner
    }

    /// Mutably borrow the inner storage source.
    pub fn inner_mut(&mut self) -> &mut S {
        &mut self.inner
    }
}

impl<S: flyby_storage::StorageSource> Lifecycle for StorageBatchSource<S> {
    fn init(&mut self) -> Result<()> {
        self.inner.init()
    }

    fn shutdown(&mut self) -> Result<()> {
        self.inner.shutdown()
    }
}

impl<S: flyby_storage::StorageSource> RawBatchSource for StorageBatchSource<S> {
    fn poll_frames(&mut self, out: &mut Vec<Vec<u8>>) -> Result<usize> {
        self.batch.reset();
        let n = self.inner.poll_batch(&mut self.batch)?;
        for (data, _) in self.batch.records() {
            out.push(data.to_vec());
        }
        Ok(n)
    }

    fn is_exhausted(&self) -> bool {
        self.inner.is_exhausted()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::{DefaultSchemaId, Encode, Metadata, Timestamp};

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    struct Tick {
        seq: u64,
    }

    impl Message for Tick {
        type Schema = DefaultSchemaId;
        fn schema_id(&self) -> DefaultSchemaId {
            DefaultSchemaId(1)
        }
        fn timestamp(&self) -> Timestamp {
            Timestamp::from_nanos(0)
        }
        fn metadata(&self) -> Metadata {
            Metadata {
                sequence: self.seq,
                suspect: false,
            }
        }
    }

    impl Encode for Tick {
        fn encoded_len(&self) -> usize {
            8
        }
        fn encode_into(&self, dst: &mut [u8]) -> Result<usize> {
            dst[..8].copy_from_slice(&self.seq.to_be_bytes());
            Ok(8)
        }
    }

    struct TickDecoder;
    impl Decoder for TickDecoder {
        type Output = Tick;
        fn decode(&mut self, raw: &[u8]) -> Result<Option<Tick>> {
            if raw.len() < 8 {
                return Ok(None);
            }
            let seq = u64::from_be_bytes(raw[..8].try_into().unwrap());
            Ok(Some(Tick { seq }))
        }
    }

    struct VecSource {
        frames: Vec<Vec<u8>>,
        idx: usize,
        init: bool,
    }

    impl Lifecycle for VecSource {
        fn init(&mut self) -> Result<()> {
            self.init = true;
            self.idx = 0;
            Ok(())
        }
        fn shutdown(&mut self) -> Result<()> {
            self.init = false;
            Ok(())
        }
    }

    impl RawBatchSource for VecSource {
        fn poll_frames(&mut self, out: &mut Vec<Vec<u8>>) -> Result<usize> {
            if !self.init {
                return Err(Error::lifecycle("not init"));
            }
            if self.idx >= self.frames.len() {
                return Ok(0);
            }
            let f = self.frames[self.idx].clone();
            self.idx += 1;
            out.push(f);
            Ok(1)
        }
        fn is_exhausted(&self) -> bool {
            self.idx >= self.frames.len()
        }
    }

    struct CollectSink {
        out: Vec<Tick>,
    }

    impl Lifecycle for CollectSink {}
    impl Sink for CollectSink {
        type Message = Tick;
        fn write(&mut self, message: &Tick) -> Result<()> {
            self.out.push(*message);
            Ok(())
        }
    }

    #[test]
    fn end_to_end_vec_source() {
        let frames: Vec<Vec<u8>> = (0..5u64)
            .map(|s| {
                let mut b = vec![0u8; 8];
                Tick { seq: s }.encode_into(&mut b).unwrap();
                b
            })
            .collect();
        let src = VecSource {
            frames,
            idx: 0,
            init: false,
        };
        let sink_id = SinkId::new(1);
        let mut pipe = SimplePipeline::new(
            src,
            TickDecoder,
            IdentityPreProcessor::default(),
            FixedPlacement::new(sink_id).unwrap(),
        );
        let collector = CollectSink { out: Vec::new() };
        // We need to keep the sink accessible — register and run, then check
        // messages_out.
        pipe.register_sink(sink_id, Box::new(collector)).unwrap();
        pipe.init().unwrap();
        let mut progress = 0;
        for _ in 0..20 {
            match pipe.step_outcome().unwrap() {
                StepOutcome::Progress => progress += 1,
                StepOutcome::Exhausted => break,
                StepOutcome::Idle | StepOutcome::BackPressured => {}
            }
        }
        assert_eq!(progress, 5);
        assert_eq!(pipe.messages_out(), 5);
        pipe.shutdown().unwrap();
    }

    #[cfg(feature = "memory")]
    #[test]
    fn network_sim_to_memory() {
        use flyby_memory::{SharedMemorySink, StubMessage};
        use flyby_net::{SimNetConfig, SimulatedNetSource};

        // Decoder that accepts any frame ≥ 8 bytes as a StubMessage using
        // the sequence from the sim payload (offset after eth/ip/udp = 42).
        struct AnyFrameDecoder;
        impl Decoder for AnyFrameDecoder {
            type Output = StubMessage;
            fn decode(&mut self, raw: &[u8]) -> Result<Option<StubMessage>> {
                if raw.len() < 50 {
                    return Ok(None);
                }
                let seq = u64::from_be_bytes(raw[42..50].try_into().unwrap());
                Ok(Some(StubMessage { seq }))
            }
        }

        let src = SimulatedNetSource::new(SimNetConfig {
            batch_size: 4,
            payload_size: 16,
            ..SimNetConfig::default()
        });
        let adapted = NetworkBatchSource::new(src, 8, 2048);
        let sink_id = SinkId::new(1);
        let mut pipe = SimplePipeline::new(
            adapted,
            AnyFrameDecoder,
            IdentityPreProcessor::default(),
            FixedPlacement::new(sink_id).unwrap(),
        );
        let mem = SharedMemorySink::<StubMessage>::new(64, 64).unwrap();
        pipe.register_sink(sink_id, Box::new(mem)).unwrap();
        pipe.init().unwrap();
        let mut wrote = 0u64;
        for _ in 0..10 {
            if matches!(pipe.step_outcome().unwrap(), StepOutcome::Progress) {
                wrote += 1;
            }
        }
        assert!(wrote > 0, "expected some messages through the pipeline");
        pipe.shutdown().unwrap();
    }
}
