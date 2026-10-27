//! The UFO: now and then a ship of lights flies in, hovers, shines a beam
//! down to the horizon and leaves.
//!
//! The visits are scheduled from the seed of the scene, so a scene replays
//! the same visits for the same sequence of time steps. Sizes and positions
//! are given for the reference focal length and scale with the picture.

use std::f32::consts::{PI, TAU};

use crate::canvas::{Canvas, GLOW_HALO_RATIO, Rgb};
use crate::rng::hash01;
use crate::sky::SkyView;

/// Focal length that the sizes below are given for, in pixels.
const REF_FOCAL: f32 = 960.0;
/// Alphas below this are invisible and not worth a draw call.
const MIN_ALPHA: f32 = 0.004;
/// Radius below which a glow is not drawn smaller, in pixels at 100% scale.
/// Smaller lights are dimmed instead, as for the lights of the road.
const MIN_CORE_RADIUS: f32 = 0.9;

/// Separates the random numbers of the UFO from those of the other parts.
const STREAM_UFO: u32 = 0x50;
const PART_WAIT: u32 = 0;
const PART_FROM_LEFT: u32 = 1;
const PART_TO_RIGHT: u32 = 2;
const PART_HOVER: u32 = 3;
const PART_PARTICLE_LEVEL: u32 = 4;
const PART_PARTICLE_SIDE: u32 = 5;

/// Seconds from the start of a visit to its end, and the three stages.
pub const VISIT_SECONDS: f32 = 12.2;
const ARRIVE_END: f32 = 3.0;
const LEAVE_START: f32 = 11.0;
const FADE_IN: f32 = 0.6;
/// Where the ship starts outside the screen, in reference pixels.
const START_MARGIN: f32 = 160.0;
/// Height of the ship as a share of the horizon height, when it arrives and when it hovers.
const ARRIVE_HEIGHT: (f32, f32) = (0.12, 0.36);
const ARRIVE_TILT: f32 = 0.28;
/// Horizontal range of the hovering position, as shares of the width.
const HOVER_X: (f32, f32) = (0.38, 0.62);
const SWAY_X: f32 = 14.0;
const SWAY_X_PERIOD: f32 = 9.0;
const SWAY_Y: f32 = 6.0;
const SWAY_Y_PERIOD: f32 = 3.7;
const SWAY_TILT: f32 = 0.06;
const SWAY_TILT_PERIOD: f32 = 6.1;
/// How far the ship flies while leaving, as shares of the width and of the horizon height.
const LEAVE_REACH: (f32, f32) = (0.9, 0.8);
const LEAVE_TILT: f32 = 0.35;
const LEAVE_SHRINK: f32 = 0.65;

/// The beam is on between these times, and fades in and out for `BEAM_FADE`.
const BEAM_ON: (f32, f32) = (4.5, 9.5);
const BEAM_FADE: f32 = 0.6;
const BEAM_TOP_ALPHA: f32 = 0.20;
const BEAM_BOTTOM_ALPHA: f32 = 0.03;
const BEAM_TOP_WIDTH: f32 = 0.6;
const BEAM_BOTTOM_WIDTH: f32 = 1.9;
/// The beam ends this far above the horizon, in reference pixels.
const BEAM_BOTTOM_GAP: f32 = 10.0;
const PARTICLES: i64 = 28;
const PARTICLE_RISE: f32 = 0.25;
const PARTICLE_ALPHA: f32 = 0.55;
const PARTICLE_RADIUS: f32 = 1.4;

/// The curve lag fades out with this rate, in 1 / seconds (a time constant of 1.25 s).
const LAG_RATE: f64 = 0.8;

/// Size of the ship: the radius of its rim, and the base radius of a light.
const RADIUS: f32 = 74.0;
const DOT: f32 = 2.3;

const RIM_LIGHTS: usize = 18;
const RIM_SPIN: f32 = 1.8;
const RIM_FLATNESS: f32 = 0.26;
const RIM_ALPHA: f32 = 0.9;
const RIM_FAR_DIM: f32 = 0.35;
/// Every `CHASER_GAP` lights one is brighter; the pattern advances this many lights a second.
const CHASER_GAP: usize = 6;
const CHASER_SPEED: f32 = 9.0;
const CHASER_SIZE: f32 = 1.6;

