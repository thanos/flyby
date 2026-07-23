//! Phase 3: Rhai `[script]` evaluation → [`TimelineAction`]s.
//!
//! Scripts run once at compile time and only schedule timeline mutations.
//! They never touch hardware backends.
//!
//! ## Supported surface
//!
//! ```rhai
//! // Time helpers (return nanoseconds)
//! at(ms(100));
//! let n = nic("nic0");
//! n.drop_rate = 0.2;            // schedules set_fault at current `at`
//! n.set_fixed(20_000);
//! n.set_burst(10_000, ms(1));
//! let c = consumer("c0");
//! c.max_per_drain = 4;
//!
//! // Or explicit helpers (no prior at() needed)
//! schedule_fault(ms(50), "nic0", 0.05);
//! schedule_fixed(ms(500), "nic0", 20_000);
//! schedule_slow_consumer(ms(800), "c0", 4);
//! ```
//!
//! Note: Rhai cannot assign properties on temporary values, so bind
//! `nic(...)` / `consumer(...)` to a `let` before setting fields.

use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;
use std::time::Duration;

use flyby_core::{Error, Result};
use rhai::{Engine, Scope};

use super::doc::ScriptDoc;
use crate::fault::FaultSpec;
use crate::timeline::TimelineAction;
use crate::traffic::{TrafficConfig, TrafficPattern};

#[derive(Debug, Default)]
struct ScriptHost {
    current_at_ns: Option<u64>,
    actions: Vec<TimelineAction>,
}

impl ScriptHost {
    fn require_at(&self) -> std::result::Result<u64, Box<rhai::EvalAltResult>> {
        self.current_at_ns
            .ok_or_else(|| "call at(ns) before mutating nic/consumer in a script".into())
    }
}

/// Shared host handle registered into Rhai.
#[derive(Clone)]
struct HostHandle(Rc<RefCell<ScriptHost>>);

/// Proxy returned by `nic("name")`.
#[derive(Clone)]
struct NicProxy {
    name: String,
    host: HostHandle,
}

/// Proxy returned by `consumer("name")`.
#[derive(Clone)]
struct ConsumerProxy {
    name: String,
    host: HostHandle,
}

/// Evaluate a `[script]` block into timeline actions.
pub fn eval_script_doc(script: &ScriptDoc, base_dir: &Path) -> Result<Vec<TimelineAction>> {
    if !script.engine.is_empty() && !script.engine.eq_ignore_ascii_case("rhai") {
        return Err(Error::config(format!(
            "unsupported script engine '{}' (only rhai)",
            script.engine
        )));
    }

    let source = match (&script.source, &script.path) {
        (Some(s), _) if !s.trim().is_empty() => s.clone(),
        (_, Some(p)) => {
            let path = if Path::new(p).is_absolute() {
                Path::new(p).to_path_buf()
            } else {
                base_dir.join(p)
            };
            std::fs::read_to_string(&path).map_err(|e| {
                Error::config(format!("failed to read script '{}': {e}", path.display()))
            })?
        }
        _ => {
            return Err(Error::config(
                "[script] requires source = '''...''' or path = \"file.rhai\"",
            ));
        }
    };

    eval_script(&source)
}

/// Evaluate raw Rhai source into timeline actions.
pub fn eval_script(source: &str) -> Result<Vec<TimelineAction>> {
    let host = HostHandle(Rc::new(RefCell::new(ScriptHost::default())));
    let mut engine = Engine::new();
    register_api(&mut engine, host.clone());

    let mut scope = Scope::new();
    engine
        .run_with_scope(&mut scope, source)
        .map_err(|e| Error::config(format!("Rhai script error: {e}")))?;

    let mut actions = host.0.borrow_mut().actions.drain(..).collect::<Vec<_>>();
    actions.sort_by_key(|a| a.at_ns());
    Ok(actions)
}

