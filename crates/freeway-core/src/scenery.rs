//! Scenery zones: the road runs through mountains for a while, then across
//! plains, and so on.
//!
//! Zones follow each other along the distance travelled and alternate between
//! the two kinds. They are made one after another from the seed, so a zone is
//! the same at any speed: its length is in meters, 16 to 34 km, which is 10 to
//! 20 minutes at 100 km/h. Mountains rise and sink over the stretch around the
//! boundary between two zones, so that the horizon never changes abruptly.

use crate::rng::hash01;

/// Length of a zone, in meters.
pub const ZONE_LENGTH: (f64, f64) = (16_000.0, 34_000.0);
/// Mountains rise or sink over this many meters on each side of a boundary.
pub const BLEND_HALF: f64 = 500.0;

// The ID tells the random values of the zones apart from the other uses of
// the seed; it has nothing to do with cryptography.
const STREAM_ZONE: u32 = 0x70;
const PART_FIRST: u32 = 0;
const PART_LENGTH: u32 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ZoneKind {
    /// Mountains on the horizon, and plenty of tunnels.
    Mountains,
    /// No mountains and no tunnels; the lights of the city show.
    Plains,
}

/// One zone: the stretch of the road from `start` to `end`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Zone {
    pub kind: ZoneKind,
    pub start: f64,
    pub end: f64,
}

/// The zones of a road. Without `enabled` there is a single zone of
/// mountains that never ends, which is the road as it was before zones.
pub struct Zones {
    seed: u64,
    enabled: bool,
    first: ZoneKind,
    /// Where each zone made so far ends; the first zone starts at 0.
    ends: Vec<f64>,
}

impl Zones {
    pub fn new(seed: u64, enabled: bool) -> Self {
        let first = if hash01(seed, STREAM_ZONE | (PART_FIRST << 8), 0) < 0.5 {
            ZoneKind::Mountains
        } else {
            ZoneKind::Plains
        };
        Self {
            seed,
            enabled,
            first,
            ends: Vec::new(),
        }
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }

    /// The zone that the point `s` meters along the road is in.
    pub fn zone_at(&mut self, s: f64) -> Zone {
        if !self.enabled {
            return Zone {
                kind: ZoneKind::Mountains,
                start: f64::NEG_INFINITY,
                end: f64::INFINITY,
            };
        }
        let index = self.index_at(s);
        self.zone(index)
    }

    /// How much of the mountains is seen at `s`: 1 in mountains, 0 on plains,
    /// and smoothly in between around a boundary.
    pub fn mountain_level(&mut self, s: f64) -> f32 {
        if !self.enabled {
            return 1.0;
        }
        let index = self.index_at(s);
        let zone = self.zone(index);
        let level = |kind| {
            if kind == ZoneKind::Mountains {
                1.0
            } else {
                0.0
            }
        };
        // Position across a boundary, from 0 half a blend before it to 1
        // half a blend after it.
        let across = |boundary: f64| smoothstep((0.5 + (s - boundary) / (2.0 * BLEND_HALF)) as f32);
        let mix =
            |from: ZoneKind, to: ZoneKind, t: f32| level(from) + (level(to) - level(from)) * t;
        if zone.end - s < BLEND_HALF {
            let next = self.zone(index + 1);
            mix(zone.kind, next.kind, across(zone.end))
        } else if index > 0 && s - zone.start < BLEND_HALF {
            let previous = self.zone(index - 1);
            mix(previous.kind, zone.kind, across(zone.start))
        } else {
            level(zone.kind)
        }
    }

    /// The number of the zone `s` is in, making zones as far as needed. A
    /// point before the start is in the first zone.
    fn index_at(&mut self, s: f64) -> usize {
        // One more than needed, so that the zone after it can be looked at.
        while self.ends.last().is_none_or(|&end| end <= s + BLEND_HALF) {
            let index = self.ends.len() as i64;
            let start = self.ends.last().copied().unwrap_or(0.0);
            let roll = f64::from(hash01(self.seed, STREAM_ZONE | (PART_LENGTH << 8), index));
            self.ends
                .push(start + ZONE_LENGTH.0 + roll * (ZONE_LENGTH.1 - ZONE_LENGTH.0));
        }
        self.ends.partition_point(|&end| end <= s)
    }

    /// Zone number `index`, which must have been made by `index_at`.
    fn zone(&self, index: usize) -> Zone {
        let kind = if index.is_multiple_of(2) {
            self.first
        } else if self.first == ZoneKind::Mountains {
            ZoneKind::Plains
        } else {
            ZoneKind::Mountains
        };
        Zone {
            kind,
            start: if index == 0 {
                0.0
            } else {
                self.ends[index - 1]
            },
            end: self.ends[index],
        }
    }
}

