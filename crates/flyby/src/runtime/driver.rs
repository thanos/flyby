//! Runtime driver: Build → Validate → Initialize → Run → Drain → Shutdown → Cleanup.

use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Instant;

use crate::api::{MetricsCollector, NullCollector, Pipeline, Result, StepOutcome};

use super::config::{RuntimeConfig, SchedulerKind};
use super::scheduler::{
    SingleThreadScheduler, WorkerPoolScheduler, run_single_thread, shutdown_flag,
};
use super::telemetry::{RuntimeEvent, RuntimeMetricKey, emit_lifecycle};

/// Pipeline lifecycle phase owned by the runtime (Part VII §2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RuntimePhase {
    /// Constructed, not yet validated.
    Built,
    /// Configuration validated.
    Validated,
    /// Pipeline `init` completed.
    Initialized,
    /// Sources started / run loop active.
    Running,
    /// Source exhausted; draining pending work.
    Draining,
    /// Shutdown in progress or completed.
    Shutdown,
    /// Post-shutdown cleanup done.
    CleanedUp,
}

impl RuntimePhase {
    /// Stable ordinal for gauges.
    pub fn as_u8(self) -> u8 {
        match self {
            Self::Built => 0,
            Self::Validated => 1,
            Self::Initialized => 2,
            Self::Running => 3,
            Self::Draining => 4,
            Self::Shutdown => 5,
            Self::CleanedUp => 6,
        }
    }
}

/// Aggregate counters for one runtime session.
#[derive(Debug, Clone, Default)]
pub struct RuntimeStats {
    /// Steps executed.
    pub steps: u64,
    /// Wall time of the run phase.
    pub elapsed: std::time::Duration,
    /// Final phase.
    pub phase: Option<RuntimePhase>,
}

/// Owns a pipeline and drives the Part VII lifecycle.
pub struct Runtime<P: Pipeline> {
    pipeline: P,
    config: RuntimeConfig,
    phase: RuntimePhase,
    metrics: Arc<dyn MetricsCollector>,
    shutdown: Arc<AtomicBool>,
    stats: RuntimeStats,
}

impl<P: Pipeline> Runtime<P> {
    /// Build a runtime around an already-constructed pipeline.
    pub fn build(pipeline: P, config: RuntimeConfig) -> Result<Self> {
        config.validate()?;
        let metrics: Arc<dyn MetricsCollector> = Arc::new(NullCollector);
        let rt = Self {
            pipeline,
            config,
            phase: RuntimePhase::Built,
            metrics,
            shutdown: shutdown_flag(),
            stats: RuntimeStats::default(),
        };
        emit_lifecycle(rt.metrics.as_ref(), rt.config.metrics, &RuntimeEvent::Built);
        Ok(rt)
    }

    /// Attach a metrics collector.
    pub fn with_metrics(mut self, metrics: impl MetricsCollector + 'static) -> Self {
        self.metrics = Arc::new(metrics);
        self
    }

    /// Borrow configuration.
    pub fn config(&self) -> &RuntimeConfig {
        &self.config
    }

    /// Current phase.
    pub fn phase(&self) -> RuntimePhase {
        self.phase
    }

    /// Session stats.
    pub fn stats(&self) -> &RuntimeStats {
        &self.stats
    }

