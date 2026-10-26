//! Distant mountain ranges on the horizon.
//!
//! Like the rest of the backdrop they stand on the cylinder around the car.
//! A few ranges are spread around it with plains in between. Each range is
//! a ridge line sampled at a fine step and is drawn as a dark silhouette
//! that hides the stars behind it, with slowly blinking red lights on a few
//! of its peaks.

use std::f64::consts::TAU;

use crate::canvas::{Canvas, GLOW_HALO_RATIO, Rgb};
use crate::rng::hash01;
use crate::sky::{REF_FOCAL, SkyView};

const SALT_MOUNTAIN: u32 = 0x40;
const PART_RANGES: u32 = 0;
const PART_WIDTH: u32 = 1;
const PART_GAP: u32 = 2;
const PART_START: u32 = 3;
const PART_PEAK: u32 = 4;
const PART_LIGHTS: u32 = 5;
const PART_LIGHT_PHASE: u32 = 6;
const PART_THIN: u32 = 7;
/// Value noise of one range: octave `n` of range `r` uses the lattice
/// points `r * RANGE_STRIDE + n * OCTAVE_STRIDE + i`.
const PART_NOISE: u32 = 8;
const RANGE_STRIDE: i64 = 64;
const OCTAVE_STRIDE: i64 = 16;

/// Number of ranges around the cylinder.
const RANGE_COUNT: (u32, u32) = (3, 5);
/// Width of one range, in degrees.
const WIDTH_DEGREES: (f32, f32) = (25.0, 70.0);
/// Ranges together cover at most this many degrees of the 360.
const MAX_COVERED_DEGREES: f32 = 180.0;
/// The plain between two ranges is at least this wide, in degrees.
const MIN_GAP_DEGREES: f32 = 15.0;
/// Height of the highest point of a range, in pixels at the reference focal length.
const PEAK_HEIGHT: (f32, f32) = (40.0, 110.0);
/// Distance between two samples of a ridge line, in degrees.
const STEP_DEGREES: f32 = 0.25;
/// How much of the width at each end the ridge takes to sink to nothing.
const EDGE_SHARE: f32 = 0.35;
/// Octaves of the ridge noise as (cycles across the range, weight). The
/// weights add up to 1, and the cycles stay below `OCTAVE_STRIDE`.
const OCTAVES: [(f32, f32); 3] = [(2.5, 0.55), (6.0, 0.3), (14.0, 0.15)];
/// The ridge never drops below this share of the envelope in the middle.
const RIDGE_FLOOR: f32 = 0.3;

/// Dark and slightly bluish, a touch lighter than the background so that the
/// ridge stands out against the glow of the horizon (#05070f).
const SILHOUETTE_COLOR: Rgb = Rgb::new(5, 7, 15);
/// The silhouette reaches this far below the horizon, in pixels at the
/// reference focal length: all the way to the end of the horizon glow, so
/// that the glow does not show under the foot of a range.
const FOOT_DEPTH: f32 = 24.0;

/// Red warning lights on some of the peaks: slower than the ones on the
/// city's towers.
const LIGHT_PERIOD: f64 = 2.5;
const LIGHT_ON: f64 = 0.6;
const LIGHT_ALPHA: f32 = 0.9;
const LIGHT_COLOR: Rgb = Rgb::new(255, 40, 30);
/// Core radius and the lift above the ridge, in pixels at the reference focal length.
const LIGHT_CORE: f32 = 1.5;
const LIGHT_LIFT: f32 = 3.0;
/// Share of the city lights that stay in front of a range.
const CITY_KEPT: f32 = 0.25;
/// Alphas below this are invisible and not worth a draw call.
const MIN_ALPHA: f32 = 0.004;

/// A warning light on a peak.
#[derive(Clone, Copy, Debug)]
struct PeakLight {
    /// Angle along the range from its start, in radians.
    offset: f64,
    /// Elevation in focal lengths, just above the ridge.
    elevation: f32,
    /// Blink phase, `0..1` of a period.
    phase: f32,
}

