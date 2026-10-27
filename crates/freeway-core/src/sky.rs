//! The night sky and the distant horizon: stars, city lights, the glow of the
//! horizon, shooting stars and, when enabled, mountains.
//!
//! The backdrop is a cylinder around the car. Everything on it is stored as
//! an angle around the cylinder and an elevation above the horizon in units
//! of the focal length, so the picture looks the same at any resolution and
//! turns with the heading of the car.

use std::f64::consts::{PI, TAU};

use crate::canvas::{Canvas, Clip, GLOW_HALO_RATIO, Rgb};
use crate::mountains::Mountains;
use crate::rng::hash01;

/// Focal length that the sizes below are given for, in pixels.
pub(crate) const REF_FOCAL: f32 = 960.0;
/// Alphas below this are invisible and not worth a draw call.
const MIN_ALPHA: f32 = 0.004;

/// Stars fill the sky from this elevation up to `STAR_TOP`, in focal lengths.
const STAR_BOTTOM: f32 = 70.0 / REF_FOCAL;
/// Higher than the top edge of any supported viewport (about 0.91 focal lengths).
const STAR_TOP: f32 = 0.92;
/// Stars per square focal length: about 450 on a 1920 x 1080 screen.
const STAR_DENSITY: f32 = 451.0;
const LARGE_STAR_SHARE: f32 = 0.03;
const TWINKLE_PERIOD: f64 = 2.7;
const STAR_COLOR: Rgb = Rgb::new(200, 215, 255);

const CITY_COUNT: usize = 900;
/// City lights stand between these elevations, in pixels at the reference focal length.
const CITY_LOW: f32 = 3.0;
const CITY_HIGH: f32 = 41.0;
const BEACON_PERIOD: f64 = 1.5;
const BEACON_ON: f64 = 0.2;
const BEACON_COLOR: Rgb = Rgb::new(255, 40, 30);
const CITY_COLORS: [Rgb; 3] = [
    Rgb::new(255, 205, 110),
    Rgb::new(255, 150, 70),
    Rgb::new(200, 220, 255),
];
const CITY_GLOW_COLOR: Rgb = Rgb::new(255, 210, 120);

/// The orange glow of the horizon, in pixels at the reference focal length
/// relative to the horizon (negative is above it).
const BAND_TOP: f32 = -170.0;
const BAND_PEAK: f32 = -20.0;
pub(crate) const BAND_BOTTOM: f32 = 24.0;
const BAND_ALPHA: f32 = 0.14;
const BAND_COLOR: Rgb = Rgb::new(255, 130, 60);

const METEOR_GAP: (f32, f32) = (15.0, 45.0);
const METEOR_LIFE: (f32, f32) = (0.5, 1.2);
/// Speed and length in pixels (per second) at the reference focal length.
const METEOR_SPEED: (f32, f32) = (700.0, 1300.0);
const METEOR_LENGTH: (f32, f32) = (90.0, 210.0);
/// Angle below the horizontal, in degrees.
const METEOR_SLOPE: (f32, f32) = (20.0, 50.0);
/// Meteors start in this elevation range and are cut short before they get
/// lower than `METEOR_FLOOR`, in focal lengths.
const METEOR_START: (f32, f32) = (0.25, 0.6);
const METEOR_FLOOR: f32 = 40.0 / REF_FOCAL;
/// Meteors appear within this many radians of the direction the car faces.
const METEOR_SPREAD: f32 = 1.6;
const METEOR_COLOR: Rgb = Rgb::new(230, 240, 255);

/// What the sky needs to know to draw itself.
#[derive(Clone, Copy, Debug)]
pub struct SkyView {
    pub width: f32,
    pub height: f32,
    pub pixel_scale: f32,
    pub focal: f32,
    /// `1.0` for left-hand traffic, `-1.0` for right-hand traffic, which
    /// mirrors the picture (see `Projector::project`).
    pub side: f32,
    /// How far the car has turned, in radians. Positive is toward the side
    /// the road bends for positive curvature.
    pub heading: f64,
    /// Seconds since the start.
    pub time: f64,
    /// Set where the picture is wider than one window's, as when the monitors
    /// are drawn as one screen; `None` is one picture with its vanishing
    /// point in the middle of `width` by `height`, all of which is drawn.
    pub span: Option<SkySpan>,
}

