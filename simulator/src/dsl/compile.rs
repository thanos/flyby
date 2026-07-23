//! Compile [`ScenarioDoc`] into a [`CompiledRun`] ready for the scheduler.

use std::path::{Path, PathBuf};
use std::time::Duration;

use flyby_core::{Error, Result};
use flyby_storage::ReplayMode;

use super::doc::*;
use super::duration::parse_duration_ns;
use super::script;
use crate::clock::ClockMode;
use crate::events::{EventSink, NullEventSink};
use crate::fault::FaultSpec;
use crate::generator::{PayloadSpec, ProtocolMessage};
use crate::pcap::{PcapConfig, PcapSource};
use crate::ring::VirtualSharedMemory;
use crate::scenario::Scenario;
use crate::scheduler::{EduControls, SimScheduler};
use crate::timeline::TimelineAction;
use crate::traffic::{TrafficConfig, TrafficPattern};
use crate::{VirtualConsumer, VirtualNic, VirtualNicConfig};

/// Leak an owned string into a `'static` str for NIC / ring / consumer names.
fn leak_name(s: impl Into<String>) -> &'static str {
    Box::leak(s.into().into_boxed_str())
}

/// Compiled NIC ready to construct a [`VirtualNic`].
#[derive(Debug, Clone)]
pub struct CompiledNic {
    /// Source name.
    pub name: String,
    /// Batch size.
    pub batch_size: usize,
    /// UDP destination port.
    pub udp_port: u16,
    /// Traffic config.
    pub traffic: TrafficConfig,
    /// Fault policy.
    pub fault: FaultSpec,
    /// Fault seed.
    pub fault_seed: u64,
}

/// Compiled pcap source.
#[derive(Debug, Clone)]
pub struct CompiledPcap {
    /// Source name.
    pub name: String,
    /// Absolute or scenario-relative path resolved at compile time.
    pub path: PathBuf,
    /// Pcap config (name field filled at build time via leak).
    pub replay: ReplayMode,
    /// Loop capture.
    pub loop_capture: bool,
    /// Fault policy.
    pub fault: FaultSpec,
    /// Fault seed.
    pub fault_seed: u64,
}

/// Compiled fabric ring.
#[derive(Debug, Clone)]
pub struct CompiledFabric {
    /// Ring name.
    pub name: String,
    /// Slot count.
    pub slots: usize,
    /// Max frame size.
    pub max_frame: usize,
}

/// Compiled consumer.
#[derive(Debug, Clone)]
pub struct CompiledConsumer {
    /// Consumer name.
    pub name: String,
    /// Max slots per drain.
    pub max_per_drain: usize,
}

/// Fully compiled FlyScenario ready to build a [`SimScheduler`].
#[derive(Debug, Clone)]
pub struct CompiledRun {
    /// Core scenario meta used by the scheduler.
    pub scenario: Scenario,
    /// Educational controls.
    pub edu: EduControls,
    /// Whether structured events should be collected (CLI hint).
    pub events_enabled: bool,
    /// Whether metrics should be attached (CLI hint).
    pub metrics_enabled: bool,
    /// Always `true` for DSL runs.
    pub simulated: bool,
    /// Virtual NICs.
    pub nics: Vec<CompiledNic>,
    /// Pcap sources.
    pub pcaps: Vec<CompiledPcap>,
    /// Shared-memory fabric (optional).
    pub fabric: Option<CompiledFabric>,
    /// Consumers.
    pub consumers: Vec<CompiledConsumer>,
    /// Timeline actions (TOML + Rhai).
    pub timeline: Vec<TimelineAction>,
    /// Storage sources declared but not yet wired into the scheduler.
    pub storage_declared: usize,
}

impl CompiledRun {
    /// Build a null-sink scheduler with all sources, fabric, consumers, timeline.
    pub fn build_scheduler(&self) -> Result<SimScheduler<NullEventSink>> {
        self.build_scheduler_with_events(NullEventSink)
    }

