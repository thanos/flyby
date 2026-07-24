//! Replaceable pipeline schedulers (Part VII §4–§5).

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;

use crate::api::{Error, Pipeline, Result, StepOutcome};

use super::config::RuntimeConfig;
use super::telemetry::{RuntimeEvent, RuntimeMetricKey, emit_lifecycle};

/// Cooperative shutdown flag shared with worker threads.
pub fn shutdown_flag() -> Arc<AtomicBool> {
    Arc::new(AtomicBool::new(false))
}

/// Drives a [`Pipeline`] until exhaustion or cooperative shutdown.
pub trait Scheduler {
    /// Run `pipeline` according to this scheduler's policy.
    fn run<P: Pipeline>(&mut self, pipeline: &mut P) -> Result<()>;

    /// Request cooperative shutdown (idempotent).
    fn request_shutdown(&self);

    /// `true` if shutdown has been requested.
    fn is_shutdown_requested(&self) -> bool;
}

/// Single-thread cooperative loop on the calling thread.
#[derive(Debug, Clone)]
pub struct SingleThreadScheduler {
    config: RuntimeConfig,
    shutdown: Arc<AtomicBool>,
}

impl SingleThreadScheduler {
    /// Create a scheduler from runtime config.
    pub fn new(config: RuntimeConfig) -> Self {
        Self {
            config,
            shutdown: shutdown_flag(),
        }
    }

    /// Create with an externally owned shutdown flag.
    pub fn with_shutdown(config: RuntimeConfig, shutdown: Arc<AtomicBool>) -> Self {
        Self { config, shutdown }
    }

    /// Borrow the shutdown flag (for signals / tests).
    pub fn shutdown_handle(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.shutdown)
    }
}

impl Scheduler for SingleThreadScheduler {
    fn run<P: Pipeline>(&mut self, pipeline: &mut P) -> Result<()> {
        run_single_thread(pipeline, &self.config, &self.shutdown)
    }

    fn request_shutdown(&self) {
        self.shutdown.store(true, Ordering::SeqCst);
    }

    fn is_shutdown_requested(&self) -> bool {
        self.shutdown.load(Ordering::SeqCst)
    }
}

/// Drive one pipeline on the current thread until exhausted or shutdown.
pub fn run_single_thread<P: Pipeline>(
    pipeline: &mut P,
    config: &RuntimeConfig,
    shutdown: &AtomicBool,
) -> Result<()> {
    let metrics_on = config.metrics;
    // Null collector path: Pipeline may have its own metrics; we only sleep/idle here.
    let _ = metrics_on;

    loop {
        if shutdown.load(Ordering::Relaxed) {
            break;
        }
        match pipeline.step_outcome()? {
            StepOutcome::Exhausted => break,
            StepOutcome::Idle => {
                if let Some(d) = config.idle_sleep() {
                    thread::sleep(d);
                }
            }
            StepOutcome::BackPressured => {
                let yield_d = config.backpressure_yield();
                if !yield_d.is_zero() {
                    thread::sleep(yield_d);
                } else {
                    thread::yield_now();
                }
            }
            StepOutcome::Progress => {}
        }
    }
    Ok(())
}

/// Worker-pool scheduler: one pipeline instance per worker via a factory.
///
/// Does **not** expose [`std::thread::JoinHandle`] in the public API — callers
/// use [`WorkerPoolScheduler::run_factory`] which joins internally.
#[derive(Debug, Clone)]
pub struct WorkerPoolScheduler {
    config: RuntimeConfig,
    shutdown: Arc<AtomicBool>,
}

impl WorkerPoolScheduler {
    /// Create from runtime config (`workers` must be ≥ 1).
    pub fn new(config: RuntimeConfig) -> Result<Self> {
        config.validate()?;
        Ok(Self {
            config,
            shutdown: shutdown_flag(),
        })
    }

    /// Shared shutdown flag.
    pub fn shutdown_handle(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.shutdown)
    }

    /// Request cooperative shutdown.
    pub fn request_shutdown(&self) {
        self.shutdown.store(true, Ordering::SeqCst);
    }

    /// Spawn `config.workers` threads, each owning a pipeline from `factory`.
    ///
    /// Each worker calls [`Lifecycle::init`][crate::api::Lifecycle::init],
    /// runs until exhausted/shutdown, then [`shutdown`][crate::api::Lifecycle::shutdown].
    pub fn run_factory<P, F>(&self, factory: F) -> Result<()>
    where
        P: Pipeline + 'static,
        F: Fn(usize) -> Result<P> + Send + Sync + 'static,
    {
        let workers = self.config.workers.max(1);
        let factory = Arc::new(factory);
        let mut handles = Vec::with_capacity(workers);
        let errors: Arc<std::sync::Mutex<Vec<String>>> =
            Arc::new(std::sync::Mutex::new(Vec::new()));

        for i in 0..workers {
            let cfg = self.config.clone();
            let shutdown = Arc::clone(&self.shutdown);
            let factory = Arc::clone(&factory);
            let errors = Arc::clone(&errors);
            handles.push(
                thread::Builder::new()
                    .name(format!("flyby-worker-{i}"))
                    .spawn(move || {
                        let run = (|| -> Result<()> {
                            let mut pipeline = factory(i)?;
                            pipeline.init()?;
                            run_single_thread(&mut pipeline, &cfg, &shutdown)?;
                            pipeline.shutdown()?;
                            Ok(())
                        })();
                        if let Err(e) = run {
                            if let Ok(mut g) = errors.lock() {
                                g.push(format!("worker {i}: {e}"));
                            }
                            shutdown.store(true, Ordering::SeqCst);
                        }
                    })
                    .map_err(|e| Error::lifecycle(format!("failed to spawn worker {i}: {e}")))?,
            );
        }

        for h in handles {
            let _ = h.join();
        }

        let errs = errors
            .lock()
            .map_err(|_| Error::lifecycle("error lock poisoned"))?;
        if let Some(first) = errs.first() {
            return Err(Error::lifecycle(first.clone()));
        }
        Ok(())
    }
}

