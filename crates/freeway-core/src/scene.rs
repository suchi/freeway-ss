//! The scene: advances the journey and draws the sky, the lights of the road and other cars.

use std::f64::consts::TAU;

use crate::canvas::{Canvas, GLOW_HALO_RATIO, Rgb};
use crate::cruise::Drift;
use crate::projection::{CAMERA_HEIGHT, NEAR_DISTANCE, Projector, VIEW_DISTANCE};
use crate::road::{RoadShape, RoadView};
use crate::scenery::Zones;
use crate::sky::{Sky, SkyView};
use crate::traffic::{CarBodies, MINUTES_PER_DAY, Traffic, TrafficLevel, TruckStyle};
use crate::tunnel::{Tunnel, TunnelLighting, Tunnels, draw_entrance};
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
    /// Whether mountains stand on the horizon.
    pub mountains: bool,
    /// How the trucks among the other cars are lit; `None` means no trucks.
    pub trucks: TruckStyle,
    /// How the lamps of the tunnels are colored; `None` means no tunnels.
    pub tunnels: TunnelLighting,
    /// Whether the other cars have bodies that hide the lights behind them.
    pub car_bodies: CarBodies,
    /// Whether the speed drifts slowly around `speed_kmh`.
    pub vary_speed: bool,
    /// Whether the traffic follows the time of day, given by `start_minute`.
    pub traffic_by_time: bool,
    /// Whether the road alternates between mountains and plains.
    pub scenery_zones: bool,
    /// The local time at the start of the journey, in minutes after
    /// midnight. The scene does not read a clock: the host gives the time.
    pub start_minute: u32,
    pub seed: u64,
}

