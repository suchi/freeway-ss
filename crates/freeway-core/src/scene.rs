//! The scene: advances the journey and draws the sky, the lights of the road and other cars.

use std::f64::consts::TAU;

use crate::canvas::{Canvas, GLOW_HALO_RATIO, Rgb};
use crate::projection::{CAMERA_HEIGHT, NEAR_DISTANCE, Projector, VIEW_DISTANCE};
use crate::road::{RoadShape, RoadView};
use crate::sky::{Sky, SkyView};
use crate::traffic::Traffic;
use crate::ufo::Ufo;

/// Longest time step the scene accepts, in seconds. A longer pause (for
/// example after the machine slept) must not make the road jump.
pub const MAX_STEP_SECONDS: f32 = 0.05;
/// Radius below which a glow is not drawn smaller, in pixels at 100% scale.
/// Smaller lights are dimmed instead so that distant points do not shimmer.
const MIN_CORE_RADIUS: f32 = 0.9;
/// Largest core radius, in pixels at 100% scale. Lights close to the camera
/// would otherwise swell into blobs.
const MAX_CORE_RADIUS: f32 = 7.0;
/// Largest core radius of a glare, the dazzle of a close headlight.
const MAX_GLARE_RADIUS: f32 = 28.0;
/// Alphas below this are invisible and not worth a draw call.
const MIN_ALPHA: f32 = 0.004;

#[derive(Clone, Copy, Debug)]
pub struct SceneConfig {
    pub speed_kmh: f32,
    pub left_hand_traffic: bool,
    /// Average seconds between two UFO visits; `None` means no UFO.
    pub ufo_mean_interval: Option<f32>,
    pub seed: u64,
}

impl Default for SceneConfig {
    fn default() -> Self {
        Self {
            speed_kmh: 100.0,
            left_hand_traffic: true,
            ufo_mean_interval: None,
            seed: 0,
        }
    }
}

/// The drawing area of one window.
#[derive(Clone, Copy, Debug)]
pub struct Viewport {
    pub width: f32,
    pub height: f32,
    /// DPI divided by 96; scales sizes that are defined on screen, not in the world.
    pub pixel_scale: f32,
}

#[derive(Clone, Copy)]
enum Kind {
    /// A point light of the given physical size in meters.
    Glow { size: f32 },
    /// A point light that may swell larger than a `Glow` when close.
    Glare { size: f32 },
    /// A pool of light on the road with the given radius in meters.
    Pool { radius: f32 },
}

/// A row of identical lights along the road.
struct Row {
    lateral: f32,
    height: f32,
    /// Distance between lights along the road, in meters.
    spacing: f64,
    /// Distance at which the light has faded out completely.
    range: f32,
    /// Paint only: whether the row is a dashed line.
    dashed: bool,
    color: Rgb,
    alpha: f32,
    kind: Kind,
}

const WHITE: Rgb = Rgb::new(255, 255, 255);
const ORANGE: Rgb = Rgb::new(255, 140, 30);
const SODIUM: Rgb = Rgb::new(255, 165, 70);

/// Dashed lines repeat every `DASH_PERIOD` meters, painted for the first
/// `DASH_LENGTH` meters. Lights on them are spaced so that a dash gets a few.
const DASH_PERIOD: f64 = 20.0;
const DASH_LENGTH: f64 = 8.0;

const fn row(
    lateral: f32,
    height: f32,
    spacing: f64,
    range: f32,
    color: Rgb,
    alpha: f32,
    kind: Kind,
) -> Row {
    Row {
        lateral,
        height,
        spacing,
        range,
        dashed: false,
        color,
        alpha,
        kind,
    }
}

const fn dashed(lateral: f32, range: f32, alpha: f32) -> Row {
    Row {
        dashed: true,
        ..row(
            lateral,
            0.0,
            2.5,
            range,
            WHITE,
            alpha,
            Kind::Glow { size: 0.12 },
        )
    }
}

