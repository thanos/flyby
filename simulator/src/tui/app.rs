//! Dashboard application state and event loop.

use std::io::{self, stdout};
use std::time::{Duration, Instant};

use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

use crate::dsl::CompiledRun;
use crate::events::{SimEvent, SimEventKind, VecEventSink};
use crate::fault::FaultSpec;
use crate::scenario::Scenario;
use crate::scheduler::{EduControls, SimScheduler, SimStats};
use crate::traffic::TrafficConfig;
use crate::{VirtualConsumer, VirtualNic, VirtualNicConfig, VirtualSharedMemory};

use super::ui;

/// How the dashboard drives the scheduler.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunMode {
    /// Auto-advance ticks while not paused.
    Auto,
    /// Wait for explicit step key.
    Paused,
}

/// Snapshot rendered each frame.
#[derive(Debug, Clone)]
pub struct DashState {
    /// Scenario name.
    pub scenario_name: String,
    /// Scenario description.
    pub scenario_desc: String,
    /// Simulator clock (ns).
    pub clock_ns: u64,
    /// Scenario duration (ns).
    pub duration_ns: u64,
    /// Aggregate stats.
    pub stats: SimStats,
    /// Ring `(len, capacity, occupancy)` when present.
    pub ring: Option<(usize, usize, f64)>,
    /// Consumer read counter.
    pub consumer_reads: u64,
    /// Auto vs paused.
    pub mode: RunMode,
    /// Ticks executed per UI frame while auto-running.
    pub ticks_per_frame: u32,
    /// Recent event lines (newest last).
    pub event_log: Vec<String>,
    /// Sparkline of packets generated per tick (last N).
    pub pps_hist: Vec<u64>,
    /// Sparkline of tick wall duration (ns).
    pub tick_lat_hist: Vec<u64>,
    /// Last batch packet count.
    pub last_batch_len: usize,
    /// Status line message.
    pub status: String,
    /// Run finished.
    pub finished: bool,
}

impl Default for DashState {
    fn default() -> Self {
        Self {
            scenario_name: String::new(),
            scenario_desc: String::new(),
            clock_ns: 0,
            duration_ns: 0,
            stats: SimStats::default(),
            ring: None,
            consumer_reads: 0,
            mode: RunMode::Paused,
            ticks_per_frame: 1,
            event_log: Vec::new(),
            pps_hist: Vec::new(),
            tick_lat_hist: Vec::new(),
            last_batch_len: 0,
            status: "Space=run  s=step  +/-=speed  r=restart  q=quit".into(),
            finished: false,
        }
    }
}

pub(super) struct Dashboard {
    source: DashSource,
    sink: VecEventSink,
    sched: SimScheduler<VecEventSink>,
    mode: RunMode,
    ticks_per_frame: u32,
    event_cursor: usize,
    event_log: Vec<String>,
    pps_hist: Vec<u64>,
    tick_lat_hist: Vec<u64>,
    prev_packets: u64,
    status: String,
    finished: bool,
}

#[derive(Clone)]
enum DashSource {
    Builtin(Scenario),
    Compiled(Box<CompiledRun>),
}

impl DashSource {
    fn scenario(&self) -> &Scenario {
        match self {
            Self::Builtin(s) => s,
            Self::Compiled(c) => &c.scenario,
        }
    }

    fn build(&self, sink: VecEventSink) -> flyby_core::Result<SimScheduler<VecEventSink>> {
        match self {
            Self::Builtin(s) => build_scheduler_builtin(s.clone(), sink),
            Self::Compiled(c) => {
                let mut sched = c.build_scheduler_with_events(sink)?;
                // TUI always starts paused.
                sched.edu_mut().paused = true;
                Ok(sched)
            }
        }
    }
}

const HIST_CAP: usize = 64;
const LOG_CAP: usize = 80;

impl Dashboard {
    pub(super) fn new(scenario: Scenario) -> flyby_core::Result<Self> {
        Self::from_source(DashSource::Builtin(scenario))
    }

    pub(super) fn new_compiled(compiled: CompiledRun) -> flyby_core::Result<Self> {
        Self::from_source(DashSource::Compiled(Box::new(compiled)))
    }

    fn from_source(source: DashSource) -> flyby_core::Result<Self> {
        let sink = VecEventSink::new();
        let sched = source.build(sink.clone())?;
        let mut dash = Self {
            source,
            sink,
            sched,
            mode: RunMode::Paused,
            ticks_per_frame: 1,
            event_cursor: 0,
            event_log: Vec::new(),
            pps_hist: Vec::new(),
            tick_lat_hist: Vec::new(),
            prev_packets: 0,
            status: "Paused — press Space to auto-run, s to step".into(),
            finished: false,
        };
        // Arm the run without ticking.
        dash.sched.edu_mut().paused = true;
        dash.sched.run()?;
        dash.refresh_events();
        Ok(dash)
    }