impl Default for SceneConfig {
    fn default() -> Self {
        Self {
            speed_kmh: 100.0,
            left_hand_traffic: true,
            ufo_mean_interval: None,
            mountains: true,
            trucks: TruckStyle::default(),
            tunnels: TunnelLighting::default(),
            car_bodies: CarBodies::default(),
            // The three kinds of variation are off here, which is the road
            // as it was before them. The settings turn them on by default.
            vary_speed: false,
            traffic_by_time: false,
            scenery_zones: false,
            start_minute: 12 * 60,
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

/// Lamps of a tunnel are twice as dense this far from the entrance, in meters.
const ENTRANCE_LENGTH: f64 = 100.0;
const TUNNEL_LAMP_SPACING: f64 = 8.0;
const EMERGENCY_RED: Rgb = Rgb::new(255, 40, 25);

/// A row of lights along a tunnel, from its entrance on.
struct TunnelRow {
    lateral: f32,
    height: f32,
    spacing: f64,
    range: f32,
    color: Rgb,
    alpha: f32,
    kind: Kind,
    /// Whether more lights stand between them near the entrance.
    doubled_at_entrance: bool,
}

/// The rows of a tunnel with lamps of the given color. The lamps hang at the
/// walls, the pools of light lie in the lanes and the red lights, of the
/// emergency equipment, stand at the left wall.
fn tunnel_rows(lamp: Rgb) -> [TunnelRow; 5] {
    let lamp_row = |lateral| TunnelRow {
        lateral,
        height: 5.0,
        spacing: TUNNEL_LAMP_SPACING,
        range: 450.0,
        color: lamp,
        alpha: 0.9,
        kind: Kind::Glow { size: 0.3 },
        doubled_at_entrance: true,
    };
    let pool_row = |lateral| TunnelRow {
        lateral,
        height: 0.0,
        spacing: TUNNEL_LAMP_SPACING,
        range: 220.0,
        color: lamp,
        alpha: 0.16,
        kind: Kind::Pool { radius: 4.0 },
        doubled_at_entrance: false,
    };
    [
        lamp_row(-4.6),
        lamp_row(5.1),
        pool_row(-1.75),
        pool_row(1.75),
        TunnelRow {
            lateral: -4.9,
            height: 1.2,
            spacing: 50.0,
            range: 300.0,
            color: EMERGENCY_RED,
            alpha: 0.8,
            kind: Kind::Glow { size: 0.12 },
            doubled_at_entrance: false,
        },
    ]
}

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
    tunnels: Tunnels,
    drift: Drift,
    zones: Zones,
    /// How tall the mountains stand at the current position, `0..1`.
    mountain_level: f32,
}

/// The local time after `time` seconds of driving, in minutes after
/// midnight (`0..1440`).
fn minute_of_day(config: &SceneConfig, time: f64) -> f32 {
    let day = f64::from(MINUTES_PER_DAY);
    let start = f64::from(config.start_minute).rem_euclid(day);
    (start + time / 60.0).rem_euclid(day) as f32
}

/// How busy the road is after `time` seconds of driving.
fn traffic_level(config: &SceneConfig, time: f64) -> TrafficLevel {
    if config.traffic_by_time {
        TrafficLevel::at_minute(minute_of_day(config, time))
    } else {
        TrafficLevel::FULL
    }
}

impl Scene {
    pub fn new(config: SceneConfig) -> Self {
        let mut zones = Zones::new(config.seed, config.scenery_zones);
        let mountain_level = zones.mountain_level(0.0);
        let mut sky = Sky::new(config.seed, config.mountains);
        sky.set_mountain_level(mountain_level);
        Self {
            config,
            road: RoadShape::new(config.seed),
            track_s: 0.0,
            heading: 0.0,
            time: 0.0,
            sky,
            // The cars start out as busy as the road is at the start time.
            traffic: Traffic::with_level(config.seed, config.trucks, traffic_level(&config, 0.0)),
            ufo: Ufo::new(config.seed, config.ufo_mean_interval),
            tunnels: Tunnels::with_zones(config.seed, config.tunnels, config.scenery_zones),
            drift: Drift::new(config.seed, config.speed_kmh, config.vary_speed),
            zones,
            mountain_level,
        }
    }

    /// The speed now, in km/h: the chosen one, plus the drift if it is on.
    pub fn speed_kmh(&self) -> f32 {
        self.config.speed_kmh + self.drift.offset()
    }

    /// The local time now, in minutes after midnight (`0..1440`): the start
    /// time plus the time driven.
    pub fn minute_of_day(&self) -> f32 {
        minute_of_day(&self.config, self.time)
    }

    /// How tall the mountains stand now, from 0 (plains) to 1 (mountains).
    pub fn mountain_level(&self) -> f32 {
        self.mountain_level
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

    /// Whether the car faces a range of mountains now.
    pub fn mountains_ahead(&self) -> bool {
        self.sky
            .mountains()
            .is_some_and(|m| m.has_range_at(self.heading))
    }

    /// Distance in meters to the entrance of the tunnel ahead, if it is in sight.
    pub fn tunnel_entrance_ahead(&self) -> Option<f32> {
        self.tunnels.entrance_ahead(self.track_s).map(|(_, d)| d)
    }

    /// Whether the car is in a tunnel now.
    pub fn in_tunnel(&self) -> bool {
        self.tunnels.sky_hidden(self.track_s)
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
        self.drift.update(dt);
        let speed_kmh = self.speed_kmh();
        let step = f64::from(speed_kmh / 3.6 * dt);
        // The car turns by the curvature of the road it is driving on.
        let turn = f64::from(self.road.curvature_at(self.track_s)) * step;
        self.heading = (self.heading + turn).rem_euclid(TAU);
        self.track_s += step;
        self.time += f64::from(dt);
        self.sky.update(self.time, self.heading);
        self.ufo.update(self.time, self.heading, self.side());
        self.traffic
            .set_level(traffic_level(&self.config, self.time));
        self.traffic.update(dt, speed_kmh / 3.6);
        self.tunnels.update(self.track_s);
        self.mountain_level = self.zones.mountain_level(self.track_s);
        self.sky.set_mountain_level(self.mountain_level);
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
        // Inside a tunnel there is no sky to see.
        if !self.tunnels.sky_hidden(self.track_s) {
            self.sky.draw(&sky_view, canvas);
            // The UFO is far away: the lights of the road and of other cars lie in front of it.
            self.ufo.draw(&sky_view, canvas);
        }
        // The hill of an entrance hides the sky behind it, but not the lights in front of it.
        if let Some((tunnel, d)) = self.tunnels.entrance_ahead(self.track_s) {
            draw_entrance(tunnel, d, &projector, &view, canvas);
        }

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
                // The street lamps of the road outside do not light a tunnel.
                if matches!(row.kind, Kind::Pool { .. }) && self.tunnels.contains(s) {
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
        // Only what fits through a tunnel is seen along it.
        lights.retain(|l| !self.tunnels.hides(self.track_s, l.d, l.lateral, l.height));
        for tunnel in self.tunnels.ahead() {
            self.tunnel_lights(tunnel, &mut lights);
        }
        lights.sort_by(|a, b| b.d.total_cmp(&a.d));
        lights
    }

    /// Adds the lamps, the light on the road and the emergency lights of a tunnel.
    fn tunnel_lights(&self, tunnel: &Tunnel, lights: &mut Vec<Light>) {
        for row in tunnel_rows(tunnel.lamp) {
            // The lamps of the entrance are closer together than those inside.
            let mut push = |offset: f64, spacing: f64, length: f64| {
                let first = (self.track_s - tunnel.start - offset) / spacing;
                let mut index = first.ceil().max(0.0);
                loop {
                    let along = offset + index * spacing;
                    if along >= length {
                        break;
                    }
                    index += 1.0;
                    let d = (tunnel.start + along - self.track_s) as f32;
                    if d >= row.range {
                        break;
                    }
                    if d < NEAR_DISTANCE {
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
            };
            let length = tunnel.end - tunnel.start;
            push(0.0, row.spacing, length);
            if row.doubled_at_entrance {
                push(row.spacing * 0.5, row.spacing, ENTRANCE_LENGTH.min(length));
            }
        }
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
        Silhouette {
            points: Vec<(f32, f32)>,
            color: Rgb,
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
                Command::Band { .. } | Command::Silhouette { .. } | Command::Beam { .. } => None,
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
        fn silhouette(&mut self, points: &[(f32, f32)], color: Rgb, alpha: f32) {
            self.0.push(Command::Silhouette {
                points: points.to_vec(),
                color,
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

    /// Checks that a command is valid and not wholly off screen. The sky
    /// keeps its own tests of the geometry; here only the road's lights are
    /// checked for what lies off screen.
    fn assert_valid(command: &Command) {
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
            Command::Silhouette {
                ref points, alpha, ..
            } => {
                assert!(alpha > 0.0 && alpha <= 1.0, "{alpha}");
                assert!(points.len() >= 3);
                assert!(points.iter().all(|(x, y)| x.is_finite() && y.is_finite()));
                return;
            }
            Command::Band { alpha, .. }
            | Command::Streak { alpha, .. }
            | Command::Beam { alpha, .. } => {
                assert!(alpha > 0.0 && alpha <= 1.0, "{command:?}");
                return;
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
            commands.iter().for_each(assert_valid);
        }
    }

    #[test]
    fn a_whole_tunnel_draws_valid_commands() {
        let mut scene = Scene::new(SceneConfig {
            seed: 11,
            speed_kmh: 160.0,
            ..Default::default()
        });
        let (mut entrance, mut inside, mut checked) = (0, 0, 0);
        for step in 0..40_000 {
            scene.update(0.05);
            if scene.tunnel_entrance_ahead().is_none() && !scene.in_tunnel() {
                if inside > 0 {
                    break;
                }
                continue;
            }
            // Every frame near the entrance, and every few inside.
            if scene.in_tunnel() {
                inside += 1;
            } else {
                entrance += 1;
            }
            if step % 3 == 0 || scene.tunnel_entrance_ahead().is_some_and(|d| d < 30.0) {
                for viewport in [
                    VIEWPORT,
                    Viewport {
                        width: 152.0,
                        height: 112.0,
                        pixel_scale: 1.0,
                    },
                ] {
                    let commands = record(&scene, viewport);
                    assert!(!commands.is_empty());
                    if viewport.width == VIEWPORT.width {
                        commands.iter().for_each(assert_valid);
                    }
                }
                checked += 1;
            }
        }
        assert!(
            entrance > 100 && inside > 100 && checked > 100,
            "{entrance} {inside} {checked}"
        );
    }

    #[test]
    fn mountains_are_drawn_only_when_the_setting_is_on() {
        let silhouettes = |scene: &Scene| {
            record(scene, VIEWPORT)
                .iter()
                .filter(|c| matches!(c, Command::Silhouette { .. }))
                .count()
        };
        // No tunnels: they would hide the mountains for a while.
        let mut on = Scene::new(SceneConfig {
            seed: 8,
            tunnels: TunnelLighting::None,
            ..Default::default()
        });
        let mut off = Scene::new(SceneConfig {
            seed: 8,
            mountains: false,
            tunnels: TunnelLighting::None,
            ..Default::default()
        });
        let mut seen = 0;
        // 40 minutes of driving turns the car through every direction.
        for step in 0..48_000 {
            on.update(0.05);
            off.update(0.05);
            // Drawing is the slow part; every two seconds is often enough.
            if step % 40 == 0 {
                let count = silhouettes(&on);
                seen += count;
                // Facing a range always shows it.
                assert!(count > 0 || !on.mountains_ahead());
                assert_eq!(silhouettes(&off), 0);
                assert!(!off.mountains_ahead());
            }
        }
        assert!(seen > 0);
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

    /// A scene whose camera is `before` meters short of the entrance of the
    /// first tunnel (inside it when `before` is negative).
    fn near_first_tunnel(lighting: TunnelLighting, before: f64) -> (Scene, Tunnel) {
        let mut scene = Scene::new(SceneConfig {
            seed: 11,
            tunnels: lighting,
            // The mountains are silhouettes too.
            mountains: false,
            ..Default::default()
        });
        // The first tunnel is not in sight at the start: drive on until it is.
        let mut s = 0.0;
        while scene.tunnels.ahead().next().is_none() {
            s += 500.0;
            scene.tunnels.update(s);
        }
        let tunnel = *scene.tunnels.ahead().next().unwrap();
        scene.track_s = tunnel.start - before;
        scene.tunnels.update(scene.track_s);
        scene.traffic.update(0.0, 0.0);
        (scene, tunnel)
    }

    fn lamp_distances(scene: &Scene, lateral: f32) -> Vec<f32> {
        let mut d: Vec<f32> = scene
            .lights()
            .iter()
            .filter(|l| l.lateral == lateral && l.height == 5.0)
            .map(|l| l.d)
            .collect();
        d.sort_by(f32::total_cmp);
        d
    }

    #[test]
    fn only_what_fits_through_a_tunnel_is_seen_beyond_its_entrance() {
        let (scene, tunnel) = near_first_tunnel(TunnelLighting::Led, 300.0);
        let lights = scene.lights();
        let beyond: Vec<&Light> = lights.iter().filter(|l| l.d > 300.0).collect();
        assert!(!beyond.is_empty());
        // The lights of the road outside are gone behind the entrance...
        for light in &beyond {
            let is_lamp = light.height == 5.0 && (light.lateral == -4.6 || light.lateral == 5.1);
            assert!(
                crate::tunnel::fits_through(light.lateral, light.height) || is_lamp,
                "lateral {} height {}",
                light.lateral,
                light.height
            );
        }
        // ...what is in the tunnel's way stays: the delineators, and its own lights.
        assert!(beyond.iter().any(|l| l.lateral == -4.0));
        assert!(beyond.iter().any(|l| l.lateral == 3.9));
        assert!(beyond.iter().any(|l| l.color == tunnel.lamp));
        // In front of the entrance nothing is hidden.
        let before = lights.iter().filter(|l| l.d < 290.0);
        assert!(before.clone().any(|l| l.lateral == 13.0));
        assert!(before.clone().any(|l| l.lateral == 4.5 && l.height == 10.0));
    }

    #[test]
    fn oncoming_cars_are_not_seen_through_a_tunnel() {
        let (mut scene, _) = near_first_tunnel(TunnelLighting::Led, 300.0);
        let mut same_direction = false;
        for _ in 0..600 {
            scene.traffic.update(0.05, 27.0);
            for light in scene.lights() {
                if light.d > 300.0 {
                    assert!(light.lateral < 5.5, "{}", light.lateral);
                    same_direction |=
                        light.lateral == -1.75 && light.color == Rgb::new(255, 35, 20);
                }
            }
        }
        assert!(same_direction);
    }

    #[test]
    fn inside_a_tunnel_only_lights_shine_and_there_is_no_sky() {
        let (scene, tunnel) = near_first_tunnel(TunnelLighting::Sodium, -200.0);
        assert!(scene.tunnels.sky_hidden(scene.track_s));
        let commands = record(&scene, VIEWPORT);
        assert!(!commands.is_empty());
        assert!(
            commands
                .iter()
                .all(|c| matches!(c, Command::Glow { .. } | Command::Pool { .. }))
        );
        // Lamps in both rows, lit orange, with the pools of light beneath.
        assert!(!lamp_distances(&scene, -4.6).is_empty());
        assert!(!lamp_distances(&scene, 5.1).is_empty());
        assert!(
            scene
                .lights()
                .iter()
                .filter(|l| l.height == 5.0)
                .all(|l| l.color == tunnel.lamp)
        );
        // The street lamps' pools of the road outside are not in the tunnel.
        let exit = (tunnel.end - scene.track_s) as f32;
        assert!(
            scene
                .lights()
                .iter()
                .filter(|l| l.d < exit && matches!(l.kind, Kind::Pool { .. }))
                .all(|l| l.color == tunnel.lamp || l.color == WHITE)
        );
    }

    #[test]
    fn the_sky_returns_after_the_exit_and_is_there_before_the_entrance() {
        let (before, tunnel) = near_first_tunnel(TunnelLighting::Led, 100.0);
        assert!(!before.tunnels.sky_hidden(before.track_s));
        assert!(matches!(record(&before, VIEWPORT)[0], Command::Band { .. }));
        let (mut after, _) = near_first_tunnel(TunnelLighting::Led, -100.0);
        after.track_s = tunnel.end + 1.0;
        after.tunnels.update(after.track_s);
        assert!(!after.tunnels.sky_hidden(after.track_s));
        assert!(matches!(record(&after, VIEWPORT)[0], Command::Band { .. }));
    }

    #[test]
    fn the_lamps_stand_every_eight_meters_and_closer_at_the_entrance() {
        let (scene, tunnel) = near_first_tunnel(TunnelLighting::Led, 50.0);
        let d = lamp_distances(&scene, -4.6);
        // The entrance is 50 meters ahead: lamps every 4 meters for 100
        // meters, then every 8 meters.
        let inside = |from: f32, to: f32| -> Vec<f32> {
            d.iter().copied().filter(|&d| d >= from && d < to).collect()
        };
        let near = inside(50.0, 150.0);
        assert_eq!(near.len(), 25, "{near:?}");
        assert!(near.windows(2).all(|w| (w[1] - w[0] - 4.0).abs() < 1e-3));
        let far = inside(160.0, 400.0);
        assert!(far.len() > 25);
        assert!(far.windows(2).all(|w| (w[1] - w[0] - 8.0).abs() < 1e-3));
        // Nothing stands beyond the exit.
        assert!(
            d.iter()
                .all(|&d| f64::from(d) <= tunnel.end - scene.track_s)
        );
    }

    #[test]
    fn red_emergency_lights_stand_every_fifty_meters() {
        let (scene, _) = near_first_tunnel(TunnelLighting::Led, -100.0);
        let mut d: Vec<f32> = scene
            .lights()
            .iter()
            .filter(|l| l.lateral == -4.9 && l.height == 1.2)
            .map(|l| l.d)
            .collect();
        d.sort_by(f32::total_cmp);
        assert!(d.len() >= 4);
        assert!(d.windows(2).all(|w| (w[1] - w[0] - 50.0).abs() < 1e-3));
    }

    #[test]
    fn no_tunnels_means_no_tunnel_lights_for_any_seed() {
        for seed in 0..12 {
            let mut scene = Scene::new(SceneConfig {
                seed,
                tunnels: TunnelLighting::None,
                ..Default::default()
            });
            for _ in 0..400 {
                scene.track_s += 40.0;
                scene.tunnels.update(scene.track_s);
                assert_eq!(scene.tunnels.ahead().count(), 0);
                assert!(!scene.tunnels.sky_hidden(scene.track_s));
                assert!(scene.lights().iter().all(|l| l.height != 5.0));
            }
        }
    }

    #[test]
    fn the_picture_is_the_same_with_and_without_tunnels_while_none_is_near() {
        let none = Scene::new(SceneConfig {
            tunnels: TunnelLighting::None,
            ..Default::default()
        });
        let mixed = Scene::new(SceneConfig {
            tunnels: TunnelLighting::Mixed,
            ..Default::default()
        });
        // The first tunnel is at least 1,000 meters away at the start.
        assert_eq!(record(&none, VIEWPORT), record(&mixed, VIEWPORT));
    }

    fn silhouettes(commands: &[Command]) -> Vec<(&Vec<(f32, f32)>, Rgb)> {
        commands
            .iter()
            .filter_map(|c| match c {
                Command::Silhouette { points, color, .. } => Some((points, *color)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn an_entrance_shows_a_hill_with_an_opening_within_sight() {
        for before in [599.0, 500.0, 300.0, 100.0, 20.0, 1.5] {
            let (scene, _) = near_first_tunnel(TunnelLighting::Led, before);
            let commands = record(&scene, VIEWPORT);
            let shapes = silhouettes(&commands);
            assert_eq!(shapes.len(), 2, "{before}");
            let (hill, opening) = (shapes[0].0, shapes[1].0);
            assert!(hill.len() >= 3 && opening.len() >= 3);
            assert!(
                hill.iter()
                    .chain(opening)
                    .all(|(x, y)| x.is_finite() && y.is_finite()),
                "{before}"
            );
            let top =
                |points: &Vec<(f32, f32)>| points.iter().map(|p| p.1).fold(f32::MAX, f32::min);
            let width = |points: &Vec<(f32, f32)>| {
                let xs = points.iter().map(|p| p.0);
                xs.clone().fold(f32::MIN, f32::max) - xs.fold(f32::MAX, f32::min)
            };
            // The hill rises above the opening and is far wider than it.
            assert!(top(hill) < top(opening), "{before}");
            assert!(width(hill) > 10.0 * width(opening), "{before}");
        }
    }

    #[test]
    fn the_hill_and_its_opening_are_fully_opaque() {
        let (scene, _) = near_first_tunnel(TunnelLighting::Led, 100.0);
        let alphas: Vec<f32> = record(&scene, VIEWPORT)
            .iter()
            .filter_map(|c| match c {
                Command::Silhouette { alpha, .. } => Some(*alpha),
                _ => None,
            })
            .collect();
        assert!(alphas.len() >= 2);
        assert!(alphas.iter().all(|&a| a == 1.0), "{alphas:?}");
    }

    #[test]
    fn there_is_no_entrance_far_away_or_once_it_is_passed() {
        for before in [650.0, 601.0, 0.5, 0.0, -50.0, -400.0] {
            let (scene, _) = near_first_tunnel(TunnelLighting::Led, before);
            assert!(
                silhouettes(&record(&scene, VIEWPORT)).is_empty(),
                "{before}"
            );
        }
        // And without tunnels there is never one.
        let (mut scene, _) = near_first_tunnel(TunnelLighting::Led, 300.0);
        scene.config.tunnels = TunnelLighting::None;
        scene.tunnels = Tunnels::new(scene.config.seed, TunnelLighting::None);
        assert!(silhouettes(&record(&scene, VIEWPORT)).is_empty());
    }

    #[test]
    fn the_entrance_is_dark_and_tinted_by_the_lamps() {
        let (led, _) = near_first_tunnel(TunnelLighting::Led, 300.0);
        let (sodium, _) = near_first_tunnel(TunnelLighting::Sodium, 300.0);
        let (led, sodium) = (record(&led, VIEWPORT), record(&sodium, VIEWPORT));
        let (led, sodium) = (silhouettes(&led), silhouettes(&sodium));
        // The hill is as dark as the mountains, the opening is darker still.
        assert_eq!(led[0].1, Rgb::new(5, 7, 15));
        assert_eq!(sodium[0].1, Rgb::new(5, 7, 15));
        assert_eq!(led[1].1, Rgb::new(6, 8, 12));
        assert_eq!(sodium[1].1, Rgb::new(10, 7, 5));
    }

    #[test]
    fn the_entrance_fades_in_from_the_dark_of_the_night() {
        let color_at = |before: f64| {
            let (scene, _) = near_first_tunnel(TunnelLighting::Led, before);
            let commands = record(&scene, VIEWPORT);
            silhouettes(&commands)[0].1
        };
        let background = Rgb::new(2, 3, 10);
        let far = color_at(599.9);
        assert!(far.r.abs_diff(background.r) <= 1 && far.b.abs_diff(background.b) <= 1);
        // Brightest channel (blue) rises steadily from 10 to 15.
        let blues: Vec<u8> = [599.9, 560.0, 520.0, 480.0, 450.0]
            .map(|d| color_at(d).b)
            .to_vec();
        assert!(blues.windows(2).all(|w| w[0] <= w[1]), "{blues:?}");
        assert_eq!(color_at(450.0), Rgb::new(5, 7, 15));
        assert_eq!(color_at(200.0), Rgb::new(5, 7, 15));
    }

    #[test]
    fn the_entrance_is_drawn_after_the_sky_and_before_the_lights() {
        // With the mountains on, the sky has silhouettes of its own.
        let mut scene = Scene::new(SceneConfig {
            seed: 11,
            ..Default::default()
        });
        let mut s = 0.0;
        while scene.tunnels.ahead().next().is_none() {
            s += 500.0;
            scene.tunnels.update(s);
        }
        let start = scene.tunnels.ahead().next().unwrap().start;
        scene.track_s = start - 300.0;
        scene.tunnels.update(scene.track_s);
        let commands = record(&scene, VIEWPORT);
        let hill_commands: Vec<usize> = commands
            .iter()
            .enumerate()
            .filter(|(_, c)| matches!(c, Command::Silhouette { .. }))
            .map(|(i, _)| i)
            .collect();
        let first_pool = commands
            .iter()
            .position(|c| matches!(c, Command::Pool { .. }))
            .unwrap();
        // The last two silhouettes are the hill and its opening; the sky
        // (stars, glow of the city) is already drawn, the lights come after.
        let [.., hill, opening] = hill_commands[..] else {
            panic!("no entrance");
        };
        assert_eq!(opening, hill + 1);
        assert!(hill > 100 && opening < first_pool);
        // Nothing of the sky comes after the hill.
        assert!(
            commands[hill..]
                .iter()
                .all(|c| !matches!(c, Command::Dot { .. } | Command::Band { .. }))
        );
    }

    #[test]
    fn driving_through_a_tunnel_shows_its_entrance_then_the_inside_then_the_sky() {
        let mut scene = Scene::new(SceneConfig {
            seed: 11,
            speed_kmh: 160.0,
            tunnels: TunnelLighting::Mixed,
            ..Default::default()
        });
        let (mut sighted, mut inside, mut left) = (false, false, false);
        for _ in 0..40_000 {
            scene.update(0.05);
            if scene.tunnel_entrance_ahead().is_some() {
                sighted = true;
            }
            if scene.in_tunnel() {
                // The entrance is drawn only until the car reaches it.
                assert!(sighted && scene.tunnel_entrance_ahead().is_none());
                inside = true;
            } else if inside {
                left = true;
                break;
            }
        }
        assert!(left);
    }

    /// A scene with every kind of variation on.
    fn varying(seed: u64) -> SceneConfig {
        SceneConfig {
            seed,
            vary_speed: true,
            traffic_by_time: true,
            scenery_zones: true,
            ..Default::default()
        }
    }

    #[test]
    fn the_variations_are_off_by_default_in_the_scene_config() {
        let config = SceneConfig::default();
        assert!(!config.vary_speed && !config.traffic_by_time && !config.scenery_zones);
    }

    #[test]
    fn without_the_variations_the_speed_is_the_chosen_one() {
        let mut scene = Scene::new(SceneConfig {
            speed_kmh: 120.0,
            ..Default::default()
        });
        for _ in 0..20_000 {
            scene.update(0.05);
            assert_eq!(scene.speed_kmh(), 120.0);
            assert_eq!(scene.mountain_level(), 1.0);
        }
        // 1,000 seconds at 120 km/h.
        assert!((scene.track_s() - 120.0 / 3.6 * 1000.0).abs() < 1.0);
    }

    #[test]
    fn the_varying_speed_stays_within_twenty_and_drives_the_car() {
        for (base, seed) in [(30.0, 1), (100.0, 2), (160.0, 3)] {
            let mut scene = Scene::new(SceneConfig {
                speed_kmh: base,
                ..varying(seed)
            });
            let (mut travelled, mut lowest, mut highest) = (0.0, f32::MAX, f32::MIN);
            for _ in 0..(3600.0 / 0.05) as usize {
                let before = scene.track_s();
                scene.update(0.05);
                let speed = scene.speed_kmh();
                assert!((speed - base).abs() <= 20.0 && (30.0..=160.0).contains(&speed));
                lowest = lowest.min(speed);
                highest = highest.max(speed);
                // The car moves at the speed that is shown.
                travelled += scene.track_s() - before;
                assert!(((scene.track_s() - before) - f64::from(speed / 3.6 * 0.05)).abs() < 1e-6);
            }
            assert!(travelled > 0.0);
            assert!(highest - lowest > 5.0, "{base}: {lowest} {highest}");
        }
    }

    #[test]
    fn the_time_of_day_runs_on_from_the_start_and_wraps_at_midnight() {
        let mut scene = Scene::new(SceneConfig {
            start_minute: 23 * 60 + 50,
            ..Default::default()
        });
        assert_eq!(scene.minute_of_day(), 1430.0);
        // 15 minutes of driving, in steps.
        for _ in 0..(15.0 * 60.0 / 0.05) as usize {
            scene.update(0.05);
        }
        assert!(
            (scene.minute_of_day() - 5.0).abs() < 0.01,
            "{}",
            scene.minute_of_day()
        );
        // A start time out of range is taken modulo a day.
        let late = Scene::new(SceneConfig {
            start_minute: 3 * 1440 + 90,
            ..Default::default()
        });
        assert_eq!(late.minute_of_day(), 90.0);
    }

    /// Cars that run in the traffic of a scene at the given hour, over a stretch of driving.
    fn running_cars(hour: u32, seed: u64) -> usize {
        let mut scene = Scene::new(SceneConfig {
            start_minute: hour * 60,
            ..varying(seed)
        });
        let mut total = 0;
        for step in 0..4000 {
            scene.update(0.05);
            if step % 50 == 0 {
                total += scene.traffic.running_count();
            }
        }
        total
    }

    #[test]
    fn the_night_is_quieter_than_the_day() {
        let (night, evening, day): (usize, usize, usize) = (
            (0..8).map(|seed| running_cars(2, seed)).sum(),
            (0..8).map(|seed| running_cars(22, seed)).sum(),
            (0..8).map(|seed| running_cars(12, seed)).sum(),
        );
        assert!(night < evening && evening < day, "{night} {evening} {day}");
    }

    #[test]
    fn the_traffic_is_the_same_all_day_without_the_variation() {
        let run = |hour: u32| {
            let mut scene = Scene::new(SceneConfig {
                start_minute: hour * 60,
                seed: 4,
                ..Default::default()
            });
            for _ in 0..2000 {
                scene.update(0.05);
            }
            record(&scene, VIEWPORT)
        };
        assert_eq!(run(2), run(12));
        assert_eq!(run(2), run(22));
    }

    #[test]
    fn the_traffic_follows_the_time_driven_across_a_band() {
        // Driving from 6:50 to 7:10 crosses into the full traffic.
        let mut scene = Scene::new(SceneConfig {
            start_minute: 6 * 60 + 50,
            ..varying(3)
        });
        assert!(scene.traffic.running_count() < scene.traffic.car_count());
        for _ in 0..(30.0 * 60.0 / 0.05) as usize {
            scene.update(0.05);
        }
        assert_eq!(scene.traffic.running_count(), scene.traffic.car_count());
    }

    /// A scene that starts on plains, and the distance at which they end.
    fn on_plains() -> (Scene, f64) {
        for seed in 0.. {
            let config = varying(seed);
            let mut zones = Zones::new(seed, true);
            let zone = zones.zone_at(0.0);
            if zone.kind == crate::scenery::ZoneKind::Plains {
                let _ = config;
                return (Scene::new(config), zone.end);
            }
        }
        unreachable!()
    }

    #[test]
    fn the_plains_have_no_mountains_no_tunnels_and_the_mountains_come_back() {
        let (mut scene, plains_end) = on_plains();
        assert_eq!(scene.mountain_level(), 0.0);
        let (mut tunnel_on_plains, mut risen) = (false, false);
        while scene.track_s() < plains_end + 2000.0 {
            scene.update(0.05);
            if scene.track_s() < plains_end - 1000.0 {
                assert_eq!(scene.mountain_level(), 0.0);
                tunnel_on_plains |= scene.in_tunnel() || scene.tunnel_entrance_ahead().is_some();
            }
            risen |= scene.mountain_level() == 1.0;
        }
        assert!(!tunnel_on_plains);
        assert!(risen);
    }

    #[test]
    fn the_sky_over_the_plains_has_no_silhouette() {
        let (mut scene, _) = on_plains();
        for _ in 0..200 {
            scene.update(0.05);
        }
        assert!(silhouettes(&record(&scene, VIEWPORT)).is_empty());
    }

    #[test]
    fn zones_and_tunnels_agree_on_where_the_mountains_are() {
        // Whenever the car is in a tunnel, the mountains stand at full height.
        for seed in 0..4 {
            let mut scene = Scene::new(SceneConfig {
                speed_kmh: 160.0,
                ..varying(seed)
            });
            let mut seen = 0;
            for _ in 0..(3.0 * 3600.0 / 0.05) as usize {
                scene.update(0.05);
                if scene.in_tunnel() {
                    seen += 1;
                    assert_eq!(scene.mountain_level(), 1.0, "{seed}");
                }
            }
            assert!(seen > 0, "{seed}");
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