const HULL_LIGHTS: usize = 26;
const HULL_WIDTH: f32 = 0.8;
const HULL_FLATNESS: f32 = 0.17;

const DOME_BASE: f32 = -10.0;
const DOME_WIDTH: f32 = 0.42;
const DOME_HEIGHT: f32 = 0.36;
const DOME_ARCS: [(f32, usize); 3] = [(1.0, 13), (0.68, 9), (0.36, 5)];
const BEACON_PERIOD: f32 = 0.5;

const UNDER_LIGHTS: usize = 3;
const UNDER_SPIN: f32 = -2.4;
const UNDER_PULSE_RATE: f32 = 1.1;

const CYAN: Rgb = Rgb::new(90, 255, 220);
const MAGENTA: Rgb = Rgb::new(255, 80, 210);
const GREEN: Rgb = Rgb::new(150, 255, 90);
const WHITE: Rgb = Rgb::new(225, 235, 255);
const RED: Rgb = Rgb::new(255, 50, 40);
const DOME_COLOR: Rgb = Rgb::new(170, 240, 255);
const RIM_COLORS: [Rgb; 3] = [CYAN, MAGENTA, GREEN];

/// Schedules the visits of the UFO and draws the one that is under way.
pub struct Ufo {
    seed: u64,
    /// `None` when the UFO is off.
    mean_interval: Option<f32>,
    /// Number of the visit that is under way or the next one.
    index: i64,
    /// When visit `index` starts, in seconds of scene time.
    start: f64,
    time: f64,
    heading: f64,
    /// Horizontal offset caused by turning, in focal lengths.
    lag: f32,
}

/// What is decided at random for one visit.
#[derive(Clone, Copy)]
struct Visit {
    from_left: bool,
    to_right: bool,
    /// Where the ship hovers, as a share of the width.
    hover: f32,
}

/// Where and how the ship is at one moment of a visit.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Pose {
    x: f32,
    y: f32,
    /// Radians, positive tips the right side down.
    tilt: f32,
    /// Size relative to the full size.
    scale: f32,
    alpha: f32,
}

impl Ufo {
    pub fn new(seed: u64, mean_interval: Option<f32>) -> Self {
        let mean_interval = mean_interval.filter(|mean| mean.is_finite() && *mean > 0.0);
        // Without a mean interval no visit is planned; `call` can still bring one.
        let start = mean_interval.map_or(f64::INFINITY, |mean| wait(seed, 0, mean));
        Self {
            seed,
            mean_interval,
            index: 0,
            start,
            time: 0.0,
            heading: 0.0,
            lag: 0.0,
        }
    }

    /// Whether a ship is in the sky now.
    pub fn is_visible(&self) -> bool {
        self.visit_time().is_some()
    }

    /// Brings a ship now, also where none is planned. Does nothing while a
    /// ship is in the sky.
    pub fn call(&mut self) {
        if self.is_visible() {
            return;
        }
        // A new index makes the visit another one than the planned visit it
        // replaces.
        self.index += 1;
        self.start = self.time;
    }

    /// Moves to `time` seconds after the start of the scene. `side` is `1.0`
    /// for left-hand traffic and `-1.0` for right-hand traffic (see `SkyView`).
    pub fn update(&mut self, time: f64, heading: f64, side: f32) {
        // The ship stays with the backdrop when the car turns, then catches up.
        let elapsed = (time - self.time).max(0.0);
        let turned = (heading - self.heading + std::f64::consts::PI)
            .rem_euclid(std::f64::consts::TAU)
            - std::f64::consts::PI;
        self.lag = self.lag * (-LAG_RATE * elapsed).exp() as f32 - side * turned as f32;
        self.time = time;
        self.heading = heading;
        // The wait for the next visit starts when the ship has gone.
        while time >= self.start + f64::from(VISIT_SECONDS) {
            self.index += 1;
            self.start = match self.mean_interval {
                Some(mean) => {
                    self.start + f64::from(VISIT_SECONDS) + wait(self.seed, self.index, mean)
                }
                None => f64::INFINITY,
            };
        }
    }