/// One range of mountains.
#[derive(Clone, Debug)]
pub struct Range {
    /// Angle around the cylinder where the range begins, in radians.
    start: f64,
    /// Angular width, in radians.
    width: f64,
    /// Height of the ridge in focal lengths at every `step` radians from
    /// the start. The first and last are 0.
    ridge: Vec<f32>,
    lights: Vec<PeakLight>,
}

impl Range {
    /// The angle where the range begins, in radians.
    pub fn start(&self) -> f64 {
        self.start
    }

    /// The angular width, in radians.
    pub fn width(&self) -> f64 {
        self.width
    }

    /// Heights of the ridge in focal lengths, from one end to the other.
    pub fn ridge(&self) -> &[f32] {
        &self.ridge
    }

    /// Number of warning lights.
    pub fn light_count(&self) -> usize {
        self.lights.len()
    }

    /// The angle between two samples of the ridge, in radians.
    fn step(&self) -> f64 {
        self.width / (self.ridge.len() - 1) as f64
    }

    /// Whether `turn` (`0..1` of a full turn around the cylinder) is in front of the range.
    fn contains_turn(&self, turn: f64) -> bool {
        (turn * TAU - self.start).rem_euclid(TAU) <= self.width
    }
}

pub struct Mountains {
    ranges: Vec<Range>,
    seed: u64,
}

impl Mountains {
    pub fn new(seed: u64) -> Self {
        let h = |part: u32, index: i64| hash01(seed, SALT_MOUNTAIN | (part << 8), index);
        let count =
            RANGE_COUNT.0 + (h(PART_RANGES, 0) * (RANGE_COUNT.1 - RANGE_COUNT.0 + 1) as f32) as u32;
        let count = count.min(RANGE_COUNT.1) as usize;

        // Ranges that would cover too much are all narrowed in the same
        // proportion above the minimum, so none gets narrower than that.
        let mut widths: Vec<f32> = (0..count)
            .map(|i| lerp(WIDTH_DEGREES, h(PART_WIDTH, i as i64)))
            .collect();
        let total: f32 = widths.iter().sum();
        if total > MAX_COVERED_DEGREES {
            let floor = WIDTH_DEGREES.0 * count as f32;
            let keep = (MAX_COVERED_DEGREES - floor) / (total - floor);
            for width in &mut widths {
                *width = WIDTH_DEGREES.0 + (*width - WIDTH_DEGREES.0) * keep;
            }
        }
        let covered: f32 = widths.iter().sum();

        // The rest of the circle is shared out among the plains.
        let shares: Vec<f32> = (0..count).map(|i| 0.3 + h(PART_GAP, i as i64)).collect();
        let share_total: f32 = shares.iter().sum();
        let spare = 360.0 - covered - MIN_GAP_DEGREES * count as f32;
        let mut start = f64::from(h(PART_START, 0)) * TAU;
        let mut ranges = Vec::with_capacity(count);
        for (i, (&width, &share)) in widths.iter().zip(&shares).enumerate() {
            let gap = MIN_GAP_DEGREES + spare * share / share_total;
            let peak = lerp(PEAK_HEIGHT, h(PART_PEAK, i as i64)) / REF_FOCAL;
            ranges.push(build_range(seed, i, start, width, peak));
            start += f64::from(width + gap).to_radians();
        }
        Self { ranges, seed }
    }

    pub fn ranges(&self) -> &[Range] {
        &self.ranges
    }

    /// Whether a range stands in the direction `angle` (radians around the cylinder).
    pub fn has_range_at(&self, angle: f64) -> bool {
        self.ranges.iter().any(|r| r.contains_turn(angle / TAU))
    }

