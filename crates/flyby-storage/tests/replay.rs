//! Integration tests: replay engine timing modes.
//!
//! Covers every [`ReplayMode`] variant:
//! - FullSpeed: always ready
//! - SingleStep: always ready (one per call)
//! - OriginalTiming: first record immediate, later records held
//! - TimeScaled: same as OriginalTiming but stretched/compressed
//! - Burst: N records then a gap

use flyby_storage::{ReplayEngine, ReplayMode};
use std::time::Duration;

// ---------------------------------------------------------------------------
// FullSpeed
// ---------------------------------------------------------------------------

#[test]
fn full_speed_never_blocks() {
    let mut engine = ReplayEngine::new(ReplayMode::FullSpeed);
    // Should return true regardless of timestamp
    for ts in [0u64, 1, 1_000_000, u64::MAX / 2] {
        assert!(engine.poll(ts), "full-speed blocked at ts={ts}");
    }
}

#[test]
fn full_speed_with_zero_timestamps() {
    let mut engine = ReplayEngine::new(ReplayMode::FullSpeed);
    for _ in 0..100 {
        assert!(engine.poll(0));
    }
}

// ---------------------------------------------------------------------------
// SingleStep
// ---------------------------------------------------------------------------

#[test]
fn single_step_always_ready() {
    let mut engine = ReplayEngine::new(ReplayMode::SingleStep);
    assert!(engine.poll(0));
    assert!(engine.poll(0));
    assert!(engine.poll(999_999_999_999));
}

// ---------------------------------------------------------------------------
// OriginalTiming
// ---------------------------------------------------------------------------

#[test]
fn original_timing_first_record_is_immediate() {
    let mut engine = ReplayEngine::new(ReplayMode::OriginalTiming);
    // First record always emitted immediately regardless of timestamp
    assert!(engine.poll(9_999_999_999_000_000_000));
}

#[test]
fn original_timing_same_timestamp_always_ready() {
    let mut engine = ReplayEngine::new(ReplayMode::OriginalTiming);
    let ts = 1_000_000_000u64;
    assert!(engine.poll(ts));
    // Duplicate timestamp — no gap to wait for
    assert!(engine.poll(ts));
}

#[test]
fn original_timing_far_future_record_not_ready() {
    let mut engine = ReplayEngine::new(ReplayMode::OriginalTiming);
    engine.poll(0); // anchor the start time at ts=0
    // Ask for a record 1 hour in the future (wall time just started)
    let one_hour_ns = 3_600_000_000_000u64;
    assert!(
        !engine.poll(one_hour_ns),
        "record 1 hour ahead should not be ready immediately"
    );
}

// ---------------------------------------------------------------------------
// TimeScaled
// ---------------------------------------------------------------------------

#[test]
fn time_scaled_first_record_immediate() {
    let mut engine = ReplayEngine::new(ReplayMode::TimeScaled { factor: 10.0 });
    assert!(engine.poll(1_000_000_000));
}

#[test]
fn time_scaled_fast_forward_does_not_block_past_records() {
    // factor 0.000001 = 1 000 000× speed: a record 1 second in original time
    // only needs to wait 1 µs in wall time.  We sleep 10 ms to be safe.
    let mut engine = ReplayEngine::new(ReplayMode::TimeScaled { factor: 0.000001 });
    engine.poll(0); // anchor
    std::thread::sleep(std::time::Duration::from_millis(10));
    let ts_one_second = 1_000_000_000u64;
    assert!(
        engine.poll(ts_one_second),
        "after 10 ms sleep, a 1 µs deadline should definitely have passed"
    );
}

#[test]
fn time_scaled_slow_down_holds_record() {
    // factor=1_000_000 = 1 000 000× slow-down: a record 1 µs ahead in original
    // time needs 1 second of wall time. Should not be ready.
    let mut engine = ReplayEngine::new(ReplayMode::TimeScaled {
        factor: 1_000_000.0,
    });
    engine.poll(0);
    assert!(
        !engine.poll(1_000),
        "1 000× slow-down: 1 µs original time = 1 s wall time, should block"
    );
}

// ---------------------------------------------------------------------------
// Burst
// ---------------------------------------------------------------------------

#[test]
fn burst_emits_configured_count_then_pauses() {
    let mut engine = ReplayEngine::new(ReplayMode::Burst {
        count: 3,
        gap: Duration::from_secs(3600),
    });
    assert!(engine.poll(0)); // 1
    assert!(engine.poll(0)); // 2
    assert!(engine.poll(0)); // 3 — triggers pause
    assert!(!engine.poll(0)); // paused
    assert!(!engine.poll(0)); // still paused
}

#[test]
fn burst_resumes_after_zero_gap() {
    let mut engine = ReplayEngine::new(ReplayMode::Burst {
        count: 1,
        gap: Duration::ZERO,
    });
    assert!(engine.poll(0)); // burst of 1, then zero-gap pause
    // A zero gap means the pause ends immediately; the next call should succeed.
    assert!(engine.poll(0));
}

#[test]
fn burst_count_one_alternates() {
    // count=1, gap=0 → emit one, (instant resume), emit one, ...
    let mut engine = ReplayEngine::new(ReplayMode::Burst {
        count: 1,
        gap: Duration::ZERO,
    });
    for _ in 0..10 {
        assert!(engine.poll(0));
    }
}

#[test]
fn burst_independent_of_timestamp() {
    let mut engine = ReplayEngine::new(ReplayMode::Burst {
        count: 2,
        gap: Duration::from_secs(9999),
    });
    // timestamps are ignored by burst mode
    assert!(engine.poll(0));
    assert!(engine.poll(u64::MAX));
    assert!(!engine.poll(42)); // pause
}