// Lateral positions assume the camera is in the left lane of left-hand
// traffic; right-hand traffic mirrors the picture on the screen.
const ROWS: [Row; 11] = [
    dashed(0.0, 150.0, 0.8),
    row(-3.5, 0.0, 3.0, 110.0, WHITE, 0.7, Kind::Glow { size: 0.12 }),
    row(3.5, 0.0, 3.0, 110.0, WHITE, 0.7, Kind::Glow { size: 0.12 }),
    dashed(9.0, 80.0, 0.4),
    row(-4.0, 0.9, 12.0, 420.0, WHITE, 0.8, Kind::Glow { size: 0.1 }),
    row(3.9, 0.9, 12.0, 420.0, ORANGE, 0.8, Kind::Glow { size: 0.1 }),
    row(
        5.1,
        0.9,
        12.0,
        250.0,
        ORANGE,
        0.45,
        Kind::Glow { size: 0.1 },
    ),
    row(
        13.0,
        0.9,
        12.0,
        250.0,
        WHITE,
        0.45,
        Kind::Glow { size: 0.1 },
    ),
    row(
        4.5,
        10.0,
        45.0,
        VIEW_DISTANCE,
        SODIUM,
        1.0,
        Kind::Glow { size: 0.3 },
    ),
    row(
        2.0,
        0.0,
        45.0,
        350.0,
        SODIUM,
        0.3,
        Kind::Pool { radius: 9.0 },
    ),
    row(
        7.0,
        0.0,
        45.0,
        350.0,
        SODIUM,
        0.3,
        Kind::Pool { radius: 9.0 },
    ),
];

/// Headlight reflections stay put in front of the car: (distance, alpha, radius in meters).
const HEADLIGHT_POOLS: [(f32, f32, f32); 2] = [(14.0, 0.3, 3.0), (32.0, 0.2, 4.0)];

/// A light ready to be projected.
struct Light {
    d: f32,
    lateral: f32,
    height: f32,
    color: Rgb,
    alpha: f32,
    kind: Kind,
}

pub struct Scene {
    config: SceneConfig,
    road: RoadShape,
    /// Distance travelled. `f64` so that hours of driving keep sub-meter precision.
    track_s: f64,
    /// How far the car has turned, in radians, kept near `0..TAU` (float rounding can give `TAU` itself).
    heading: f64,
    /// Seconds of (capped) steps since the start.
    time: f64,
    sky: Sky,
    traffic: Traffic,
    ufo: Ufo,
}

impl Scene {
    pub fn new(config: SceneConfig) -> Self {
        Self {
            config,
            road: RoadShape::new(config.seed),
            track_s: 0.0,
            heading: 0.0,
            time: 0.0,
            sky: Sky::new(config.seed),
            traffic: Traffic::new(config.seed),
            ufo: Ufo::new(config.seed, config.ufo_mean_interval),
        }
    }

    pub fn track_s(&self) -> f64 {
        self.track_s
    }

    pub fn heading(&self) -> f64 {
        self.heading
    }

    /// Number of shooting stars in the sky now.
    pub fn meteors_visible(&self) -> usize {
        self.sky.meteors_visible()
    }

    /// Whether a UFO is in the sky now.
    pub fn ufo_visible(&self) -> bool {
        self.ufo.is_visible()
    }

    /// Moves the car forward by `dt` seconds (capped at `MAX_STEP_SECONDS`).
    pub fn update(&mut self, dt: f32) {
        let dt = if dt.is_finite() {
            dt.clamp(0.0, MAX_STEP_SECONDS)
        } else {
            0.0
        };
        let step = f64::from(self.config.speed_kmh / 3.6 * dt);
        // The car turns by the curvature of the road it is driving on.
        let turn = f64::from(self.road.curvature_at(self.track_s)) * step;
        self.heading = (self.heading + turn).rem_euclid(TAU);
        self.track_s += step;
        self.time += f64::from(dt);
        self.sky.update(self.time, self.heading);
        self.ufo.update(self.time, self.heading, self.side());
        self.traffic.update(dt, self.config.speed_kmh / 3.6);
    }