    /// Whether the city light number `index` at `turn` (`0..1` of a full
    /// turn) is hidden: in front of a range only a few of them stay, like
    /// the lights of a small town at the foot of the mountains.
    pub fn thins_city_light(&self, turn: f32, index: i64) -> bool {
        self.ranges.iter().any(|r| r.contains_turn(f64::from(turn)))
            && hash01(self.seed, SALT_MOUNTAIN | (PART_THIN << 8), index) >= CITY_KEPT
    }

    /// Draws the silhouettes of the ranges that are in view.
    pub fn draw_silhouettes(&self, view: &SkyView, canvas: &mut dyn Canvas) {
        let foot = view.horizon_y() + FOOT_DEPTH * view.scale();
        for range in &self.ranges {
            for run in visible_runs(range, view) {
                let mut points: Vec<(f32, f32)> =
                    run.iter().map(|&(x, h)| (x, view.y(h))).collect();
                // Closed along the foot, which lies below the horizon.
                points.push((run[run.len() - 1].0, foot));
                points.push((run[0].0, foot));
                canvas.silhouette(&points, SILHOUETTE_COLOR);
            }
        }
    }

    /// Draws the warning lights on the peaks.
    pub fn draw_lights(&self, view: &SkyView, canvas: &mut dyn Canvas) {
        let k = view.scale();
        let core = (LIGHT_CORE * k).max(0.9 * view.pixel_scale);
        for range in &self.ranges {
            for light in &range.lights {
                let cycle = (view.time / LIGHT_PERIOD + f64::from(light.phase)).rem_euclid(1.0);
                let on = LIGHT_ON / LIGHT_PERIOD;
                if cycle >= on {
                    continue;
                }
                // It swells and fades smoothly instead of switching.
                let alpha = LIGHT_ALPHA * (cycle / on * std::f64::consts::PI).sin() as f32;
                let x = view.screen_x(range.start + light.offset);
                let y = view.y(light.elevation);
                if alpha >= MIN_ALPHA && view.contains(x, y, core * GLOW_HALO_RATIO) {
                    canvas.glow(x, y, core, LIGHT_COLOR, alpha);
                }
            }
        }
    }
}

fn lerp(range: (f32, f32), t: f32) -> f32 {
    range.0 + (range.1 - range.0) * t
}