    fn restart(&mut self) -> flyby_core::Result<()> {
        if self.sched.is_running() {
            let _ = self.sched.finish_run();
        }
        self.sink.clear();
        self.event_cursor = 0;
        self.event_log.clear();
        self.pps_hist.clear();
        self.tick_lat_hist.clear();
        self.prev_packets = 0;
        self.finished = false;
        self.mode = RunMode::Paused;
        self.sched = self.source.build(self.sink.clone())?;
        self.sched.edu_mut().paused = true;
        self.sched.run()?;
        self.status = "Restarted — paused".into();
        self.refresh_events();
        Ok(())
    }

    pub(super) fn step_once(&mut self) -> flyby_core::Result<()> {
        if self.finished {
            self.status = "Finished — press r to restart".into();
            return Ok(());
        }
        // Clear pause so step can run past breakpoint guards that set paused.
        self.sched.resume();
        let more = self.sched.step()?;
        self.ingest_tick_samples();
        self.refresh_events();
        if !more {
            if self.sched.is_paused() && self.sched.is_running() {
                self.status = "Breakpoint — paused".into();
                self.mode = RunMode::Paused;
            } else {
                let _ = self.sched.finish_run();
                self.finished = true;
                self.mode = RunMode::Paused;
                self.status = "Scenario complete (simulated) — r to restart, q to quit".into();
            }
        }
        Ok(())
    }

    fn auto_ticks(&mut self) -> flyby_core::Result<()> {
        if self.mode != RunMode::Auto || self.finished {
            return Ok(());
        }
        for _ in 0..self.ticks_per_frame {
            self.step_once()?;
            if self.finished || self.mode == RunMode::Paused {
                break;
            }
        }
        Ok(())
    }

    fn ingest_tick_samples(&mut self) {
        let packets = self.sched.stats().packets_generated;
        let delta = packets.saturating_sub(self.prev_packets);
        self.prev_packets = packets;
        push_hist(&mut self.pps_hist, delta, HIST_CAP);

        // Pull latest TickCompleted latency from new events.
        let new_events = self.sink.events_from(self.event_cursor);
        for ev in &new_events {
            if let SimEventKind::TickCompleted { duration_ns, .. } = ev.kind {
                push_hist(&mut self.tick_lat_hist, duration_ns, HIST_CAP);
            }
        }
    }

    fn refresh_events(&mut self) {
        let new_events = self.sink.events_from(self.event_cursor);
        self.event_cursor = self.sink.len();
        let quiet = self.mode == RunMode::Auto && self.ticks_per_frame > 1;
        for ev in new_events {
            if quiet
                && matches!(
                    ev.kind,
                    SimEventKind::PacketGenerated { .. }
                        | SimEventKind::SlotWritten { .. }
                        | SimEventKind::ConsumerRead { .. }
                        | SimEventKind::TickCompleted { .. }
                )
            {
                continue;
            }
            // Even at ×1 auto, skip per-packet noise — keep faults + lifecycle.
            if matches!(
                ev.kind,
                SimEventKind::PacketGenerated { .. }
                    | SimEventKind::SlotWritten { .. }
                    | SimEventKind::ConsumerRead { .. }
            ) && self.mode == RunMode::Auto
            {
                continue;
            }
            self.event_log.push(format_event(&ev));
        }
        if self.event_log.len() > LOG_CAP {
            let drop_n = self.event_log.len() - LOG_CAP;
            self.event_log.drain(0..drop_n);
        }
        self.sink.truncate_to(2_000);
        self.event_cursor = self.sink.len();
    }

    pub(super) fn snapshot(&self) -> DashState {
        DashState {
            scenario_name: self.source.scenario().name.clone(),
            scenario_desc: self.source.scenario().description.clone(),
            clock_ns: self.sched.clock_ns().unwrap_or(0),
            duration_ns: self.sched.duration_ns(),
            stats: self.sched.stats().clone(),
            ring: self.sched.ring_occupancy(),
            consumer_reads: self.sched.consumer_reads(),
            mode: self.mode,
            ticks_per_frame: self.ticks_per_frame,
            event_log: self.event_log.clone(),
            pps_hist: self.pps_hist.clone(),
            tick_lat_hist: self.tick_lat_hist.clone(),
            last_batch_len: self.sched.last_batch().map(|b| b.len()).unwrap_or(0),
            status: self.status.clone(),
            finished: self.finished,
        }
    }

