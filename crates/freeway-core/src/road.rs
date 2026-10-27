//! Shape of the road: horizontal bends and vertical hills.
//!
//! The road is described by two functions of the distance travelled: its
//! curvature (how fast the heading turns) and its grade change (how fast the
//! slope changes). Each is chosen per segment from a hash of the segment
//! number, so the road is endless, needs no storage and is the same for the
//! same seed.

use crate::projection::VIEW_DISTANCE;
use crate::rng::hash01;

/// Parameters of one kind of road variation.
#[derive(Clone, Copy, Debug)]
struct Profile {
    stream: u32,
    /// Length of a segment, in meters.
    segment_length: f64,
    /// Probability that a segment is dead straight (or flat).
    still_probability: f32,
    /// Range of the absolute target value of a moving segment.
    magnitude: (f32, f32),
    /// Range of the share of a segment spent easing in from the previous target.
    blend_share: (f32, f32),
}

const CURVATURE: Profile = Profile {
    stream: 1,
    segment_length: 300.0,
    still_probability: 0.38,
    magnitude: (0.001, 0.004),
    blend_share: (0.35, 0.40),
};

const GRADE_CHANGE: Profile = Profile {
    stream: 2,
    segment_length: 450.0,
    still_probability: 0.5,
    magnitude: (0.00004, 0.00016),
    blend_share: (0.40, 0.45),
};

impl Profile {
    fn target(&self, seed: u64, segment: i64) -> f32 {
        let hash = |part: u32| hash01(seed, self.stream << 2 | part, segment);
        if hash(0) < self.still_probability {
            return 0.0;
        }
        let sign = if hash(1) < 0.5 { -1.0 } else { 1.0 };
        sign * (self.magnitude.0 + hash(2) * (self.magnitude.1 - self.magnitude.0))
    }

    fn blend(&self, seed: u64, segment: i64) -> f32 {
        let fraction = hash01(seed, self.stream << 2 | 3, segment);
        self.blend_share.0 + fraction * (self.blend_share.1 - self.blend_share.0)
    }

    /// The value at distance `s`. It eases from the previous segment's
    /// target to this segment's target at the start of the segment and then
    /// holds, so it is continuous across segment borders.
    fn value_at(&self, seed: u64, s: f64) -> f32 {
        let segment = (s / self.segment_length).floor();
        let t = ((s - segment * self.segment_length) / self.segment_length) as f32;
        let segment = segment as i64;
        let progress = (t / self.blend(seed, segment)).clamp(0.0, 1.0);
        let eased = progress * progress * (3.0 - 2.0 * progress);
        let from = self.target(seed, segment - 1);
        let to = self.target(seed, segment);
        from + (to - from) * eased
    }
}

/// An endless road determined by a seed.
#[derive(Clone, Copy, Debug)]
pub struct RoadShape {
    seed: u64,
}

impl RoadShape {
    pub fn new(seed: u64) -> Self {
        Self { seed }
    }

    /// Curvature in radians per meter at distance `s` along the road.
    pub fn curvature_at(&self, s: f64) -> f32 {
        CURVATURE.value_at(self.seed, s)
    }

    /// Change of the slope per meter at distance `s` along the road.
    pub fn grade_change_at(&self, s: f64) -> f32 {
        GRADE_CHANGE.value_at(self.seed, s)
    }
}

const STEPS: usize = VIEW_DISTANCE as usize;

/// How the road ahead bends and rises, sampled every meter.
///
/// The camera always follows the road, so at distance 0 the offsets, the
/// heading and the slope are all zero.
#[derive(Clone, Debug)]
pub struct RoadView {
    lateral: Vec<f32>,
    height: Vec<f32>,
}

impl RoadView {
    /// Integrates curvature twice (heading, then lateral offset) and grade
    /// change twice (slope, then height) from `track_s` to `VIEW_DISTANCE`
    /// ahead.
    pub fn new(shape: &RoadShape, track_s: f64) -> Self {
        let mut lateral = Vec::with_capacity(STEPS + 1);
        let mut height = Vec::with_capacity(STEPS + 1);
        let (mut heading, mut x) = (0.0f64, 0.0f64);
        let (mut slope, mut y) = (0.0f64, 0.0f64);
        lateral.push(0.0);
        height.push(0.0);
        for step in 0..STEPS {
            let s = track_s + step as f64 + 0.5;
            let next_heading = heading + f64::from(shape.curvature_at(s));
            let next_slope = slope + f64::from(shape.grade_change_at(s));
            x += (heading + next_heading) * 0.5;
            y += (slope + next_slope) * 0.5;
            (heading, slope) = (next_heading, next_slope);
            lateral.push(x as f32);
            height.push(y as f32);
        }
        Self { lateral, height }
    }