/// Smooth rise from 0 at `t = 0` to 1 at `t = 1`.
fn smoothstep(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Value noise in `0..1` at `x` cycles along the range.
fn value_noise(seed: u64, range: usize, octave: usize, x: f32) -> f32 {
    let cell = x.floor();
    let lattice = |i: f32| {
        let index = range as i64 * RANGE_STRIDE + octave as i64 * OCTAVE_STRIDE + i as i64;
        hash01(seed, SALT_MOUNTAIN | (PART_NOISE << 8), index)
    };
    let t = smoothstep(x - cell);
    lattice(cell) + (lattice(cell + 1.0) - lattice(cell)) * t
}

fn build_range(seed: u64, index: usize, start: f64, width_degrees: f32, peak: f32) -> Range {
    let samples = (width_degrees / STEP_DEGREES).round() as usize + 1;
    let ridge: Vec<f32> = (0..samples)
        .map(|i| {
            let u = i as f32 / (samples - 1) as f32;
            // Sinks to nothing at both ends and is level in the middle.
            let envelope = smoothstep(u.min(1.0 - u) / EDGE_SHARE);
            let noise: f32 = OCTAVES
                .iter()
                .enumerate()
                .map(|(octave, &(cycles, weight))| {
                    weight * value_noise(seed, index, octave, u * cycles)
                })
                .sum();
            peak * envelope * lerp((RIDGE_FLOOR, 1.0), noise)
        })
        .collect();

    // Lights go on the highest peaks: the highest sample, and for some
    // ranges another that is well apart from it.
    let h = |part: u32, i: i64| hash01(seed, SALT_MOUNTAIN | (part << 8), index as i64 * 8 + i);
    let wanted = 1 + usize::from(h(PART_LIGHTS, 0) < 0.5);
    let apart = samples / 4;
    let mut chosen: Vec<usize> = Vec::new();
    for _ in 0..wanted {
        let best = (0..samples)
            .filter(|&i| chosen.iter().all(|&c| i.abs_diff(c) >= apart))
            .max_by(|&a, &b| ridge[a].total_cmp(&ridge[b]));
        if let Some(best) = best {
            chosen.push(best);
        }
    }
    let step = f64::from(width_degrees).to_radians() / (samples - 1) as f64;
    let lights = chosen
        .iter()
        .enumerate()
        .map(|(n, &i)| PeakLight {
            offset: i as f64 * step,
            elevation: ridge[i] + LIGHT_LIFT / REF_FOCAL,
            phase: h(PART_LIGHT_PHASE, n as i64),
        })
        .collect();
    Range {
        start,
        width: f64::from(width_degrees).to_radians(),
        ridge,
        lights,
    }
}

/// The parts of `range` that are on screen, as (x, height) points from left
/// to right. A part ends at the seam behind the car, where the wrapped
/// angle jumps; the seam is far outside the view, but the range straddling
/// it is split there all the same.
fn visible_runs(range: &Range, view: &SkyView) -> Vec<Vec<(f32, f32)>> {
    let step = range.step();
    let points: Vec<(f32, f32)> = range
        .ridge
        .iter()
        .enumerate()
        .map(|(i, &h)| (view.screen_x(range.start + i as f64 * step), h))
        .collect();
    let seam = std::f32::consts::PI * view.focal;
    let mut runs: Vec<&[(f32, f32)]> = Vec::new();
    let mut from = 0;
    for i in 1..points.len() {
        if (points[i].0 - points[i - 1].0).abs() > seam {
            runs.push(&points[from..i]);
            from = i;
        }
    }
    runs.push(&points[from..]);

    runs.into_iter()
        .filter_map(|run| {
            // The run is ordered by x, rising for left-hand traffic and
            // falling for right-hand traffic.
            let mut run: Vec<(f32, f32)> = run.to_vec();
            if run.first()?.0 > run.last()?.0 {
                run.reverse();
            }
            if run.iter().any(|p| !p.0.is_finite()) {
                return None;
            }
            // Keep the points on screen and one off screen on each side.
            let first = run.iter().rposition(|p| p.0 < 0.0).unwrap_or(0);
            let last = run
                .iter()
                .position(|p| p.0 > view.width)
                .unwrap_or(run.len() - 1);
            if run[last].0 < 0.0 || run[first].0 > view.width {
                return None;
            }
            let kept = run[first..=last].to_vec();
            (kept.len() >= 2).then_some(kept)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    enum Command {
        Silhouette(Vec<(f32, f32)>),
        Glow { x: f32, y: f32, alpha: f32 },
    }

    #[derive(Default)]
    struct Recorder(Vec<Command>);

    impl Canvas for Recorder {
        fn glow(&mut self, x: f32, y: f32, _: f32, _: Rgb, alpha: f32) {
            self.0.push(Command::Glow { x, y, alpha });
        }
        fn light_pool(&mut self, _: f32, _: f32, _: f32, _: f32, _: Rgb, _: f32) {
            unreachable!();
        }
        fn dot(&mut self, _: f32, _: f32, _: f32, _: Rgb, _: f32) {
            unreachable!();
        }
        fn sky_band(&mut self, _: f32, _: f32, _: f32, _: Rgb, _: f32) {
            unreachable!();
        }
        fn streak(&mut self, _: (f32, f32), _: (f32, f32), _: f32, _: Rgb, _: f32) {
            unreachable!();
        }
        fn silhouette(&mut self, points: &[(f32, f32)], _: Rgb) {
            self.0.push(Command::Silhouette(points.to_vec()));
        }
        fn beam(&mut self, _: f32, _: f32, _: f32, _: f32, _: f32, _: Rgb, _: f32, _: f32) {
            unreachable!();
        }
    }

    fn view(side: f32, heading: f64, time: f64) -> SkyView {
        SkyView {
            width: 1280.0,
            height: 720.0,
            pixel_scale: 1.0,
            focal: 640.0,
            side,
            heading,
            time,
        }
    }

    fn silhouettes(mountains: &Mountains, view: &SkyView) -> Vec<Vec<(f32, f32)>> {
        let mut recorder = Recorder::default();
        mountains.draw_silhouettes(view, &mut recorder);
        recorder
            .0
            .into_iter()
            .filter_map(|c| match c {
                Command::Silhouette(points) => Some(points),
                Command::Glow { .. } => None,
            })
            .collect()
    }

    fn lights(mountains: &Mountains, view: &SkyView) -> Vec<(f32, f32, f32)> {
        let mut recorder = Recorder::default();
        mountains.draw_lights(view, &mut recorder);
        recorder
            .0
            .into_iter()
            .filter_map(|c| match c {
                Command::Glow { x, y, alpha } => Some((x, y, alpha)),
                Command::Silhouette(_) => None,
            })
            .collect()
    }

    /// The heading that puts the middle of `range` at the center of the screen.
    fn facing(range: &Range) -> f64 {
        range.start + range.width * 0.5
    }

    #[test]
    fn the_same_seed_gives_the_same_shape() {
        let shape = |seed| {
            Mountains::new(seed)
                .ranges()
                .iter()
                .map(|r| (r.start, r.width, r.ridge.clone()))
                .collect::<Vec<_>>()
        };
        assert_eq!(shape(5), shape(5));
        assert_ne!(shape(5), shape(6));
    }

    #[test]
    fn counts_widths_and_heights_follow_the_design() {
        let mut counts = std::collections::BTreeSet::new();
        for seed in 0..300 {
            let mountains = Mountains::new(seed);
            let ranges = mountains.ranges();
            counts.insert(ranges.len());
            assert!((3..=5).contains(&ranges.len()), "{seed}");
            let mut covered = 0.0;
            for range in ranges {
                let degrees = range.width.to_degrees();
                assert!((24.99..=70.01).contains(&degrees), "{seed}: {degrees}");
                covered += degrees;
                let highest = range.ridge.iter().copied().fold(0.0, f32::max);
                assert!(highest <= 110.0 / REF_FOCAL + 1e-6, "{seed}: {highest}");
                assert!(highest > 0.0);
                assert!(range.ridge.iter().all(|h| h.is_finite() && *h >= 0.0));
            }
            assert!(covered <= 180.01, "{seed}: {covered}");
        }
        // Every number of ranges comes up.
        assert_eq!(counts.len(), 3);
    }

    #[test]
    fn the_peaks_differ_from_range_to_range() {
        let highest = |seed| {
            Mountains::new(seed)
                .ranges()
                .iter()
                .map(|r| r.ridge.iter().copied().fold(0.0, f32::max) * REF_FOCAL)
                .collect::<Vec<_>>()
        };
        let all: Vec<f32> = (0..50).flat_map(highest).collect();
        assert!(all.iter().any(|&h| h < 70.0), "no low range");
        assert!(all.iter().any(|&h| h > 85.0), "no high range");
    }

    #[test]
    fn ranges_do_not_overlap_and_leave_plains() {
        for seed in 0..300 {
            let ranges = Mountains::new(seed).ranges().to_vec();
            let n = ranges.len();
            for (i, range) in ranges.iter().enumerate() {
                let next = &ranges[(i + 1) % n];
                // The plain from the end of this range to the start of the next.
                let gap = (next.start - (range.start + range.width)).rem_euclid(TAU);
                assert!(
                    gap.to_degrees() >= MIN_GAP_DEGREES as f64 - 0.01,
                    "{seed}: {}",
                    gap.to_degrees()
                );
            }
            let covered: f64 = ranges.iter().map(|r| r.width).sum();
            assert!(covered <= TAU / 2.0 + 1e-6);
        }
    }

    #[test]
    fn a_range_sinks_to_nothing_at_both_ends() {
        for seed in 0..50 {
            for range in Mountains::new(seed).ranges() {
                assert_eq!(range.ridge[0], 0.0);
                assert_eq!(*range.ridge.last().unwrap(), 0.0);
                // And it is smooth: no sample jumps by more than a few pixels.
                for pair in range.ridge.windows(2) {
                    assert!((pair[1] - pair[0]).abs() * REF_FOCAL < 8.0);
                }
            }
        }
    }

    #[test]
    fn the_ridge_is_sampled_every_quarter_degree() {
        let mountains = Mountains::new(1);
        for range in mountains.ranges() {
            let step = range.step().to_degrees();
            assert!((step - 0.25).abs() < 0.01, "{step}");
        }
    }

    #[test]
    fn one_or_two_lights_stand_on_the_highest_peaks() {
        let mut seen = std::collections::BTreeSet::new();
        for seed in 0..100 {
            for range in Mountains::new(seed).ranges() {
                seen.insert(range.light_count());
                assert!((1..=2).contains(&range.light_count()));
                for light in &range.lights {
                    assert!(light.elevation > 0.0);
                    assert!((0.0..1.0).contains(&light.phase));
                    assert!(light.offset > 0.0 && light.offset < range.width);
                }
            }
        }
        assert_eq!(seen.len(), 2);
    }

    #[test]
    fn facing_a_range_draws_its_silhouette() {
        let mountains = Mountains::new(3);
        let range = &mountains.ranges()[0];
        let drawn = silhouettes(&mountains, &view(1.0, facing(range), 0.0));
        assert!(!drawn.is_empty());
        let v = view(1.0, facing(range), 0.0);
        for points in &drawn {
            assert!(points.len() >= 4);
            // The ridge is above the horizon, the foot below it.
            let (ridge, foot) = points.split_at(points.len() - 2);
            assert!(ridge.iter().all(|p| p.1 <= v.horizon_y() + 1e-3));
            assert!(ridge.iter().any(|p| p.1 < v.horizon_y() - 5.0));
            assert!(foot.iter().all(|p| p.1 > v.horizon_y()));
        }
    }

    #[test]
    fn nothing_is_drawn_facing_a_plain() {
        let mut plains = 0;
        for seed in 0..100 {
            let mountains = Mountains::new(seed);
            let ranges = mountains.ranges();
            for (i, range) in ranges.iter().enumerate() {
                let end = range.start + range.width;
                let gap = (ranges[(i + 1) % ranges.len()].start - end).rem_euclid(TAU);
                // The narrow view spans 0.625 radians; a plain wider than that fills it.
                if gap < 0.7 {
                    continue;
                }
                plains += 1;
                for time in [0.0, 0.5, 1.0, 2.0] {
                    let v = SkyView {
                        width: 400.0,
                        ..view(1.0, end + gap * 0.5, time)
                    };
                    assert!(silhouettes(&mountains, &v).is_empty(), "{seed}");
                    assert!(lights(&mountains, &v).is_empty(), "{seed}");
                }
            }
        }
        assert!(plains > 100, "{plains}");
    }

    #[test]
    fn polygons_are_finite_and_never_wider_than_the_screen_plus_a_step() {
        for seed in 0..40 {
            let mountains = Mountains::new(seed);
            for side in [1.0, -1.0] {
                // A full turn in 1 degree steps, which crosses every seam.
                for degrees in 0..360 {
                    let v = view(side, f64::from(degrees).to_radians(), 0.0);
                    let step_px = STEP_DEGREES.to_radians() * v.focal;
                    for points in silhouettes(&mountains, &v) {
                        assert!(points.iter().all(|p| p.0.is_finite() && p.1.is_finite()));
                        let left = points.iter().map(|p| p.0).fold(f32::MAX, f32::min);
                        let right = points.iter().map(|p| p.0).fold(f32::MIN, f32::max);
                        assert!(
                            right - left <= v.width + 2.0 * step_px + 1e-3,
                            "{seed} {degrees}: {left} {right}"
                        );
                        // Left to right along the ridge, then back along the foot.
                        let ridge = &points[..points.len() - 2];
                        assert!(ridge.windows(2).all(|w| w[0].0 <= w[1].0));
                    }
                }
            }
        }
    }

    #[test]
    fn the_silhouette_follows_the_backdrop_when_the_car_turns() {
        let mountains = Mountains::new(2);
        let range = &mountains.ranges()[0];
        let before = silhouettes(&mountains, &view(1.0, facing(range), 0.0));
        let after = silhouettes(&mountains, &view(1.0, facing(range) + 0.05, 0.0));
        let min_x = |polygons: &[Vec<(f32, f32)>]| {
            polygons
                .iter()
                .flatten()
                .map(|p| p.0)
                .fold(f32::MAX, f32::min)
        };
        // The backdrop slides left when the heading grows (left-hand traffic).
        assert!(min_x(&after) < min_x(&before));
    }

    #[test]
    fn the_lights_blink_slowly_and_out_of_step() {
        let mountains = Mountains::new(7);
        let range = &mountains.ranges()[0];
        let heading = facing(range);
        let mut on_steps = 0;
        for step in 0..500 {
            let time = f64::from(step) * 0.01;
            let drawn = lights(&mountains, &view(1.0, heading, time));
            if !drawn.is_empty() {
                on_steps += 1;
            }
            assert!(drawn.iter().all(|l| l.2 > 0.0 && l.2 <= 1.0));
        }
        // Each light is on for 0.6 s of a 2.5 s period, so over 5 s (two
        // periods) some light is on for at least 1.2 s but never for all of it.
        assert!(on_steps >= 100, "{on_steps}");
        assert!(on_steps < 500, "{on_steps}");
    }

    #[test]
    fn a_light_sits_just_above_the_ridge_on_screen() {
        let mountains = Mountains::new(7);
        let range = &mountains.ranges()[0];
        let v = view(1.0, facing(range), 0.0);
        let mut found = false;
        for step in 0..250 {
            let v = SkyView {
                time: f64::from(step) * 0.01,
                ..v
            };
            for (_, y, _) in lights(&mountains, &v) {
                found = true;
                assert!(y < v.horizon_y());
            }
        }
        assert!(found);
    }

    #[test]
    fn a_range_is_found_by_its_direction() {
        let mountains = Mountains::new(9);
        let range = &mountains.ranges()[0];
        assert!(mountains.has_range_at(range.start() + range.width() * 0.5));
        assert!(mountains.has_range_at(range.start() + range.width() * 0.5 + TAU));
        assert!(mountains.has_range_at(range.start() + range.width() * 0.5 - TAU));
        // The middle of the plain after the range.
        let next = mountains.ranges()[1].start();
        let end = range.start() + range.width();
        assert!(!mountains.has_range_at(end + (next - end).rem_euclid(TAU) * 0.5));
    }

    #[test]
    fn city_lights_in_front_of_a_range_are_thinned_to_a_quarter() {
        let mountains = Mountains::new(9);
        let range = &mountains.ranges()[0];
        let turn = ((range.start + range.width * 0.5) / TAU).rem_euclid(1.0) as f32;
        let thinned = (0..2000)
            .filter(|&i| mountains.thins_city_light(turn, i))
            .count();
        assert!((1400..1600).contains(&thinned), "{thinned}");
        // A plain keeps them all.
        let plain = ((range.start + range.width + 0.05) / TAU).rem_euclid(1.0) as f32;
        let next = &mountains.ranges()[1];
        if (next.start - (range.start + range.width)).rem_euclid(TAU) > 0.1 {
            assert!((0..200).all(|i| !mountains.thins_city_light(plain, i)));
        }
    }
}