    /// Seconds since the start of the visit, while one is under way.
    fn visit_time(&self) -> Option<f32> {
        let t = self.time - self.start;
        (0.0..f64::from(VISIT_SECONDS))
            .contains(&t)
            .then_some(t as f32)
    }

    fn visit(&self) -> Visit {
        let roll = |part| hash01(self.seed, STREAM_UFO | part, self.index);
        Visit {
            from_left: roll(PART_FROM_LEFT) < 0.5,
            to_right: roll(PART_TO_RIGHT) < 0.5,
            hover: lerp(HOVER_X, roll(PART_HOVER)),
        }
    }

    pub fn draw(&self, view: &SkyView, canvas: &mut dyn Canvas) {
        let Some(t) = self.visit_time() else {
            return;
        };
        let Some(mut pose) = self.visit().pose(t, view) else {
            return;
        };
        pose.x += self.lag * view.focal;
        if pose.alpha < MIN_ALPHA {
            return;
        }
        let mut craft = Craft::new(view, canvas, pose);
        craft.draw_beam(self.seed, t);
        craft.draw_ship(t);
    }
}

/// Time to wait after the ship `index - 1` has gone, from an exponential
/// distribution, so that visits are random with the given mean.
fn wait(seed: u64, index: i64, mean: f32) -> f64 {
    let roll = f64::from(hash01(seed, STREAM_UFO | PART_WAIT, index));
    -f64::from(mean) * (1.0 - roll).ln()
}

fn lerp(range: (f32, f32), t: f32) -> f32 {
    range.0 + (range.1 - range.0) * t
}

fn smoothstep(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn direction(positive: bool) -> f32 {
    if positive { 1.0 } else { -1.0 }
}

impl Visit {
    /// The pose `t` seconds into the visit, or `None` when the ship is not there.
    fn pose(&self, t: f32, view: &SkyView) -> Option<Pose> {
        if !(0.0..VISIT_SECONDS).contains(&t) {
            return None;
        }
        let k = view.focal / REF_FOCAL;
        let horizon = view.horizon_y();
        let hover_x = view.width * self.hover;
        let sway = |t: f32| {
            let u = t - ARRIVE_END;
            Pose {
                x: hover_x + SWAY_X * k * (TAU * u / SWAY_X_PERIOD).sin(),
                y: horizon * ARRIVE_HEIGHT.1 + SWAY_Y * k * (TAU * u / SWAY_Y_PERIOD).sin(),
                tilt: SWAY_TILT * (TAU * u / SWAY_TILT_PERIOD).sin(),
                scale: 1.0,
                alpha: 1.0,
            }
        };
        let pose = if t < ARRIVE_END {
            let s = smoothstep(t / ARRIVE_END);
            let start_x = if self.from_left {
                -START_MARGIN * k
            } else {
                view.width + START_MARGIN * k
            };
            Pose {
                x: lerp((start_x, hover_x), s),
                y: horizon * lerp(ARRIVE_HEIGHT, s),
                tilt: ARRIVE_TILT * direction(self.from_left) * (1.0 - s),
                scale: 1.0,
                alpha: (t / FADE_IN).min(1.0),
            }
        } else if t < LEAVE_START {
            sway(t)
        } else {
            // The ship leaves from where it hovered, speeding up.
            let e = (t - LEAVE_START) / (VISIT_SECONDS - LEAVE_START);
            let base = sway(LEAVE_START);
            let out = direction(self.to_right);
            Pose {
                x: base.x + out * LEAVE_REACH.0 * view.width * e * e,
                y: base.y - LEAVE_REACH.1 * horizon * e * e,
                tilt: base.tilt + out * LEAVE_TILT * e,
                scale: 1.0 - LEAVE_SHRINK * e,
                alpha: 1.0 - e * e,
            }
        };
        Some(pose)
    }
}

/// How strongly the beam shines `t` seconds into the visit, `0..=1`.
fn beam_level(t: f32) -> f32 {
    ((t - BEAM_ON.0) / BEAM_FADE).clamp(0.0, 1.0) * ((BEAM_ON.1 - t) / BEAM_FADE).clamp(0.0, 1.0)
}

/// Draws the parts of the ship around its center.
struct Craft<'a> {
    view: &'a SkyView,
    canvas: &'a mut dyn Canvas,
    pose: Pose,
    cos: f32,
    sin: f32,
    /// Pixels per reference pixel, including the shrinking of a leaving ship.
    unit: f32,
}