    /// Build a scheduler with a custom event sink.
    pub fn build_scheduler_with_events<E: EventSink + Clone + 'static>(
        &self,
        events: E,
    ) -> Result<SimScheduler<E>> {
        if self.nics.is_empty() && self.pcaps.is_empty() {
            return Err(Error::config(
                "scenario has no [[nic]] or [[pcap]] sources (storage-only runs not yet supported)",
            ));
        }

        let mut sched = SimScheduler::with_events(self.scenario.clone(), events.clone())
            .with_edu(self.edu.clone())
            .with_timeline(self.timeline.clone());

        if let Some(fab) = &self.fabric {
            sched = sched.with_ring(VirtualSharedMemory::new(
                leak_name(fab.name.clone()),
                fab.slots,
                fab.max_frame,
            ));
        }

        for nic in &self.nics {
            let cfg = VirtualNicConfig {
                name: leak_name(nic.name.clone()),
                traffic: nic.traffic.clone(),
                fault: nic.fault.clone(),
                fault_seed: nic.fault_seed,
                udp_dst_port: nic.udp_port,
            };
            sched.add_nic(VirtualNic::new(cfg, events.clone()));
        }

        for pcap in &self.pcaps {
            let cfg = PcapConfig {
                name: leak_name(pcap.name.clone()),
                replay: pcap.replay.clone(),
                fault: pcap.fault.clone(),
                fault_seed: pcap.fault_seed,
                loop_capture: pcap.loop_capture,
            };
            let src = PcapSource::from_path(&pcap.path, cfg, events.clone())
                .map_err(|e| Error::config(format!("pcap '{}': {e}", pcap.path.display())))?;
            sched.add_nic(src);
        }

        for c in &self.consumers {
            let name = leak_name(c.name.clone());
            if c.max_per_drain == usize::MAX {
                sched.add_consumer(VirtualConsumer::new(name));
            } else {
                sched.add_consumer(VirtualConsumer::slow(name, c.max_per_drain));
            }
        }

        Ok(sched)
    }
}

