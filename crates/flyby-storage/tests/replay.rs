//! Integration tests: replay engine timing modes.

use flyby_storage::{ReplayEngine, ReplayMode};
use std::time::Duration;

#[test]
fn full_speed_never_blocks() {
    let mut engine = ReplayEngine::new(ReplayMode::FullSpeed).unwrap();
    for ts in [0u64, 1, 1_000_000, u64::MAX / 2] {
        assert!(engine.poll(ts), "full-speed blocked at ts={ts}");
    }
}

#[test]
fn single_step_requires_advance() {
    let mut engine = ReplayEngine::new(ReplayMode::SingleStep).unwrap();
    assert!(engine.poll(0));
    assert!(!engine.poll(0));
    engine.advance();
    assert!(engine.poll(0));
}

#[test]
fn original_timing_first_record_is_immediate() {
    let mut engine = ReplayEngine::new(ReplayMode::OriginalTiming).unwrap();
    assert!(engine.poll(9_999_999_999_000_000_000));
}

#[test]
fn original_timing_same_timestamp_always_ready() {
    let mut engine = ReplayEngine::new(ReplayMode::OriginalTiming).unwrap();
    let ts = 1_000_000_000u64;
    assert!(engine.poll(ts));
    assert!(engine.poll(ts));
}

#[test]
fn original_timing_far_future_record_not_ready() {
    let mut engine = ReplayEngine::new(ReplayMode::OriginalTiming).unwrap();
    engine.poll(0);
    let one_hour_ns = 3_600_000_000_000u64;
    assert!(
        !engine.poll(one_hour_ns),
        "record 1 hour ahead should not be ready immediately"
    );
}

#[test]
fn time_scaled_first_record_immediate() {
    let mut engine = ReplayEngine::new(ReplayMode::TimeScaled { factor: 10.0 }).unwrap();
    assert!(engine.poll(1_000_000_000));
}

#[test]
fn time_scaled_fast_forward_does_not_block_past_records() {
    let mut engine = ReplayEngine::new(ReplayMode::TimeScaled { factor: 0.000001 }).unwrap();
    engine.poll(0);
    std::thread::sleep(std::time::Duration::from_millis(10));
    let ts_one_second = 1_000_000_000u64;
    assert!(
        engine.poll(ts_one_second),
        "after 10 ms sleep, a 1 µs deadline should definitely have passed"
    );
}

#[test]
fn time_scaled_slow_down_holds_record() {
    let mut engine = ReplayEngine::new(ReplayMode::TimeScaled {
        factor: 1_000_000.0,
    })
    .unwrap();
    engine.poll(0);
    assert!(
        !engine.poll(1_000),
        "slow-down should block far-future records"
    );
}

#[test]
fn burst_emits_configured_count_then_pauses() {
    let mut engine = ReplayEngine::new(ReplayMode::Burst {
        count: 3,
        gap: Duration::from_secs(3600),
    })
    .unwrap();
    assert!(engine.poll(0));
    assert!(engine.poll(0));
    assert!(engine.poll(0));
    assert!(!engine.poll(0));
    assert!(!engine.poll(0));
}

#[test]
fn burst_resumes_after_zero_gap() {
    let mut engine = ReplayEngine::new(ReplayMode::Burst {
        count: 1,
        gap: Duration::ZERO,
    })
    .unwrap();
    assert!(engine.poll(0));
    assert!(engine.poll(0));
}
