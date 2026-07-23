//! Part VII runtime integration tests.

use std::sync::{Arc, Mutex};

use flyby::api::{
    Decoder, DefaultSchemaId, Error, ErrorKind, Lifecycle, Message, Metadata, Pipeline, Result,
    Sink, SinkId, Timestamp,
};
use flyby::pipeline::{
    CallbackPlacement, FixedPlacement, IdentityPreProcessor, RawBatchSource, RoundRobinPlacement,
    SimplePipeline, schema_hash_placement,
};
use flyby::runtime::{
    AffinityRequest, BackpressureStrategy, CpuAffinityPolicy, Runtime, RuntimeConfig, RuntimePhase,
    Scheduler, SchedulerKind, SingleThreadScheduler, WorkerPoolScheduler, apply_affinity,
    run_pipeline,
};

#[derive(Debug, Clone, PartialEq, Eq)]
struct Tick {
    seq: u64,
}

impl Message for Tick {
    type Schema = DefaultSchemaId;
    fn schema_id(&self) -> Self::Schema {
        DefaultSchemaId(1)
    }
    fn timestamp(&self) -> Timestamp {
        Timestamp::from_nanos(self.seq)
    }
    fn metadata(&self) -> Metadata {
        Metadata {
            sequence: self.seq,
            ..Metadata::default()
        }
    }
}

struct TickDecoder;
impl Decoder for TickDecoder {
    type Output = Tick;
    fn decode(&mut self, bytes: &[u8]) -> Result<Option<Tick>> {
        if bytes.len() < 8 {
            return Ok(None);
        }
        let mut arr = [0u8; 8];
        arr.copy_from_slice(&bytes[..8]);
        Ok(Some(Tick {
            seq: u64::from_le_bytes(arr),
        }))
    }
}

struct VecSource {
    frames: Vec<Vec<u8>>,
    idx: usize,
    inited: bool,
}

impl VecSource {
    fn new(seqs: &[u64]) -> Self {
        Self {
            frames: seqs.iter().map(|s| s.to_le_bytes().to_vec()).collect(),
            idx: 0,
            inited: false,
        }
    }
}

impl Lifecycle for VecSource {
    fn init(&mut self) -> Result<()> {
        self.inited = true;
        Ok(())
    }
    fn shutdown(&mut self) -> Result<()> {
        self.inited = false;
        Ok(())
    }
}

impl RawBatchSource for VecSource {
    fn poll_frames(&mut self, out: &mut Vec<Vec<u8>>) -> Result<usize> {
        if self.idx >= self.frames.len() {
            return Ok(0);
        }
        let n = (self.frames.len() - self.idx).min(4);
        for _ in 0..n {
            out.push(self.frames[self.idx].clone());
            self.idx += 1;
        }
        Ok(n)
    }
    fn is_exhausted(&self) -> bool {
        self.idx >= self.frames.len()
    }
}

struct CountingSink {
    written: Arc<Mutex<Vec<u64>>>,
    fail_after: Option<usize>,
}

impl CountingSink {
    fn new(written: Arc<Mutex<Vec<u64>>>) -> Self {
        Self {
            written,
            fail_after: None,
        }
    }
    fn with_bp_after(written: Arc<Mutex<Vec<u64>>>, n: usize) -> Self {
        Self {
            written,
            fail_after: Some(n),
        }
    }
}

impl Lifecycle for CountingSink {}

impl Sink for CountingSink {
    type Message = Tick;
    fn write(&mut self, message: &Tick) -> Result<()> {
        let mut g = self.written.lock().unwrap();
        if let Some(limit) = self.fail_after {
            if g.len() >= limit {
                return Err(Error::new(ErrorKind::BackPressure, "full"));
            }
        }
        g.push(message.seq);
        Ok(())
    }
}

#[test]
fn runtime_lifecycle_phases() {
    let written = Arc::new(Mutex::new(Vec::new()));
    let mut pipe = SimplePipeline::new(
        VecSource::new(&[1, 2, 3]),
        TickDecoder,
        IdentityPreProcessor::default(),
        FixedPlacement::new(SinkId::new(1)).unwrap(),
    );
    pipe.register_sink(
        SinkId::new(1),
        Box::new(CountingSink::new(Arc::clone(&written))),
    )
    .unwrap();

    let mut rt = Runtime::build(pipe, RuntimeConfig::default()).unwrap();
    assert_eq!(rt.phase(), RuntimePhase::Built);
    rt.validate().unwrap();
    assert_eq!(rt.phase(), RuntimePhase::Validated);
    let stats = rt.run().unwrap();
    assert_eq!(rt.phase(), RuntimePhase::CleanedUp);
    assert!(stats.steps > 0);
    assert_eq!(written.lock().unwrap().len(), 3);
}