    fn handle_key(&mut self, key: KeyCode, modifiers: KeyModifiers) -> flyby_core::Result<bool> {
        // true => quit
        match key {
            KeyCode::Char('q') | KeyCode::Esc => return Ok(true),
            KeyCode::Char('c') if modifiers.contains(KeyModifiers::CONTROL) => return Ok(true),
            KeyCode::Char(' ') => {
                if self.finished {
                    self.status = "Finished — press r to restart".into();
                } else if self.mode == RunMode::Auto {
                    self.mode = RunMode::Paused;
                    self.sched.pause();
                    self.status = "Paused".into();
                } else {
                    self.mode = RunMode::Auto;
                    self.sched.resume();
                    self.status = format!("Auto-run ×{}", self.ticks_per_frame);
                }
            }
            KeyCode::Char('s') | KeyCode::Right => {
                self.mode = RunMode::Paused;
                self.step_once()?;
                if !self.finished {
                    self.status = format!("Stepped — tick {}", self.sched.stats().ticks);
                }
            }
            KeyCode::Char('+') | KeyCode::Char('=') => {
                self.ticks_per_frame = (self.ticks_per_frame.saturating_mul(2)).min(1024);
                self.status = format!("Speed ×{} ticks/frame", self.ticks_per_frame);
            }
            KeyCode::Char('-') | KeyCode::Char('_') => {
                self.ticks_per_frame = (self.ticks_per_frame / 2).max(1);
                self.status = format!("Speed ×{} ticks/frame", self.ticks_per_frame);
            }
            KeyCode::Char('r') => {
                self.restart()?;
            }
            _ => {}
        }
        Ok(false)
    }
}

fn push_hist(buf: &mut Vec<u64>, value: u64, cap: usize) {
    buf.push(value);
    if buf.len() > cap {
        let drop_n = buf.len() - cap;
        buf.drain(0..drop_n);
    }
}

fn format_event(ev: &SimEvent) -> String {
    let t = format!("{:.3}ms", ev.clock_ns as f64 / 1_000_000.0);
    match &ev.kind {
        SimEventKind::PacketGenerated { nic, len, seq } => {
            format!("{t} gen {nic} seq={seq} len={len}")
        }
        SimEventKind::PacketDropped { nic, seq } => format!("{t} DROP {nic} seq={seq}"),
        SimEventKind::PacketCorrupted { nic, seq } => format!("{t} CORRUPT {nic} seq={seq}"),
        SimEventKind::SlotWritten { ring, seq } => format!("{t} write {ring} seq={seq}"),
        SimEventKind::ConsumerRead { ring, seq } => format!("{t} read {ring} seq={seq}"),
        SimEventKind::QueueOverflow { ring } => format!("{t} OVERFLOW {ring}"),
        SimEventKind::TickCompleted { tick, duration_ns } => {
            format!("{t} tick={tick} wall={duration_ns}ns")
        }
        SimEventKind::SimulatorStarted { scenario } => format!("{t} start {scenario}"),
        SimEventKind::SimulatorStopped { ticks, .. } => format!("{t} stop ticks={ticks}"),
        other => format!("{t} {other:?}"),
    }
}

fn build_scheduler_builtin(
    scenario: Scenario,
    sink: VecEventSink,
) -> flyby_core::Result<SimScheduler<VecEventSink>> {
    let nic = VirtualNic::new(
        VirtualNicConfig {
            name: "nic0",
            traffic: TrafficConfig {
                pattern: scenario.traffic.pattern.clone(),
                payload_size: scenario.traffic.payload_size,
                batch_size: scenario.traffic.batch_size,
                payload: scenario.traffic.payload.clone(),
            },
            fault: FaultSpec {
                drop_rate: scenario.fault.drop_rate,
                corrupt_rate: scenario.fault.corrupt_rate,
                latency_spike_rate: scenario.fault.latency_spike_rate,
                latency_spike_ns: scenario.fault.latency_spike_ns,
            },
            fault_seed: 0,
            udp_dst_port: 9000,
        },
        sink.clone(),
    );

    let is_slow = scenario.name == "slow_consumer";
    let is_overflow = scenario.name == "queue_overflow";
    let ring_slots = if is_overflow { 16 } else { 4096 };
    let frame = 42 + scenario.traffic.payload_size.max(1);

    let mut sched = SimScheduler::with_events(scenario, sink)
        .with_ring(VirtualSharedMemory::new("ring0", ring_slots, frame))
        .with_edu(EduControls {
            paused: true,
            ..EduControls::default()
        });
    sched.add_nic(nic);
    if is_slow {
        sched.add_consumer(VirtualConsumer::slow("c0", 1));
    } else {
        sched.add_consumer(VirtualConsumer::new("c0"));
    }
    Ok(sched)
}