/// Compile a parsed document.
pub fn compile_doc(mut doc: ScenarioDoc, base_dir: &Path) -> Result<CompiledRun> {
    if !doc.scenario.simulated {
        return Err(Error::config(
            "scenario.simulated must be true — FlyScenario results are always simulated",
        ));
    }

    // Evaluate Rhai first so scripted actions merge with [[timeline]].
    let mut script_actions = Vec::new();
    if let Some(script) = doc.script.take() {
        script_actions = script::eval_script_doc(&script, base_dir)?;
    }

    let duration_ns = parse_duration_ns(&doc.scenario.duration)?;
    let tick_ns = parse_duration_ns(&doc.scenario.tick)?;
    if tick_ns == 0 {
        return Err(Error::config("scenario.tick must be > 0"));
    }

    let clock_mode = match doc.scenario.clock.to_ascii_lowercase().as_str() {
        "virtual" => ClockMode::Virtual { start_ns: 0 },
        "realtime" | "real" | "real_time" => ClockMode::RealTime,
        other => {
            return Err(Error::config(format!(
                "unknown clock '{other}' (use virtual|realtime)"
            )));
        }
    };

    let mode = doc.scenario.mode.to_ascii_lowercase();
    let mut edu = EduControls::default();
    if mode == "edu" {
        edu.paused = true;
    }
    if let Some(e) = &doc.edu {
        if e.paused_start {
            edu.paused = true;
        }
        edu.breakpoint_tick = e.breakpoint_tick;
        if let Some(bp) = &e.breakpoint_ns {
            edu.breakpoint_ns = Some(parse_duration_ns(bp)?);
        }
    }

    let trace = doc.trace.clone().unwrap_or_default();
    let events_enabled = if mode == "benchmark" {
        false
    } else {
        trace.events
    };
    let metrics_enabled = trace.metrics;

    let nics: Vec<CompiledNic> = doc
        .nic
        .iter()
        .map(|n| compile_nic(n, doc.scenario.seed))
        .collect::<Result<_>>()?;

    let pcaps: Vec<CompiledPcap> = doc
        .pcap
        .iter()
        .map(|p| compile_pcap(p, base_dir, doc.scenario.seed))
        .collect::<Result<_>>()?;

    let fabric = match &doc.fabric {
        Some(f) => Some(compile_fabric(f, &nics)?),
        None if !doc.consumer.is_empty() || !nics.is_empty() || !pcaps.is_empty() => {
            // Default fabric when consumers/sources exist.
            let max_frame = nics
                .iter()
                .map(|n| 42 + n.traffic.payload_size.max(1))
                .chain(std::iter::once(128))
                .max()
                .unwrap_or(128);
            Some(CompiledFabric {
                name: "ring0".into(),
                slots: 4096,
                max_frame,
            })
        }
        None => None,
    };

    let consumers: Vec<CompiledConsumer> = if doc.consumer.is_empty() {
        // Default consumer when a fabric is present.
        if fabric.is_some() {
            vec![CompiledConsumer {
                name: "c0".into(),
                max_per_drain: usize::MAX,
            }]
        } else {
            Vec::new()
        }
    } else {
        doc.consumer
            .iter()
            .map(compile_consumer)
            .collect::<Result<_>>()?
    };

    let mut timeline = Vec::new();
    for t in &doc.timeline {
        timeline.push(compile_timeline_entry(t, &nics)?);
    }
    timeline.extend(script_actions);
    timeline.sort_by_key(|a| a.at_ns());

    // Scenario.traffic/fault mirror the first NIC for scheduler batch sizing.
    let (traffic, fault) = if let Some(n) = nics.first() {
        (n.traffic.clone(), n.fault.clone())
    } else {
        (TrafficConfig::default(), FaultSpec::default())
    };

    let scenario = Scenario {
        name: doc.scenario.name.clone(),
        description: doc.scenario.description.clone(),
        traffic,
        fault,
        duration: Duration::from_nanos(duration_ns),
        clock_mode,
        tick_ns,
    };

    Ok(CompiledRun {
        scenario,
        edu,
        events_enabled,
        metrics_enabled,
        simulated: true,
        nics,
        pcaps,
        fabric,
        consumers,
        timeline,
        storage_declared: doc.storage.len(),
    })
}

fn compile_nic(n: &NicDoc, scenario_seed: u64) -> Result<CompiledNic> {
    let traffic = compile_traffic(&n.traffic, &n.payload, n.batch_size)?;
    let fault = compile_fault(&n.fault)?;
    let fault_seed = n.fault.seed.unwrap_or(scenario_seed);
    Ok(CompiledNic {
        name: n.name.clone(),
        batch_size: n.batch_size,
        udp_port: n.udp_port,
        traffic,
        fault,
        fault_seed,
    })
}

fn compile_pcap(p: &PcapDoc, base: &Path, scenario_seed: u64) -> Result<CompiledPcap> {
    let path = resolve_path(base, &p.path);
    let replay = compile_replay(&p.replay, p.scale)?;
    let fault = compile_fault(&p.fault)?;
    Ok(CompiledPcap {
        name: p.name.clone(),
        path,
        replay,
        loop_capture: p.r#loop,
        fault,
        fault_seed: p.fault.seed.unwrap_or(scenario_seed),
    })
}

fn compile_fabric(f: &FabricDoc, nics: &[CompiledNic]) -> Result<CompiledFabric> {
    let max_frame = f.max_frame.unwrap_or_else(|| {
        nics.iter()
            .map(|n| 42 + n.traffic.payload_size.max(1))
            .max()
            .unwrap_or(128)
            .max(64)
    });
    if f.slots == 0 {
        return Err(Error::config("fabric.slots must be > 0"));
    }
    Ok(CompiledFabric {
        name: f.name.clone(),
        slots: f.slots,
        max_frame,
    })
}

fn compile_consumer(c: &ConsumerDoc) -> Result<CompiledConsumer> {
    let max_per_drain = parse_max_per_drain(c.max_per_drain.as_ref())?;
    Ok(CompiledConsumer {
        name: c.name.clone(),
        max_per_drain,
    })
}