fn register_api(engine: &mut Engine, host: HostHandle) {
    engine.register_type_with_name::<NicProxy>("Nic");
    engine.register_type_with_name::<ConsumerProxy>("Consumer");

    // Time helpers → nanoseconds (as i64 for Rhai ergonomics).
    engine.register_fn("ns", |n: i64| n);
    engine.register_fn("us", |n: i64| n.saturating_mul(1_000));
    engine.register_fn("ms", |n: i64| n.saturating_mul(1_000_000));
    engine.register_fn("s", |n: i64| n.saturating_mul(1_000_000_000));
    // Also accept float for ms(1.5).
    engine.register_fn("ms", |n: f64| (n * 1_000_000.0).round() as i64);
    engine.register_fn("s", |n: f64| (n * 1_000_000_000.0).round() as i64);
    engine.register_fn("us", |n: f64| (n * 1_000.0).round() as i64);

    {
        let h = host.clone();
        engine.register_fn("at", move |t: i64| {
            h.0.borrow_mut().current_at_ns = Some(t.max(0) as u64);
        });
    }

    {
        let h = host.clone();
        engine.register_fn("nic", move |name: &str| NicProxy {
            name: name.to_string(),
            host: h.clone(),
        });
    }

    {
        let h = host.clone();
        engine.register_fn("consumer", move |name: &str| ConsumerProxy {
            name: name.to_string(),
            host: h.clone(),
        });
    }

    // nic.drop_rate = x
    engine.register_set("drop_rate", |nic: &mut NicProxy, rate: f64| {
        let at = nic.host.0.borrow().require_at()?;
        nic.host
            .0
            .borrow_mut()
            .actions
            .push(TimelineAction::SetFault {
                at_ns: at,
                nic: nic.name.clone(),
                fault: FaultSpec {
                    drop_rate: rate,
                    ..FaultSpec::default()
                },
            });
        Ok(())
    });

    engine.register_fn("set_drop_rate", |nic: &mut NicProxy, rate: f64| {
        let at = nic.host.0.borrow().require_at()?;
        nic.host
            .0
            .borrow_mut()
            .actions
            .push(TimelineAction::SetFault {
                at_ns: at,
                nic: nic.name.clone(),
                fault: FaultSpec {
                    drop_rate: rate,
                    ..FaultSpec::default()
                },
            });
        Ok::<(), Box<rhai::EvalAltResult>>(())
    });

    engine.register_fn("set_fixed", |nic: &mut NicProxy, pps: i64| {
        let at = nic.host.0.borrow().require_at()?;
        nic.host
            .0
            .borrow_mut()
            .actions
            .push(TimelineAction::SetTraffic {
                at_ns: at,
                nic: nic.name.clone(),
                traffic: TrafficConfig {
                    pattern: TrafficPattern::FixedRate {
                        pps: pps.max(0) as u64,
                    },
                    ..TrafficConfig::default()
                },
            });
        Ok::<(), Box<rhai::EvalAltResult>>(())
    });

    engine.register_fn(
        "set_burst",
        |nic: &mut NicProxy, burst_size: i64, gap_ns: i64| {
            let at = nic.host.0.borrow().require_at()?;
            nic.host
                .0
                .borrow_mut()
                .actions
                .push(TimelineAction::SetTraffic {
                    at_ns: at,
                    nic: nic.name.clone(),
                    traffic: TrafficConfig {
                        pattern: TrafficPattern::Burst {
                            burst_size: burst_size.max(1) as usize,
                            gap: Duration::from_nanos(gap_ns.max(0) as u64),
                        },
                        batch_size: 256,
                        ..TrafficConfig::default()
                    },
                });
            Ok::<(), Box<rhai::EvalAltResult>>(())
        },
    );

    // consumer.max_per_drain = n  (or "unlimited" via set_unlimited)
    engine.register_set("max_per_drain", |c: &mut ConsumerProxy, n: i64| {
        let at = c.host.0.borrow().require_at()?;
        let max = if n <= 0 { usize::MAX } else { n as usize };
        c.host
            .0
            .borrow_mut()
            .actions
            .push(TimelineAction::SlowConsumer {
                at_ns: at,
                consumer: c.name.clone(),
                max_per_drain: max,
            });
        Ok(())
    });

    engine.register_fn("set_unlimited", |c: &mut ConsumerProxy| {
        let at = c.host.0.borrow().require_at()?;
        c.host
            .0
            .borrow_mut()
            .actions
            .push(TimelineAction::SlowConsumer {
                at_ns: at,
                consumer: c.name.clone(),
                max_per_drain: usize::MAX,
            });
        Ok::<(), Box<rhai::EvalAltResult>>(())
    });

    // Explicit schedule_* helpers (no prior at() needed).
    {
        let h = host.clone();
        engine.register_fn("schedule_fault", move |at_ns: i64, nic: &str, rate: f64| {
            h.0.borrow_mut().actions.push(TimelineAction::SetFault {
                at_ns: at_ns.max(0) as u64,
                nic: nic.to_string(),
                fault: FaultSpec {
                    drop_rate: rate,
                    ..FaultSpec::default()
                },
            });
        });
    }
    {
        let h = host.clone();
        engine.register_fn("schedule_fixed", move |at_ns: i64, nic: &str, pps: i64| {
            h.0.borrow_mut().actions.push(TimelineAction::SetTraffic {
                at_ns: at_ns.max(0) as u64,
                nic: nic.to_string(),
                traffic: TrafficConfig {
                    pattern: TrafficPattern::FixedRate {
                        pps: pps.max(0) as u64,
                    },
                    ..TrafficConfig::default()
                },
            });
        });
    }
    {
        let h = host.clone();
        engine.register_fn(
            "schedule_slow_consumer",
            move |at_ns: i64, consumer: &str, max: i64| {
                let max_per_drain = if max <= 0 { usize::MAX } else { max as usize };
                h.0.borrow_mut().actions.push(TimelineAction::SlowConsumer {
                    at_ns: at_ns.max(0) as u64,
                    consumer: consumer.to_string(),
                    max_per_drain,
                });
            },
        );
    }
}