    /// Draws the sky, then the lights of the road from far to near.
    pub fn draw(&self, viewport: Viewport, canvas: &mut dyn Canvas) {
        // `clamp` below panics on a negative or NaN scale.
        let pixel_scale = if viewport.pixel_scale.is_finite() && viewport.pixel_scale > 0.0 {
            viewport.pixel_scale
        } else {
            1.0
        };
        let viewport = Viewport {
            pixel_scale,
            ..viewport
        };
        let projector = Projector::new(
            viewport.width,
            viewport.height,
            self.config.left_hand_traffic,
        );
        let view = RoadView::new(&self.road, self.track_s);

        let sky_view = SkyView {
            width: viewport.width,
            height: viewport.height,
            pixel_scale,
            focal: projector.focal(),
            side: self.side(),
            heading: self.heading,
            time: self.time,
        };
        self.sky.draw(&sky_view, canvas);
        // The UFO is far away: the lights of the road and of other cars lie in front of it.
        self.ufo.draw(&sky_view, canvas);

        for light in &self.lights() {
            let (bend, rise) = view.offset(light.d);
            let Some(p) = projector.project(light.d, light.lateral + bend, light.height + rise)
            else {
                continue;
            };
            match light.kind {
                Kind::Glow { size } | Kind::Glare { size } => {
                    let largest = if matches!(light.kind, Kind::Glare { .. }) {
                        MAX_GLARE_RADIUS
                    } else {
                        MAX_CORE_RADIUS
                    };
                    let natural = size * p.scale;
                    let minimum = MIN_CORE_RADIUS * viewport.pixel_scale;
                    let radius = natural.clamp(minimum, largest * viewport.pixel_scale);
                    let alpha = (light.alpha * (natural / minimum).min(1.0)).min(1.0);
                    let halo = radius * GLOW_HALO_RATIO;
                    if alpha >= MIN_ALPHA && touches(viewport, p.x, p.y, halo, halo) {
                        canvas.glow(p.x, p.y, radius, light.color, alpha);
                    }
                }
                Kind::Pool { radius } => {
                    let radius_x = radius * p.scale;
                    // The ground is seen at a grazing angle: its depth is
                    // foreshortened by camera height over distance.
                    let radius_y = radius_x * CAMERA_HEIGHT / light.d;
                    let alpha = light.alpha.min(1.0);
                    if alpha >= MIN_ALPHA
                        && radius_y >= 0.5 * viewport.pixel_scale
                        && touches(viewport, p.x, p.y, radius_x, radius_y)
                    {
                        canvas.light_pool(p.x, p.y, radius_x, radius_y, light.color, alpha);
                    }
                }
            }
        }
    }

    /// `1.0` for left-hand traffic, `-1.0` for right-hand traffic.
    fn side(&self) -> f32 {
        if self.config.left_hand_traffic {
            1.0
        } else {
            -1.0
        }
    }

    fn lights(&self) -> Vec<Light> {
        let mut lights = Vec::new();
        for row in ROWS.iter() {
            // Rows are anchored to the road, so they slide toward the car.
            let mut index = (self.track_s / row.spacing).ceil();
            loop {
                let s = index * row.spacing;
                let d = (s - self.track_s) as f32;
                if d >= row.range {
                    break;
                }
                index += 1.0;
                if d < NEAR_DISTANCE || (row.dashed && s.rem_euclid(DASH_PERIOD) >= DASH_LENGTH) {
                    continue;
                }
                lights.push(Light {
                    d,
                    lateral: row.lateral,
                    height: row.height,
                    color: row.color,
                    alpha: row.alpha * (1.0 - d / row.range),
                    kind: row.kind,
                });
            }
        }
        lights.extend(HEADLIGHT_POOLS.iter().map(|&(d, alpha, radius)| Light {
            d,
            lateral: -1.75,
            height: 0.0,
            color: WHITE,
            alpha,
            kind: Kind::Pool { radius },
        }));
        lights.extend(self.traffic.lights().into_iter().map(|car| Light {
            d: car.d,
            lateral: car.lateral,
            height: car.height,
            color: car.color,
            alpha: car.alpha,
            kind: if car.glare {
                Kind::Glare { size: car.size }
            } else {
                Kind::Glow { size: car.size }
            },
        }));
        lights.sort_by(|a, b| b.d.total_cmp(&a.d));
        lights
    }
}