fn compile_timeline_entry(t: &TimelineDoc, nics: &[CompiledNic]) -> Result<TimelineAction> {
    let at_ns = parse_duration_ns(&t.at)?;
    match t.action.as_str() {
        "set_traffic" => {
            let nic = t
                .nic
                .clone()
                .ok_or_else(|| Error::config("set_traffic requires nic = \"...\""))?;
            let base = nics
                .iter()
                .find(|n| n.name == nic)
                .map(|n| n.traffic.clone())
                .unwrap_or_default();
            let traffic = merge_traffic_override(base, t)?;
            Ok(TimelineAction::SetTraffic {
                at_ns,
                nic,
                traffic,
            })
        }
        "set_fault" => {
            let nic = t
                .nic
                .clone()
                .ok_or_else(|| Error::config("set_fault requires nic = \"...\""))?;
            let mut fault = FaultSpec::default();
            if let Some(v) = t.drop_rate {
                fault.drop_rate = v;
            }
            if let Some(v) = t.corrupt_rate {
                fault.corrupt_rate = v;
            }
            if let Some(v) = t.latency_spike_rate {
                fault.latency_spike_rate = v;
            }
            if let Some(spike) = &t.latency_spike {
                fault.latency_spike_ns = parse_duration_ns(spike)?;
            }
            Ok(TimelineAction::SetFault { at_ns, nic, fault })
        }
        "slow_consumer" => {
            let consumer = t
                .consumer
                .clone()
                .ok_or_else(|| Error::config("slow_consumer requires consumer = \"...\""))?;
            let max_per_drain = parse_max_per_drain(t.max_per_drain.as_ref())?;
            Ok(TimelineAction::SlowConsumer {
                at_ns,
                consumer,
                max_per_drain,
            })
        }
        other => Err(Error::config(format!(
            "unknown timeline action '{other}' (use set_traffic|set_fault|slow_consumer)"
        ))),
    }
}

fn merge_traffic_override(mut base: TrafficConfig, t: &TimelineDoc) -> Result<TrafficConfig> {
    let pattern_name = t.pattern.as_deref().unwrap_or("fixed");
    base.pattern = match pattern_name {
        "fixed" => TrafficPattern::FixedRate {
            pps: t.pps.unwrap_or(1_000),
        },
        "burst" => TrafficPattern::Burst {
            burst_size: t.burst_size.unwrap_or(1_000),
            gap: Duration::from_nanos(parse_duration_ns(t.gap.as_deref().unwrap_or("1ms"))?),
        },
        "gaussian" => TrafficPattern::Gaussian {
            mean_pps: t.mean_pps.unwrap_or(50_000.0),
            std_pps: t.std_pps.unwrap_or(10_000.0),
            seed: t.seed.unwrap_or(0),
        },
        "full" => TrafficPattern::FullSpeed,
        other => {
            return Err(Error::config(format!("unknown traffic pattern '{other}'")));
        }
    };
    Ok(base)
}

pub(super) fn compile_traffic(
    traffic: &TrafficDoc,
    payload: &PayloadDoc,
    batch_size: usize,
) -> Result<TrafficConfig> {
    let pattern = match traffic.pattern.to_ascii_lowercase().as_str() {
        "fixed" => TrafficPattern::FixedRate {
            pps: traffic.pps.unwrap_or(1_000),
        },
        "burst" => TrafficPattern::Burst {
            burst_size: traffic.burst_size.unwrap_or(1_000),
            gap: Duration::from_nanos(parse_duration_ns(traffic.gap.as_deref().unwrap_or("1ms"))?),
        },
        "gaussian" => TrafficPattern::Gaussian {
            mean_pps: traffic.mean_pps.unwrap_or(50_000.0),
            std_pps: traffic.std_pps.unwrap_or(10_000.0),
            seed: traffic.seed.unwrap_or(0),
        },
        "full" => TrafficPattern::FullSpeed,
        other => {
            return Err(Error::config(format!(
                "unknown traffic pattern '{other}' (use fixed|burst|gaussian|full)"
            )));
        }
    };

    let (payload_spec, payload_size) = compile_payload(payload)?;
    Ok(TrafficConfig {
        pattern,
        payload_size,
        batch_size,
        payload: payload_spec,
    })
}