/// Build a single-thread scheduler from config (worker pools use
/// [`WorkerPoolScheduler::run_factory`] directly).
pub fn build_scheduler(config: RuntimeConfig) -> SingleThreadScheduler {
    SingleThreadScheduler::new(config)
}

/// Convenience: record a step metric sample.
#[allow(dead_code)] // used by future dashboard / OTel bridges
pub fn record_step(
    metrics: &dyn crate::api::MetricsCollector,
    enabled: bool,
    steps: u64,
    duration_ns: u64,
) {
    if !enabled {
        return;
    }
    metrics.record_counter(&RuntimeMetricKey::Steps, 1);
    metrics.record_histogram(&RuntimeMetricKey::StepDurationNs, duration_ns as f64);
    emit_lifecycle(metrics, true, &RuntimeEvent::Step { n: steps });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::{
        CountingCollector, DefaultSchemaId, Error, Lifecycle, Message, Metadata, Pipeline, Result,
        Sink, SinkId, StepOutcome, Timestamp,
    };
    use std::sync::Arc;

    #[derive(Clone)]
    struct Tick(u64);
    impl Message for Tick {
        type Schema = DefaultSchemaId;
        fn schema_id(&self) -> Self::Schema {
            DefaultSchemaId(1)
        }
        fn timestamp(&self) -> Timestamp {
            Timestamp::from_nanos(self.0)
        }
        fn metadata(&self) -> Metadata {
            Metadata::default()
        }
    }

    struct ScriptedPipe {
        outcomes: Vec<StepOutcome>,
        idx: usize,
    }

    impl Lifecycle for ScriptedPipe {}
    impl Pipeline for ScriptedPipe {
        type Message = Tick;
        fn step(&mut self) -> Result<bool> {
            Ok(matches!(self.step_outcome()?, StepOutcome::Progress))
        }
        fn step_outcome(&mut self) -> Result<StepOutcome> {
            if self.idx >= self.outcomes.len() {
                return Ok(StepOutcome::Exhausted);
            }
            let o = self.outcomes[self.idx];
            self.idx += 1;
            Ok(o)
        }
        fn register_sink(
            &mut self,
            _id: SinkId,
            _sink: Box<dyn Sink<Message = Self::Message>>,
        ) -> Result<()> {
            Ok(())
        }
    }

    #[test]
    fn run_single_thread_handles_idle_bp_and_shutdown() {
        let flag = shutdown_flag();
        let mut pipe = ScriptedPipe {
            outcomes: vec![
                StepOutcome::Idle,
                StepOutcome::BackPressured,
                StepOutcome::Progress,
                StepOutcome::Exhausted,
            ],
            idx: 0,
        };
        let cfg = RuntimeConfig {
            idle_sleep_ms: Some(0),
            backpressure_yield_ms: 0,
            ..RuntimeConfig::default()
        };
        run_single_thread(&mut pipe, &cfg, &flag).unwrap();

        let mut pipe = ScriptedPipe {
            outcomes: vec![StepOutcome::BackPressured; 3],
            idx: 0,
        };
        let cfg = RuntimeConfig {
            backpressure_yield_ms: 1,
            idle_sleep_ms: Some(1),
            ..RuntimeConfig::default()
        };
        // Interrupt after starting: seed one idle then shutdown mid-loop via flag.
        let flag = shutdown_flag();
        flag.store(true, Ordering::SeqCst);
        run_single_thread(&mut pipe, &cfg, &flag).unwrap();
    }

    #[test]
    fn single_thread_scheduler_trait_and_handles() {
        let shared = shutdown_flag();
        let mut sched =
            SingleThreadScheduler::with_shutdown(RuntimeConfig::default(), Arc::clone(&shared));
        assert!(!sched.is_shutdown_requested());
        assert!(!sched.shutdown_handle().load(Ordering::SeqCst));
        let mut pipe = ScriptedPipe {
            outcomes: vec![StepOutcome::Progress, StepOutcome::Exhausted],
            idx: 0,
        };
        sched.run(&mut pipe).unwrap();
        sched.request_shutdown();
        assert!(sched.is_shutdown_requested());
        assert!(shared.load(Ordering::SeqCst));
    }

    #[test]
    fn worker_pool_propagates_factory_error() {
        let cfg = RuntimeConfig::default().with_workers(1);
        let pool = WorkerPoolScheduler::new(cfg).unwrap();
        let err = pool
            .run_factory(|_| -> Result<ScriptedPipe> { Err(Error::lifecycle("factory boom")) })
            .unwrap_err();
        assert!(err.to_string().contains("factory boom") || err.to_string().contains("worker"));
        pool.request_shutdown();
        assert!(pool.shutdown_handle().load(Ordering::SeqCst));
    }

    #[test]
    fn record_step_respects_enabled_flag() {
        let metrics = CountingCollector::new();
        record_step(&metrics, false, 1, 10);
        assert_eq!(metrics.calls.load(std::sync::atomic::Ordering::Relaxed), 0);
        record_step(&metrics, true, 2, 20);
        assert!(metrics.calls.load(std::sync::atomic::Ordering::Relaxed) >= 2);
    }
}