/// Where the vanishing point is, and which part is drawn, in a `SkyView`
/// whose picture is larger than the part.
#[derive(Clone, Copy, Debug)]
pub struct SkySpan {
    pub center: (f32, f32),
    pub clip: Clip,
}

impl SkyView {
    /// Screen x of the backdrop at `angle` radians around the cylinder.
    ///
    /// Turning the car by a positive heading moves the backdrop the opposite
    /// way to the bend of the road: for left-hand traffic the road bends to
    /// the right of the screen and the backdrop slides left.
    pub fn screen_x(&self, angle: f64) -> f32 {
        let wrapped = (angle - self.heading + PI).rem_euclid(TAU) - PI;
        self.center().0 + self.side * wrapped as f32 * self.focal
    }

    /// The vanishing point.
    fn center(&self) -> (f32, f32) {
        self.span
            .map_or((self.width * 0.5, self.height * 0.5), |span| span.center)
    }

    /// The part of the picture that is drawn.
    pub(crate) fn clip(&self) -> Clip {
        self.span
            .map_or(Clip::whole(self.width, self.height), |span| span.clip)
    }

    pub(crate) fn horizon_y(&self) -> f32 {
        self.center().1
    }

    pub(crate) fn y(&self, elevation: f32) -> f32 {
        self.horizon_y() - elevation * self.focal
    }

    pub(crate) fn scale(&self) -> f32 {
        self.focal / REF_FOCAL
    }

    pub(crate) fn contains(&self, x: f32, y: f32, margin: f32) -> bool {
        self.clip().touches(x, y, margin, margin)
    }
}

struct Star {
    /// Position around the cylinder, `0..1` of a full turn.
    turn: f32,
    elevation: f32,
    /// Brightness at its dimmest twinkle, `0..1`.
    brightness: f32,
    /// Twinkle phase, `0..1` of a period.
    phase: f32,
    large: bool,
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum CityKind {
    Dot(Rgb),
    Glow,
    /// A red aircraft warning light that blinks.
    Beacon,
}

struct CityLight {
    turn: f32,
    /// Whether the mountains in front of it hide it; always false when
    /// there are no mountains.
    thinned: bool,
    elevation: f32,
    brightness: f32,
    phase: f32,
    kind: CityKind,
}

struct Meteor {
    start: f64,
    life: f32,
    angle: f64,
    elevation: f32,
    /// `1.0` or `-1.0`: the way it moves around the cylinder.
    direction: f32,
    /// Angle below the horizontal, in radians.
    slope: f32,
    /// In focal lengths per second.
    speed: f32,
    /// In focal lengths.
    length: f32,
    peak: f32,
}

pub struct Sky {
    seed: u64,
    stars: Vec<Star>,
    city: Vec<CityLight>,
    meteors: Vec<Meteor>,
    mountains: Option<Mountains>,
    /// How tall the mountains stand now, `0..1`; the city lights that they
    /// hide come back as they sink.
    mountain_level: f32,
    /// Number of meteors started so far; also the index of the next one.
    started: i64,
    next_start: f64,
}

const STREAM_STAR: u32 = 0x10;
const STREAM_CITY: u32 = 0x20;
const STREAM_METEOR: u32 = 0x30;

fn lerp(range: (f32, f32), t: f32) -> f32 {
    range.0 + (range.1 - range.0) * t
}

impl Sky {
    /// A sky for `seed`; `mountains` adds ranges of mountains on the horizon.
    pub fn new(seed: u64, mountains: bool) -> Self {
        let mountains = mountains.then(|| Mountains::new(seed));
        let star_count = (STAR_DENSITY * TAU as f32 * (STAR_TOP - STAR_BOTTOM)).round() as i64;
        let stars = (0..star_count)
            .map(|i| {
                let h = |part: u32| hash01(seed, STREAM_STAR | part, i);
                Star {
                    turn: h(0),
                    elevation: lerp((STAR_BOTTOM, STAR_TOP), h(1)),
                    brightness: lerp((50.0, 220.0), h(2)) / 255.0,
                    phase: h(3),
                    large: h(4) < LARGE_STAR_SHARE,
                }
            })
            .collect();
        let city = (0..CITY_COUNT as i64)
            .map(|i| {
                let h = |part: u32| hash01(seed, STREAM_CITY | part, i);
                // Squaring crowds the lights toward the horizon.
                let low = h(1) * h(1);
                let pick = h(2);
                let kind = if pick < 0.92 {
                    let color = h(5);
                    // Warm yellow, orange and cool white in the ratio 45:25:30.
                    let index = if color < 0.45 {
                        0
                    } else if color < 0.70 {
                        1
                    } else {
                        2
                    };
                    CityKind::Dot(CITY_COLORS[index])
                } else if pick < 0.97 {
                    CityKind::Glow
                } else {
                    CityKind::Beacon
                };
                let turn = h(0);
                CityLight {
                    turn,
                    thinned: mountains
                        .as_ref()
                        .is_some_and(|m| m.thins_city_light(turn, i)),
                    elevation: lerp((CITY_LOW, CITY_HIGH), low) / REF_FOCAL,
                    brightness: lerp((0.35, 0.9), h(3)),
                    phase: h(4),
                    kind,
                }
            })
            .collect();
        let mut sky = Self {
            seed,
            stars,
            city,
            mountains,
            mountain_level: 1.0,
            meteors: Vec::new(),
            started: 0,
            next_start: 0.0,
        };
        sky.next_start = sky.gap(0);
        sky
    }

