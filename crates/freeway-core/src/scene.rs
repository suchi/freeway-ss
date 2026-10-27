//! The scene: advances the journey and draws the sky, the lights of the road and other cars.

use std::collections::BTreeMap;
use std::f64::consts::TAU;

use crate::body::{Corners, depth_at, faces_camera};
use crate::canvas::{Canvas, GLOW_HALO_RATIO, Rgb};
use crate::cruise::Drift;
use crate::projection::{CAMERA_HEIGHT, NEAR_DISTANCE, Projector, VIEW_DISTANCE};
use crate::road::{RoadShape, RoadView};
use crate::scenery::Zones;
use crate::sky::{Sky, SkyView};
use crate::traffic::{
    BodyBox, CarColor, CarShapes, FacePart, MINUTES_PER_DAY, Traffic, TrafficLevel, TruckStyle,
};
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
    /// How the bodies of the other cars are drawn; bodies hide the lights
    /// behind them.
    pub car_shapes: CarShapes,
    /// The color of the bodies.
    pub car_color: CarColor,
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
            car_shapes: CarShapes::default(),
            car_color: CarColor::default(),
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
    /// A face of the body of a car, given by its corners as (distance,
    /// lateral position, height). It covers what is drawn before it; the
    /// light's `alpha` is how much. `sight` is the point whose view through a
    /// tunnel tells whether the car is seen.
    Body {
        corners: Corners,
        sight: (f32, f32, f32),
    },
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
/// A light of a car is drawn this much nearer than the body, that is, after it.
const OWN_LIGHT_LEAD: f32 = 0.001;
/// The faces of a car body, a little lighter the more they face the sky.
const BODY_END: Rgb = Rgb::new(0, 0, 0);
const BODY_SIDE: Rgb = Rgb::new(14, 15, 18);
const BODY_ROOF: Rgb = Rgb::new(22, 23, 27);
/// Windows are darker and bluer than the paint.
const BODY_GLASS: Rgb = Rgb::new(12, 14, 20);

/// The street lamps on the median: where they stand across the road, how
/// high, how far apart along it, and how far their pools of light on the
/// road reach.
const LAMP_LATERAL: f32 = 4.5;
const LAMP_HEIGHT: f32 = 10.0;
const LAMP_SPACING: f64 = 45.0;
const LAMP_POOL_RANGE: f32 = 350.0;
/// The radius of the pools of light under the street lamps, in meters, and
/// where across the road the two pools of a lamp are.
const LAMP_POOL_RADIUS: f32 = 9.0;
const LAMP_POOL_LATERALS: [f32; 2] = [2.0, 7.0];
/// The lamp is not a point but this wide, in meters. The edge of a shadow is
/// soft by this much times the height of the edge over the road, divided by
/// how far the lamp is above that edge (see `Shadow::of`).
const LAMP_HEAD_RADIUS: f32 = 1.5;
/// A shadow is cut out of the pool of light in this many layers of the
/// pool, each cut by a shadow that is a little larger than the one before.
const SHADOW_LAYERS: usize = 5;
/// A convex shadow of a box has at most eight corners, and clipping it at the
/// near and far end of the view adds two.
const MAX_SHADOW_POINTS: usize = 10;

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
        LAMP_LATERAL,
        LAMP_HEIGHT,
        LAMP_SPACING,
        VIEW_DISTANCE,
        SODIUM,
        1.0,
        Kind::Glow { size: 0.3 },
    ),
    row(
        LAMP_POOL_LATERALS[0],
        0.0,
        LAMP_SPACING,
        LAMP_POOL_RANGE,
        SODIUM,
        0.3,
        Kind::Pool {
            radius: LAMP_POOL_RADIUS,
        },
    ),
    row(
        LAMP_POOL_LATERALS[1],
        0.0,
        LAMP_SPACING,
        LAMP_POOL_RANGE,
        SODIUM,
        0.3,
        Kind::Pool {
            radius: LAMP_POOL_RADIUS,
        },
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
    /// Where the light is drawn among the others, from the highest to the lowest.
    order: f32,
    /// The car the light or the body face belongs to.
    car: Option<u32>,
    lateral: f32,
    height: f32,
    color: Rgb,
    alpha: f32,
    kind: Kind,
    /// The street lamp, as its number along the road, if the light is the pool
    /// of light under it.
    lamp: Option<i64>,
    /// The shadows that cars cast on the pool of light, if it is one.
    shadows: Vec<Shadow>,
}

impl Light {
    /// Whether the light lies on the road surface (a pool of light or a
    /// glow on the ground), not on a car or a pole.
    fn on_road(&self) -> bool {
        self.height == 0.0 && matches!(self.kind, Kind::Glow { .. } | Kind::Pool { .. })
    }

    /// Where the light is drawn when cars have bodies, from the first to the
    /// last: the pools of light under the street lamps, the other lights on
    /// the road, and then the bodies and the lights above the road.
    fn layer(&self) -> u8 {
        match self.kind {
            _ if self.lamp.is_some() => 0,
            _ if self.on_road() => 1,
            _ => 2,
        }
    }

    /// The point whose view through a tunnel decides whether the light is
    /// seen, as (distance, lateral position, height). The point of a face of
    /// a car body is the middle of its end that faces the camera.
    fn sight_point(&self) -> (f32, f32, f32) {
        match self.kind {
            Kind::Body { sight, .. } => sight,
            _ => (self.d, self.lateral, self.height),
        }
    }
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
            traffic: Traffic::with_shapes(
                config.seed,
                config.trucks,
                traffic_level(&config, 0.0),
                config.car_shapes,
            ),
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