    /// Lateral offset and height offset of the road `d` meters ahead.
    pub fn offset(&self, d: f32) -> (f32, f32) {
        let d = d.clamp(0.0, STEPS as f32);
        let index = (d as usize).min(STEPS - 1);
        let t = d - index as f32;
        let lerp = |values: &[f32]| values[index] + (values[index + 1] - values[index]) * t;
        (lerp(&self.lateral), lerp(&self.height))
    }

    /// A road without bends and hills.
    #[cfg(test)]
    pub(crate) fn flat() -> Self {
        Self {
            lateral: vec![0.0; STEPS + 1],
            height: vec![0.0; STEPS + 1],
        }
    }

    #[cfg(test)]
    fn lateral(&self) -> &[f32] {
        &self.lateral
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn curvature_and_grade_are_continuous_over_segment_borders() {
        let shape = RoadShape::new(99);
        for border in 1..200 {
            for (profile, at) in [
                (
                    CURVATURE,
                    RoadShape::curvature_at as fn(&RoadShape, f64) -> f32,
                ),
                (GRADE_CHANGE, RoadShape::grade_change_at),
            ] {
                let s = border as f64 * profile.segment_length;
                let (before, after) = (at(&shape, s - 1e-6), at(&shape, s + 1e-6));
                assert!(
                    (before - after).abs() < profile.magnitude.1 * 1e-3,
                    "border {border}: {before} vs {after}"
                );
            }
        }
    }

    #[test]
    fn values_stay_within_range() {
        let shape = RoadShape::new(5);
        for i in 0..5000 {
            let s = f64::from(i) * 7.3 - 10_000.0;
            assert!(shape.curvature_at(s).abs() <= CURVATURE.magnitude.1);
            assert!(shape.grade_change_at(s).abs() <= GRADE_CHANGE.magnitude.1);
        }
    }

    #[test]
    fn road_has_both_straight_and_curved_stretches() {
        let shape = RoadShape::new(11);
        let curvatures: Vec<f32> = (0..200)
            .map(|i| shape.curvature_at(f64::from(i) * 300.0 + 299.0))
            .collect();
        let straight = curvatures.iter().filter(|c| **c == 0.0).count();
        assert!((40..120).contains(&straight), "{straight}");
    }

    #[test]
    fn view_starts_at_zero_with_zero_heading() {
        let view = RoadView::new(&RoadShape::new(3), 12_345.6);
        assert_eq!(view.offset(0.0), (0.0, 0.0));
        let lateral = view.lateral();
        // Heading is the difference of neighbouring points; it starts at the
        // (zero) heading of the camera and changes only by the curvature.
        assert!((lateral[1] - lateral[0]).abs() < 0.01);
        for pair in lateral.windows(3) {
            let turn = (pair[2] - pair[1]) - (pair[1] - pair[0]);
            assert!(turn.abs() <= CURVATURE.magnitude.1 * 1.01, "{turn}");
        }
    }

    #[test]
    fn same_seed_and_distance_give_the_same_view() {
        let a = RoadView::new(&RoadShape::new(8), 777.7);
        let b = RoadView::new(&RoadShape::new(8), 777.7);
        let c = RoadView::new(&RoadShape::new(9), 777.7);
        assert_eq!(a.offset(300.0), b.offset(300.0));
        assert_ne!(a.offset(300.0), c.offset(300.0));
    }

    #[test]
    fn view_moves_smoothly_with_the_camera() {
        let shape = RoadShape::new(21);
        let before = RoadView::new(&shape, 5000.0);
        let after = RoadView::new(&shape, 5000.1);
        let (a, b) = (before.offset(300.0), after.offset(300.0));
        assert!((a.0 - b.0).abs() < 1.0 && (a.1 - b.1).abs() < 0.5);
    }

    #[test]
    fn offset_clamps_outside_the_table() {
        let view = RoadView::new(&RoadShape::new(1), 0.0);
        assert_eq!(view.offset(-5.0), view.offset(0.0));
        assert_eq!(view.offset(10_000.0), view.offset(VIEW_DISTANCE));
    }
}
