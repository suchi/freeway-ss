//! The cruising speed that drifts slowly around the speed the user chose.
//!
//! The speed is the chosen one plus an offset. Every few minutes a new target
//! offset is picked from the seed, and the offset follows it like a critically
//! damped spring, so that the car never accelerates hard and the speed has no
//! kinks. With the drift off, the offset is always 0.

use crate::rng::hash01;
use crate::settings::{MAX_SPEED_KMH, MIN_SPEED_KMH};

/// The speed stays within this many km/h of the chosen one.
pub const MAX_OFFSET_KMH: f32 = 20.0;
/// A new target offset is picked after a time in this range, in seconds.
const RETARGET_SECONDS: (f32, f32) = (120.0, 300.0);
/// Time constant of the follow, in seconds. A change of 40 km/h takes about
/// two minutes to settle.
const FOLLOW_SECONDS: f32 = 30.0;
/// The most the speed changes per second, in km/h per second: 0.3 m/s².
pub const MAX_ACCELERATION_KMH_PER_S: f32 = 0.3 * 3.6;

// The ID tells the random values of the drift apart from the other uses of
// the seed; it has nothing to do with cryptography.
const STREAM_DRIFT: u32 = 0x80;
const PART_TARGET: i64 = 0;
const PART_INTERVAL: i64 = 1;

/// The offset from the chosen speed.
#[derive(Clone, Copy, Debug)]
pub struct Drift {
    seed: u64,
    enabled: bool,
    /// The offsets the speed may take, in km/h: at most `MAX_OFFSET_KMH` to
    /// either side, and never out of the range of the settings.
    lowest: f32,
    highest: f32,
    /// Offset and its rate of change, in km/h and km/h per second.
    offset: f32,
    rate: f32,
    target: f32,
    /// Number of targets picked so far, minus one.
    index: i64,
    until_retarget: f32,
}

impl Drift {
    /// A drift around `base_kmh`; without `enabled` the offset stays 0.
    pub fn new(seed: u64, base_kmh: f32, enabled: bool) -> Self {
        let lowest = (MIN_SPEED_KMH as f32 - base_kmh).clamp(-MAX_OFFSET_KMH, 0.0);
        let highest = (MAX_SPEED_KMH as f32 - base_kmh).clamp(0.0, MAX_OFFSET_KMH);
        let mut drift = Self {
            seed,
            enabled,
            lowest,
            highest,
            offset: 0.0,
            rate: 0.0,
            target: 0.0,
            index: 0,
            until_retarget: 0.0,
        };
        if enabled {
            drift.target = drift.pick_target(0);
            drift.until_retarget = drift.interval(0);
        }
        drift
    }

    /// The offset now, in km/h.
    pub fn offset(&self) -> f32 {
        self.offset
    }

    fn roll(&self, part: i64, index: i64) -> f32 {
        hash01(self.seed, STREAM_DRIFT, index * 2 + part)
    }

    fn pick_target(&self, index: i64) -> f32 {
        self.lowest + (self.highest - self.lowest) * self.roll(PART_TARGET, index)
    }

    fn interval(&self, index: i64) -> f32 {
        RETARGET_SECONDS.0
            + (RETARGET_SECONDS.1 - RETARGET_SECONDS.0) * self.roll(PART_INTERVAL, index)
    }

    /// Moves the offset for `dt` seconds.
    pub fn update(&mut self, dt: f32) {
        if !self.enabled || !(dt.is_finite() && dt > 0.0) {
            return;
        }
        self.until_retarget -= dt;
        if self.until_retarget <= 0.0 {
            self.index += 1;
            self.target = self.pick_target(self.index);
            self.until_retarget += self.interval(self.index);
        }
        // A critically damped spring: the rate follows the distance to the
        // target and is damped, so it grows and fades without a jump.
        let omega = 1.0 / FOLLOW_SECONDS;
        self.rate += (omega * omega * (self.target - self.offset) - 2.0 * omega * self.rate) * dt;
        self.rate = self
            .rate
            .clamp(-MAX_ACCELERATION_KMH_PER_S, MAX_ACCELERATION_KMH_PER_S);
        self.offset += self.rate * dt;
        if !(self.lowest..=self.highest).contains(&self.offset) {
            self.offset = self.offset.clamp(self.lowest, self.highest);
            self.rate = 0.0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DT: f32 = 0.05;

    /// The offset after every step of `minutes` minutes.
    fn offsets(drift: &mut Drift, minutes: f32) -> Vec<f32> {
        (0..(minutes * 60.0 / DT) as usize)
            .map(|_| {
                drift.update(DT);
                drift.offset()
            })
            .collect()
    }

    #[test]
    fn a_disabled_drift_never_moves() {
        let mut drift = Drift::new(1, 100.0, false);
        assert!(offsets(&mut drift, 30.0).iter().all(|&o| o == 0.0));
    }

    #[test]
    fn the_offset_stays_within_twenty_and_the_speeds_within_the_range() {
        for base in [30.0, 50.0, 100.0, 150.0, 160.0] {
            for seed in 0..6 {
                let mut drift = Drift::new(seed, base, true);
                for offset in offsets(&mut drift, 60.0) {
                    assert!(offset.abs() <= MAX_OFFSET_KMH, "{offset}");
                    let speed = base + offset;
                    assert!((30.0..=160.0).contains(&speed), "{base} {speed}");
                }
            }
        }
    }

    #[test]
    fn the_acceleration_stays_below_the_limit() {
        for seed in 0..6 {
            let mut drift = Drift::new(seed, 100.0, true);
            let mut previous = drift.offset();
            for offset in offsets(&mut drift, 60.0) {
                let acceleration = (offset - previous).abs() / DT / 3.6;
                assert!(acceleration <= 0.3 + 1e-3, "{acceleration}");
                previous = offset;
            }
        }
    }

    #[test]
    fn the_speed_really_changes_over_tens_of_minutes() {
        let mut drift = Drift::new(3, 100.0, true);
        let all = offsets(&mut drift, 30.0);
        let lowest = all.iter().copied().fold(f32::MAX, f32::min);
        let highest = all.iter().copied().fold(f32::MIN, f32::max);
        assert!(highest - lowest > 10.0, "{lowest} {highest}");
    }

    #[test]
    fn the_speed_changes_slowly() {
        // Over any ten seconds the speed changes by less than 6 km/h, so it
        // takes minutes to cross the whole range of 40 km/h.
        let mut drift = Drift::new(5, 100.0, true);
        let all = offsets(&mut drift, 60.0);
        let window = (10.0 / DT) as usize;
        for pair in all.windows(window + 1) {
            assert!((pair[window] - pair[0]).abs() < 6.0);
        }
    }

    #[test]
    fn the_same_seed_gives_the_same_drift() {
        let run = |seed| offsets(&mut Drift::new(seed, 100.0, true), 10.0);
        assert_eq!(run(4), run(4));
        assert_ne!(run(4), run(5));
    }

    #[test]
    fn a_bad_step_does_not_move_the_offset() {
        let mut drift = Drift::new(1, 100.0, true);
        for dt in [f32::NAN, f32::INFINITY, -1.0, 0.0] {
            drift.update(dt);
            assert_eq!(drift.offset(), 0.0);
        }
    }

    #[test]
    fn targets_are_picked_every_two_to_five_minutes() {
        let drift = Drift::new(2, 100.0, true);
        for index in 0..200 {
            let interval = drift.interval(index);
            assert!((120.0..300.0).contains(&interval), "{interval}");
        }
    }
}