        for light in &self.lights_on(&view, &projector) {
            if let Kind::Body { corners, .. } = light.kind {
                draw_body(&corners, light, viewport, &projector, &view, canvas);
                continue;
            }
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
                        let cut = shadow_layers(
                            &light.shadows,
                            (p.x, p.y, radius_x, radius_y),
                            &projector,
                            &view,
                        );
                        if cut.is_empty() {
                            canvas.light_pool(p.x, p.y, radius_x, radius_y, light.color, alpha);
                        }
                        // The layers add up to the whole pool where no shadow is.
                        let part = alpha / SHADOW_LAYERS as f32;
                        for shadows in &cut {
                            canvas.shaded_light_pool(
                                p.x,
                                p.y,
                                radius_x,
                                radius_y,
                                light.color,
                                part,
                                shadows,
                            );
                        }
                    }
                }
                Kind::Body { .. } => {}
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

    /// How much the bodies of the other cars hide; `None` where they have none.
    fn body_opacity(&self) -> Option<f32> {
        self.config
            .car_shapes
            .has_bodies()
            .then(|| self.config.car_color.opacity())
    }

    #[cfg(test)]
    fn lights(&self) -> Vec<Light> {
        let projector = Projector::new(1280.0, 720.0, self.config.left_hand_traffic);
        self.lights_on(&RoadView::new(&self.road, self.track_s), &projector)
    }

    /// The lights that make the picture, in the order they are drawn: from
    /// far to near.
    fn lights_on(&self, view: &RoadView, projector: &Projector) -> Vec<Light> {
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
                    order: d,
                    car: None,
                    lateral: row.lateral,
                    height: row.height,
                    color: row.color,
                    alpha: row.alpha * (1.0 - d / row.range),
                    kind: row.kind,
                    lamp: matches!(row.kind, Kind::Pool { .. })
                        .then(|| (s / row.spacing).round() as i64),
                    shadows: Vec::new(),
                });
            }
        }
        lights.extend(HEADLIGHT_POOLS.iter().map(|&(d, alpha, radius)| Light {
            d,
            order: d,
            car: None,
            lateral: -1.75,
            height: 0.0,
            color: WHITE,
            alpha,
            kind: Kind::Pool { radius },
            lamp: None,
            shadows: Vec::new(),
        }));
        lights.extend(self.traffic.lights().into_iter().map(|car| Light {
            d: car.d,
            order: car.d,
            car: Some(car.car),
            lateral: car.lateral,
            height: car.height,
            color: car.color,
            alpha: car.alpha,
            kind: if car.glare {
                Kind::Glare { size: car.size }
            } else {
                Kind::Glow { size: car.size }
            },
            lamp: None,
            shadows: Vec::new(),
        }));
        let mut boxes = Vec::new();
        if let Some(opacity) = self.body_opacity() {
            boxes = self.traffic.boxes();
            for body in &boxes {
                self.push_body(body, opacity, &mut lights);
            }
        }
        // Of the box of a body only the faces that turn toward the camera are seen.
        lights.retain(|l| match l.kind {
            Kind::Body { corners, .. } => faces_camera(&corners, projector, view),
            _ => true,
        });
        // Only what fits through a tunnel is seen along it.
        let sight = self.tunnels.sight(self.track_s, view);
        lights.retain(|l| {
            let (d, lateral, height) = l.sight_point();
            !sight.hides(d, lateral, height)
        });
        let outside = lights.len();
        for tunnel in self.tunnels.ahead() {
            self.tunnel_lights(tunnel, &mut lights);
        }
        // A wall or the hill may stand in the way of the tunnel's own lights too.
        let mut index = 0;
        lights.retain(|l| {
            index += 1;
            let (d, lateral, height) = l.sight_point();
            index <= outside || !sight.blocked(d, lateral, height)
        });
        // The cars shade the pools of light of the street lamps they pass.
        let mut shadows = self.shadows_by_lamp(&boxes, |(d, lateral, height)| {
            sight.hides(d, lateral, height)
        });
        for light in &mut lights {
            if let Some(found) = light.lamp.and_then(|lamp| shadows.get_mut(&lamp)) {
                light.shadows.clone_from(found);
            }
        }
        self.place_among_bodies(&mut lights, view, projector);
        // With bodies, the lights on the road come first (see `Light::layer`):
        // a body covers the lights behind it, and those in front of it lie
        // below it.
        let with_bodies = self.config.car_shapes.has_bodies();
        let layer = |l: &Light| if with_bodies { l.layer() } else { 0 };
        lights.sort_by(|a, b| layer(a).cmp(&layer(b)).then(b.order.total_cmp(&a.order)));
        lights
    }

    /// Adds the faces of a box of a body and its shadows.
    fn push_body(&self, body: &BodyBox, opacity: f32, lights: &mut Vec<Light>) {
        for face in body.faces() {
            // The faces of a car stand where the end that faces the camera is.
            let near = face.corners.iter().map(|c| c.0).fold(f32::MAX, f32::min);
            lights.push(Light {
                d: near,
                order: near,
                car: Some(face.car),
                lateral: face.lateral,
                height: 0.0,
                color: match face.part {
                    FacePart::End => BODY_END,
                    FacePart::Side => BODY_SIDE,
                    FacePart::Roof => BODY_ROOF,
                    FacePart::Glass => BODY_GLASS,
                },
                alpha: opacity,
                kind: Kind::Body {
                    corners: face.corners,
                    sight: face.sight,
                },
                lamp: None,
                shadows: Vec::new(),
            });
        }
    }

    /// The shadows that the cars cast on the pools of light of the street
    /// lamps, by the lamp. A car shades a pool wherever its shadow reaches it,
    /// so a shadow grows from the edge of the pool, where there is no light
    /// to take away, and never appears or vanishes in the middle of one.
    fn shadows_by_lamp(
        &self,
        boxes: &[BodyBox],
        hidden: impl Fn((f32, f32, f32)) -> bool,
    ) -> BTreeMap<i64, Vec<Shadow>> {
        let mut found: BTreeMap<i64, Vec<Shadow>> = BTreeMap::new();
        for body in boxes {
            // The low parts, wheels and frames, cast no shadow of their own.
            let Some(heights) = body.shadow else {
                continue;
            };
            if hidden(body.sight) {
                continue;
            }
            // A lamp farther along the road than a pool is wide from the car
            // has no pool where the shadow falls.
            let (low, high) = (body.near - LAMP_POOL_RADIUS, body.far + LAMP_POOL_RADIUS);
            let mut index = ((self.track_s + f64::from(low)) / LAMP_SPACING).ceil();
            loop {
                let s = index * LAMP_SPACING;
                let lamp = (s - self.track_s) as f32;
                if lamp > high {
                    break;
                }
                let key = index as i64;
                index += 1.0;
                if lamp < NEAR_DISTANCE || lamp >= LAMP_POOL_RANGE {
                    continue;
                }
                if let Some(shadow) = Shadow::of(body, heights, lamp)
                    && shadow.reaches_pools(lamp)
                {
                    found.entry(key).or_default().push(shadow);
                }
            }
        }
        found
    }

    /// Settles which lights are covered by the bodies of other cars and
    /// which are drawn on top of them.
    ///
    /// The faces of a car are drawn at the distance of the end that faces
    /// the camera. A light farther than that is drawn before them and so
    /// is covered; it need not be tested, except against the sides and the
    /// roof, which reach to the far end of the car: a light seen over the
    /// side can still be nearer than the side is there. The lights of a car
    /// are drawn right after its own body.
    fn place_among_bodies(&self, lights: &mut Vec<Light>, view: &RoadView, projector: &Projector) {
        let Some(opacity) = self.body_opacity() else {
            return;
        };
        let faces: Vec<(u32, Corners, f32)> = lights
            .iter()
            .filter_map(|l| match (l.kind, l.car) {
                (Kind::Body { corners, .. }, Some(car)) => Some((car, corners, l.d)),
                _ => None,
            })
            .collect();
        if faces.is_empty() {
            return;
        }
        // The part of the screen each face takes, a pixel wider than its corners.
        let bounds: Vec<Option<(f32, f32, f32, f32)>> = faces
            .iter()
            .map(|&(_, corners, _)| {
                let mut found = (f32::MAX, f32::MIN, f32::MAX, f32::MIN);
                for (d, lateral, height) in corners {
                    let (bend, rise) = view.offset(d);
                    let p = projector.project(d, lateral + bend, height + rise)?;
                    found = (
                        found.0.min(p.x - 1.0),
                        found.1.max(p.x + 1.0),
                        found.2.min(p.y - 1.0),
                        found.3.max(p.y + 1.0),
                    );
                }
                Some(found)
            })
            .collect();
        let mut hidden = vec![false; lights.len()];
        for (light, hidden) in lights.iter_mut().zip(&mut hidden) {
            if matches!(light.kind, Kind::Body { .. }) {
                continue;
            }
            // The lights of a car lie right after its body, at the same place.
            let own = faces
                .iter()
                .filter(|f| Some(f.0) == light.car)
                .map(|f| f.2)
                .reduce(f32::min);
            if let Some(near) = own {
                light.order = near - OWN_LIGHT_LEAD;
            }
            if light.on_road() || !matches!(light.kind, Kind::Glow { .. } | Kind::Glare { .. }) {
                continue;
            }
            let (bend, rise) = view.offset(light.d);
            let Some(p) = projector.project(light.d, light.lateral + bend, light.height + rise)
            else {
                continue;
            };
            for (&(car, corners, near), bounds) in faces.iter().zip(&bounds) {
                // Most lights are far from a face on the screen; skip them quickly.
                let outside = bounds.is_none_or(|(left, right, top, bottom)| {
                    p.x < left || p.x > right || p.y < top || p.y > bottom
                });
                if outside || Some(car) == light.car || light.d < near {
                    continue;
                }
                let Some(depth) = depth_at(&corners, projector, view, (p.x, p.y)) else {
                    continue;
                };
                if light.d > depth && opacity >= 1.0 {
                    // Hidden by an opaque body: nothing of it is drawn.
                    *hidden = true;
                    break;
                }
                if light.d <= depth {
                    // In front of the face it is seen over, although it is farther
                    // than the end of the car.
                    light.order = light.order.min(near - OWN_LIGHT_LEAD);
                }
            }
        }
        let mut index = 0;
        lights.retain(|_| {
            index += 1;
            !hidden[index - 1]
        });
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
                        order: d,
                        car: None,
                        lateral: row.lateral,
                        height: row.height,
                        color: row.color,
                        alpha: row.alpha * (1.0 - d / row.range),
                        kind: row.kind,
                        lamp: None,
                        shadows: Vec::new(),
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

/// The shadow of a box on the road: a convex polygon of (distance, lateral
/// position) on the road surface.
#[derive(Clone, Copy, Debug)]
struct Shadow {
    points: [(f32, f32); MAX_SHADOW_POINTS],
    count: usize,
    /// How far the edge of the shadow is soft to each side of it, in meters.
    soft: f32,
}

impl Shadow {
    /// The shadow that the box casts on the road in the light of a street
    /// lamp `lamp` meters ahead; `None` where it falls out of view.
    ///
    /// Each corner of the box is projected from the lamp onto the road, and
    /// the shadow is the convex hull of the projections.
    fn of(body: &BodyBox, heights: (f32, f32), lamp: f32) -> Option<Self> {
        // The corners of a box at its lowest and highest height, or the points
        // of the outline of a car.
        let corners: Vec<(f32, f32, f32)> = match &body.solid {
            Some(solid) => solid.casts(),
            None => [heights.0, heights.1]
                .into_iter()
                .flat_map(|height| {
                    [
                        (body.near, body.left),
                        (body.near, body.right),
                        (body.far, body.right),
                        (body.far, body.left),
                    ]
                    .map(|(d, lateral)| (d, lateral, height))
                })
                .collect(),
        };
        let mut cast = Vec::with_capacity(corners.len());
        for (d, lateral, height) in corners {
            if height >= LAMP_HEIGHT {
                return None;
            }
            // The line from the lamp through the corner meets the road
            // this many times as far from the lamp as the corner is.
            let stretch = LAMP_HEIGHT / (LAMP_HEIGHT - height);
            cast.push((
                lamp + (d - lamp) * stretch,
                LAMP_LATERAL + (lateral - LAMP_LATERAL) * stretch,
            ));
        }
        let heights = (
            heights.0,
            body.solid.as_ref().map_or(heights.1, |s| s.top()),
        );
        let hull = convex_hull(cast);
        let hull = clip_distance(hull, |d| d - NEAR_DISTANCE);
        let hull = clip_distance(hull, |d| VIEW_DISTANCE - 0.01 - d);
        if hull.len() < 3 || hull.len() > MAX_SHADOW_POINTS {
            return None;
        }
        let mut points = [(0.0, 0.0); MAX_SHADOW_POINTS];
        points[..hull.len()].copy_from_slice(&hull);
        // The lamp is wide: an edge higher over the road is softer.
        let top = heights.1;
        Some(Self {
            points,
            count: hull.len(),
            soft: LAMP_HEAD_RADIUS * top / (LAMP_HEIGHT - top),
        })
    }

    fn points(&self) -> &[(f32, f32)] {
        &self.points[..self.count]
    }

    /// Whether the shadow, with its soft edge, touches a pool of light of the
    /// street lamp `lamp` meters ahead.
    fn reaches_pools(&self, lamp: f32) -> bool {
        LAMP_POOL_LATERALS.iter().any(|&lateral| {
            distance_to_polygon((lamp, lateral), self.points()) < LAMP_POOL_RADIUS + self.soft
        })
    }

    /// The polygon of one of the `SHADOW_LAYERS` layers of the shadow, from
    /// the smallest, which is all dark, to the largest, which holds all that
    /// is a little dark. Each is cut off at the ends of the view.
    fn layer(&self, index: usize) -> Vec<(f32, f32)> {
        let spread = 2.0 * (index as f32 + 0.5) / SHADOW_LAYERS as f32 - 1.0;
        let polygon = offset_polygon(self.points(), self.soft * spread);
        let polygon = clip_distance(polygon, |d| d - NEAR_DISTANCE);
        clip_distance(polygon, |d| VIEW_DISTANCE - 0.01 - d)
    }
}

/// The convex polygon, counterclockwise, with its edges moved out by `delta`
/// meters (in by `-delta` where that is negative, but never more than the
/// shape allows).
fn offset_polygon(points: &[(f32, f32)], delta: f32) -> Vec<(f32, f32)> {
    let count = points.len();
    if count < 3 || delta == 0.0 {
        return points.to_vec();
    }
    // Each edge as (start, direction, outward normal).
    let mut edges = Vec::with_capacity(count);
    let mut width = f32::MAX;
    for (index, &a) in points.iter().enumerate() {
        let b = points[(index + 1) % count];
        let length = (b.0 - a.0).hypot(b.1 - a.1);
        if length < 1e-6 {
            return points.to_vec();
        }
        let direction = ((b.0 - a.0) / length, (b.1 - a.1) / length);
        let normal = (direction.1, -direction.0);
        let depth = points
            .iter()
            .map(|p| -((p.0 - a.0) * normal.0 + (p.1 - a.1) * normal.1))
            .fold(0.0, f32::max);
        width = width.min(depth);
        edges.push((a, direction, normal));
    }
    let delta = if delta < 0.0 {
        delta.max(-0.45 * width)
    } else {
        delta
    };
    (0..count)
        .map(|index| {
            // The corner after edge `index` is where it meets the next edge.
            let (a, u, normal) = edges[index];
            let (b, v, _) = edges[(index + 1) % count];
            let start = (a.0 + normal.0 * delta, a.1 + normal.1 * delta);
            let next = (
                b.0 + edges[(index + 1) % count].2.0 * delta,
                b.1 + edges[(index + 1) % count].2.1 * delta,
            );
            let cross = u.0 * v.1 - u.1 * v.0;
            let corner = points[(index + 1) % count];
            let moved = if cross.abs() < 1e-4 {
                (corner.0 + normal.0 * delta, corner.1 + normal.1 * delta)
            } else {
                let t = ((next.0 - start.0) * v.1 - (next.1 - start.1) * v.0) / cross;
                (start.0 + u.0 * t, start.1 + u.1 * t)
            };
            // A sharp corner would reach far out: keep it near.
            let shift = (moved.0 - corner.0, moved.1 - corner.1);
            let reach = shift.0.hypot(shift.1);
            let most = 2.5 * delta.abs();
            if reach > most {
                (
                    corner.0 + shift.0 * most / reach,
                    corner.1 + shift.1 * most / reach,
                )
            } else {
                moved
            }
        })
        .collect()
}

/// The distance of the point from the convex polygon, `0.0` inside it.
fn distance_to_polygon(point: (f32, f32), points: &[(f32, f32)]) -> f32 {
    let count = points.len();
    let mut inside = true;
    let mut nearest = f32::MAX;
    for index in 0..count {
        let (a, b) = (points[index], points[(index + 1) % count]);
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        if dx * (point.1 - a.1) - dy * (point.0 - a.0) < 0.0 {
            inside = false;
        }
        let length_squared = dx * dx + dy * dy;
        let t = if length_squared > 0.0 {
            (((point.0 - a.0) * dx + (point.1 - a.1) * dy) / length_squared).clamp(0.0, 1.0)
        } else {
            0.0
        };
        nearest = nearest.min((point.0 - a.0 - dx * t).hypot(point.1 - a.1 - dy * t));
    }
    if inside { 0.0 } else { nearest }
}

/// The convex hull of the points, counterclockwise (Andrew's monotone chain).
fn convex_hull(mut points: Vec<(f32, f32)>) -> Vec<(f32, f32)> {
    points.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));
    points.dedup();
    if points.len() < 3 {
        return points;
    }
    // One side of the hull, from the first point to the last of `points`.
    let side = |points: &mut dyn Iterator<Item = (f32, f32)>| {
        let mut chain: Vec<(f32, f32)> = Vec::new();
        for p in points {
            while chain.len() >= 2 {
                let (o, a) = (chain[chain.len() - 2], chain[chain.len() - 1]);
                if (a.0 - o.0) * (p.1 - o.1) - (a.1 - o.1) * (p.0 - o.0) > 0.0 {
                    break;
                }
                chain.pop();
            }
            chain.push(p);
        }
        chain.pop();
        chain
    };
    let mut hull = side(&mut points.iter().copied());
    hull.extend(side(&mut points.iter().rev().copied()));
    hull
}