/// Run the Ratatui dashboard for `scenario` until the user quits.
pub fn run_dashboard(scenario: Scenario) -> flyby_core::Result<()> {
    run_dashboard_inner(Dashboard::new(scenario)?)
}

/// Run the Ratatui dashboard for a compiled FlyScenario DSL document.
pub fn run_dashboard_compiled(compiled: CompiledRun) -> flyby_core::Result<()> {
    run_dashboard_inner(Dashboard::new_compiled(compiled)?)
}

fn run_dashboard_inner(mut dash: Dashboard) -> flyby_core::Result<()> {
    enable_raw_mode().map_err(io_err)?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen).map_err(io_err)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).map_err(io_err)?;

    let tick_rate = Duration::from_millis(50);
    let mut last_tick = Instant::now();

    let result = (|| -> flyby_core::Result<()> {
        loop {
            terminal
                .draw(|frame| ui::draw(frame, &dash.snapshot()))
                .map_err(io_err)?;

            let timeout = tick_rate.saturating_sub(last_tick.elapsed());
            if event::poll(timeout).map_err(io_err)?
                && let Event::Key(key) = event::read().map_err(io_err)?
                && key.kind == KeyEventKind::Press
                && dash.handle_key(key.code, key.modifiers)?
            {
                break;
            }

            if last_tick.elapsed() >= tick_rate {
                dash.auto_ticks()?;
                last_tick = Instant::now();
            }
        }
        Ok(())
    })();

    disable_raw_mode().map_err(io_err)?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen).map_err(io_err)?;
    terminal.show_cursor().map_err(io_err)?;
    result
}