    pub fn star_count(&self) -> usize {
        self.stars.len()
    }

    pub fn city_light_count(&self) -> usize {
        self.city.len()
    }

    /// Number of city lights that mountains hide.
    pub fn thinned_city_light_count(&self) -> usize {
        self.city.iter().filter(|c| c.thinned).count()
    }

    /// The mountains, if there are any.
    pub fn mountains(&self) -> Option<&Mountains> {
        self.mountains.as_ref()
    }

    /// Sets how tall the mountains stand, from 0 (flat) to 1 (full height).
    /// The city lights that the mountains hide fade in as they sink.
    pub fn set_mountain_level(&mut self, level: f32) {
        let level = if level.is_finite() {
            level.clamp(0.0, 1.0)
        } else {
            1.0
        };
        self.mountain_level = level;
        if let Some(mountains) = &mut self.mountains {
            mountains.set_level(level);
        }
    }

    /// Number of meteors that have started so far.
    pub fn meteors_started(&self) -> i64 {
        self.started
    }

    /// Number of meteors that are in the sky now.
    pub fn meteors_visible(&self) -> usize {
        self.meteors.len()
    }

    /// Seconds between meteor `index` and the one before it.
    fn gap(&self, index: i64) -> f64 {
        f64::from(lerp(METEOR_GAP, hash01(self.seed, STREAM_METEOR, index)))
    }

    /// Starts the meteors that are due at `time` and drops those that have
    /// burnt out. `heading` is where the car faces, so that new meteors
    /// appear in front of it.
    pub fn update(&mut self, time: f64, heading: f64) {
        while self.next_start <= time {
            let meteor = self.meteor(self.started, self.next_start, heading);
            self.meteors.push(meteor);
            self.started += 1;
            self.next_start += self.gap(self.started);
        }
        self.meteors.retain(|m| time < m.start + f64::from(m.life));
    }

    fn meteor(&self, index: i64, start: f64, heading: f64) -> Meteor {
        let h = |part: u32| hash01(self.seed, STREAM_METEOR | part, index);
        let slope = lerp(METEOR_SLOPE, h(1)).to_radians();
        let speed = lerp(METEOR_SPEED, h(2)) / REF_FOCAL;
        let elevation = lerp(METEOR_START, h(3));
        // It must not sink below the floor, so a steep and fast one is cut short.
        let seconds_to_floor = (elevation - METEOR_FLOOR) / (slope.sin() * speed);
        Meteor {
            start,
            life: lerp(METEOR_LIFE, h(4)).min(seconds_to_floor),
            angle: heading + f64::from((h(5) - 0.5) * METEOR_SPREAD),
            elevation,
            direction: if h(6) < 0.5 { -1.0 } else { 1.0 },
            slope,
            speed,
            length: lerp(METEOR_LENGTH, h(7)) / REF_FOCAL,
            peak: if h(8) < 0.2 { 1.0 } else { 0.6 },
        }
    }