/// The part of a convex polygon of (distance, lateral position) where
/// `inside` of the distance is not negative.
fn clip_distance(polygon: Vec<(f32, f32)>, inside: impl Fn(f32) -> f32) -> Vec<(f32, f32)> {
    let mut clipped = Vec::with_capacity(polygon.len() + 1);
    for (index, &a) in polygon.iter().enumerate() {
        let b = polygon[(index + 1) % polygon.len()];
        let (va, vb) = (inside(a.0), inside(b.0));
        if va >= 0.0 {
            clipped.push(a);
        }
        if (va >= 0.0) != (vb >= 0.0) {
            let t = va / (va - vb);
            clipped.push((a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t));
        }
    }
    clipped
}

/// The shadows on a pool of light, as polygons of the screen for each layer
/// of the pool; empty where no shadow touches the pool, which is the ellipse
/// `(x, y, radius_x, radius_y)`.
fn shadow_layers(
    shadows: &[Shadow],
    (x, y, radius_x, radius_y): (f32, f32, f32, f32),
    projector: &Projector,
    view: &RoadView,
) -> Vec<Vec<Vec<(f32, f32)>>> {
    // The shadow lies on the road, which bends and rises with its distance.
    let project = |polygon: &[(f32, f32)]| -> Option<Vec<(f32, f32)>> {
        polygon
            .iter()
            .map(|&(d, lateral)| {
                let (bend, rise) = view.offset(d);
                let p = projector.project(d, lateral + bend, rise)?;
                Some((p.x, p.y))
            })
            .collect()
    };
    let mut layers: Vec<Vec<Vec<(f32, f32)>>> = vec![Vec::new(); SHADOW_LAYERS];
    for shadow in shadows {
        // The largest layer holds all of the others.
        let outer = shadow.layer(SHADOW_LAYERS - 1);
        let Some(outline) = project(&outer).filter(|_| outer.len() >= 3) else {
            continue;
        };
        let (left, right) = outline.iter().fold((f32::MAX, f32::MIN), |(lo, hi), p| {
            (lo.min(p.0), hi.max(p.0))
        });
        let (top, bottom) = outline.iter().fold((f32::MAX, f32::MIN), |(lo, hi), p| {
            (lo.min(p.1), hi.max(p.1))
        });
        let touches_pool = right >= x - radius_x
            && left <= x + radius_x
            && bottom >= y - radius_y
            && top <= y + radius_y;
        if !touches_pool {
            continue;
        }
        for (index, layer) in layers.iter_mut().enumerate() {
            let polygon = shadow.layer(index);
            if polygon.len() < 3 {
                continue;
            }
            if let Some(points) = project(&polygon) {
                layer.push(points);
            }
        }
    }
    if layers.iter().all(Vec::is_empty) {
        Vec::new()
    } else {
        layers
    }
}