/// Whether an ellipse of the given radii overlaps the viewport.
fn touches(viewport: Viewport, x: f32, y: f32, radius_x: f32, radius_y: f32) -> bool {
    x.is_finite()
        && y.is_finite()
        && x + radius_x >= 0.0
        && x - radius_x <= viewport.width
        && y + radius_y >= 0.0
        && y - radius_y <= viewport.height
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, PartialEq)]
    enum Command {
        Glow {
            x: f32,
            y: f32,
            radius: f32,
            alpha: f32,
        },
        Pool {
            x: f32,
            y: f32,
            rx: f32,
            ry: f32,
            alpha: f32,
        },
        Dot {
            x: f32,
            y: f32,
            size: f32,
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
        Beam {
            center_x: f32,
            top_y: f32,
            bottom_y: f32,
            alpha: f32,
        },
    }

    impl Command {
        /// The command's anchor on the screen, if it has a single one.
        fn position(&self) -> Option<(f32, f32)> {
            match *self {
                Command::Glow { x, y, .. }
                | Command::Pool { x, y, .. }
                | Command::Dot { x, y, .. } => Some((x, y)),
                Command::Streak { head, .. } => Some(head),
                Command::Band { .. } | Command::Beam { .. } => None,
            }
        }

        fn is_sky(&self) -> bool {
            !matches!(self, Command::Pool { .. })
        }
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
        fn light_pool(&mut self, x: f32, y: f32, rx: f32, ry: f32, _: Rgb, alpha: f32) {
            self.0.push(Command::Pool {
                x,
                y,
                rx,
                ry,
                alpha,
            });
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
        fn beam(
            &mut self,
            x: f32,
            top: f32,
            _: f32,
            bottom: f32,
            _: f32,
            _: Rgb,
            alpha: f32,
            _: f32,
        ) {
            self.0.push(Command::Beam {
                center_x: x,
                top_y: top,
                bottom_y: bottom,
                alpha,
            });
        }
    }

    const VIEWPORT: Viewport = Viewport {
        width: 1280.0,
        height: 720.0,
        pixel_scale: 1.0,
    };

    fn record(scene: &Scene, viewport: Viewport) -> Vec<Command> {
        let mut recorder = Recorder::default();
        scene.draw(viewport, &mut recorder);
        recorder.0
    }

    fn ufo_config(seed: u64) -> SceneConfig {
        SceneConfig {
            seed,
            ufo_mean_interval: Some(5.0),
            ..Default::default()
        }
    }

    /// Steps `scene` until a UFO has been in the sky for `extra` seconds.
    fn step_into_visit(scene: &mut Scene, extra: f32) {
        let mut steps = 0;
        while !scene.ufo_visible() {
            scene.update(0.05);
            steps += 1;
            assert!(steps < 40_000, "no UFO within 2000 seconds");
        }
        for _ in 0..(extra / 0.05) as usize {
            scene.update(0.05);
        }
    }

    #[test]
    fn a_ufo_adds_lights_and_a_beam_only_while_it_is_there() {
        let mut with = Scene::new(ufo_config(2));
        let mut without = Scene::new(SceneConfig {
            ufo_mean_interval: None,
            ..ufo_config(2)
        });
        // Until the first visit the two scenes draw exactly the same.
        while !with.ufo_visible() {
            with.update(0.05);
            without.update(0.05);
            if !with.ufo_visible() {
                assert_eq!(record(&with, VIEWPORT), record(&without, VIEWPORT));
            }
        }
        // 7 seconds into the visit the beam is down.
        for _ in 0..140 {
            with.update(0.05);
            without.update(0.05);
        }
        assert!(with.ufo_visible());
        assert!(!without.ufo_visible());
        let ufo_commands = record(&with, VIEWPORT);
        let road_commands = record(&without, VIEWPORT);
        let glows = |commands: &[Command]| {
            commands
                .iter()
                .filter(|c| matches!(c, Command::Glow { .. }))
                .count()
        };
        let beams = |commands: &[Command]| {
            commands
                .iter()
                .filter(|c| matches!(c, Command::Beam { .. }))
                .count()
        };
        assert!(glows(&ufo_commands) > glows(&road_commands) + 50);
        assert_eq!(beams(&ufo_commands), 1);
        assert_eq!(beams(&road_commands), 0);
    }

    #[test]
    fn a_ufo_draws_valid_commands_through_a_whole_visit() {
        // The full screen and the small preview window.
        for viewport in [
            VIEWPORT,
            Viewport {
                width: 152.0,
                height: 112.0,
                pixel_scale: 1.0,
            },
        ] {
            check_ufo_visit(viewport);
        }
    }

    fn check_ufo_visit(viewport: Viewport) {
        let mut scene = Scene::new(ufo_config(9));
        step_into_visit(&mut scene, 0.0);
        let mut beams = 0;
        while scene.ufo_visible() {
            scene.update(0.05);
            for command in record(&scene, viewport) {
                match command {
                    Command::Glow {
                        x,
                        y,
                        radius,
                        alpha,
                    } => {
                        let halo = radius * GLOW_HALO_RATIO;
                        assert!([x, y, radius].iter().all(|v| v.is_finite()), "{command:?}");
                        assert!(radius > 0.0 && alpha > 0.0 && alpha <= 1.0, "{command:?}");
                        assert!(
                            x + halo >= 0.0
                                && x - halo <= viewport.width
                                && y + halo >= 0.0
                                && y - halo <= viewport.height,
                            "{command:?}"
                        );
                    }
                    Command::Beam {
                        center_x,
                        top_y,
                        bottom_y,
                        alpha,
                    } => {
                        assert!([center_x, top_y, bottom_y].iter().all(|v| v.is_finite()));
                        beams += 1;
                        assert!(top_y < bottom_y && alpha > 0.0 && alpha <= 1.0);
                    }
                    _ => {}
                }
            }
        }
        assert!(beams > 50, "{beams}");
    }

    #[test]
    fn the_ufo_is_drawn_behind_the_lights_of_the_road() {
        let mut scene = Scene::new(ufo_config(2));
        step_into_visit(&mut scene, 7.0);
        let commands = record(&scene, VIEWPORT);
        let beam = commands
            .iter()
            .position(|c| matches!(c, Command::Beam { .. }))
            .unwrap();
        let first_pool = commands
            .iter()
            .position(|c| matches!(c, Command::Pool { .. }))
            .unwrap();
        assert!(beam < first_pool);
    }

    #[test]
    fn draws_something_with_valid_parameters() {
        let mut scene = Scene::new(SceneConfig {
            seed: 4,
            ..Default::default()
        });
        for _ in 0..200 {
            scene.update(0.016);
            let commands = record(&scene, VIEWPORT);
            assert!(!commands.is_empty());
            for command in &commands {
                // The sky keeps its own tests of the geometry; here only
                // the road's lights are checked for what lies off screen.
                let (values, alpha, radii) = match *command {
                    Command::Glow {
                        x,
                        y,
                        radius,
                        alpha,
                    } => ([x, y], alpha, [radius * GLOW_HALO_RATIO; 2]),
                    Command::Pool {
                        x,
                        y,
                        rx,
                        ry,
                        alpha,
                    } => ([x, y], alpha, [rx, ry]),
                    Command::Dot { x, y, size, alpha } => ([x, y], alpha, [size; 2]),
                    Command::Band { alpha, .. }
                    | Command::Streak { alpha, .. }
                    | Command::Beam { alpha, .. } => {
                        assert!(alpha > 0.0 && alpha <= 1.0, "{command:?}");
                        continue;
                    }
                };
                assert!(
                    values.iter().chain(&radii).all(|v| v.is_finite()),
                    "{command:?}"
                );
                assert!(radii.iter().all(|r| *r > 0.0), "{command:?}");
                assert!(alpha > 0.0 && alpha <= 1.0, "{command:?}");
                // Nothing that lies completely off screen is emitted.
                assert!(
                    values[0] + radii[0] >= 0.0
                        && values[0] - radii[0] <= VIEWPORT.width
                        && values[1] + radii[1] >= 0.0
                        && values[1] - radii[1] <= VIEWPORT.height,
                    "{command:?}"
                );
            }
        }
    }

    #[test]
    fn the_sky_is_drawn_before_the_road() {
        let mut scene = Scene::new(SceneConfig::default());
        scene.update(0.03);
        let commands = record(&scene, VIEWPORT);
        let first_road = commands.iter().position(|c| !c.is_sky()).unwrap();
        assert!(first_road > 100);
        assert!(commands[first_road..].iter().all(|c| !matches!(
            c,
            Command::Dot { .. } | Command::Band { .. } | Command::Streak { .. }
        )));
        assert!(matches!(commands[0], Command::Band { .. }));
    }

    #[test]
    fn right_hand_traffic_mirrors_the_picture() {
        let left = Scene::new(SceneConfig::default());
        let right = Scene::new(SceneConfig {
            left_hand_traffic: false,
            ..Default::default()
        });
        let (a, b) = (record(&left, VIEWPORT), record(&right, VIEWPORT));
        assert_eq!(a.len(), b.len());
        // Every light lands on the mirrored side of the screen center.
        for (left, right) in a.iter().zip(&b) {
            assert_eq!(std::mem::discriminant(left), std::mem::discriminant(right));
            if let (Some((xl, yl)), Some((xr, yr))) = (left.position(), right.position()) {
                assert!((xl + xr - VIEWPORT.width).abs() < 1e-2 && yl == yr);
            }
        }
    }

    /// Far road lights bend one way on the screen and the backdrop slides
    /// the other way, for both traffic sides.
    #[test]
    fn the_backdrop_turns_against_the_bend_of_the_road() {
        for left_hand_traffic in [true, false] {
            let side = if left_hand_traffic { 1.0f32 } else { -1.0 };
            let mut checked = 0;
            for seed in 0..40 {
                let mut scene = Scene::new(SceneConfig {
                    seed,
                    left_hand_traffic,
                    ..Default::default()
                });
                scene.update(0.016);
                let curvature = scene.road.curvature_at(scene.track_s);
                if curvature.abs() < 0.001 {
                    continue;
                }
                // Where the road just ahead lies on the screen. Far ahead it
                // may bend the other way than at the car.
                let projector = Projector::new(1280.0, 720.0, left_hand_traffic);
                let (bend, rise) = RoadView::new(&scene.road, scene.track_s).offset(20.0);
                let straight = projector.project(20.0, 0.0, 0.0).unwrap().x;
                let road_x = projector.project(20.0, bend, rise).unwrap().x - straight;
                // Where a fixed point of the backdrop goes while turning.
                let view = |scene: &Scene| SkyView {
                    width: 1280.0,
                    height: 720.0,
                    pixel_scale: 1.0,
                    focal: projector.focal(),
                    side,
                    heading: scene.heading(),
                    time: 0.0,
                };
                let before = view(&scene).screen_x(0.0);
                scene.update(0.05);
                let moved = view(&scene).screen_x(0.0) - before;
                assert!(road_x != 0.0 && moved != 0.0);
                assert!(road_x * moved < 0.0, "seed {seed}: {road_x} {moved}");
                checked += 1;
            }
            assert!(checked > 5, "too few curved roads: {checked}");
        }
    }

    #[test]
    fn heading_stays_within_a_turn() {
        let mut scene = Scene::new(SceneConfig {
            seed: 3,
            speed_kmh: 300.0,
            ..Default::default()
        });
        for _ in 0..20_000 {
            scene.update(0.05);
            assert!((0.0..TAU).contains(&scene.heading()));
        }
    }

    #[test]
    fn lights_are_ordered_from_far_to_near() {
        let mut scene = Scene::new(SceneConfig::default());
        scene.update(0.03);
        let lights = scene.lights();
        assert!(lights.len() > 100);
        assert!(lights.windows(2).all(|w| w[0].d >= w[1].d));
        assert!(
            lights
                .iter()
                .all(|l| (NEAR_DISTANCE..VIEW_DISTANCE).contains(&l.d))
        );
    }

    #[test]
    fn other_cars_are_part_of_the_picture() {
        let mut scene = Scene::new(SceneConfig::default());
        let (mut red, mut white_near_lane) = (false, false);
        for _ in 0..400 {
            scene.update(0.05);
            for light in scene.lights() {
                red |= light.color == Rgb::new(255, 35, 20);
                // Only oncoming cars drive beyond the median at 7 m.
                white_near_lane |= light.lateral > 6.0 && light.height == 0.65;
            }
        }
        assert!(red && white_near_lane);
    }

    #[test]
    fn update_caps_long_steps_and_ignores_invalid_ones() {
        let mut scene = Scene::new(SceneConfig {
            speed_kmh: 72.0,
            ..Default::default()
        });
        scene.update(10.0);
        let capped = scene.track_s();
        assert!((capped - 20.0 * f64::from(MAX_STEP_SECONDS)).abs() < 1e-6);
        scene.update(f32::NAN);
        scene.update(-1.0);
        assert_eq!(scene.track_s(), capped);
    }

    #[test]
    fn dimmer_when_smaller_than_the_minimum_radius() {
        let scene = Scene::new(SceneConfig::default());
        for command in record(&scene, VIEWPORT) {
            if let Command::Glow { radius, .. } = command {
                assert!(radius >= MIN_CORE_RADIUS - 1e-6);
            }
        }
    }

    #[test]
    fn invalid_pixel_scales_do_not_panic() {
        let scene = Scene::new(SceneConfig::default());
        for pixel_scale in [0.0, -1.0, f32::NAN, f32::INFINITY] {
            record(
                &scene,
                Viewport {
                    pixel_scale,
                    ..VIEWPORT
                },
            );
        }
    }

    #[test]
    fn small_viewports_are_handled() {
        let scene = Scene::new(SceneConfig::default());
        let tiny = Viewport {
            width: 1.0,
            height: 1.0,
            pixel_scale: 1.0,
        };
        record(&scene, tiny);
    }
}