#[test]
fn drop_newest_backpressure() {
    let written = Arc::new(Mutex::new(Vec::new()));
    let cfg = RuntimeConfig::default()
        .with_backpressure(BackpressureStrategy::DropNewest)
        .with_batch_size(8);
    let mut pipe = SimplePipeline::new(
        VecSource::new(&[10, 20, 30]),
        TickDecoder,
        IdentityPreProcessor::default(),
        FixedPlacement::new(SinkId::new(1)).unwrap(),
    )
    .with_runtime(cfg.clone());
    pipe.register_sink(
        SinkId::new(1),
        Box::new(CountingSink::with_bp_after(Arc::clone(&written), 1)),
    )
    .unwrap();

    run_pipeline(&mut pipe, &cfg).unwrap();
    assert_eq!(written.lock().unwrap().len(), 1);
    assert!(pipe.messages_dropped() >= 2);
    assert!(pipe.backpressure_events() >= 2);
}

#[test]
fn round_robin_placement() {
    let a = Arc::new(Mutex::new(Vec::new()));
    let b = Arc::new(Mutex::new(Vec::new()));
    let mut pipe = SimplePipeline::new(
        VecSource::new(&[1, 2, 3, 4]),
        TickDecoder,
        IdentityPreProcessor::default(),
        RoundRobinPlacement::new(vec![SinkId::new(1), SinkId::new(2)]).unwrap(),
    );
    pipe.register_sink(SinkId::new(1), Box::new(CountingSink::new(Arc::clone(&a))))
        .unwrap();
    pipe.register_sink(SinkId::new(2), Box::new(CountingSink::new(Arc::clone(&b))))
        .unwrap();
    run_pipeline(&mut pipe, &RuntimeConfig::default()).unwrap();
    assert_eq!(a.lock().unwrap().len(), 2);
    assert_eq!(b.lock().unwrap().len(), 2);
}

#[test]
fn callback_and_schema_hash_placement() {
    let written = Arc::new(Mutex::new(Vec::new()));
    let place = CallbackPlacement::new(|m: &Tick| {
        if m.seq % 2 == 0 {
            Ok(SinkId::new(1))
        } else {
            Ok(SinkId::NONE)
        }
    });
    let mut pipe = SimplePipeline::new(
        VecSource::new(&[1, 2, 3, 4]),
        TickDecoder,
        IdentityPreProcessor::default(),
        place,
    );
    pipe.register_sink(
        SinkId::new(1),
        Box::new(CountingSink::new(Arc::clone(&written))),
    )
    .unwrap();
    run_pipeline(&mut pipe, &RuntimeConfig::default()).unwrap();
    assert_eq!(*written.lock().unwrap(), vec![2, 4]);

    let _ = schema_hash_placement::<Tick>(vec![SinkId::new(1)]).unwrap();
}

#[test]
fn single_thread_scheduler_runs() {
    let written = Arc::new(Mutex::new(Vec::new()));
    let mut pipe = SimplePipeline::new(
        VecSource::new(&[7, 8]),
        TickDecoder,
        IdentityPreProcessor::default(),
        FixedPlacement::new(SinkId::new(1)).unwrap(),
    );
    pipe.register_sink(
        SinkId::new(1),
        Box::new(CountingSink::new(Arc::clone(&written))),
    )
    .unwrap();
    pipe.init().unwrap();
    let mut sched = SingleThreadScheduler::new(RuntimeConfig::default());
    sched.run(&mut pipe).unwrap();
    pipe.shutdown().unwrap();
    assert_eq!(written.lock().unwrap().len(), 2);
}

#[test]
fn worker_pool_factory() {
    let written = Arc::new(Mutex::new(Vec::new()));
    let written2 = Arc::clone(&written);
    let cfg = RuntimeConfig::default()
        .with_scheduler(SchedulerKind::WorkerPool)
        .with_workers(2);
    let pool = WorkerPoolScheduler::new(cfg).unwrap();
    pool.run_factory(move |worker| {
        let seqs = if worker == 0 {
            vec![1u64, 2]
        } else {
            vec![3, 4]
        };
        let mut pipe = SimplePipeline::new(
            VecSource::new(&seqs),
            TickDecoder,
            IdentityPreProcessor::default(),
            FixedPlacement::new(SinkId::new(1)).unwrap(),
        );
        pipe.register_sink(
            SinkId::new(1),
            Box::new(CountingSink::new(Arc::clone(&written2))),
        )?;
        Ok(pipe)
    })
    .unwrap();
    assert_eq!(written.lock().unwrap().len(), 4);
}

#[test]
fn affinity_noop_and_strict() {
    let req = AffinityRequest::for_worker(CpuAffinityPolicy::None, 0);
    apply_affinity(&req, true).unwrap();
    let req = AffinityRequest::for_worker(CpuAffinityPolicy::PinWorkers, 0);
    assert!(apply_affinity(&req, true).is_err());
    apply_affinity(&req, false).unwrap();
}

#[test]
fn toml_scheduler_worker_pool() {
    let cfg = RuntimeConfig::from_toml_str(
        r#"
        [runtime]
        workers = 2
        batch_size = 64
        backpressure = "drop_newest"
        scheduler = "worker_pool"
        metrics = false
        "#,
    )
    .unwrap();
    assert_eq!(cfg.scheduler, SchedulerKind::WorkerPool);
    assert_eq!(cfg.backpressure, BackpressureStrategy::DropNewest);
}