    /// Draws the sky, which lies behind everything on the road.
    pub fn draw(&self, view: &SkyView, canvas: &mut dyn Canvas) {
        let k = view.scale();
        let horizon = view.horizon_y();
        canvas.sky_band(
            horizon + BAND_TOP * k,
            horizon + BAND_PEAK * k,
            horizon + BAND_BOTTOM * k,
            BAND_COLOR,
            BAND_ALPHA,
        );
        self.draw_stars(view, canvas);
        self.draw_meteors(view, canvas);
        // The mountains cover the glow, the stars and the meteors behind
        // them; the lights of the city lie in front of them.
        if let Some(mountains) = &self.mountains {
            mountains.draw_silhouettes(view, canvas);
            mountains.draw_lights(view, canvas);
        }
        self.draw_city(view, canvas);
    }

    fn draw_stars(&self, view: &SkyView, canvas: &mut dyn Canvas) {
        let k = view.scale();
        let size = (1.4 * k).max(view.pixel_scale);
        let core = (1.0 * k).max(0.9 * view.pixel_scale);
        for star in &self.stars {
            let x = view.screen_x(f64::from(star.turn) * TAU);
            let y = view.y(star.elevation);
            let wave = (TAU * (view.time / TWINKLE_PERIOD + f64::from(star.phase))).sin();
            let twinkle = 0.75 + 0.25 * (0.5 + 0.5 * wave as f32);
            let alpha = star.brightness * twinkle;
            if star.large {
                if alpha >= MIN_ALPHA && view.contains(x, y, core * GLOW_HALO_RATIO) {
                    canvas.glow(x, y, core, STAR_COLOR, alpha);
                }
            } else if alpha >= MIN_ALPHA && view.contains(x, y, size) {
                canvas.dot(x, y, size, STAR_COLOR, alpha);
            }
        }
    }

    fn draw_city(&self, view: &SkyView, canvas: &mut dyn Canvas) {
        let k = view.scale();
        let size = (1.2 * k).max(view.pixel_scale);
        let core = (1.1 * k).max(0.9 * view.pixel_scale);
        for light in &self.city {
            // A light behind the mountains shows as they sink.
            let shown = if light.thinned {
                1.0 - self.mountain_level
            } else {
                1.0
            };
            if shown <= 0.0 {
                continue;
            }
            let x = view.screen_x(f64::from(light.turn) * TAU);
            let y = view.y(light.elevation);
            let (alpha, margin) = match light.kind {
                CityKind::Dot(_) => (light.brightness * shown, size),
                CityKind::Glow => (light.brightness * 0.8 * shown, core * GLOW_HALO_RATIO),
                CityKind::Beacon => {
                    let cycle =
                        (view.time / BEACON_PERIOD + f64::from(light.phase)).rem_euclid(1.0);
                    let on = cycle < BEACON_ON / BEACON_PERIOD;
                    (if on { 0.9 * shown } else { 0.0 }, core * GLOW_HALO_RATIO)
                }
            };
            if alpha < MIN_ALPHA || !view.contains(x, y, margin) {
                continue;
            }
            match light.kind {
                CityKind::Dot(color) => canvas.dot(x, y, size, color, alpha),
                CityKind::Glow => canvas.glow(x, y, core, CITY_GLOW_COLOR, alpha),
                CityKind::Beacon => canvas.glow(x, y, core, BEACON_COLOR, alpha),
            }
        }
    }