/// Draws a face of a car body as a polygon, if it is in view.
fn draw_body(
    corners: &[(f32, f32, f32); 4],
    light: &Light,
    viewport: Viewport,
    projector: &Projector,
    view: &RoadView,
    canvas: &mut dyn Canvas,
) {
    let mut points = [(0.0, 0.0); 4];
    for (point, &(d, lateral, height)) in points.iter_mut().zip(corners) {
        // The corners are laid out like the lights of the road, so a car on
        // a bend or a hill is bent and lifted with the road.
        let (bend, rise) = view.offset(d);
        let Some(p) = projector.project(d, lateral + bend, height + rise) else {
            return;
        };
        *point = (p.x, p.y);
    }
    let (left, right) = points.iter().fold((f32::MAX, f32::MIN), |(lo, hi), p| {
        (lo.min(p.0), hi.max(p.0))
    });
    let (top, bottom) = points.iter().fold((f32::MAX, f32::MIN), |(lo, hi), p| {
        (lo.min(p.1), hi.max(p.1))
    });
    if light.alpha >= MIN_ALPHA
        && touches(
            viewport,
            (left + right) * 0.5,
            (top + bottom) * 0.5,
            (right - left) * 0.5,
            (bottom - top) * 0.5,
        )
    {
        canvas.silhouette(&points, light.color, light.alpha);
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
        ShadedPool {
            x: f32,
            y: f32,
            rx: f32,
            ry: f32,
            alpha: f32,
            shadows: Vec<Vec<(f32, f32)>>,
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
                | Command::ShadedPool { x, y, .. }
                | Command::Dot { x, y, .. } => Some((x, y)),
                Command::Streak { head, .. } => Some(head),
                Command::Band { .. } | Command::Silhouette { .. } | Command::Beam { .. } => None,
            }
        }

        fn is_sky(&self) -> bool {
            !matches!(self, Command::Pool { .. } | Command::ShadedPool { .. })
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
        fn shaded_light_pool(
            &mut self,
            x: f32,
            y: f32,
            rx: f32,
            ry: f32,
            _: Rgb,
            alpha: f32,
            shadows: &[Vec<(f32, f32)>],
        ) {
            self.0.push(Command::ShadedPool {
                x,
                y,
                rx,
                ry,
                alpha,
                shadows: shadows.to_vec(),
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
            Command::ShadedPool {
                x,
                y,
                rx,
                ry,
                alpha,
                ref shadows,
            } => {
                for points in shadows {
                    assert!(points.len() >= 3);
                    assert!(points.iter().all(|(x, y)| x.is_finite() && y.is_finite()));
                }
                ([x, y], alpha, [rx, ry])
            }
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
                if light.sight_point().0 > 300.0 {
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
    fn zoned_config(seed: u64) -> SceneConfig {
        SceneConfig {
            seed,
            scenery_zones: true,
            ..Default::default()
        }
    }

    #[test]
    fn the_foot_of_an_entrance_hill_never_floats_above_the_glow_of_the_horizon() {
        // The glow of the horizon reaches `BAND_BOTTOM` below the horizon (at
        // the reference focal length); the hill must stand on or below it,
        // also when the road climbs to the entrance.
        let focal = Projector::new(VIEWPORT.width, VIEWPORT.height, true).focal();
        let limit = VIEWPORT.height * 0.5 + crate::sky::BAND_BOTTOM * focal / crate::sky::REF_FOCAL;
        let (mut frames, mut climbing) = (0, 0);
        for seed in 0..12 {
            let mut scene = Scene::new(zoned_config(seed));
            for step in 0..12_000 {
                scene.update(MAX_STEP_SECONDS);
                if step % 4 != 0 || scene.tunnel_entrance_ahead().is_none() {
                    continue;
                }
                let commands = record(&scene, VIEWPORT);
                let shapes = silhouettes(&commands);
                let hill = shapes[shapes.len() - 2].0;
                let lowest = hill.iter().map(|p| p.1).fold(f32::MIN, f32::max);
                frames += 1;
                climbing += usize::from(lowest < VIEWPORT.height * 0.5 + 24.0);
                assert!(lowest >= limit - 0.5, "seed {seed} step {step}: {lowest}");
            }
        }
        assert!(frames > 500, "{frames}");
        // Some of the frames have a road that climbs to the entrance.
        assert!(climbing > 0, "{climbing}");
    }

    #[test]
    fn a_hill_that_already_stands_below_the_glow_keeps_its_shape() {
        // Close to the entrance the road level is below the glow of the
        // horizon (flat road: nearer than camera height over the glow's
        // depth, 48 meters), and the hill has its own width and a foot on
        // the road.
        let focal = Projector::new(VIEWPORT.width, VIEWPORT.height, true).focal();
        for before in [30.0, 20.0, 10.0] {
            let (scene, tunnel) = near_first_tunnel(TunnelLighting::Led, before);
            let (_, rise) = RoadView::new(&scene.road, scene.track_s).offset(before as f32);
            let commands = record(&scene, VIEWPORT);
            let hill = silhouettes(&commands)[0].0;
            let xs = hill.iter().map(|p| p.0);
            let width = xs.clone().fold(f32::MIN, f32::max) - xs.fold(f32::MAX, f32::min);
            let expected = 2.0 * tunnel.hill_half_width * focal / before as f32;
            assert!(
                (width - expected).abs() < 0.5,
                "{before}: {width} {expected}"
            );
            let lowest = hill.iter().map(|p| p.1).fold(f32::MIN, f32::max);
            let foot = VIEWPORT.height * 0.5 + (CAMERA_HEIGHT - rise) * focal / before as f32;
            assert!((lowest - foot).abs() < 0.5, "{before}: {lowest} {foot}");
        }
    }

    /// The part of the screen the opening of a tunnel `d` meters ahead takes.
    fn opening_box(scene: &Scene, d: f32) -> (f32, f32, f32, f32) {
        let projector = Projector::new(VIEWPORT.width, VIEWPORT.height, true);
        let view = RoadView::new(&scene.road, scene.track_s);
        let (bend, rise) = view.offset(d);
        let mut found = (f32::MAX, f32::MIN, f32::MAX, f32::MIN);
        for (lateral, height) in crate::tunnel::OPENING {
            let p = projector
                .project(d, lateral + bend, height + rise)
                .expect("the opening is in view");
            found = (
                found.0.min(p.x),
                found.1.max(p.x),
                found.2.min(p.y),
                found.3.max(p.y),
            );
        }
        found
    }

    /// Counts the lights beyond the opening of the tunnel that the camera
    /// looks into (or out of), and those of them that lie outside it by more
    /// than `tolerance` pixels.
    fn lights_outside_the_opening(scene: &Scene, tolerance: f32) -> (usize, usize) {
        let Some(tunnel) = scene.tunnels.ahead().find(|t| t.end > scene.track_s) else {
            return (0, 0);
        };
        // From a short way before the entrance on, the camera looks out of the exit.
        let entrance = (tunnel.start - scene.track_s) as f32;
        let at = if entrance >= NEAR_DISTANCE + 0.5 {
            entrance
        } else {
            (tunnel.end - scene.track_s) as f32
        };
        if !(NEAR_DISTANCE..VIEW_DISTANCE - 1.0).contains(&at) {
            return (0, 0);
        }
        let (left, right, top, bottom) = opening_box(scene, at);
        let projector = Projector::new(VIEWPORT.width, VIEWPORT.height, true);
        // The walls are checked a little wider than the opening.
        let tolerance = tolerance + crate::tunnel::SIGHT_MARGIN * projector.focal() / at;
        let view = RoadView::new(&scene.road, scene.track_s);
        let (mut beyond, mut outside) = (0, 0);
        for light in scene.lights() {
            if light.d <= at + 0.5 || matches!(light.kind, Kind::Body { .. }) {
                continue;
            }
            let (bend, rise) = view.offset(light.d);
            let Some(p) = projector.project(light.d, light.lateral + bend, light.height + rise)
            else {
                continue;
            };
            beyond += 1;
            outside += usize::from(
                p.x < left - tolerance
                    || p.x > right + tolerance
                    || p.y < top - tolerance
                    || p.y > bottom + tolerance,
            );
        }
        (beyond, outside)
    }

    #[test]
    fn nothing_beyond_the_opening_of_a_tunnel_is_seen_outside_the_opening() {
        let (mut beyond, mut outside) = (0, 0);
        for seed in 0..8 {
            let mut scene = Scene::new(zoned_config(seed));
            for step in 0..24_000 {
                scene.update(MAX_STEP_SECONDS);
                if step % 8 != 0 {
                    continue;
                }
                let (b, o) = lights_outside_the_opening(&scene, 2.0);
                beyond += b;
                outside += o;
            }
        }
        // Many lights were looked at, and none is seen through the wall.
        assert!(beyond > 5000, "{beyond}");
        assert_eq!(outside, 0, "{outside} of {beyond}");
    }

    #[test]
    fn the_lamps_on_the_walls_do_not_flicker_as_the_car_drives_by() {
        use std::collections::HashMap;
        // A lamp seen from afar gets hidden by the wall of a bend and shows
        // again as the car comes near: at most twice, not a flicker.
        let mut most = 0;
        let mut lamps_seen = 0;
        for seed in 0..4 {
            let mut scene = Scene::new(zoned_config(seed));
            // The lamps by their place on the road (in half meters): whether
            // they were drawn in the last frame, and how often that changed.
            let mut states: HashMap<i64, (bool, u32)> = HashMap::new();
            for step in 0..16_000 {
                scene.update(MAX_STEP_SECONDS);
                if step % 2 != 0 || !scene.tunnels.sky_hidden(scene.track_s) {
                    continue;
                }
                let key = |d: f32| ((scene.track_s + f64::from(d)) * 2.0).round() as i64;
                let drawn: Vec<i64> = scene
                    .lights()
                    .iter()
                    .filter(|l| l.height == 5.0 && (l.lateral == -4.6 || l.lateral == 5.1))
                    .map(|l| key(l.d))
                    .collect();
                for (&place, state) in states.iter_mut() {
                    let d = place as f64 / 2.0 - scene.track_s;
                    if (15.0..400.0).contains(&d) {
                        let now = drawn.contains(&place);
                        state.1 += u32::from(now != state.0);
                        state.0 = now;
                    }
                }
                for &place in &drawn {
                    let d = place as f64 / 2.0 - scene.track_s;
                    if (15.0..400.0).contains(&d) {
                        states.entry(place).or_insert((true, 0));
                    }
                }
            }
            lamps_seen += states.len();
            most = most.max(states.values().map(|s| s.1).max().unwrap_or(0));
        }
        assert!(lamps_seen > 1000, "{lamps_seen}");
        assert!(most <= 2, "{most}");
    }

    /// The lights seen over a body (the screen position of the light is on
    /// the face), as (index of the light, index of the body, whether the light
    /// is behind the face), in each frame of a drive.
    fn covered_pairs(config: SceneConfig, mut each: impl FnMut(&[Light], usize, usize, bool)) {
        let projector = Projector::new(1280.0, 720.0, config.left_hand_traffic);
        let mut scene = Scene::new(config);
        for step in 0..3000 {
            scene.update(MAX_STEP_SECONDS);
            if step % 4 != 0 {
                continue;
            }
            let view = RoadView::new(&scene.road, scene.track_s);
            let lights = scene.lights_on(&view, &projector);
            for (index, light) in lights.iter().enumerate() {
                if light.on_road() || matches!(light.kind, Kind::Body { .. } | Kind::Pool { .. }) {
                    continue;
                }
                let (bend, rise) = view.offset(light.d);
                let Some(p) = projector.project(light.d, light.lateral + bend, light.height + rise)
                else {
                    continue;
                };
                // A light seen over two bodies cannot be drawn right for both
                // when the one that is nearer has the shorter car; it is rare
                // and small, and left out.
                let covering: Vec<(usize, bool)> = lights
                    .iter()
                    .enumerate()
                    .filter_map(|(other, body)| {
                        let Kind::Body { corners, .. } = body.kind else {
                            return None;
                        };
                        if body.car == light.car {
                            return None;
                        }
                        let depth = crate::body::depth_at(&corners, &projector, &view, (p.x, p.y))?;
                        Some((other, light.d > depth))
                    })
                    .collect();
                if let [(other, is_behind)] = covering[..] {
                    each(&lights, index, other, is_behind);
                }
            }
        }
    }

    #[test]
    fn a_light_is_drawn_before_the_bodies_in_front_of_it_and_after_those_behind_it() {
        // A light seen over a body is under it when it is farther than the body
        // there, and over it when it is nearer: for every pair, the whole
        // drive, and not by the length of the car, which led to flicker.
        let (mut behind, mut front) = (0, 0);
        for seed in 1..=2 {
            covered_pairs(
                bodies_config(seed, CarColor::TranslucentBlack),
                |lights, light, body, is_behind| {
                    // The lights of cars are drawn right after their own bodies; one
                    // that lies behind the body of another car, which is farther
                    // than its own, cannot be drawn before that body as well.
                    if lights[light].car.is_some() {
                        return;
                    }
                    assert_eq!(light < body, is_behind, "{seed}");
                    behind += usize::from(is_behind);
                    front += usize::from(!is_behind);
                },
            );
        }
        assert!(behind > 200 && front > 200, "{behind} {front}");
    }

    #[test]
    fn an_opaque_body_hides_the_lights_behind_it() {
        let (mut behind, mut front) = (0, 0);
        for seed in 1..=2 {
            covered_pairs(
                bodies_config(seed, CarColor::Black),
                |lights, light, body, is_behind| {
                    behind += usize::from(is_behind);
                    front += usize::from(!is_behind);
                    let (l, b) = (&lights[light], &lights[body]);
                    assert!(
                        light > body,
                        "{seed}: light d {} order {} lat {} h {} car {:?} kind {} / body d {} order {} car {:?} corners {:?}",
                        l.d,
                        l.order,
                        l.lateral,
                        l.height,
                        l.car,
                        matches!(l.kind, Kind::Glare { .. }),
                        b.d,
                        b.order,
                        b.car,
                        if let Kind::Body { corners, .. } = b.kind {
                            corners
                        } else {
                            [(0.0, 0.0, 0.0); 4]
                        }
                    );
                },
            );
        }
        // The lights behind are gone, and those in front are there.
        assert_eq!(behind, 0, "{behind}");
        assert!(front > 200, "{front}");
    }

    #[test]
    fn the_median_shows_through_a_translucent_body_but_not_an_opaque_one() {
        let count = |car_color| {
            let mut scene = Scene::new(bodies_config(2, car_color));
            let mut found = 0;
            for _ in 0..3000 {
                scene.update(MAX_STEP_SECONDS);
                found += scene
                    .lights()
                    .iter()
                    .filter(|l| l.color == ORANGE && matches!(l.kind, Kind::Glow { .. }))
                    .count();
            }
            found
        };
        assert!(count(CarColor::Black) < count(CarColor::TranslucentBlack));
    }

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
        for step in 0..3000 {
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

    /// A scene whose cars have bodies of realistic shapes.
    fn bodies_config(seed: u64, car_color: CarColor) -> SceneConfig {
        SceneConfig {
            car_shapes: CarShapes::Blocks,
            car_color,
            ..plain_config(seed)
        }
    }

    /// A scene whose cars have bodies with the outlines of cars.
    fn detailed_config(seed: u64, car_color: CarColor) -> SceneConfig {
        SceneConfig {
            car_shapes: CarShapes::Detailed,
            ..bodies_config(seed, car_color)
        }
    }

    /// A scene whose cars have no bodies.
    fn plain_config(seed: u64) -> SceneConfig {
        SceneConfig {
            seed,
            // The mountains and the tunnels are silhouettes too.
            mountains: false,
            tunnels: TunnelLighting::None,
            ..Default::default()
        }
    }

    /// Polygons of points of the screen.
    type Polygons = Vec<Vec<(f32, f32)>>;

    /// The points, color and opacity of a silhouette.
    type Shape<'a> = (&'a Vec<(f32, f32)>, Rgb, f32);

    /// The silhouettes; with no mountains and no tunnels, they are car bodies.
    fn body_shapes(commands: &[Command]) -> Vec<Shape<'_>> {
        commands
            .iter()
            .filter_map(|c| match c {
                Command::Silhouette {
                    points,
                    color,
                    alpha,
                } => Some((points, *color, *alpha)),
                _ => None,
            })
            .collect()
    }

    /// The pools of light that are drawn with shadows cut out, as the shadows
    /// of each of the layers of the pool and the part of the pool it draws.
    fn shaded_pools(commands: &[Command]) -> Vec<(f32, &Polygons)> {
        commands
            .iter()
            .filter_map(|c| match c {
                Command::ShadedPool { alpha, shadows, .. } => Some((*alpha, shadows)),
                _ => None,
            })
            .collect()
    }

    fn driven(config: SceneConfig, seconds: u32) -> Scene {
        let mut scene = Scene::new(config);
        for _ in 0..seconds * 20 {
            scene.update(0.05);
        }
        scene
    }

    #[test]
    fn transparent_cars_draw_no_body() {
        for seed in [1, 2, 3] {
            let scene = driven(plain_config(seed), 30);
            assert!(body_shapes(&record(&scene, VIEWPORT)).is_empty(), "{seed}");
            assert!(
                scene
                    .lights()
                    .iter()
                    .all(|l| !matches!(l.kind, Kind::Body { .. }))
            );
        }
    }

    #[test]
    fn black_bodies_are_opaque_and_translucent_ones_let_a_tenth_through() {
        for (car_color, expected) in [(CarColor::Black, 1.0), (CarColor::TranslucentBlack, 0.9)] {
            let mut drawn = 0;
            for seed in [1, 2, 3] {
                let scene = driven(bodies_config(seed, car_color), 20);
                for (points, color, alpha) in body_shapes(&record(&scene, VIEWPORT)) {
                    drawn += 1;
                    assert!(
                        [BODY_END, BODY_SIDE, BODY_ROOF].contains(&color),
                        "{color:?}"
                    );
                    assert_eq!(alpha, expected);
                    assert_eq!(points.len(), 4);
                    assert!(points.iter().all(|(x, y)| x.is_finite() && y.is_finite()));
                }
            }
            assert!(drawn > 10, "{car_color:?}");
        }
    }

    #[test]
    fn the_faces_of_a_body_differ_in_shade_and_only_those_facing_the_camera_are_drawn() {
        let mut counts = [0usize; 3];
        for (seed, left_hand_traffic) in [(1, true), (2, false), (3, true)] {
            let mut scene = Scene::new(SceneConfig {
                left_hand_traffic,
                ..bodies_config(seed, CarColor::Black)
            });
            for step in 0..4000 {
                scene.update(MAX_STEP_SECONDS);
                if step % 5 != 0 {
                    continue;
                }
                let faces = scene.traffic.bodies().len();
                let commands = record(&scene, VIEWPORT);
                let drawn = body_shapes(&commands);
                // The box has four candidate faces, and at most three can be seen at once.
                assert!(
                    drawn.len() <= faces * 3 / 4 + 1,
                    "{} of {faces}",
                    drawn.len()
                );
                for (_, color, _) in drawn {
                    counts[[BODY_END, BODY_SIDE, BODY_ROOF]
                        .iter()
                        .position(|&c| c == color)
                        .unwrap()] += 1;
                }
            }
        }
        // The ends and the sides are seen all the time, a roof only now and then.
        assert!(counts[0] > 200 && counts[1] > 200, "{counts:?}");
        assert_eq!(BODY_END, Rgb::new(0, 0, 0));
        assert_eq!(BODY_SIDE, Rgb::new(14, 15, 18));
        assert_eq!(BODY_ROOF, Rgb::new(22, 23, 27));
    }

    #[test]
    fn translucent_faces_have_the_same_shades_at_nine_tenths() {
        let scene = driven(bodies_config(2, CarColor::TranslucentBlack), 20);
        let commands = record(&scene, VIEWPORT);
        let shapes = body_shapes(&commands);
        assert!(shapes.iter().any(|s| s.1 == BODY_SIDE));
        assert!(shapes.iter().all(|s| s.2 == 0.9));
    }

    #[test]
    fn bodies_add_nothing_but_the_bodies_and_hide_lights_behind_them() {
        // The same traffic in both: bodies keep the cars apart, which moves them.
        let loose = |config: SceneConfig| {
            let mut scene = Scene::new(config);
            scene.traffic = Traffic::with_level(config.seed, config.trucks, TrafficLevel::FULL);
            for _ in 0..20 * 20 {
                scene.update(0.05);
            }
            scene
        };
        let (transparent, black) = (
            loose(plain_config(5)),
            loose(bodies_config(5, CarColor::Black)),
        );
        let mut without: Vec<String> = record(&transparent, VIEWPORT)
            .iter()
            .map(|c| format!("{c:?}"))
            .collect();
        let with: Vec<String> = record(&black, VIEWPORT)
            .iter()
            .filter(|c| !matches!(c, Command::Silhouette { .. } | Command::ShadedPool { .. }))
            .map(|c| format!("{c:?}"))
            .collect();
        // Every light that remains was there without bodies (the pools that
        // are cut by shadows are drawn another way).
        for command in &with {
            let found = without.iter().position(|c| c == command);
            assert!(found.is_some(), "{command}");
            without.swap_remove(found.unwrap());
        }
    }

    #[test]
    fn a_body_comes_before_the_lights_of_its_car_and_the_lights_in_front() {
        let mut checked = (0, 0);
        for seed in [1, 2, 3, 4] {
            let scene = driven(bodies_config(seed, CarColor::Black), 40);
            let lights = scene.lights();
            // The lights come in layers, each from far to near.
            assert!(lights.windows(2).all(|w| w[0].layer() <= w[1].layer()));
            assert!(
                lights
                    .windows(2)
                    .all(|w| w[0].layer() != w[1].layer() || w[0].order >= w[1].order)
            );
            for (index, face) in lights.iter().enumerate() {
                let Kind::Body { corners, .. } = face.kind else {
                    continue;
                };
                let own = lights.iter().enumerate().filter(|(_, l)| {
                    l.car == face.car && l.layer() == 2 && !matches!(l.kind, Kind::Body { .. })
                });
                for (other, _) in own {
                    assert!(index < other, "{seed}");
                    checked.0 += 1;
                }
                // Whatever is nearer than the whole car comes after it.
                let near = corners.iter().map(|c| c.0).fold(f32::MAX, f32::min);
                for (other, light) in lights.iter().enumerate() {
                    if light.d < near && light.layer() == 2 {
                        assert!(index < other, "{seed}");
                        checked.1 += 1;
                    }
                }
            }
        }
        // Tail lights and headlights, side markers of trucks, and the rest.
        assert!(checked.0 > 50 && checked.1 > 500, "{checked:?}");
    }

    #[test]
    fn bodies_are_drawn_among_the_lights() {
        // Some lights are drawn before the first body and some after the last.
        let (mut before, mut after) = (0, 0);
        for seed in 1..=4 {
            let scene = driven(bodies_config(seed, CarColor::Black), 20);
            let commands = record(&scene, VIEWPORT);
            let is_body = |c: &Command| matches!(c, Command::Silhouette { .. });
            let first = commands.iter().position(is_body).unwrap();
            let last = commands.iter().rposition(is_body).unwrap();
            let is_light = |c: &Command| {
                matches!(
                    c,
                    Command::Glow { .. } | Command::Pool { .. } | Command::ShadedPool { .. }
                )
            };
            before += commands[..first].iter().filter(|c| is_light(c)).count();
            after += commands[last..].iter().filter(|c| is_light(c)).count();
        }
        assert!(before > 0 && after > 0, "{before} {after}");
    }

    #[test]
    fn the_body_of_a_car_ahead_is_as_wide_as_a_car_on_the_screen() {
        // A lead car stands in the middle of the picture; its end face is 1.8
        // meters wide, which is 1.8 * focal / distance pixels.
        let scene = driven(
            SceneConfig {
                trucks: TruckStyle::None,
                ..bodies_config(1, CarColor::Black)
            },
            10,
        );
        let focal = Projector::new(VIEWPORT.width, VIEWPORT.height, true).focal();
        let commands = record(&scene, VIEWPORT);
        let shapes = body_shapes(&commands);
        let mut found = 0;
        for face in scene.traffic.bodies() {
            let d = face.corners[0].0;
            if face.lateral != -1.75 {
                continue;
            }
            let expected = 1.8 * focal / d;
            found += shapes
                .iter()
                .filter(|(points, _, _)| {
                    let xs = points.iter().map(|p| p.0);
                    let width = xs.clone().fold(f32::MIN, f32::max) - xs.fold(f32::MAX, f32::min);
                    (width - expected).abs() < 0.01
                })
                .count();
        }
        assert!(found > 0);
    }

    #[test]
    fn no_body_of_an_oncoming_car_is_seen_through_a_tunnel() {
        let (mut scene, _) = near_first_tunnel(TunnelLighting::Led, 300.0);
        scene.config.car_shapes = CarShapes::Blocks;
        scene.config.car_color = CarColor::Black;
        let mut same_direction = false;
        for _ in 0..600 {
            scene.traffic.update(0.05, 27.0);
            for light in scene.lights() {
                if light.sight_point().0 > 300.0 && matches!(light.kind, Kind::Body { .. }) {
                    assert!(light.lateral < 5.5, "{}", light.lateral);
                    same_direction = true;
                }
            }
        }
        assert!(same_direction);
    }

    #[test]
    fn cars_that_do_not_run_have_no_body_at_night() {
        let quiet = SceneConfig {
            traffic_by_time: true,
            start_minute: 3 * 60,
            ..bodies_config(7, CarColor::Black)
        };
        let busy = SceneConfig {
            start_minute: 12 * 60,
            ..quiet
        };
        let (quiet, busy) = (Scene::new(quiet), Scene::new(busy));
        let count = |scene: &Scene| scene.traffic.bodies().len();
        assert!(count(&quiet) < count(&busy));
        assert!(count(&quiet) > 0 || quiet.traffic.running_count() == 0);
    }

    /// A box of a car in the passing lane, `ahead` meters ahead of the street lamp
    /// (the box is in the lane, 1.8 meters wide and 4.5 long, 1.45 high).
    fn box_beside_a_lamp(ahead: f32, lamp: f32) -> BodyBox {
        BodyBox {
            car: 1,
            lateral: 1.75,
            sight: (lamp + ahead, 1.75, 0.7),
            near: lamp + ahead,
            far: lamp + ahead + 4.5,
            left: 0.85,
            right: 2.65,
            bottom: 0.0,
            top: 1.45,
            has_end: true,
            center: lamp + ahead + 2.25,
            half_length: 2.25,
            shadow: Some((0.0, 1.45)),
            covered: None,
            solid: None,
        }
    }

    /// The shadow of the whole box.
    fn cast(body: &BodyBox, lamp: f32) -> Option<Shadow> {
        Shadow::of(body, (body.bottom, body.top), lamp)
    }

    fn extent(shadow: &Shadow, axis: fn(&(f32, f32)) -> f32) -> (f32, f32) {
        shadow
            .points()
            .iter()
            .map(axis)
            .fold((f32::MAX, f32::MIN), |(lo, hi), v| (lo.min(v), hi.max(v)))
    }

    #[test]
    fn the_shadow_under_a_lamp_stays_near_the_feet_of_the_car() {
        // The car is right under the lamp, along the road; the lamp stands
        // beside the lane, so the shadow reaches a little away from it.
        let body = box_beside_a_lamp(-2.25, 100.0);
        let shadow = cast(&body, 100.0).unwrap();
        let (low, high) = extent(&shadow, |p| p.1);
        // The top is 1.45 m high under a lamp 10 m high: it casts 17% farther away.
        assert!(
            (low - (4.5 + (0.85 - 4.5) * 10.0 / 8.55)).abs() < 1e-3,
            "{low}"
        );
        assert!((high - 2.65).abs() < 1e-3, "{high}");
        let (near, far) = extent(&shadow, |p| p.0);
        assert!(near > 97.0 && far < 103.0, "{near} {far}");
        assert!(near < 97.75 && far > 102.25);
    }

    #[test]
    fn the_shadow_of_a_car_away_from_the_lamp_stretches_away_from_it() {
        let lamp = 100.0;
        // A car 9 m ahead of the lamp: the shadow reaches on ahead of it.
        let ahead = box_beside_a_lamp(9.0, lamp);
        let (_, far) = extent(&cast(&ahead, lamp).unwrap(), |p| p.0);
        assert!(far > ahead.far + 1.5, "{far} {}", ahead.far);
        // A car 15 m behind the lamp: it reaches on back.
        let behind = box_beside_a_lamp(-15.0, lamp);
        let (near, _) = extent(&cast(&behind, lamp).unwrap(), |p| p.0);
        assert!(near < behind.near - 1.5, "{near} {}", behind.near);
        // And always away from the lamp across the road too.
        for body in [ahead, behind] {
            let (low, _) = extent(&cast(&body, lamp).unwrap(), |p| p.1);
            assert!(low < body.left - 0.4, "{low}");
        }
    }

    #[test]
    fn the_shadow_of_a_box_is_convex_finite_and_clipped_to_the_view() {
        for lamp in [1.0, 50.0, 349.0] {
            for ahead in [-20.0, -4.0, 0.0, 6.0, 20.0] {
                for (bottom, top) in [(0.0, 1.45), (0.0, 1.0), (1.2, 3.8)] {
                    let body = BodyBox {
                        bottom,
                        top,
                        ..box_beside_a_lamp(ahead, lamp)
                    };
                    let Some(shadow) = cast(&body, lamp) else {
                        continue;
                    };
                    let points = shadow.points();
                    assert!((3..=MAX_SHADOW_POINTS).contains(&points.len()));
                    for &(d, lateral) in points {
                        assert!(d.is_finite() && lateral.is_finite());
                        assert!((NEAR_DISTANCE - 1e-3..VIEW_DISTANCE).contains(&d), "{d}");
                    }
                    // Convex: every turn goes the same way.
                    let turns: Vec<f32> = (0..points.len())
                        .map(|i| {
                            let (a, b, c) = (
                                points[i],
                                points[(i + 1) % points.len()],
                                points[(i + 2) % points.len()],
                            );
                            (b.0 - a.0) * (c.1 - b.1) - (b.1 - a.1) * (c.0 - b.0)
                        })
                        .collect();
                    assert!(
                        turns.iter().all(|&t| t >= -1e-3) || turns.iter().all(|&t| t <= 1e-3),
                        "{turns:?}"
                    );
                }
            }
        }
        // A box higher than the lamp casts no shadow.
        let body = BodyBox {
            top: LAMP_HEIGHT,
            ..box_beside_a_lamp(0.0, 50.0)
        };
        assert!(cast(&body, 50.0).is_none());
    }

    #[test]
    fn cars_shade_the_pools_of_the_street_lamps_when_they_have_bodies() {
        let (mut frames, mut shadows) = (0, 0);
        for seed in 1..=3 {
            let mut scene = Scene::new(bodies_config(seed, CarColor::Black));
            for step in 0..2000 {
                scene.update(MAX_STEP_SECONDS);
                if step % 5 != 0 {
                    continue;
                }
                let lights = scene.lights();
                // Only the pools of the street lamps have shadows, and they
                // are drawn before the other lights on the road.
                assert!(lights.iter().all(|l| l.lamp.is_some()
                    == matches!(l.kind, Kind::Pool { radius } if radius == LAMP_POOL_RADIUS)
                    || !matches!(l.kind, Kind::Pool { .. })));
                assert!(
                    lights
                        .iter()
                        .all(|l| l.shadows.is_empty() || (l.lamp.is_some() && l.layer() == 0))
                );
                let shaded = lights.iter().filter(|l| !l.shadows.is_empty()).count();
                shadows += shaded;
                frames += usize::from(shaded > 0);
                // Both pools of a lamp have the same shadows.
                for light in lights.iter().filter(|l| !l.shadows.is_empty()) {
                    let twin = lights
                        .iter()
                        .find(|l| l.lamp == light.lamp && l.lateral != light.lateral)
                        .unwrap();
                    assert_eq!(twin.shadows.len(), light.shadows.len());
                }
                let commands = record(&scene, VIEWPORT);
                // No dark shape is laid over the picture: the only silhouettes
                // are bodies, and the pools are cut.
                for (part, layers) in shaded_pools(&commands) {
                    assert!(part > 0.0 && part.is_finite());
                    for points in layers.iter() {
                        assert!(points.len() >= 3);
                        assert!(points.iter().all(|(x, y)| x.is_finite() && y.is_finite()));
                    }
                }
            }
        }
        assert!(frames > 100 && shadows > 200, "{frames} {shadows}");
    }

    #[test]
    fn a_shaded_pool_is_drawn_in_layers_that_add_up_to_the_pool() {
        for seed in 1..=3 {
            let scene = driven(bodies_config(seed, CarColor::Black), 40);
            let commands = record(&scene, VIEWPORT);
            let mut index = 0;
            while index < commands.len() {
                if let Command::ShadedPool { x, y, alpha, .. } = &commands[index] {
                    // The layers of a pool come one after the other.
                    let layers: Vec<&Command> = commands[index..]
                        .iter()
                        .take_while(|c| {
                            matches!(c, Command::ShadedPool { x: cx, y: cy, .. } if cx == x && cy == y)
                        })
                        .take(SHADOW_LAYERS)
                        .collect();
                    assert_eq!(layers.len(), SHADOW_LAYERS, "{seed}");
                    let total: f32 = layers
                        .iter()
                        .map(|c| match c {
                            Command::ShadedPool { alpha, .. } => *alpha,
                            _ => unreachable!(),
                        })
                        .sum();
                    assert!((total - alpha * SHADOW_LAYERS as f32).abs() < 1e-5);
                    index += SHADOW_LAYERS;
                } else {
                    index += 1;
                }
            }
        }
    }

    #[test]
    fn a_shadow_darkens_the_pools_before_the_other_lights_and_the_bodies() {
        // The shaded pools come before every body, and the lights on the road
        // that are not pools of street lamps come after them.
        let scene = driven(bodies_config(2, CarColor::Black), 40);
        let commands = record(&scene, VIEWPORT);
        let last_shaded = commands
            .iter()
            .rposition(|c| matches!(c, Command::ShadedPool { .. }))
            .unwrap();
        let first_body = commands
            .iter()
            .position(|c| matches!(c, Command::Silhouette { .. }))
            .unwrap();
        assert!(last_shaded < first_body);
        assert!(
            commands[last_shaded..first_body]
                .iter()
                .any(|c| matches!(c, Command::Pool { .. } | Command::Glow { .. }))
        );
    }

    /// The light that the shadow takes from the pools of the street lamp `lamp`
    /// meters ahead, as the parts of the pools, summed over their area.
    fn light_taken(shadow: &Shadow, lamp: f32) -> f32 {
        let profile = |u: f32| {
            if u >= 1.0 {
                0.0
            } else if u < 0.5 {
                1.0 - 1.3 * u
            } else {
                0.7 * (1.0 - u)
            }
        };
        let layers: Vec<Vec<(f32, f32)>> = (0..SHADOW_LAYERS).map(|i| shadow.layer(i)).collect();
        let step = 0.25;
        let mut taken = 0.0;
        for lateral_pool in LAMP_POOL_LATERALS {
            let mut d = lamp - LAMP_POOL_RADIUS;
            while d <= lamp + LAMP_POOL_RADIUS {
                let mut l = lateral_pool - LAMP_POOL_RADIUS;
                while l <= lateral_pool + LAMP_POOL_RADIUS {
                    let light = profile(
                        ((d - lamp).powi(2) + (l - lateral_pool).powi(2)).sqrt() / LAMP_POOL_RADIUS,
                    );
                    let dark = layers
                        .iter()
                        .filter(|polygon| {
                            polygon.len() >= 3 && distance_to_polygon((d, l), polygon) == 0.0
                        })
                        .count();
                    taken += light * dark as f32 / SHADOW_LAYERS as f32 * step * step;
                    l += step;
                }
                d += step;
            }
        }
        taken
    }

    #[test]
    fn a_shadow_grows_and_fades_smoothly_as_a_car_drives_by_a_lamp() {
        let lamp = 100.0;
        for (bottom, top, shadow_top) in [(0.0, 1.45, 1.45), (1.2, 3.8, 3.8)] {
            let mut series = Vec::new();
            let mut ahead = -40.0;
            while ahead <= 40.0 {
                let body = BodyBox {
                    bottom,
                    top,
                    shadow: Some((bottom, shadow_top)),
                    ..box_beside_a_lamp(ahead, lamp)
                };
                let taken = match cast(&body, lamp) {
                    Some(shadow) if shadow.reaches_pools(lamp) => light_taken(&shadow, lamp),
                    _ => 0.0,
                };
                series.push(taken);
                ahead += 0.2;
            }
            let most = series.iter().copied().fold(0.0, f32::max);
            assert!(most > 1.0, "{most}");
            // A car moves 0.2 m: the light it takes never jumps.
            let jump = series
                .windows(2)
                .map(|w| (w[1] - w[0]).abs())
                .fold(0.0, f32::max);
            assert!(jump < 0.08 * most, "{jump} {most}");
            // No shadow at the ends, and it is there in the middle.
            assert_eq!((series[0], series[series.len() - 1]), (0.0, 0.0));
            // It rises to the most and falls back once, not on and off.
            let on_off = series
                .windows(2)
                .filter(|w| (w[0] > 0.0) != (w[1] > 0.0))
                .count();
            assert_eq!(on_off, 2, "{on_off}");
        }
    }

    #[test]
    fn a_shadow_that_does_not_reach_the_pools_is_not_made() {
        let lamp = 100.0;
        for ahead in [-60.0, -30.0, 25.0, 60.0] {
            let body = box_beside_a_lamp(ahead, lamp);
            let shadow = cast(&body, lamp).unwrap();
            assert!(!shadow.reaches_pools(lamp), "{ahead}");
        }
        assert!(
            cast(&box_beside_a_lamp(-2.25, lamp), lamp)
                .unwrap()
                .reaches_pools(lamp)
        );
        // The scene makes none for a car far from every lamp.
        let scene = Scene::new(bodies_config(1, CarColor::Black));
        let far = BodyBox {
            sight: (300.0, 1.75, 0.7),
            ..box_beside_a_lamp(247.0, 50.0)
        };
        let found = scene.shadows_by_lamp(&[far], |_| false);
        for (lamp, shadows) in found {
            assert!(!shadows.is_empty(), "{lamp}");
        }
    }

    #[test]
    fn the_edge_of_a_shadow_is_soft_in_proportion_to_its_height() {
        let low = cast(&box_beside_a_lamp(-2.25, 100.0), 100.0).unwrap();
        let high = Shadow::of(
            &BodyBox {
                top: 3.8,
                ..box_beside_a_lamp(-2.25, 100.0)
            },
            (0.0, 3.8),
            100.0,
        )
        .unwrap();
        assert!(
            low.soft > 0.1 && high.soft > 2.0 * low.soft,
            "{} {}",
            low.soft,
            high.soft
        );
        // The layers hold one another, from the smallest to the largest.
        let area = |polygon: &[(f32, f32)]| {
            (0..polygon.len())
                .map(|i| {
                    let (a, b) = (polygon[i], polygon[(i + 1) % polygon.len()]);
                    a.0 * b.1 - b.0 * a.1
                })
                .sum::<f32>()
                * 0.5
        };
        for shadow in [low, high] {
            let areas: Vec<f32> = (0..SHADOW_LAYERS).map(|i| area(&shadow.layer(i))).collect();
            assert!(areas.windows(2).all(|w| w[0] < w[1]), "{areas:?}");
            assert!(areas.iter().all(|a| *a > 0.0));
        }
    }

    #[test]
    fn transparent_cars_cast_no_shadow() {
        for seed in [1, 2, 3] {
            let scene = driven(plain_config(seed), 30);
            assert!(shaded_pools(&record(&scene, VIEWPORT)).is_empty());
            assert!(scene.lights().iter().all(|l| l.shadows.is_empty()));
        }
    }

    #[test]
    fn there_are_no_shadows_inside_a_tunnel() {
        let (mut scene, tunnel) = near_first_tunnel(TunnelLighting::Led, -300.0);
        scene.config.car_shapes = CarShapes::Blocks;
        scene.config.car_color = CarColor::Black;
        assert!(scene.in_tunnel());
        let mut cars_seen = 0;
        for _ in 0..600 {
            scene.traffic.update(0.05, 27.0);
            let lights = scene.lights();
            cars_seen += lights
                .iter()
                .filter(|l| matches!(l.kind, Kind::Body { .. }))
                .count();
            assert!(
                lights.iter().all(|l| l.shadows.is_empty()),
                "{}",
                tunnel.start
            );
        }
        assert!(cars_seen > 0);
    }

    #[test]
    fn bodies_are_valid_through_a_whole_drive_on_both_sides_of_the_road() {
        for left_hand_traffic in [true, false] {
            let mut scene = Scene::new(SceneConfig {
                left_hand_traffic,
                trucks: TruckStyle::NorthAmerican,
                ..bodies_config(9, CarColor::TranslucentBlack)
            });
            for _ in 0..1200 {
                scene.update(0.05);
                for command in record(&scene, VIEWPORT) {
                    assert_valid(&command);
                }
            }
        }
    }

    #[test]
    fn detailed_bodies_are_valid_through_a_whole_drive_on_both_sides_of_the_road() {
        for left_hand_traffic in [true, false] {
            for trucks in [TruckStyle::Japanese, TruckStyle::NorthAmerican] {
                let mut scene = Scene::new(SceneConfig {
                    left_hand_traffic,
                    trucks,
                    ..detailed_config(9, CarColor::TranslucentBlack)
                });
                let mut windows = 0;
                for _ in 0..1200 {
                    scene.update(0.05);
                    let commands = record(&scene, VIEWPORT);
                    for command in &commands {
                        assert_valid(command);
                    }
                    // Windows are bluer than the paint.
                    windows += scene
                        .lights()
                        .iter()
                        .filter(|l| matches!(l.kind, Kind::Body { .. }) && l.color == BODY_GLASS)
                        .count();
                }
                assert!(windows > 100, "{windows}");
            }
        }
    }

    #[test]
    fn a_light_is_drawn_before_the_detailed_bodies_in_front_of_it_and_after_those_behind_it() {
        let (mut behind, mut front) = (0, 0);
        for seed in 1..=2 {
            covered_pairs(
                detailed_config(seed, CarColor::TranslucentBlack),
                |lights, light, body, is_behind| {
                    if lights[light].car.is_some() {
                        return;
                    }
                    assert_eq!(light < body, is_behind, "{seed}");
                    behind += usize::from(is_behind);
                    front += usize::from(!is_behind);
                },
            );
        }
        assert!(behind > 200 && front > 200, "{behind} {front}");
    }

    #[test]
    fn an_opaque_detailed_body_hides_the_lights_behind_it() {
        // No light that is seen over a body, and farther than it, is drawn.
        let mut hidden = 0;
        for seed in 1..=2 {
            let config = detailed_config(seed, CarColor::Black);
            let projector = Projector::new(1280.0, 720.0, config.left_hand_traffic);
            let mut scene = Scene::new(config);
            for step in 0..2000 {
                scene.update(MAX_STEP_SECONDS);
                if step % 8 != 0 {
                    continue;
                }
                let view = RoadView::new(&scene.road, scene.track_s);
                let lights = scene.lights_on(&view, &projector);
                let faces: Vec<(Option<u32>, Corners)> = lights
                    .iter()
                    .filter_map(|l| match l.kind {
                        Kind::Body { corners, .. } => Some((l.car, corners)),
                        _ => None,
                    })
                    .collect();
                for light in &lights {
                    if light.on_road()
                        || !matches!(light.kind, Kind::Glow { .. } | Kind::Glare { .. })
                        || light.car.is_some()
                    {
                        continue;
                    }
                    let (bend, rise) = view.offset(light.d);
                    let Some(p) =
                        projector.project(light.d, light.lateral + bend, light.height + rise)
                    else {
                        continue;
                    };
                    for (_, corners) in &faces {
                        if let Some(depth) = depth_at(corners, &projector, &view, (p.x, p.y)) {
                            assert!(light.d <= depth + 1e-3, "{seed}: {} {depth}", light.d);
                            hidden += 1;
                        }
                    }
                }
            }
        }
        assert!(hidden > 50, "{hidden}");
    }

    /// A hash of the commands drawn over a drive, which tests that a picture
    /// without car bodies is that of v1.10.0. With `exact`, the numbers of the
    /// commands count; otherwise only which commands are drawn in which order.
    fn commands_hash(config: SceneConfig, exact: bool) -> u64 {
        let mut scene = Scene::new(config);
        let mut hash = 0xcbf2_9ce4_8422_2325u64;
        for step in 0..900 {
            scene.update(0.05);
            if step % 150 == 149 {
                for command in record(&scene, VIEWPORT) {
                    let text = format!("{command:?}");
                    let text = if exact {
                        text.as_str()
                    } else {
                        text.split([' ', '{']).next().unwrap_or_default()
                    };
                    for byte in text.bytes() {
                        hash = (hash ^ u64::from(byte)).wrapping_mul(0x100_0000_01b3);
                    }
                }
            }
        }
        hash
    }

    /// The scene of the golden hashes: every kind of variation and the UFO on.
    fn golden_config(seed: u64, trucks: TruckStyle) -> SceneConfig {
        SceneConfig {
            seed,
            trucks,
            ufo_mean_interval: Some(20.0),
            traffic_by_time: true,
            scenery_zones: true,
            vary_speed: true,
            start_minute: 3 * 60,
            ..Default::default()
        }
    }

    /// The exact hashes were taken on Linux from v1.10.0. The sine and the
    /// like give other last digits elsewhere (Windows differs in two of the
    /// nine), so the exact test is for Linux, and the other one holds
    /// everywhere.
    #[test]
    #[cfg(target_os = "linux")]
    fn without_car_bodies_the_commands_are_those_of_v1_10_0() {
        for (trucks, golden) in [
            (
                TruckStyle::None,
                [
                    3_747_085_965_833_721_906,
                    10_061_296_263_306_752_667,
                    56_210_606_116_699_451,
                ],
            ),
            (
                TruckStyle::Japanese,
                [
                    12_498_055_424_280_975_874,
                    8_061_650_184_240_585_894,
                    15_783_032_712_771_803_535,
                ],
            ),
            (
                TruckStyle::NorthAmerican,
                [
                    2_220_682_745_943_549_350,
                    4_681_926_326_815_209_319,
                    14_456_919_141_875_129_913,
                ],
            ),
        ] {
            let hashes: Vec<u64> = (0..3)
                .map(|seed| commands_hash(golden_config(seed, trucks), true))
                .collect();
            assert_eq!(hashes, golden, "{trucks:?}");
            // The color means nothing without bodies.
            let black = SceneConfig {
                car_color: CarColor::Black,
                ..golden_config(0, trucks)
            };
            assert_eq!(commands_hash(black, true), golden[0], "{trucks:?}");
        }
    }

    #[test]
    fn without_car_bodies_the_commands_are_those_of_v1_10_0_in_kind_and_order() {
        for (trucks, golden) in [
            (TruckStyle::None, KINDS_NONE),
            (TruckStyle::Japanese, KINDS_JAPANESE),
            (TruckStyle::NorthAmerican, KINDS_NORTH_AMERICAN),
        ] {
            let hashes: Vec<u64> = (0..3)
                .map(|seed| commands_hash(golden_config(seed, trucks), false))
                .collect();
            assert_eq!(hashes, golden, "{trucks:?}");
        }
    }

    const KINDS_NONE: [u64; 3] = [
        468_534_814_160_060_606,
        8_137_298_111_563_663_754,
        12_072_154_355_330_509_400,
    ];
    const KINDS_JAPANESE: [u64; 3] = [
        7_114_710_342_343_259_971,
        3_605_832_487_212_935_424,
        14_851_655_165_617_784_221,
    ];
    const KINDS_NORTH_AMERICAN: [u64; 3] = [
        4_942_396_396_457_689_779,
        14_159_349_977_691_203_828,
        16_922_474_812_910_385_076,
    ];

    #[test]
    fn the_shapes_of_car_bodies_draw_what_they_should() {
        for seed in [1, 2, 3] {
            // Trucks are made of several boxes that cast shadows when they are
            // blocks, so there are none: cars cast one shadow in all.
            let config = |car_shapes| SceneConfig {
                car_shapes,
                trucks: TruckStyle::None,
                ..bodies_config(seed, CarColor::Black)
            };
            let drawn = |car_shapes| {
                let scene = driven(config(car_shapes), 30);
                let commands = record(&scene, VIEWPORT);
                (body_shapes(&commands).len(), shaded_pools(&commands).len())
            };
            let (none_bodies, none_shadows) = drawn(CarShapes::None);
            assert_eq!((none_bodies, none_shadows), (0, 0), "{seed}");
            let (boxes_bodies, boxes_shadows) = drawn(CarShapes::Boxes);
            let (blocks_bodies, blocks_shadows) = drawn(CarShapes::Blocks);
            let (detailed_bodies, detailed_shadows) = drawn(CarShapes::Detailed);
            assert!(boxes_bodies > 5, "{seed}");
            // The same cars in the same places: a block car is made of more
            // faces, and casts one shadow, as the box of a simple car does.
            assert!(blocks_bodies > boxes_bodies, "{seed}");
            assert_eq!(blocks_shadows, boxes_shadows, "{seed}");
            // A detailed car is made of more faces still, with the same shadows.
            assert!(detailed_bodies > blocks_bodies, "{seed}");
            assert!(detailed_shadows > 0, "{seed}");
        }
    }
}
