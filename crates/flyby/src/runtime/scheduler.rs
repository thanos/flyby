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