/// Smooth rise from 0 at `t = 0` to 1 at `t = 1`.
fn smoothstep(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every zone of the first `meters` of the road.
    fn collect(seed: u64, meters: f64) -> Vec<Zone> {
        let mut zones = Zones::new(seed, true);
        let mut all = Vec::new();
        let mut s = 0.0;
        while s < meters {
            let zone = zones.zone_at(s);
            all.push(zone);
            s = zone.end;
        }
        all
    }

    #[test]
    fn zones_are_16_to_34_kilometers_and_alternate() {
        for seed in 0..10 {
            let zones = collect(seed, 5_000_000.0);
            assert!(zones.len() > 100);
            for zone in &zones {
                let length = zone.end - zone.start;
                assert!((16_000.0..=34_000.0).contains(&length), "{length}");
            }
            for pair in zones.windows(2) {
                assert_ne!(pair[0].kind, pair[1].kind);
                assert_eq!(pair[0].end, pair[1].start);
            }
            assert_eq!(zones[0].start, 0.0);
        }
    }

    #[test]
    fn both_kinds_come_first_for_some_seeds() {
        let firsts: Vec<ZoneKind> = (0..40).map(|seed| collect(seed, 1.0)[0].kind).collect();
        assert!(firsts.contains(&ZoneKind::Mountains));
        assert!(firsts.contains(&ZoneKind::Plains));
    }

    #[test]
    fn a_zone_lasts_ten_to_twenty_minutes_at_100_kmh() {
        let zones = collect(3, 5_000_000.0);
        let mean = zones.iter().map(|z| z.end - z.start).sum::<f64>() / zones.len() as f64;
        let minutes = mean / (100.0 / 3.6) / 60.0;
        assert!((14.0..16.0).contains(&minutes), "{minutes}");
    }

    #[test]
    fn the_same_seed_gives_the_same_zones_however_the_road_is_asked() {
        let mut ahead = Zones::new(7, true);
        // Asking far ahead first makes the same zones as asking in order.
        let far = ahead.zone_at(900_000.0);
        let mut in_order = Zones::new(7, true);
        for s in (0..90).map(|i| f64::from(i) * 10_000.0) {
            in_order.zone_at(s);
        }
        assert_eq!(in_order.zone_at(900_000.0), far);
        assert_eq!(collect(7, 900_000.0), collect(7, 900_000.0));
        assert_ne!(collect(7, 900_000.0), collect(8, 900_000.0));
    }

    #[test]
    fn the_level_is_one_in_mountains_and_zero_on_plains() {
        let mut zones = Zones::new(2, true);
        for zone in collect(2, 600_000.0) {
            let middle = (zone.start + zone.end) / 2.0;
            let expected = if zone.kind == ZoneKind::Mountains {
                1.0
            } else {
                0.0
            };
            assert_eq!(zones.mountain_level(middle), expected);
            // Just outside the blend at both ends.
            assert_eq!(
                zones.mountain_level(zone.start + BLEND_HALF + 1.0),
                expected
            );
            assert_eq!(zones.mountain_level(zone.end - BLEND_HALF - 1.0), expected);
        }
    }

    #[test]
    fn the_level_changes_smoothly_across_a_boundary() {
        let mut zones = Zones::new(4, true);
        let boundaries: Vec<f64> = collect(4, 300_000.0).iter().map(|z| z.end).collect();
        for boundary in boundaries {
            let mut previous = zones.mountain_level(boundary - 600.0);
            let mut changed = 0.0;
            let mut s = boundary - 600.0;
            while s < boundary + 600.0 {
                s += 1.0;
                let level = zones.mountain_level(s);
                assert!((0.0..=1.0).contains(&level));
                // The steepest part of a smoothstep over 1,000 m is 1.5 per 1,000 m.
                assert!((level - previous).abs() < 0.0016, "{boundary} {s}");
                changed += (level - previous).abs();
                previous = level;
            }
            // It goes all the way from one level to the other, without overshoot.
            assert!((changed - 1.0).abs() < 1e-3, "{changed}");
            // Halfway at the boundary itself.
            assert!((zones.mountain_level(boundary) - 0.5).abs() < 1e-6);
        }
    }

    #[test]
    fn disabled_zones_are_mountains_everywhere() {
        let mut zones = Zones::new(5, false);
        for s in [0.0, 1.0e3, 1.0e6, 1.0e9] {
            assert_eq!(zones.mountain_level(s), 1.0);
            assert_eq!(zones.zone_at(s).kind, ZoneKind::Mountains);
        }
        assert!(!zones.enabled());
    }

    #[test]
    fn hours_of_driving_make_few_zones() {
        // 160 km/h for 24 hours is about 3,800 km.
        let mut zones = Zones::new(9, true);
        let mut s = 0.0;
        while s < 160.0 / 3.6 * 86_400.0 {
            zones.mountain_level(s);
            s += 100.0;
        }
        assert!(zones.ends.len() < 400, "{}", zones.ends.len());
    }
}
