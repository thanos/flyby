//! FlyScenario DSL integration tests.

use flyby_simulator::dsl::{self, compile_str};
use flyby_simulator::timeline::TimelineAction;
use std::path::PathBuf;
use std::time::Duration;

fn scenarios_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../scenarios")
}

#[test]
fn parse_constant_rate_file() {
    let path = scenarios_dir().join("constant_rate.fly.toml");
    let run = dsl::compile_path(&path).expect("compile constant_rate");
    assert_eq!(run.scenario.name, "constant_rate");
    assert_eq!(run.scenario.duration, Duration::from_secs(1));
    assert_eq!(run.nics.len(), 1);
    assert!(run.simulated);
    let mut sched = run.build_scheduler().unwrap();
    let stats = sched.run().unwrap();
    assert!(stats.ticks > 0);
    assert!(stats.packets_generated > 0);
}

#[test]
fn timeline_actions_compile() {
    let path = scenarios_dir().join("market_open_lossy.fly.toml");
    let run = dsl::compile_path(&path).unwrap();
    assert_eq!(run.timeline.len(), 3);
    assert!(matches!(
        &run.timeline[0],
        TimelineAction::SetFault {
            at_ns: 200_000_000,
            ..
        }
    ));
    let mut sched = run.build_scheduler().unwrap();
    let stats = sched.run().unwrap();
    assert!(stats.packets_dropped > 0, "timeline drop should fire");
}

#[test]
fn rhai_script_emits_timeline() {
    let path = scenarios_dir().join("rhai_drop_ramp.fly.toml");
    let run = dsl::compile_path(&path).unwrap();
    assert!(
        run.timeline.len() >= 10,
        "expected ramp actions, got {}",
        run.timeline.len()
    );
    let mut sched = run.build_scheduler().unwrap();
    let stats = sched.run().unwrap();
    assert!(stats.packets_generated > 0);
    assert!(stats.packets_dropped > 0);
}

#[test]
fn rejects_simulated_false() {
    let toml = r#"
        [scenario]
        name = "bad"
        simulated = false
        [[nic]]
        name = "nic0"
    "#;
    let err = compile_str(toml, ".").unwrap_err();
    assert!(err.to_string().contains("simulated"));
}

#[test]
fn rhai_eval_script_direct() {
    let actions = dsl::eval_script(
        r#"
        for i in 0..3 {
            schedule_fault(ms(i * 10), "nic0", 0.1 * i);
        }
        "#,
    )
    .unwrap();
    assert_eq!(actions.len(), 3);
}

#[test]
fn looks_like_scenario_file() {
    assert!(dsl::looks_like_scenario_file("scenarios/foo.fly.toml"));
    assert!(dsl::looks_like_scenario_file("x.toml"));
    assert!(!dsl::looks_like_scenario_file("constant_rate"));
}