    /// Shared shutdown flag.
    pub fn shutdown_handle(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.shutdown)
    }

    /// Request cooperative shutdown.
    pub fn request_shutdown(&self) {
        self.shutdown
            .store(true, std::sync::atomic::Ordering::SeqCst);
    }

    /// Mutably borrow the pipeline (e.g. register sinks before validate/init).
    pub fn pipeline_mut(&mut self) -> &mut P {
        &mut self.pipeline
    }

    /// Borrow the pipeline.
    pub fn pipeline(&self) -> &P {
        &self.pipeline
    }

    fn set_phase(&mut self, phase: RuntimePhase) {
        self.phase = phase;
        if self.config.metrics {
            self.metrics
                .record_gauge(&RuntimeMetricKey::Phase, phase.as_u8() as f64);
        }
    }

    /// Validate configuration (idempotent).
    pub fn validate(&mut self) -> Result<()> {
        self.config.validate()?;
        self.set_phase(RuntimePhase::Validated);
        emit_lifecycle(
            self.metrics.as_ref(),
            self.config.metrics,
            &RuntimeEvent::Validated,
        );
        Ok(())
    }

    /// Initialize the pipeline (sources + sinks).
    pub fn initialize(&mut self) -> Result<()> {
        if self.phase < RuntimePhase::Validated {
            self.validate()?;
        }
        self.pipeline.init()?;
        self.set_phase(RuntimePhase::Initialized);
        emit_lifecycle(
            self.metrics.as_ref(),
            self.config.metrics,
            &RuntimeEvent::Initialized,
        );
        Ok(())
    }

    /// Run until exhausted or shutdown, then drain/shutdown/cleanup.
    pub fn run(&mut self) -> Result<RuntimeStats> {
        if self.phase < RuntimePhase::Initialized {
            self.initialize()?;
        }
        self.set_phase(RuntimePhase::Running);
        emit_lifecycle(
            self.metrics.as_ref(),
            self.config.metrics,
            &RuntimeEvent::Started,
        );
        if self.config.metrics {
            self.metrics
                .record_gauge(&RuntimeMetricKey::Workers, self.config.workers as f64);
        }

        let start = Instant::now();
        match self.config.scheduler {
            SchedulerKind::Default | SchedulerKind::SingleThread | SchedulerKind::WorkerPool => {
                // Single pipeline instance: worker-pool multi-instance needs run_factory.
                let mut sched = SingleThreadScheduler::with_shutdown(
                    self.config.clone(),
                    Arc::clone(&self.shutdown),
                );
                // Manual loop so we can count steps / drain.
                loop {
                    if self.shutdown.load(std::sync::atomic::Ordering::Relaxed) {
                        break;
                    }
                    let step_start = Instant::now();
                    let outcome = self.pipeline.step_outcome()?;
                    self.stats.steps += 1;
                    if self.config.metrics {
                        self.metrics.record_counter(&RuntimeMetricKey::Steps, 1);
                        self.metrics.record_histogram(
                            &RuntimeMetricKey::StepDurationNs,
                            step_start.elapsed().as_nanos() as f64,
                        );
                    }
                    match outcome {
                        StepOutcome::Exhausted => {
                            self.set_phase(RuntimePhase::Draining);
                            emit_lifecycle(
                                self.metrics.as_ref(),
                                self.config.metrics,
                                &RuntimeEvent::Draining,
                            );
                            break;
                        }
                        StepOutcome::Idle => {
                            if let Some(d) = self.config.idle_sleep() {
                                std::thread::sleep(d);
                            }
                        }
                        StepOutcome::BackPressured => {
                            if self.config.metrics {
                                self.metrics
                                    .record_counter(&RuntimeMetricKey::BackpressureEvents, 1);
                            }
                            let y = self.config.backpressure_yield();
                            if y.is_zero() {
                                std::thread::yield_now();
                            } else {
                                std::thread::sleep(y);
                            }
                        }
                        StepOutcome::Progress => {}
                    }
                    let _ = &mut sched;
                }
            }
        }

        self.stats.elapsed = start.elapsed();
        self.shutdown_pipeline()?;
        self.cleanup()?;
        self.stats.phase = Some(self.phase);
        Ok(self.stats.clone())
    }

    /// Shutdown pipeline stages.
    pub fn shutdown_pipeline(&mut self) -> Result<()> {
        self.set_phase(RuntimePhase::Shutdown);
        emit_lifecycle(
            self.metrics.as_ref(),
            self.config.metrics,
            &RuntimeEvent::Shutdown,
        );
        self.pipeline.shutdown()?;
        Ok(())
    }

    /// Mark cleanup complete.
    pub fn cleanup(&mut self) -> Result<()> {
        self.set_phase(RuntimePhase::CleanedUp);
        emit_lifecycle(
            self.metrics.as_ref(),
            self.config.metrics,
            &RuntimeEvent::Cleanup,
        );
        Ok(())
    }
}

/// Run a pipeline with the given config using a worker-pool factory.
pub fn run_with_worker_factory<P, F>(config: RuntimeConfig, factory: F) -> Result<()>
where
    P: Pipeline + 'static,
    F: Fn(usize) -> Result<P> + Send + Sync + 'static,
{
    let pool = WorkerPoolScheduler::new(config)?;
    pool.run_factory(factory)
}

/// Convenience: init + single-thread run + shutdown without the full `Runtime` type.
pub fn run_pipeline<P: Pipeline>(pipeline: &mut P, config: &RuntimeConfig) -> Result<()> {
    pipeline.init()?;
    let flag = shutdown_flag();
    run_single_thread(pipeline, config, &flag)?;
    pipeline.shutdown()?;
    Ok(())
}