impl<'a> Craft<'a> {
    fn new(view: &'a SkyView, canvas: &'a mut dyn Canvas, pose: Pose) -> Self {
        Self {
            view,
            canvas,
            pose,
            cos: pose.tilt.cos(),
            sin: pose.tilt.sin(),
            unit: view.focal / REF_FOCAL * pose.scale,
        }
    }

    /// Screen position of a point given relative to the center of the ship,
    /// in reference pixels, before tilting.
    fn place(&self, local_x: f32, local_y: f32) -> (f32, f32) {
        (
            self.pose.x + (local_x * self.cos - local_y * self.sin) * self.unit,
            self.pose.y + (local_x * self.sin + local_y * self.cos) * self.unit,
        )
    }

    /// A light at a point of the ship; `radius` is in reference pixels.
    fn light(&mut self, local_x: f32, local_y: f32, radius: f32, color: Rgb, alpha: f32) {
        let (x, y) = self.place(local_x, local_y);
        self.glow(x, y, radius * self.unit, color, alpha);
    }

    /// A light at a screen position; `natural` is its radius in pixels.
    fn glow(&mut self, x: f32, y: f32, natural: f32, color: Rgb, alpha: f32) {
        let minimum = MIN_CORE_RADIUS * self.view.pixel_scale;
        let radius = natural.max(minimum);
        let alpha = (alpha * self.pose.alpha * (natural / minimum).min(1.0)).min(1.0);
        let halo = radius * GLOW_HALO_RATIO;
        let visible = self.view.contains(x, y, halo);
        if alpha >= MIN_ALPHA && visible {
            self.canvas.glow(x, y, radius, color, alpha);
        }
    }

    fn draw_beam(&mut self, seed: u64, t: f32) {
        let level = beam_level(t);
        if level <= 0.0 {
            return;
        }
        let k = self.view.focal / REF_FOCAL;
        let (x, top) = self.place(0.0, 16.0);
        let bottom = self.view.horizon_y() - BEAM_BOTTOM_GAP * k;
        if bottom - top < 1.0 {
            return;
        }
        let top_half = BEAM_TOP_WIDTH * RADIUS * self.unit;
        let bottom_half = BEAM_BOTTOM_WIDTH * RADIUS * self.unit;
        let alpha = level * self.pose.alpha;
        self.canvas.beam(
            x,
            top,
            top_half,
            bottom,
            bottom_half,
            CYAN,
            BEAM_TOP_ALPHA * alpha,
            BEAM_BOTTOM_ALPHA * alpha,
        );
        // Specks of light rise along the beam toward the ship.
        for i in 0..PARTICLES {
            let start = hash01(seed, STREAM_UFO | PART_PARTICLE_LEVEL, i);
            let across = hash01(seed, STREAM_UFO | PART_PARTICLE_SIDE, i) * 2.0 - 1.0;
            let v = (start + t * PARTICLE_RISE).fract();
            let half = lerp((bottom_half, top_half), v);
            let y = lerp((bottom, top), v);
            // Fade in at the horizon and out at the ship.
            let fade = (PI * v).sin();
            self.glow(
                x + across * half * 0.9,
                y,
                PARTICLE_RADIUS * k,
                CYAN,
                PARTICLE_ALPHA * level * fade,
            );
        }
    }

    fn draw_ship(&mut self, t: f32) {
        self.draw_underside(t);
        self.draw_hull();
        self.draw_dome(t);
        self.draw_rim(t);
    }

    fn draw_rim(&mut self, t: f32) {
        let chaser = (t * CHASER_SPEED) as usize % RIM_LIGHTS;
        for i in 0..RIM_LIGHTS {
            let angle = TAU * i as f32 / RIM_LIGHTS as f32 + RIM_SPIN * t;
            let (sin, cos) = angle.sin_cos();
            // Lights on the far side of the rim (sin < 0) are dimmer.
            let mut alpha = if sin < 0.0 {
                RIM_ALPHA * RIM_FAR_DIM
            } else {
                RIM_ALPHA
            };
            let mut radius = DOT;
            if (i + RIM_LIGHTS - chaser).is_multiple_of(CHASER_GAP) {
                alpha = 1.0;
                radius *= CHASER_SIZE;
            }
            self.light(
                RADIUS * cos,
                RADIUS * RIM_FLATNESS * sin,
                radius,
                RIM_COLORS[i % RIM_COLORS.len()],
                alpha,
            );
        }
    }