fn io_err(e: io::Error) -> flyby_core::Error {
    flyby_core::Error::new(flyby_core::ErrorKind::Io, format!("tui: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsl::compile_str;
    use std::time::Duration;

    #[test]
    fn build_scheduler_arms_paused() {
        let sink = VecEventSink::new();
        let mut sched = build_scheduler_builtin(Scenario::constant_rate(), sink).unwrap();
        sched.edu_mut().paused = true;
        let stats = sched.run().unwrap();
        assert_eq!(stats.ticks, 0);
        assert!(sched.is_running());
    }

    #[test]
    fn format_event_covers_kinds() {
        let cases = [
            (
                SimEventKind::PacketGenerated {
                    nic: "n0",
                    len: 64,
                    seq: 1,
                },
                "gen",
            ),
            (
                SimEventKind::PacketDropped {
                    nic: "nic0",
                    seq: 7,
                },
                "DROP",
            ),
            (
                SimEventKind::PacketCorrupted { nic: "n0", seq: 2 },
                "CORRUPT",
            ),
            (SimEventKind::SlotWritten { ring: "r0", seq: 3 }, "write"),
            (SimEventKind::ConsumerRead { ring: "r0", seq: 4 }, "read"),
            (SimEventKind::QueueOverflow { ring: "r0" }, "OVERFLOW"),
            (
                SimEventKind::TickCompleted {
                    tick: 9,
                    duration_ns: 12,
                },
                "tick=",
            ),
            (
                SimEventKind::SimulatorStarted {
                    scenario: "s".into(),
                },
                "start",
            ),
            (
                SimEventKind::SimulatorStopped {
                    ticks: 3,
                    elapsed_ns: 100,
                },
                "stop",
            ),
            (
                SimEventKind::ParserError {
                    message: "bad".into(),
                },
                "ParserError",
            ),
            (
                SimEventKind::PlacementDecision {
                    strategy: "rr",
                    count: 2,
                },
                "PlacementDecision",
            ),
            (
                SimEventKind::RecordRead {
                    backend: "file",
                    offset: 0,
                    len: 8,
                },
                "RecordRead",
            ),
            (
                SimEventKind::RecordDropped {
                    backend: "file",
                    offset: 8,
                },
                "RecordDropped",
            ),
        ];
        for (kind, needle) in cases {
            let line = format_event(&SimEvent {
                clock_ns: 1_000_000,
                kind,
            });
            assert!(line.contains(needle), "missing {needle} in {line}");
        }
    }

    #[test]
    fn dashboard_step_advances() {
        let mut dash = Dashboard::new(Scenario {
            duration: Duration::from_millis(5),
            tick_ns: 1_000_000,
            ..Scenario::constant_rate()
        })
        .unwrap();
        dash.step_once().unwrap();
        assert_eq!(dash.sched.stats().ticks, 1);
        let snap = dash.snapshot();
        assert_eq!(snap.stats.ticks, 1);
        assert!(!snap.pps_hist.is_empty());
    }

    #[test]
    fn dashboard_keys_toggle_speed_restart_and_quit() {
        let mut dash = Dashboard::new(Scenario {
            duration: Duration::from_millis(20),
            tick_ns: 1_000_000,
            ..Scenario::constant_rate()
        })
        .unwrap();

        assert!(
            !dash
                .handle_key(KeyCode::Char('+'), KeyModifiers::NONE)
                .unwrap()
        );
        assert_eq!(dash.ticks_per_frame, 2);
        dash.handle_key(KeyCode::Char('='), KeyModifiers::NONE)
            .unwrap();
        assert_eq!(dash.ticks_per_frame, 4);
        dash.handle_key(KeyCode::Char('-'), KeyModifiers::NONE)
            .unwrap();
        assert_eq!(dash.ticks_per_frame, 2);
        dash.handle_key(KeyCode::Char('_'), KeyModifiers::NONE)
            .unwrap();
        assert_eq!(dash.ticks_per_frame, 1);

        dash.handle_key(KeyCode::Char(' '), KeyModifiers::NONE)
            .unwrap();
        assert_eq!(dash.mode, RunMode::Auto);
        dash.auto_ticks().unwrap();
        assert!(dash.sched.stats().ticks >= 1);

        dash.handle_key(KeyCode::Char(' '), KeyModifiers::NONE)
            .unwrap();
        assert_eq!(dash.mode, RunMode::Paused);

        dash.handle_key(KeyCode::Char('s'), KeyModifiers::NONE)
            .unwrap();
        dash.handle_key(KeyCode::Right, KeyModifiers::NONE).unwrap();

        dash.handle_key(KeyCode::Char('r'), KeyModifiers::NONE)
            .unwrap();
        assert_eq!(dash.mode, RunMode::Paused);
        assert!(!dash.finished);

        assert!(
            dash.handle_key(KeyCode::Char('q'), KeyModifiers::NONE)
                .unwrap()
        );
        assert!(dash.handle_key(KeyCode::Esc, KeyModifiers::NONE).unwrap());
        assert!(
            dash.handle_key(KeyCode::Char('c'), KeyModifiers::CONTROL)
                .unwrap()
        );
    }

    #[test]
    fn dashboard_runs_to_finish_and_handles_restart_prompt() {
        let mut dash = Dashboard::new(Scenario {
            duration: Duration::from_millis(2),
            tick_ns: 1_000_000,
            ..Scenario::constant_rate()
        })
        .unwrap();
        for _ in 0..16 {
            dash.step_once().unwrap();
            if dash.finished {
                break;
            }
        }
        assert!(dash.finished);
        dash.step_once().unwrap(); // status: finished prompt
        dash.handle_key(KeyCode::Char(' '), KeyModifiers::NONE)
            .unwrap();
        assert!(dash.status.contains("Finished") || dash.status.contains("restart"));
        dash.handle_key(KeyCode::Char('r'), KeyModifiers::NONE)
            .unwrap();
        assert!(!dash.finished);
    }

    #[test]
    fn dashboard_compiled_and_special_scenarios() {
        let compiled = compile_str(
            r#"
            [scenario]
            name = "tui_compiled"
            duration = "5ms"
            tick = "1ms"
            [[nic]]
            name = "nic0"
            [nic.traffic]
            pattern = "fixed"
            pps = 1000
            [[consumer]]
            name = "c0"
            "#,
            ".",
        )
        .unwrap();
        let mut dash = Dashboard::new_compiled(compiled).unwrap();
        dash.step_once().unwrap();
        assert_eq!(dash.snapshot().scenario_name, "tui_compiled");

        let slow = build_scheduler_builtin(Scenario::slow_consumer(), VecEventSink::new()).unwrap();
        let _ = slow;
        let overflow =
            build_scheduler_builtin(Scenario::queue_overflow(), VecEventSink::new()).unwrap();
        let _ = overflow;
    }

    #[test]
    fn push_hist_caps_length() {
        let mut buf = Vec::new();
        for i in 0..10 {
            push_hist(&mut buf, i, 4);
        }
        assert_eq!(buf.len(), 4);
        assert_eq!(buf, vec![6, 7, 8, 9]);
    }

    #[test]
    fn dash_state_default_is_paused() {
        let s = DashState::default();
        assert_eq!(s.mode, RunMode::Paused);
        assert_eq!(s.ticks_per_frame, 1);
        assert!(!s.finished);
    }
}
