use super::math::interpolate_entity_states;
use super::*;
use crate::{EntityState, WorldSnapshot};

fn create_test_snapshot(tick: u32, time_ms: u64, entity_count: usize) -> WorldSnapshot {
    let mut snapshot = WorldSnapshot::new(tick, time_ms);
    for i in 0..entity_count {
        let mut state = EntityState::new(i as u32, 0);
        state.position = [tick as f32 * 10.0 + i as f32, 0.0, 0.0];
        snapshot.entities.push(state);
    }
    snapshot
}

#[test]
fn test_interpolation_engine_initialization() {
    let mut engine = InterpolationEngine::with_defaults();
    assert!(!engine.is_ready());

    engine.push_snapshot(create_test_snapshot(0, 0, 2));
    engine.push_snapshot(create_test_snapshot(1, 16, 2));
    engine.push_snapshot(create_test_snapshot(2, 32, 2));

    assert!(engine.is_ready());
}

#[test]
fn test_lerp_interpolation() {
    let mut from = EntityState::new(1, 0);
    from.position = [0.0, 0.0, 0.0];

    let mut to = EntityState::new(1, 0);
    to.position = [10.0, 20.0, 30.0];

    let result = interpolate_entity_states(&from, &to, 0.5);

    assert!((result.position.x - 5.0).abs() < 0.001);
    assert!((result.position.y - 10.0).abs() < 0.001);
    assert!((result.position.z - 15.0).abs() < 0.001);
}

#[test]
fn test_slerp_interpolation() {
    let mut from = EntityState::new(1, 0);
    from.encode_orientation([0.0, 0.0, 0.0, 1.0]);

    let mut to = EntityState::new(1, 0);
    let half_angle = std::f32::consts::FRAC_PI_4;
    to.encode_orientation([0.0, half_angle.sin(), 0.0, half_angle.cos()]);

    let result = interpolate_entity_states(&from, &to, 0.5);

    let expected_half = std::f32::consts::FRAC_PI_8;
    assert!((result.orientation.y - expected_half.sin()).abs() < 0.1);
}

#[test]
fn test_baseline_loss_deadlock() {
    let config = InterpolationConfig {
        max_buffer_snapshots: 5,
        ..Default::default()
    };
    let mut engine = InterpolationEngine::new(config);

    let s10 = create_test_snapshot(10, 1000, 1);
    engine.push_snapshot(s10.clone());
    assert!(engine.get_snapshot_by_tick(10).is_some());

    let mut s20 = create_test_snapshot(20, 2000, 1);
    s20.is_delta = true;
    s20.baseline_tick = 10;
    engine.push_snapshot(s20.clone());
    assert!(engine.get_snapshot_by_tick(20).is_some());

    for i in 1..=10 {
        let tick = 20 + i;
        let time = 2000 + i as u64 * 100;
        let mut s = create_test_snapshot(tick, time, 1);
        s.is_delta = true;
        s.baseline_tick = 20 + i - 1;
        engine.push_snapshot(s);
    }

    assert!(engine.get_snapshot_by_tick(10).is_none());

    let mut s_late = create_test_snapshot(50, 5000, 1);
    s_late.is_delta = true;
    s_late.baseline_tick = 10;

    engine.push_snapshot(s_late);

    assert!(engine.get_snapshot_by_tick(50).is_none());
}
