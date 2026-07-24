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

#[test]
fn rhai_nic_and_consumer_proxies() {
    let actions = dsl::eval_script(
        r#"
        at(ms(10));
        let n = nic("nic0");
        n.drop_rate = 0.25;
        n.set_fixed(5_000);
        n.set_burst(16, ms(2));
        let c = consumer("c0");
        c.max_per_drain = 4;
        c.set_unlimited();
        schedule_fixed(ms(50), "nic1", 1000);
        schedule_slow_consumer(ms(60), "c1", 2);
        "#,
    )
    .unwrap();
    assert!(actions.len() >= 6);
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, TimelineAction::SetTraffic { .. }))
    );
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, TimelineAction::SlowConsumer { .. }))
    );
}

#[test]
fn rhai_requires_at_before_mutation() {
    let err = dsl::eval_script(
        r#"
        let n = nic("nic0");
        n.drop_rate = 0.1;
        "#,
    )
    .unwrap_err();
    assert!(err.to_string().contains("at(") || err.to_string().contains("Rhai"));
}

#[test]
fn compile_rejects_bad_patterns_and_empty_script() {
    let err = compile_str(
        r#"
        [scenario]
        name = "bad_pat"
        [[nic]]
        name = "nic0"
        [nic.traffic]
        pattern = "not_a_real_pattern"
        "#,
        ".",
    )
    .unwrap_err();
    assert!(err.to_string().contains("pattern") || err.to_string().contains("unknown"));

    let err = compile_str(
        r#"
        [scenario]
        name = "bad_engine"
        [[nic]]
        name = "nic0"
        [script]
        engine = "lua"
        source = "print(1)"
        "#,
        ".",
    )
    .unwrap_err();
    assert!(err.to_string().contains("engine") || err.to_string().contains("rhai"));

    let err = compile_str(
        r#"
        [scenario]
        name = "empty_script"
        [[nic]]
        name = "nic0"
        [script]
        source = ""
        "#,
        ".",
    )
    .unwrap_err();
    assert!(err.to_string().contains("script") || err.to_string().contains("source"));
}

#[test]
fn compile_inline_timeline_and_edu() {
    let run = compile_str(
        r#"
        [scenario]
        name = "edu_timeline"
        duration = "100ms"
        tick = "10ms"
        [[nic]]
        name = "nic0"
        [nic.traffic]
        pattern = "fixed"
        pps = 1000
        [[consumer]]
        name = "c0"
        [[timeline]]
        at = "20ms"
        action = "set_fault"
        nic = "nic0"
        drop_rate = 0.5
        [[timeline]]
        at = "40ms"
        action = "slow_consumer"
        consumer = "c0"
        max_per_drain = 1
        [edu]
        breakpoint_tick = 5
        "#,
        ".",
    )
    .unwrap();
    assert_eq!(run.timeline.len(), 2);
    let mut sched = run.build_scheduler().unwrap();
    let stats = sched.run().unwrap();
    assert!(stats.ticks <= 5);
}