fn compile_payload(p: &PayloadDoc) -> Result<(PayloadSpec, usize)> {
    match p.kind.to_ascii_lowercase().as_str() {
        "fixed_seq" | "fixed" => {
            let size = p.size.unwrap_or(8).max(1);
            Ok((PayloadSpec::FixedSeq, size))
        }
        "random" => {
            let size = p.size.unwrap_or(32).max(1);
            Ok((
                PayloadSpec::Random {
                    seed: p.seed.unwrap_or(0),
                },
                size,
            ))
        }
        "gaussian_size" => {
            let max = p.max.unwrap_or(128).max(1);
            Ok((
                PayloadSpec::GaussianSize {
                    mean: p.mean.unwrap_or(64.0),
                    std_dev: p.std_dev.unwrap_or(16.0),
                    seed: p.seed.unwrap_or(0),
                    max,
                },
                max,
            ))
        }
        "protocol" => {
            let proto = p.proto.as_deref().unwrap_or("market_quote");
            let symbol = p.symbol.as_deref().unwrap_or("AAPL");
            let msg = match proto {
                "market_quote" => ProtocolMessage::market_quote(symbol),
                "fix_quote" => ProtocolMessage::fix_quote(symbol),
                other => {
                    return Err(Error::config(format!(
                        "unknown protocol '{other}' (use market_quote|fix_quote)"
                    )));
                }
            };
            let size = msg.nominal_size();
            Ok((PayloadSpec::Protocol(msg), size))
        }
        "custom" => Err(Error::config(
            "payload kind 'custom' is Rust-only; use PayloadSpec::Custom in code",
        )),
        other => Err(Error::config(format!(
            "unknown payload kind '{other}' (use fixed_seq|random|gaussian_size|protocol)"
        ))),
    }
}

pub(super) fn compile_fault(f: &FaultDoc) -> Result<FaultSpec> {
    let latency_spike_ns = match &f.latency_spike {
        Some(s) => parse_duration_ns(s)?,
        None => 0,
    };
    Ok(FaultSpec {
        drop_rate: f.drop_rate,
        corrupt_rate: f.corrupt_rate,
        latency_spike_rate: f.latency_spike_rate,
        latency_spike_ns,
    })
}

fn compile_replay(name: &str, scale: f64) -> Result<ReplayMode> {
    match name.to_ascii_lowercase().as_str() {
        "full" | "full_speed" | "fullspeed" => Ok(ReplayMode::FullSpeed),
        "original" | "original_timing" => Ok(ReplayMode::OriginalTiming),
        "scaled" | "time_scaled" => Ok(ReplayMode::TimeScaled { factor: scale }),
        "burst" => Ok(ReplayMode::Burst {
            count: 64,
            gap: Duration::from_millis(1),
        }),
        "single_step" | "singlestep" | "step" => Ok(ReplayMode::SingleStep),
        other => Err(Error::config(format!(
            "unknown replay mode '{other}' (use full|original|scaled|burst|single_step)"
        ))),
    }
}

fn parse_max_per_drain(v: Option<&toml::Value>) -> Result<usize> {
    match v {
        None => Ok(usize::MAX),
        Some(toml::Value::String(s)) if s.eq_ignore_ascii_case("unlimited") => Ok(usize::MAX),
        Some(toml::Value::Integer(i)) if *i > 0 => Ok(*i as usize),
        Some(toml::Value::Integer(_)) => Err(Error::config("max_per_drain must be > 0")),
        Some(other) => Err(Error::config(format!(
            "max_per_drain must be a positive integer or \"unlimited\", got {other}"
        ))),
    }
}

fn resolve_path(base: &Path, p: &str) -> PathBuf {
    let path = Path::new(p);
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        base.join(path)
    }
}