    fn draw_hull(&mut self) {
        for i in 0..HULL_LIGHTS {
            let angle = TAU * i as f32 / HULL_LIGHTS as f32;
            let (sin, cos) = angle.sin_cos();
            let alpha = if sin < 0.0 { 0.5 } else { 0.2 };
            self.light(
                RADIUS * HULL_WIDTH * cos,
                RADIUS * HULL_FLATNESS * sin,
                DOT * 0.6,
                WHITE,
                alpha,
            );
        }
    }

    fn draw_dome(&mut self, t: f32) {
        for (size, count) in DOME_ARCS {
            for j in 0..count {
                let angle = PI * j as f32 / (count - 1) as f32;
                self.light(
                    size * RADIUS * DOME_WIDTH * angle.cos(),
                    DOME_BASE - size * RADIUS * DOME_HEIGHT * angle.sin(),
                    DOT * 0.65,
                    DOME_COLOR,
                    0.45,
                );
            }
        }
        // The red light on top blinks.
        if ((t / BEACON_PERIOD) as usize).is_multiple_of(2) {
            self.light(
                0.0,
                DOME_BASE - RADIUS * DOME_HEIGHT - 4.0,
                DOT * 1.2,
                RED,
                1.0,
            );
        }
    }

    fn draw_underside(&mut self, t: f32) {
        for j in 0..UNDER_LIGHTS {
            let angle = UNDER_SPIN * t + TAU * j as f32 / UNDER_LIGHTS as f32;
            let pulse = 0.55 + 0.35 * (TAU * UNDER_PULSE_RATE * t + j as f32 * 2.1).sin();
            self.light(
                RADIUS * 0.35 * angle.cos(),
                14.0 + RADIUS * 0.09 * angle.sin(),
                DOT * 1.1,
                MAGENTA,
                pulse,
            );
        }
        self.light(0.0, 34.0, 26.0, CYAN, 0.10);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view() -> SkyView {
        SkyView {
            width: 1920.0,
            height: 1080.0,
            pixel_scale: 1.0,
            focal: 960.0,
            side: 1.0,
            heading: 0.0,
            time: 0.0,
            span: None,
        }
    }

    #[derive(Default)]
    struct Counter {
        glows: usize,
        beams: usize,
        others: usize,
    }

    impl Canvas for Counter {
        fn glow(&mut self, _: f32, _: f32, _: f32, _: Rgb, _: f32) {
            self.glows += 1;
        }
        fn light_pool(&mut self, _: f32, _: f32, _: f32, _: f32, _: Rgb, _: f32) {
            self.others += 1;
        }
        fn dot(&mut self, _: f32, _: f32, _: f32, _: Rgb, _: f32) {
            self.others += 1;
        }
        fn sky_band(&mut self, _: f32, _: f32, _: f32, _: Rgb, _: f32) {
            self.others += 1;
        }
        fn streak(&mut self, _: (f32, f32), _: (f32, f32), _: f32, _: Rgb, _: f32) {
            self.others += 1;
        }
        fn silhouette(&mut self, _: &[(f32, f32)], _: Rgb, _: f32) {
            self.others += 1;
        }
        fn beam(&mut self, _: f32, _: f32, _: f32, _: f32, _: f32, _: Rgb, _: f32, _: f32) {
            self.beams += 1;
        }
    }

    fn visit(from_left: bool, to_right: bool) -> Visit {
        Visit {
            from_left,
            to_right,
            hover: 0.5,
        }
    }

    /// A UFO that is `t` seconds into its first visit.
    fn ufo_at(seed: u64, t: f32) -> Ufo {
        let mut ufo = Ufo::new(seed, Some(30.0));
        ufo.time = ufo.start + f64::from(t);
        ufo
    }

    fn count(ufo: &Ufo) -> Counter {
        let mut counter = Counter::default();
        ufo.draw(&view(), &mut counter);
        counter
    }

    /// Steps a UFO in frames of 0.05 s and returns the start of each visit.
    fn visit_starts(seed: u64, mean: Option<f32>, seconds: f64) -> Vec<f64> {
        let mut ufo = Ufo::new(seed, mean);
        let mut starts = Vec::new();
        let mut was_visible = false;
        let steps = (seconds / 0.05) as u64;
        for step in 1..=steps {
            let time = step as f64 * 0.05;
            ufo.update(time, 0.0, 1.0);
            let visible = ufo.is_visible();
            if visible && !was_visible {
                starts.push(time);
            }
            was_visible = visible;
        }
        starts
    }

    #[test]
    fn the_same_seed_gives_the_same_visits() {
        let first = visit_starts(7, Some(20.0), 600.0);
        assert!(first.len() >= 5, "{first:?}");
        assert_eq!(first, visit_starts(7, Some(20.0), 600.0));
        assert_ne!(first, visit_starts(8, Some(20.0), 600.0));
    }

    #[test]
    fn no_ufo_comes_when_it_is_off() {
        for mean in [
            None,
            Some(0.0),
            Some(-5.0),
            Some(f32::NAN),
            Some(f32::INFINITY),
        ] {
            assert!(visit_starts(3, mean, 3600.0).is_empty(), "{mean:?}");
        }
        let mut ufo = Ufo::new(3, None);
        ufo.update(1.0e6, 1.0, 1.0);
        assert!(!ufo.is_visible());
        assert_eq!(count(&ufo).glows, 0);
    }

    #[test]
    fn two_ships_are_never_in_the_sky_at_once() {
        for seed in 0..20 {
            let starts = visit_starts(seed, Some(5.0), 1200.0);
            assert!(starts.len() > 20, "{seed}: {}", starts.len());
            for pair in starts.windows(2) {
                assert!(
                    pair[1] - pair[0] >= f64::from(VISIT_SECONDS),
                    "{seed}: {pair:?}"
                );
            }
        }
    }

    #[test]
    fn the_waits_average_the_mean_interval() {
        let mean = 600.0;
        let mut total = 0.0;
        let mut samples = 0;
        for seed in 0..200 {
            for index in 0..50 {
                let w = wait(seed, index, mean);
                assert!(w.is_finite() && w >= 0.0, "{w}");
                total += w;
                samples += 1;
            }
        }
        let average = total / f64::from(samples);
        assert!((average - 600.0).abs() < 36.0, "{average}");
    }

    #[test]
    fn a_long_pause_skips_visits_without_looping_forever() {
        let mut ufo = Ufo::new(1, Some(10.0));
        ufo.update(1.0e7, 0.0, 1.0);
        assert!(ufo.index > 1000);
        assert!(ufo.start <= 1.0e7 + f64::from(VISIT_SECONDS) + 1000.0);
    }

    #[test]
    fn the_ship_starts_outside_the_screen_and_hovers_inside() {
        let view = view();
        for (from_left, to_right) in [(true, true), (true, false), (false, true), (false, false)] {
            let visit = visit(from_left, to_right);
            let arriving = visit.pose(0.0, &view).unwrap();
            assert_eq!(arriving.alpha, 0.0);
            let reach = (RADIUS + DOT * GLOW_HALO_RATIO) * view.focal / REF_FOCAL;
            assert!(
                arriving.x + reach < 0.0 || arriving.x - reach > view.width,
                "{arriving:?}"
            );
            let mut t = ARRIVE_END;
            while t < LEAVE_START {
                let pose = visit.pose(t, &view).unwrap();
                assert!(pose.x > 0.0 && pose.x < view.width, "{t}: {pose:?}");
                assert!(pose.y > 0.0 && pose.y < view.horizon_y(), "{t}: {pose:?}");
                assert_eq!(pose.alpha, 1.0);
                t += 0.1;
            }
        }
    }

    #[test]
    fn the_ship_moves_without_jumps_and_is_gone_after_the_visit() {
        let view = view();
        let visit = visit(true, false);
        let mut previous = visit.pose(0.0, &view).unwrap();
        let mut t = 0.01;
        while t < VISIT_SECONDS {
            let pose = visit.pose(t, &view).unwrap();
            assert!(
                (pose.x - previous.x).abs() < 40.0,
                "{t}: {previous:?} {pose:?}"
            );
            assert!(
                (pose.y - previous.y).abs() < 20.0,
                "{t}: {previous:?} {pose:?}"
            );
            assert!((0.0..=1.0).contains(&pose.alpha), "{t}: {pose:?}");
            previous = pose;
            t += 0.01;
        }
        // The ship has faded out and flown off toward the chosen side.
        let end = visit.pose(VISIT_SECONDS - 0.01, &view).unwrap();
        assert!(end.alpha < 0.05, "{end:?}");
        assert!(end.x < 0.0, "{end:?}");
        let right = Visit {
            to_right: true,
            ..visit
        }
        .pose(VISIT_SECONDS - 0.01, &view)
        .unwrap();
        assert!(right.x > view.width, "{right:?}");
        assert!(visit.pose(VISIT_SECONDS, &view).is_none());
        assert!(visit.pose(-0.1, &view).is_none());
        assert!(visit.pose(f32::NAN, &view).is_none());
    }

    #[test]
    fn drawing_follows_the_visit() {
        // Before and after a visit there is nothing to draw.
        let mut ufo = Ufo::new(5, Some(30.0));
        assert!(!ufo.is_visible());
        assert_eq!(count(&ufo).glows, 0);
        ufo.time = ufo.start + f64::from(VISIT_SECONDS) + 0.1;
        assert!(!ufo.is_visible());
        assert_eq!(count(&ufo).glows, 0);

        // Arriving: lights but no beam yet.
        let arriving = count(&ufo_at(5, 2.5));
        assert!(arriving.glows > 40, "{}", arriving.glows);
        assert_eq!(arriving.beams, 0);
        // Hovering with the beam down: the beam and its specks.
        let hovering = count(&ufo_at(5, 7.0));
        assert_eq!(hovering.beams, 1);
        assert!(hovering.glows > arriving.glows, "{}", hovering.glows);
        assert_eq!(hovering.others, 0);
        // Beam gone again before the ship leaves.
        assert_eq!(count(&ufo_at(5, 10.0)).beams, 0);
    }

    #[test]
    fn the_beam_fades_in_and_out() {
        assert_eq!(beam_level(4.4), 0.0);
        assert!(beam_level(4.8) > 0.0 && beam_level(4.8) < 1.0);
        assert_eq!(beam_level(7.0), 1.0);
        assert!(beam_level(9.2) > 0.0 && beam_level(9.2) < 1.0);
        assert_eq!(beam_level(9.6), 0.0);
    }

    #[test]
    fn the_lag_after_a_turn_fades_away() {
        let mut ufo = Ufo::new(1, Some(600.0));
        ufo.update(0.05, 0.1, 1.0);
        let first = ufo.lag;
        // The backdrop slides left for left-hand traffic when the heading grows.
        assert!(first < 0.0, "{first}");
        let mut previous = first.abs();
        for step in 2..=100 {
            ufo.update(0.05 * step as f64, 0.1, 1.0);
            assert!(ufo.lag.abs() < previous);
            previous = ufo.lag.abs();
        }
        assert!(ufo.lag.abs() < first.abs() * 0.02, "{}", ufo.lag);
        // The other side of the road mirrors the picture.
        let mut mirrored = Ufo::new(1, Some(600.0));
        mirrored.update(0.05, 0.1, -1.0);
        assert!((mirrored.lag + first).abs() < 1e-6);
    }

    #[test]
    fn the_lag_is_right_across_the_wrap_of_the_heading() {
        let mut ufo = Ufo::new(1, Some(600.0));
        ufo.update(0.05, TAU_F64 - 0.01, 1.0);
        let before = ufo.lag;
        ufo.update(0.05, 0.01, 1.0);
        // The heading grew by 0.02 across the wrap, not fell by about a turn.
        assert!(ufo.lag < before, "{before} {}", ufo.lag);
        assert!((ufo.lag - before).abs() < 0.05);
    }

    const TAU_F64: f64 = std::f64::consts::TAU;
}