    fn draw_meteors(&self, view: &SkyView, canvas: &mut dyn Canvas) {
        let width = (1.5 * view.scale()).max(view.pixel_scale);
        for m in &self.meteors {
            let age = (view.time - m.start) as f32;
            let progress = age / m.life;
            if !(0.0..=1.0).contains(&progress) {
                continue;
            }
            // Fades in and out over its life.
            let alpha = m.peak * (progress * std::f32::consts::PI).sin();
            let travelled = m.speed * age;
            let angle = m.angle + f64::from(m.direction * m.slope.cos() * travelled);
            let head = (
                view.screen_x(angle),
                view.y(m.elevation - m.slope.sin() * travelled),
            );
            // On screen the meteor moves along (side * direction * cos, sin).
            let (dx, dy) = (view.side * m.direction * m.slope.cos(), m.slope.sin());
            let length = m.length * view.focal;
            let tail = (head.0 - dx * length, head.1 - dy * length);
            let reach = length + width;
            if alpha >= MIN_ALPHA && view.contains(head.0, head.1, reach) {
                canvas.streak(tail, head, width, METEOR_COLOR, alpha);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, PartialEq)]
    enum Command {
        Dot {
            x: f32,
            y: f32,
            size: f32,
            alpha: f32,
        },
        Glow {
            x: f32,
            y: f32,
            radius: f32,
            alpha: f32,
        },
        Band {
            top: f32,
            peak: f32,
            bottom: f32,
            alpha: f32,
        },
        Streak {
            tail: (f32, f32),
            head: (f32, f32),
            width: f32,
            alpha: f32,
        },
        Silhouette(Vec<(f32, f32)>),
    }

    #[derive(Default)]
    struct Recorder(Vec<Command>);

    impl Canvas for Recorder {
        fn glow(&mut self, x: f32, y: f32, radius: f32, _: Rgb, alpha: f32) {
            self.0.push(Command::Glow {
                x,
                y,
                radius,
                alpha,
            });
        }
        fn light_pool(&mut self, _: f32, _: f32, _: f32, _: f32, _: Rgb, _: f32) {
            unreachable!("the sky has no light pools");
        }
        fn dot(&mut self, x: f32, y: f32, size: f32, _: Rgb, alpha: f32) {
            self.0.push(Command::Dot { x, y, size, alpha });
        }
        fn sky_band(&mut self, top: f32, peak: f32, bottom: f32, _: Rgb, alpha: f32) {
            self.0.push(Command::Band {
                top,
                peak,
                bottom,
                alpha,
            });
        }
        fn streak(&mut self, tail: (f32, f32), head: (f32, f32), width: f32, _: Rgb, alpha: f32) {
            self.0.push(Command::Streak {
                tail,
                head,
                width,
                alpha,
            });
        }
        fn silhouette(&mut self, points: &[(f32, f32)], _: Rgb, _: f32) {
            self.0.push(Command::Silhouette(points.to_vec()));
        }
        fn beam(&mut self, _: f32, _: f32, _: f32, _: f32, _: f32, _: Rgb, _: f32, _: f32) {
            unreachable!("the sky has no beams");
        }
    }

    fn view(width: f32, height: f32) -> SkyView {
        SkyView {
            width,
            height,
            pixel_scale: 1.0,
            focal: (width * 0.5).max(height * 0.55),
            side: 1.0,
            heading: 0.0,
            time: 0.0,
            span: None,
        }
    }

    fn record(sky: &Sky, view: &SkyView) -> Vec<Command> {
        let mut recorder = Recorder::default();
        sky.draw(view, &mut recorder);
        recorder.0
    }

    #[test]
    fn counts_follow_the_design() {
        let sky = Sky::new(1, false);
        assert_eq!(sky.city_light_count(), 900);
        // About 1400 stars would span the screen height of 1080p; the whole
        // sky is taller so that portrait screens are filled too.
        assert!(
            (2300..2500).contains(&sky.star_count()),
            "{}",
            sky.star_count()
        );
        let visible = record(&sky, &view(1920.0, 1080.0))
            .iter()
            .filter(|c| matches!(c, Command::Dot { .. } | Command::Glow { .. }))
            .count();
        // About 450 stars plus the city lights that are in view.
        assert!((500..800).contains(&visible), "{visible}");
    }

    #[test]
    fn same_seed_gives_the_same_sky_and_other_seeds_differ() {
        let positions = |seed| {
            Sky::new(seed, false)
                .stars
                .iter()
                .take(20)
                .map(|s| (s.turn, s.elevation))
                .collect::<Vec<_>>()
        };
        assert_eq!(positions(3), positions(3));
        assert_ne!(positions(3), positions(4));
    }

    #[test]
    fn generated_values_are_in_range() {
        let sky = Sky::new(9, false);
        for s in &sky.stars {
            assert!((0.0..1.0).contains(&s.turn));
            assert!((STAR_BOTTOM..=STAR_TOP).contains(&s.elevation));
            assert!(s.brightness > 0.0 && s.brightness <= 1.0 && s.phase.is_finite());
        }
        for c in &sky.city {
            assert!((0.0..1.0).contains(&c.turn));
            assert!(c.elevation >= CITY_LOW / REF_FOCAL - 1e-6);
            assert!(c.elevation <= CITY_HIGH / REF_FOCAL + 1e-6);
            assert!(c.elevation < STAR_BOTTOM);
        }
        let beacons = sky
            .city
            .iter()
            .filter(|c| c.kind == CityKind::Beacon)
            .count();
        assert!((10..50).contains(&beacons), "{beacons}");
    }

    #[test]
    fn a_full_turn_brings_the_stars_back() {
        let sky = Sky::new(2, false);
        let mut a = view(1280.0, 720.0);
        a.heading = 0.7;
        let mut b = a;
        b.heading += TAU;
        let first = record(&sky, &a);
        let second = record(&sky, &b);
        assert_eq!(first.len(), second.len());
        for (p, q) in first.iter().zip(&second) {
            if let (Command::Dot { x: xp, y: yp, .. }, Command::Dot { x: xq, y: yq, .. }) = (p, q) {
                assert!((xp - xq).abs() < 1e-2 && yp == yq);
            }
        }
    }

    #[test]
    fn backdrop_slides_against_the_bend_of_the_road() {
        let mut v = view(1280.0, 720.0);
        let before = v.screen_x(1.0);
        v.heading = 0.1;
        assert!(v.screen_x(1.0) < before);
        // Right-hand traffic mirrors it.
        let mut v = view(1280.0, 720.0);
        v.side = -1.0;
        let before = v.screen_x(1.0);
        v.heading = 0.1;
        assert!(v.screen_x(1.0) > before);
    }

    #[test]
    fn the_same_view_shows_a_similar_number_of_stars_at_any_resolution() {
        let sky = Sky::new(5, false);
        let count = |v: SkyView| record(&sky, &v).len() as f32;
        let (small, large) = (count(view(1280.0, 720.0)), count(view(3840.0, 2160.0)));
        assert!((small / large - 1.0).abs() < 0.1, "{small} {large}");
    }

    #[test]
    fn commands_are_valid() {
        let mut sky = Sky::new(11, false);
        let mut v = view(1280.0, 720.0);
        for step in 0..4000 {
            v.time = f64::from(step) * 0.05;
            v.heading = v.time * 0.1;
            sky.update(v.time, v.heading);
            for command in record(&sky, &v) {
                let (values, alpha): (Vec<f32>, f32) = match command {
                    Command::Dot { x, y, size, alpha } => {
                        assert!(size > 0.0);
                        (vec![x, y, size], alpha)
                    }
                    Command::Glow {
                        x,
                        y,
                        radius,
                        alpha,
                    } => {
                        assert!(radius > 0.0);
                        (vec![x, y, radius], alpha)
                    }
                    Command::Band {
                        top,
                        peak,
                        bottom,
                        alpha,
                    } => {
                        assert!(top < peak && peak < bottom);
                        (vec![top, peak, bottom], alpha)
                    }
                    Command::Streak {
                        tail,
                        head,
                        width,
                        alpha,
                    } => {
                        assert!(width > 0.0);
                        (vec![tail.0, tail.1, head.0, head.1, width], alpha)
                    }
                    Command::Silhouette(points) => {
                        assert!(points.len() >= 3);
                        assert!(points.iter().all(|p| p.0.is_finite() && p.1.is_finite()));
                        continue;
                    }
                };
                assert!(values.iter().all(|v| v.is_finite()));
                assert!(alpha > 0.0 && alpha <= 1.0, "{alpha}");
            }
        }
    }

    /// Runs `seconds` of 50 ms steps and returns the times at which meteors started.
    fn start_times(seed: u64, seconds: f64) -> Vec<f64> {
        let mut sky = Sky::new(seed, false);
        let mut starts = Vec::new();
        let mut started = 0;
        let mut time = 0.0;
        while time < seconds {
            time += 0.05;
            sky.update(time, 0.0);
            if sky.meteors_started() > started {
                started = sky.meteors_started();
                starts.push(time);
            }
        }
        starts
    }

    #[test]
    fn meteors_start_every_15_to_45_seconds() {
        let starts = start_times(6, 3600.0);
        assert!(starts.len() > 3600 / 45 - 1 && starts.len() < 3600 / 15 + 1);
        assert!(starts[0] >= 15.0 && starts[0] <= 45.05);
        for pair in starts.windows(2) {
            let gap = pair[1] - pair[0];
            // The steps of 50 ms blur the gaps a little.
            assert!((14.9..=45.1).contains(&gap), "{gap}");
        }
    }

    #[test]
    fn meteors_are_reproducible() {
        assert_eq!(start_times(8, 600.0), start_times(8, 600.0));
        assert_ne!(start_times(8, 600.0), start_times(9, 600.0));
    }

    #[test]
    fn a_meteor_burns_out_and_stays_above_the_floor() {
        let mut sky = Sky::new(12, false);
        let first = sky.next_start;
        sky.update(first, 0.0);
        assert_eq!(sky.meteors_visible(), 1);
        let meteor = &sky.meteors[0];
        assert!((0.0..=1.2).contains(&meteor.life) && meteor.life > 0.1);
        let end_elevation = meteor.elevation - meteor.slope.sin() * meteor.speed * meteor.life;
        assert!(end_elevation >= METEOR_FLOOR - 1e-4);
        let after = first + f64::from(meteor.life) + 0.01;
        sky.update(after, 0.0);
        assert_eq!(sky.meteors_visible(), 0);
    }

    #[test]
    fn every_meteor_stays_above_the_floor() {
        let sky = Sky::new(13, false);
        for i in 0..2000 {
            let m = sky.meteor(i, 0.0, 0.0);
            assert!(m.life >= 0.1, "{}", m.life);
            let end = m.elevation - m.slope.sin() * m.speed * m.life;
            assert!(end >= METEOR_FLOOR - 1e-4);
        }
    }

    #[test]
    fn a_meteor_is_drawn_while_it_burns() {
        let mut sky = Sky::new(14, false);
        let mut v = view(1280.0, 720.0);
        let first = sky.next_start;
        sky.update(first, 0.0);
        v.time = first + f64::from(sky.meteors[0].life) * 0.5;
        let streaks = record(&sky, &v)
            .into_iter()
            .filter(|c| matches!(c, Command::Streak { .. }))
            .count();
        assert_eq!(streaks, 1);
    }

    /// The heading that puts the middle of the first range at the center.
    fn facing_the_first_range(sky: &Sky) -> f64 {
        let range = &sky.mountains().unwrap().ranges()[0];
        range.start() + range.width() * 0.5
    }

    fn silhouettes(commands: &[Command]) -> usize {
        commands
            .iter()
            .filter(|c| matches!(c, Command::Silhouette(_)))
            .count()
    }

    #[test]
    fn mountains_come_only_when_asked_and_do_not_change_the_rest_of_the_sky() {
        let with = Sky::new(21, true);
        let without = Sky::new(21, false);
        assert!(without.mountains().is_none());
        assert_eq!(without.thinned_city_light_count(), 0);
        assert_eq!(with.city_light_count(), without.city_light_count());
        assert_eq!(with.star_count(), without.star_count());
        for degrees in (0..360).step_by(5) {
            let mut v = view(1280.0, 720.0);
            v.heading = f64::from(degrees).to_radians();
            assert_eq!(silhouettes(&record(&without, &v)), 0);
        }
        let mut v = view(1280.0, 720.0);
        v.heading = facing_the_first_range(&with);
        assert!(silhouettes(&record(&with, &v)) >= 1);
    }

    #[test]
    fn the_silhouette_is_drawn_after_the_stars_and_before_the_city() {
        let sky = Sky::new(22, true);
        let mut v = view(1280.0, 720.0);
        v.heading = facing_the_first_range(&sky);
        let commands = record(&sky, &v);
        let first = commands
            .iter()
            .position(|c| matches!(c, Command::Silhouette(_)))
            .unwrap();
        let last = commands
            .iter()
            .rposition(|c| matches!(c, Command::Silhouette(_)))
            .unwrap();
        let streak = commands
            .iter()
            .rposition(|c| matches!(c, Command::Streak { .. }));
        assert!(streak.is_none_or(|i| i < first));
        // The stars come first; only the lights of the city follow.
        assert!(
            commands[..first]
                .iter()
                .any(|c| matches!(c, Command::Band { .. }))
        );
        assert!(commands[..first].len() > 100);
        assert!(
            commands[last + 1..]
                .iter()
                .all(|c| matches!(c, Command::Dot { .. } | Command::Glow { .. }))
        );
    }

    #[test]
    fn city_lights_in_front_of_a_range_are_thinned_and_the_rest_stay() {
        let with = Sky::new(23, true);
        let without = Sky::new(23, false);
        let thinned = with.thinned_city_light_count();
        // The ranges cover at most half of the circle and keep a quarter.
        assert!(thinned > 0 && thinned < with.city_light_count() * 3 / 8);

        let mut v = view(1920.0, 1080.0);
        v.heading = facing_the_first_range(&with);
        let range = &with.mountains().unwrap().ranges()[0];
        let half = (range.width() * 0.5) as f32 * v.focal;
        let (left, right) = (v.width * 0.5 - half, v.width * 0.5 + half);
        // Lights of the city stand within 41 px of the horizon; stars are
        // higher. Count those in front of the range.
        let city_lights = |sky: &Sky| {
            record(sky, &v)
                .iter()
                .filter(|c| match c {
                    Command::Dot { x, y, .. } | Command::Glow { x, y, .. } => {
                        (left..right).contains(x) && (0.0..45.0).contains(&(v.horizon_y() - y))
                    }
                    _ => false,
                })
                .count()
        };
        let (kept, all) = (city_lights(&with), city_lights(&without));
        assert!(all > 30, "{all}");
        // A quarter stays; the bound leaves room for chance.
        assert!(kept * 5 < all * 2, "{kept} of {all}");
        assert!(kept > 0, "{kept} of {all}");
    }

    #[test]
    fn flat_mountains_leave_the_sky_as_if_there_were_none() {
        let mut with = Sky::new(23, true);
        let without = Sky::new(23, false);
        with.set_mountain_level(0.0);
        let mut v = view(1920.0, 1080.0);
        v.heading = facing_the_first_range(&with);
        // No silhouette, and the city lights that the mountains would hide
        // are all there.
        assert_eq!(record(&with, &v), record(&without, &v));
    }

    #[test]
    fn city_lights_return_as_the_mountains_sink() {
        let mut sky = Sky::new(23, true);
        let mut v = view(1920.0, 1080.0);
        v.heading = facing_the_first_range(&sky);
        let lit = |sky: &Sky| {
            record(sky, &v)
                .iter()
                .filter(|c| matches!(c, Command::Dot { .. } | Command::Glow { .. }))
                .count()
        };
        let counts: Vec<usize> = [1.0, 0.75, 0.5, 0.25, 0.0]
            .iter()
            .map(|&level| {
                sky.set_mountain_level(level);
                lit(&sky)
            })
            .collect();
        // Lights are dimmed, not removed, until the mountains are gone, so
        // the count does not fall; it grows when the level reaches 0.
        assert!(counts.windows(2).all(|p| p[0] <= p[1]), "{counts:?}");
        assert!(counts[4] > counts[0], "{counts:?}");
        // A half-sunk range shows the hidden lights at half strength.
        sky.set_mountain_level(0.5);
        let dimmest = record(&sky, &v)
            .iter()
            .filter_map(|c| match c {
                Command::Dot { alpha, .. } => Some(*alpha),
                _ => None,
            })
            .fold(f32::MAX, f32::min);
        assert!(dimmest < 0.2, "{dimmest}");
    }

    #[test]
    fn a_bad_mountain_level_is_ignored() {
        let mut sky = Sky::new(23, true);
        let mut v = view(1920.0, 1080.0);
        v.heading = facing_the_first_range(&sky);
        let full = record(&sky, &v);
        for level in [f32::NAN, f32::INFINITY, 3.0] {
            sky.set_mountain_level(level);
            assert_eq!(record(&sky, &v), full);
        }
    }

    #[test]
    fn commands_with_mountains_are_valid() {
        let mut sky = Sky::new(24, true);
        let mut v = view(1280.0, 720.0);
        let mut silhouettes_seen = 0;
        for step in 0..4000 {
            v.time = f64::from(step) * 0.05;
            v.heading = v.time * 0.1;
            sky.update(v.time, v.heading);
            for command in record(&sky, &v) {
                match command {
                    Command::Silhouette(points) => {
                        silhouettes_seen += 1;
                        assert!(points.len() >= 4);
                        assert!(points.iter().all(|p| p.0.is_finite() && p.1.is_finite()));
                    }
                    Command::Glow { alpha, .. } => assert!(alpha > 0.0 && alpha <= 1.0),
                    _ => {}
                }
            }
        }
        assert!(silhouettes_seen > 0);
    }

    #[test]
    fn invalid_sizes_do_not_panic() {
        for mountains in [false, true] {
            let sky = Sky::new(1, mountains);
            record(&sky, &view(1.0, 1.0));
        }
    }
}
