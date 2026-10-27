//! Other cars on the road: tail lights ahead, passing cars and oncoming headlights.
//!
//! Cars are kept relative to the camera (`rel`, meters ahead) so that hours of
//! driving never lose precision. Everything about a car (where it appears, how
//! fast it is, when it brakes) is hashed from the seed, the car's number and
//! how often it has been placed again, so the same seed gives the same traffic.

use crate::canvas::Rgb;
use crate::projection::{NEAR_DISTANCE, VIEW_DISTANCE};
use crate::rng::hash01;

const LEAD_CARS: u32 = 5;
const PASSING_CARS: u32 = 6;
const ONCOMING_CARS: u32 = 14;

/// Lateral positions assume the camera is in the left lane of left-hand
/// traffic; the projection mirrors them for right-hand traffic.
const OWN_LANE: f32 = -1.75;
const PASSING_LANE: f32 = 1.75;
const ONCOMING_LANES: [f32; 2] = [7.25, 10.75];

/// A same-direction car farther ahead than this is placed again.
const FAR_AHEAD: f32 = 680.0;
/// A car left farther behind than this is placed again.
const FAR_BEHIND: f32 = 260.0;
/// An oncoming car this far behind the camera is placed again.
const ONCOMING_GONE: f32 = -20.0;

/// A lead car closer than `FOLLOW_START` speeds up so that it settles near
/// `FOLLOW_MIN` instead of the camera running into it.
const FOLLOW_START: f32 = 45.0;
const FOLLOW_MIN: f32 = 20.0;
/// A lead car that has stayed this long pulls away to make room for new ones.
const ESCAPE_RATIO: f32 = 1.12;

/// Mean time between brakings of a car and the shortest gap, in seconds.
const MEAN_BRAKE_INTERVAL: f32 = 15.0;
const MIN_BRAKE_INTERVAL: f32 = 2.0;

const TAIL_RED: Rgb = Rgb::new(255, 35, 20);
const HEADLIGHT_WHITE: Rgb = Rgb::new(255, 245, 215);
const MARKER_AMBER: Rgb = Rgb::new(255, 150, 30);
/// Tail lights fade out at this distance.
const TAIL_RANGE: f32 = 650.0;
/// Glare of an oncoming car is seen within this distance.
const GLARE_RANGE: f32 = 90.0;
const GLARE_ALPHA: f32 = 0.25;

// Stream IDs keep the random values of one car independent of each other.
// They only tell the streams of a non-cryptographic hash apart.
const STREAM_POSITION: u32 = 10;
const STREAM_SPEED: u32 = 11;
const STREAM_LANE: u32 = 12;
const STREAM_BRAKE_GAP: u32 = 13;
const STREAM_BRAKE_LENGTH: u32 = 14;
const STREAM_ESCAPE: u32 = 15;
const STREAM_PHASE: u32 = 16;
const STREAM_TRUCK: u32 = 17;
const STREAM_ACTIVE: u32 = 18;

/// The share of cars that are trucks, when trucks are enabled.
pub const TRUCK_RATIO: f32 = 0.2;
/// Body of a truck in meters. Only the positions of its lights depend on it.
const TRUCK_LENGTH: f32 = 12.0;
const TRUCK_HALF_WIDTH: f32 = 1.25;
/// Width, height and length of the body of a car and of a truck, in meters.
const CAR_BODY: BodySize = BodySize {
    width: 1.8,
    height: 1.45,
    length: 4.5,
};
const TRUCK_BODY: BodySize = BodySize {
    width: 2.0 * TRUCK_HALF_WIDTH,
    height: 3.6,
    length: TRUCK_LENGTH,
};
/// The far end of a body is cut off this far before the view ends, where the
/// projection stops.
const VIEW_MARGIN: f32 = 0.01;

/// A lead truck drives at this fraction of the camera's speed, where a
/// lead car drives at 0.85 to 0.95.
const TRUCK_LEAD_RATIO: (f32, f32) = (0.80, 0.10);
/// A passing truck is this much faster than the camera compared to a car.
const TRUCK_PASSING_FACTOR: f32 = 0.5;

/// How busy the road is: the share of the cars that run at all, and the
/// share of trucks among the cars that run. A car is placed again when it
/// leaves the view, and only then takes a new share into account, so the
/// traffic changes over a minute or so instead of at once.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TrafficLevel {
    pub active: f32,
    pub trucks: f32,
}

impl TrafficLevel {
    /// All cars run, and a fifth of them are trucks.
    pub const FULL: TrafficLevel = TrafficLevel {
        active: 1.0,
        trucks: TRUCK_RATIO,
    };

    /// The traffic at `minute` minutes after midnight (any value; a day has
    /// 1,440 minutes): thin and full of trucks at night, full by day.
    pub fn at_minute(minute: f32) -> Self {
        let minute = if minute.is_finite() {
            minute.rem_euclid(MINUTES_PER_DAY)
        } else {
            return Self::FULL;
        };
        match minute {
            m if m < 5.0 * 60.0 => TrafficLevel {
                active: 0.4,
                trucks: 0.4,
            },
            m if m < 7.0 * 60.0 || m >= 21.0 * 60.0 => TrafficLevel {
                active: 0.7,
                trucks: 0.25,
            },
            _ => Self::FULL,
        }
    }
}

/// Minutes in a day.
pub const MINUTES_PER_DAY: f32 = 24.0 * 60.0;

/// How the lights of trucks are arranged.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TruckStyle {
    /// There are no trucks.
    None,
    /// Marker lamps of Japanese heavy trucks.
    #[default]
    Japanese,
    /// Clearance and identification lamps of North American trucks.
    NorthAmerican,
}

impl TruckStyle {
    /// Every choice, in the order of the stored values 0, 1, 2.
    pub const ALL: [TruckStyle; 3] = [
        TruckStyle::None,
        TruckStyle::Japanese,
        TruckStyle::NorthAmerican,
    ];

    /// The style for a stored value; `None` when the value is not valid.
    pub fn from_raw(raw: Option<u32>) -> Option<Self> {
        Self::ALL.get(usize::try_from(raw?).ok()?).copied()
    }

    /// The value to store; `from_raw` reads it back.
    pub fn to_raw(self) -> u32 {
        match self {
            TruckStyle::None => 0,
            TruckStyle::Japanese => 1,
            TruckStyle::NorthAmerican => 2,
        }
    }
}

/// Whether the other cars have bodies that hide the lights behind them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CarBodies {
    /// Only the lights of the cars are drawn.
    #[default]
    Transparent,
    /// Black bodies that let a tenth of the light behind them through.
    TranslucentBlack,
    /// Black bodies that hide everything behind them.
    Black,
}

impl CarBodies {
    /// Every choice, in the order of the stored values 0, 1, 2.
    pub const ALL: [CarBodies; 3] = [
        CarBodies::Transparent,
        CarBodies::TranslucentBlack,
        CarBodies::Black,
    ];

    /// The choice for a stored value; `None` when the value is not valid.
    pub fn from_raw(raw: Option<u32>) -> Option<Self> {
        Self::ALL.get(usize::try_from(raw?).ok()?).copied()
    }

    /// How much a body hides what is behind it, from 0 to 1; `None` when
    /// the cars have no bodies.
    pub fn opacity(self) -> Option<f32> {
        match self {
            CarBodies::Transparent => None,
            CarBodies::TranslucentBlack => Some(0.9),
            CarBodies::Black => Some(1.0),
        }
    }

    /// The value to store; `from_raw` reads it back.
    pub fn to_raw(self) -> u32 {
        match self {
            CarBodies::Transparent => 0,
            CarBodies::TranslucentBlack => 1,
            CarBodies::Black => 2,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    /// Same direction, same lane, a little slower than the camera.
    Lead,
    /// Same direction, passing lane, faster than the camera.
    Passing,
    Oncoming,
}

#[derive(Clone, Copy)]
struct BodySize {
    width: f32,
    height: f32,
    length: f32,
}

/// A flat face of the body of a car: a quadrilateral whose corners are
/// positioned like the lights of the road, as (distance ahead of the camera,
/// lateral position, height above the road).
#[derive(Clone, Copy, Debug)]
pub struct BodyFace {
    /// The car the face belongs to, which is also that of its lights.
    pub car: u32,
    /// Lateral position of the car, which tells whether a tunnel hides it.
    pub lateral: f32,
    pub corners: [(f32, f32, f32); 4],
}

/// A light of a car, positioned like the lights of the road.
#[derive(Clone, Copy, Debug)]
pub struct CarLight {
    /// The car the light belongs to.
    pub car: u32,
    /// Distance ahead of the camera, in meters.
    pub d: f32,
    pub lateral: f32,
    pub height: f32,
    /// Physical size of the core, in meters.
    pub size: f32,
    pub color: Rgb,
    pub alpha: f32,
    /// A big soft light that may be drawn larger than a point light.
    pub glare: bool,
}

struct Car {
    id: u32,
    kind: Kind,
    lateral: f32,
    /// Meters ahead of the camera; negative behind it.
    rel: f32,
    /// Lead cars: speed as a fraction of the camera's.
    ratio: f32,
    /// Passing cars: meters per second faster than the camera. Oncoming
    /// cars: their own speed.
    speed: f32,
    /// Offset of the flicker seen through the median barrier.
    phase: f32,
    /// Seconds since the car was last placed.
    age: f32,
    /// Lead cars start to pull away at this age.
    escape_after: f32,
    /// How often the car has been placed again.
    generation: u32,
    /// Whether the car runs at all; decided each time the car is placed. A
    /// car that does not run is simulated but not drawn, so that it takes
    /// nothing from the other cars.
    active: bool,
    /// Whether the car is a truck; decided each time the car is placed.
    truck: bool,
    /// How often the car has braked.
    brakes: u32,
    until_brake: f32,
    braking_left: f32,
}

impl Car {
    fn new(seed: u64, id: u32, style: TruckStyle, level: TrafficLevel) -> Self {
        let kind = if id < LEAD_CARS {
            Kind::Lead
        } else if id < LEAD_CARS + PASSING_CARS {
            Kind::Passing
        } else {
            Kind::Oncoming
        };
        let mut car = Self {
            id,
            kind,
            lateral: 0.0,
            rel: 0.0,
            ratio: 1.0,
            speed: 0.0,
            phase: 0.0,
            age: 0.0,
            escape_after: 0.0,
            generation: 0,
            active: true,
            truck: false,
            brakes: 0,
            until_brake: 0.0,
            braking_left: 0.0,
        };
        car.place(seed, style, level, true);
        car.until_brake = car.brake_interval(seed);
        car
    }

    /// Random number for this car and its current placement.
    fn roll(&self, seed: u64, stream: u32) -> f32 {
        hash01(
            seed,
            stream,
            i64::from(self.id) << 32 | i64::from(self.generation),
        )
    }

    /// Sets position, speed and lane. `initial` spreads the cars over the
    /// whole view; later they enter where they cannot be seen, except that
    /// lead cars keep appearing ahead.
    fn place(&mut self, seed: u64, style: TruckStyle, level: TrafficLevel, initial: bool) {
        let position = self.roll(seed, STREAM_POSITION);
        let speed = self.roll(seed, STREAM_SPEED);
        // New streams keep the other random values of the car as they were.
        self.active = self.roll(seed, STREAM_ACTIVE) < level.active;
        self.truck = style != TruckStyle::None && self.roll(seed, STREAM_TRUCK) < level.trucks;
        self.age = 0.0;
        self.phase = self.roll(seed, STREAM_PHASE) * std::f32::consts::TAU;
        self.escape_after = 40.0 + 80.0 * self.roll(seed, STREAM_ESCAPE);
        match self.kind {
            Kind::Lead => {
                self.lateral = OWN_LANE;
                self.rel = 80.0 + 450.0 * position;
                self.ratio = if self.truck {
                    TRUCK_LEAD_RATIO.0 + TRUCK_LEAD_RATIO.1 * speed
                } else {
                    0.85 + 0.10 * speed
                };
            }
            Kind::Passing => {
                self.lateral = PASSING_LANE;
                self.rel = if initial {
                    20.0 + 500.0 * position
                } else {
                    -(30.0 + 150.0 * position)
                };
                self.speed = 2.0 + 8.0 * speed;
                if self.truck {
                    self.speed *= TRUCK_PASSING_FACTOR;
                }
            }
            Kind::Oncoming => {
                self.lateral = ONCOMING_LANES[usize::from(self.roll(seed, STREAM_LANE) >= 0.5)];
                self.rel = if initial {
                    600.0 * position
                } else {
                    600.0 + 400.0 * position
                };
                self.speed = 24.0 + 8.0 * speed;
            }
        }
    }

    fn brake_interval(&self, seed: u64) -> f32 {
        let gap = hash01(
            seed,
            STREAM_BRAKE_GAP,
            i64::from(self.id) << 32 | i64::from(self.brakes),
        );
        // The gap between random events is exponentially distributed.
        (-MEAN_BRAKE_INTERVAL * (1.0 - gap).ln()).max(MIN_BRAKE_INTERVAL)
    }

    fn brake_length(&self, seed: u64) -> f32 {
        let length = hash01(
            seed,
            STREAM_BRAKE_LENGTH,
            i64::from(self.id) << 32 | i64::from(self.brakes),
        );
        0.8 + length
    }

    fn braking(&self) -> bool {
        self.braking_left > 0.0
    }

    /// How fast the car's distance ahead of the camera changes, in meters
    /// per second: positive while it pulls away, negative while it closes in.
    fn rel_velocity(&self, own_speed: f32) -> f32 {
        match self.kind {
            Kind::Lead => {
                let ratio = if self.age >= self.escape_after {
                    ESCAPE_RATIO
                } else {
                    self.ratio
                };
                let speed = own_speed * ratio;
                if speed >= own_speed {
                    return speed - own_speed;
                }
                // Closer than `FOLLOW_START`, ease up to a little faster
                // than the camera, so that the gap settles in between.
                let near =
                    ((FOLLOW_START - self.rel) / (FOLLOW_START - FOLLOW_MIN)).clamp(0.0, 1.0);
                let speed = speed + (own_speed * 1.05 - speed) * near;
                speed - own_speed
            }
            Kind::Passing => self.speed,
            Kind::Oncoming => -(own_speed + self.speed),
        }
    }

    fn step(&mut self, seed: u64, style: TruckStyle, level: TrafficLevel, dt: f32, own_speed: f32) {
        self.age += dt;
        self.rel += self.rel_velocity(own_speed) * dt;
        if self.kind != Kind::Oncoming {
            self.step_brake(seed, dt);
        }
        let gone = match self.kind {
            Kind::Oncoming => self.rel < ONCOMING_GONE,
            _ => self.rel > FAR_AHEAD || self.rel < -FAR_BEHIND,
        };
        if gone {
            self.generation = self.generation.wrapping_add(1);
            self.place(seed, style, level, false);
        }
    }

    fn step_brake(&mut self, seed: u64, dt: f32) {
        if self.braking() {
            self.braking_left -= dt;
            if !self.braking() {
                self.until_brake = self.brake_interval(seed);
            }
        } else {
            self.until_brake -= dt;
            if self.until_brake <= 0.0 {
                self.braking_left = self.brake_length(seed);
                self.brakes = self.brakes.wrapping_add(1);
            }
        }
    }

    fn push_lights(&self, style: TruckStyle, out: &mut Vec<CarLight>) {
        let d = self.rel;
        if !self.active || !(NEAR_DISTANCE..VIEW_DISTANCE).contains(&d) {
            return;
        }
        if self.truck {
            self.push_truck_lights(style, out);
            return;
        }
        match self.kind {
            Kind::Lead | Kind::Passing => {
                let fade = (1.0 - d / TAIL_RANGE).max(0.0);
                let (size, alpha) = if self.braking() {
                    (0.09, fade)
                } else {
                    (0.07, 0.6 * fade)
                };
                for side in [-0.7, 0.7] {
                    out.push(CarLight {
                        car: self.id,
                        d,
                        lateral: self.lateral + side,
                        height: 0.8,
                        size,
                        color: TAIL_RED,
                        alpha,
                        glare: false,
                    });
                }
                if self.braking() {
                    out.push(CarLight {
                        car: self.id,
                        d,
                        lateral: self.lateral,
                        height: 1.15,
                        size: 0.05,
                        color: TAIL_RED,
                        alpha: fade,
                        glare: false,
                    });
                }
            }
            Kind::Oncoming => {
                // The median barrier chops the light as the car passes it.
                let flicker = 0.875 + 0.125 * (d * 0.8 + self.phase).sin();
                let alpha = (1.0 - d / VIEW_DISTANCE) * flicker;
                for side in [-0.7, 0.7] {
                    out.push(CarLight {
                        car: self.id,
                        d,
                        lateral: self.lateral + side,
                        height: 0.65,
                        size: 0.13,
                        color: HEADLIGHT_WHITE,
                        alpha,
                        glare: false,
                    });
                }
                if d < GLARE_RANGE {
                    out.push(CarLight {
                        car: self.id,
                        d,
                        lateral: self.lateral,
                        height: 0.65,
                        size: 1.2,
                        color: HEADLIGHT_WHITE,
                        alpha: GLARE_ALPHA * (1.0 - d / GLARE_RANGE),
                        glare: true,
                    });
                }
            }
        }
    }
}

impl Car {
    /// The faces of the body that the camera can see: the end that faces
    /// it, and for a car in another lane the side that faces it. `rel` is
    /// the end that faces the camera, and the body reaches the length of the
    /// car farther away. A body that has partly passed the camera is cut off
    /// at `NEAR_DISTANCE`, so that it does not vanish all at once.
    fn push_body(&self, out: &mut Vec<BodyFace>) {
        let size = if self.truck { TRUCK_BODY } else { CAR_BODY };
        let far = (self.rel + size.length).min(VIEW_DISTANCE - VIEW_MARGIN);
        if !self.active || self.rel >= VIEW_DISTANCE - VIEW_MARGIN || far <= NEAR_DISTANCE {
            return;
        }
        let half = size.width * 0.5;
        let (left, right) = (self.lateral - half, self.lateral + half);
        if self.rel >= NEAR_DISTANCE {
            let d = self.rel;
            out.push(BodyFace {
                car: self.id,
                lateral: self.lateral,
                corners: [
                    (d, left, 0.0),
                    (d, right, 0.0),
                    (d, right, size.height),
                    (d, left, size.height),
                ],
            });
        }
        // The camera is in the own lane: a car in that lane shows no side,
        // and a car in another lane shows the side toward the own lane.
        if self.lateral > OWN_LANE {
            let near = self.rel.max(NEAR_DISTANCE);
            out.push(BodyFace {
                car: self.id,
                lateral: self.lateral,
                corners: [
                    (near, left, 0.0),
                    (far, left, 0.0),
                    (far, left, size.height),
                    (near, left, size.height),
                ],
            });
        }
    }
}

impl Car {
    /// Lights of a truck. `rel` is the end of the body that faces the
    /// camera (the rear of a car ahead, the front of an oncoming car); the
    /// body reaches `TRUCK_LENGTH` meters farther away.
    fn push_truck_lights(&self, style: TruckStyle, out: &mut Vec<CarLight>) {
        let oncoming = self.kind == Kind::Oncoming;
        // The side that faces the camera. The camera is in the own lane, so
        // a truck in that lane has no near side; it gets the right one.
        let side = if self.lateral > OWN_LANE { -1.0 } else { 1.0 };
        // Position of the amber marker lamp near the front of the body, and
        // of the red one near the rear, counted from the end facing the camera.
        let (front_z, rear_z) = if oncoming {
            (0.5, TRUCK_LENGTH - 0.5)
        } else {
            (TRUCK_LENGTH - 0.5, 0.5)
        };
        let braking = self.braking();
        let mut lamp = |z: f32, offset: f32, height: f32, size: f32, color: Rgb, strength: f32| {
            let d = self.rel + z;
            if !(NEAR_DISTANCE..VIEW_DISTANCE).contains(&d) {
                return;
            }
            let alpha = if oncoming {
                // The median barrier chops the light as the truck passes it.
                let flicker = 0.875 + 0.125 * (d * 0.8 + self.phase).sin();
                (1.0 - d / VIEW_DISTANCE) * flicker
            } else {
                (1.0 - d / TAIL_RANGE).max(0.0)
            } * strength;
            if alpha > 0.0 {
                out.push(CarLight {
                    car: self.id,
                    d,
                    lateral: self.lateral + offset,
                    height,
                    size,
                    color,
                    alpha: alpha.min(1.0),
                    glare: false,
                });
            }
        };
        // Small lamps high on the body are dimmer than the main lamps.
        let (small_size, small_strength) = (0.05, 0.5);
        let marker = |z: f32| (z, side * TRUCK_HALF_WIDTH);
        if oncoming {
            for lateral in [-0.9, 0.9] {
                lamp(0.0, lateral, 0.9, 0.13, HEADLIGHT_WHITE, 1.0);
            }
        } else {
            let (size, strength) = if braking { (0.09, 1.0) } else { (0.07, 0.6) };
            for lateral in [-1.05, 1.05] {
                lamp(0.0, lateral, 1.0, size, TAIL_RED, strength);
            }
        }
        match style {
            TruckStyle::None => {}
            TruckStyle::Japanese => {
                let (end_color, end_height) = if oncoming {
                    (MARKER_AMBER, 3.3)
                } else {
                    (TAIL_RED, 3.4)
                };
                for lateral in [-1.15, 1.15] {
                    lamp(
                        0.0,
                        lateral,
                        end_height,
                        small_size,
                        end_color,
                        small_strength,
                    );
                }
                for index in 0..7 {
                    let (z, offset) = marker(1.8 * index as f32);
                    lamp(z, offset, 0.9, small_size, MARKER_AMBER, small_strength);
                }
            }
            TruckStyle::NorthAmerican => {
                let (z, offset) = marker(front_z);
                lamp(z, offset, 1.0, small_size, MARKER_AMBER, small_strength);
                let (z, offset) = marker(rear_z);
                lamp(z, offset, 1.0, small_size, TAIL_RED, small_strength);
                if oncoming {
                    // Five amber lamps on the cab roof.
                    for index in 0..5 {
                        let lateral = -0.9 + 0.45 * index as f32;
                        lamp(0.0, lateral, 3.0, small_size, MARKER_AMBER, small_strength);
                    }
                } else {
                    // Three identification lamps and two clearance lamps.
                    for lateral in [-0.3, 0.0, 0.3] {
                        lamp(0.0, lateral, 3.5, small_size, TAIL_RED, small_strength);
                    }
                    for lateral in [-1.2, 1.2] {
                        lamp(0.0, lateral, 3.5, small_size, TAIL_RED, small_strength);
                    }
                }
            }
        }
        if oncoming && self.rel < GLARE_RANGE {
            out.push(CarLight {
                car: self.id,
                d: self.rel,
                lateral: self.lateral,
                height: 0.9,
                size: 1.2,
                color: HEADLIGHT_WHITE,
                alpha: GLARE_ALPHA * (1.0 - self.rel / GLARE_RANGE),
                glare: true,
            });
        }
    }
}

pub struct Traffic {
    seed: u64,
    trucks: TruckStyle,
    level: TrafficLevel,
    cars: Vec<Car>,
}

impl Traffic {
    pub fn new(seed: u64, trucks: TruckStyle) -> Self {
        Self::with_level(seed, trucks, TrafficLevel::FULL)
    }

    /// Traffic that is as busy as `level` from the start.
    pub fn with_level(seed: u64, trucks: TruckStyle, level: TrafficLevel) -> Self {
        let count = LEAD_CARS + PASSING_CARS + ONCOMING_CARS;
        Self {
            seed,
            trucks,
            level,
            cars: (0..count)
                .map(|id| Car::new(seed, id, trucks, level))
                .collect(),
        }
    }

    /// Number of cars, whether they run or not.
    pub fn car_count(&self) -> usize {
        self.cars.len()
    }

    /// Number of cars that run, and are drawn when they are in view.
    pub fn running_count(&self) -> usize {
        self.cars.iter().filter(|c| c.active).count()
    }

    /// Sets how busy the road is for the cars that are placed from now on.
    pub fn set_level(&mut self, level: TrafficLevel) {
        self.level = level;
    }

    /// Moves the cars for `dt` seconds while the camera drives at
    /// `own_speed` meters per second.
    pub fn update(&mut self, dt: f32, own_speed: f32) {
        if !(dt.is_finite() && dt > 0.0) {
            return;
        }
        let own_speed = if own_speed.is_finite() {
            own_speed.max(0.0)
        } else {
            0.0
        };
        for car in &mut self.cars {
            car.step(self.seed, self.trucks, self.level, dt, own_speed);
        }
    }

    /// The lights in front of the camera, in no particular order.
    pub fn lights(&self) -> Vec<CarLight> {
        let mut lights = Vec::new();
        for car in &self.cars {
            car.push_lights(self.trucks, &mut lights);
        }
        lights
    }

    /// The faces of the bodies of the cars in front of the camera, in no
    /// particular order.
    pub fn bodies(&self) -> Vec<BodyFace> {
        let mut faces = Vec::new();
        for car in &self.cars {
            car.push_body(&mut faces);
        }
        faces
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const OWN_SPEED: f32 = 100.0 / 3.6;
    const DT: f32 = 0.05;

    fn run(traffic: &mut Traffic, steps: usize, mut each: impl FnMut(&Traffic)) {
        for _ in 0..steps {
            traffic.update(DT, OWN_SPEED);
            each(traffic);
        }
    }

    fn snapshot(traffic: &Traffic) -> Vec<(f32, f32, bool)> {
        traffic
            .cars
            .iter()
            .map(|c| (c.rel, c.lateral, c.braking()))
            .collect()
    }

    #[test]
    fn the_same_seed_gives_the_same_traffic() {
        let (mut a, mut b) = (
            Traffic::new(5, TruckStyle::None),
            Traffic::new(5, TruckStyle::None),
        );
        run(&mut a, 6000, |_| {});
        run(&mut b, 6000, |_| {});
        assert_eq!(snapshot(&a), snapshot(&b));
        let mut c = Traffic::new(6, TruckStyle::None);
        run(&mut c, 6000, |_| {});
        assert_ne!(snapshot(&a), snapshot(&c));
    }

    #[test]
    fn traffic_stays_healthy_for_an_hour() {
        for seed in [0, 1, 2] {
            let mut traffic = Traffic::new(seed, TruckStyle::None);
            let count = traffic.cars.len();
            let mut brakes = 0;
            run(&mut traffic, 72_000, |t| {
                assert_eq!(t.cars.len(), count);
                for car in &t.cars {
                    assert!(car.rel.is_finite() && car.speed.is_finite());
                    match car.kind {
                        Kind::Oncoming => assert!((ONCOMING_GONE..=1000.0).contains(&car.rel)),
                        Kind::Lead => {
                            // The camera never catches up with a lead car.
                            assert!(
                                (FOLLOW_MIN - 1.0..=FAR_AHEAD).contains(&car.rel),
                                "{}",
                                car.rel
                            );
                        }
                        Kind::Passing => assert!((-FAR_BEHIND..=FAR_AHEAD).contains(&car.rel)),
                    }
                }
                // Something is always ahead on the own lane.
                assert!(
                    t.cars
                        .iter()
                        .any(|c| c.kind == Kind::Lead && c.rel > NEAR_DISTANCE)
                );
                brakes = t.cars.iter().map(|c| c.brakes).sum::<u32>();
            });
            assert!(brakes > 1000, "{brakes}");
        }
    }

    #[test]
    fn cars_do_not_bunch_up() {
        let mut traffic = Traffic::new(3, TruckStyle::None);
        let mut visible = 0;
        let mut samples = 0;
        run(&mut traffic, 72_000, |t| {
            visible += t
                .cars
                .iter()
                .filter(|c| (NEAR_DISTANCE..VIEW_DISTANCE).contains(&c.rel))
                .count();
            samples += 1;
        });
        let mean = visible as f32 / samples as f32;
        // Out of 25 cars, a good part is in view on average.
        assert!((8.0..22.0).contains(&mean), "{mean}");
    }

    #[test]
    fn passing_cars_overtake_from_behind() {
        let mut traffic = Traffic::new(1, TruckStyle::None);
        let (mut was_behind, mut overtook) = (false, false);
        let mut previous = None;
        run(&mut traffic, 72_000, |t| {
            for car in t.cars.iter().filter(|c| c.kind == Kind::Passing) {
                if car.rel < 0.0 {
                    was_behind = true;
                }
                if let Some((id, rel)) = previous
                    && id == car.id
                    && rel < 0.0
                    && car.rel >= 0.0
                {
                    overtook = true;
                }
            }
            previous = t
                .cars
                .iter()
                .find(|c| c.kind == Kind::Passing)
                .map(|c| (c.id, c.rel));
        });
        assert!(was_behind && overtook);
    }

    #[test]
    fn braking_lasts_between_0_8_and_1_8_seconds() {
        let mut traffic = Traffic::new(2, TruckStyle::None);
        let mut lengths = Vec::new();
        let mut steps = vec![0u32; traffic.cars.len()];
        run(&mut traffic, 72_000, |t| {
            for (car, n) in t.cars.iter().zip(&mut steps) {
                if car.braking() {
                    *n += 1;
                } else if *n > 0 {
                    lengths.push(*n as f32 * DT);
                    *n = 0;
                }
            }
        });
        assert!(lengths.len() > 1000);
        for length in lengths {
            assert!((0.8 - 1e-3..=1.8 + DT + 1e-3).contains(&length), "{length}");
        }
        // Oncoming cars never show brake lights.
        assert!(
            traffic
                .cars
                .iter()
                .all(|c| c.kind != Kind::Oncoming || !c.braking())
        );
    }

    #[test]
    fn oncoming_cars_come_closer_and_get_replaced() {
        let mut traffic = Traffic::new(4, TruckStyle::None);
        let mut replaced = 0;
        let mut last: Vec<u32> = traffic.cars.iter().map(|c| c.generation).collect();
        run(&mut traffic, 6000, |t| {
            for (car, last) in t.cars.iter().zip(&mut last) {
                if car.kind == Kind::Oncoming && car.generation != *last {
                    replaced += 1;
                    // New cars enter beyond the view.
                    assert!(car.rel >= VIEW_DISTANCE, "{}", car.rel);
                }
                *last = car.generation;
            }
        });
        assert!(replaced > 50, "{replaced}");
    }

    #[test]
    fn lights_are_valid_and_in_view() {
        let mut traffic = Traffic::new(7, TruckStyle::None);
        run(&mut traffic, 6000, |t| {
            for light in t.lights() {
                assert!((NEAR_DISTANCE..VIEW_DISTANCE).contains(&light.d));
                assert!(light.alpha > 0.0 && light.alpha <= 1.0, "{light:?}");
                assert!(light.lateral.is_finite() && light.height > 0.0 && light.size > 0.0);
            }
        });
    }

    #[test]
    fn a_braking_car_shows_bigger_brighter_lights_and_a_third_lamp() {
        let mut car = Car::new(0, 0, TruckStyle::None, TrafficLevel::FULL);
        car.rel = 100.0;
        let lights = |car: &Car| {
            let mut out = Vec::new();
            car.push_lights(TruckStyle::None, &mut out);
            out
        };
        car.braking_left = 0.0;
        let normal = lights(&car);
        car.braking_left = 1.0;
        let braking = lights(&car);
        assert_eq!(normal.len(), 2);
        assert_eq!(braking.len(), 3);
        assert!(braking[0].alpha > normal[0].alpha && braking[0].size > normal[0].size);
        assert_eq!(braking[2].lateral, car.lateral);
    }

    #[test]
    fn glare_grows_as_an_oncoming_car_comes_near() {
        let mut car = Car::new(
            0,
            LEAD_CARS + PASSING_CARS,
            TruckStyle::None,
            TrafficLevel::FULL,
        );
        let glare = |car: &mut Car, d: f32| {
            car.rel = d;
            let mut out = Vec::new();
            car.push_lights(TruckStyle::None, &mut out);
            out.iter().find(|l| l.glare).map(|l| l.alpha)
        };
        assert_eq!(glare(&mut car, 200.0), None);
        let far = glare(&mut car, 80.0).unwrap();
        let near = glare(&mut car, 10.0).unwrap();
        assert!(near > far && near <= GLARE_ALPHA);
    }

    fn truck_of(kind_id: u32, style: TruckStyle, rel: f32) -> Car {
        let mut car = Car::new(0, kind_id, style, TrafficLevel::FULL);
        car.truck = true;
        car.rel = rel;
        car.braking_left = 0.0;
        car
    }

    fn lights_of(car: &Car, style: TruckStyle) -> Vec<CarLight> {
        let mut out = Vec::new();
        car.push_lights(style, &mut out);
        out
    }

    fn count(lights: &[CarLight], height: f32, color: Rgb) -> usize {
        lights
            .iter()
            .filter(|l| (l.height - height).abs() < 1e-4 && l.color == color && !l.glare)
            .count()
    }

    const LEAD_ID: u32 = 0;
    const PASSING_ID: u32 = LEAD_CARS;
    const ONCOMING_ID: u32 = LEAD_CARS + PASSING_CARS;

    /// Kinds of cars that are seen from behind.
    const REAR_IDS: [u32; 2] = [LEAD_ID, PASSING_ID];

    #[test]
    fn about_one_car_in_five_is_a_truck_of_every_kind() {
        let mut totals = [(0u32, 0u32); 3];
        for seed in 0..120 {
            let mut traffic = Traffic::new(seed, TruckStyle::Japanese);
            let mut last: Vec<u32> = traffic.cars.iter().map(|c| c.generation).collect();
            let mut tally = |traffic: &Traffic, last: &mut Vec<u32>, first: bool| {
                for (car, last) in traffic.cars.iter().zip(last) {
                    if first || car.generation != *last {
                        let slot = &mut totals[car.kind as usize];
                        slot.0 += u32::from(car.truck);
                        slot.1 += 1;
                    }
                    *last = car.generation;
                }
            };
            tally(&traffic, &mut last, true);
            run(&mut traffic, 6000, |t| tally(t, &mut last, false));
        }
        for (trucks, placed) in totals {
            assert!(placed > 1000, "{placed}");
            let ratio = trucks as f32 / placed as f32;
            assert!((0.15..0.25).contains(&ratio), "{ratio}");
        }
    }

    /// The shares of cars that run and of trucks among all the cars placed
    /// in `steps` steps, for traffic of the given level.
    fn shares(level: TrafficLevel, style: TruckStyle) -> (f32, f32) {
        let (mut placed, mut active, mut trucks) = (0u32, 0u32, 0u32);
        for seed in 0..60 {
            let mut traffic = Traffic::with_level(seed, style, level);
            let mut last: Vec<u32> = traffic.cars.iter().map(|c| c.generation).collect();
            let mut tally = |traffic: &Traffic, last: &mut Vec<u32>, first: bool| {
                for (car, last) in traffic.cars.iter().zip(last) {
                    if first || car.generation != *last {
                        placed += 1;
                        active += u32::from(car.active);
                        trucks += u32::from(car.active && car.truck);
                    }
                    *last = car.generation;
                }
            };
            tally(&traffic, &mut last, true);
            run(&mut traffic, 6000, |t| tally(t, &mut last, false));
        }
        assert!(placed > 1000, "{placed}");
        // Whether a car is a truck is decided for every car placed, but only
        // the ones that run are seen: count trucks among those.
        (
            active as f32 / placed as f32,
            trucks as f32 / active.max(1) as f32,
        )
    }

    #[test]
    fn the_hours_of_the_day_have_the_designed_traffic() {
        let at = |hour: f32| TrafficLevel::at_minute(hour * 60.0);
        let night = TrafficLevel {
            active: 0.4,
            trucks: 0.4,
        };
        let edge = TrafficLevel {
            active: 0.7,
            trucks: 0.25,
        };
        for hour in [0.0, 1.5, 4.99] {
            assert_eq!(at(hour), night, "{hour}");
        }
        for hour in [5.0, 6.5, 6.99, 21.0, 22.5, 23.99] {
            assert_eq!(at(hour), edge, "{hour}");
        }
        for hour in [7.0, 12.0, 20.99] {
            assert_eq!(at(hour), TrafficLevel::FULL, "{hour}");
        }
        assert_eq!(TrafficLevel::FULL.active, 1.0);
        assert_eq!(TrafficLevel::FULL.trucks, 0.2);
    }

    #[test]
    fn the_day_wraps_around_midnight() {
        for day in [1.0, 2.0, 30.0] {
            for hour in [0.5, 6.0, 12.0, 22.0] {
                assert_eq!(
                    TrafficLevel::at_minute((day * 24.0 + hour) * 60.0),
                    TrafficLevel::at_minute(hour * 60.0)
                );
            }
        }
        // Just before midnight and just after it are on both sides of 21:00 and 5:00.
        assert_eq!(
            TrafficLevel::at_minute(-30.0),
            TrafficLevel::at_minute(23.5 * 60.0)
        );
        assert_eq!(TrafficLevel::at_minute(f32::NAN), TrafficLevel::FULL);
    }

    #[test]
    fn the_shares_of_running_cars_and_trucks_follow_the_level() {
        for hour in [2.0, 6.0, 12.0, 22.0] {
            let level = TrafficLevel::at_minute(hour * 60.0);
            let (active, trucks) = shares(level, TruckStyle::Japanese);
            assert!((active - level.active).abs() < 0.04, "{hour} {active}");
            assert!((trucks - level.trucks).abs() < 0.05, "{hour} {trucks}");
        }
    }

    #[test]
    fn a_full_level_is_the_traffic_as_it_always_was() {
        for seed in 0..10 {
            let mut a = Traffic::new(seed, TruckStyle::Japanese);
            let mut b = Traffic::with_level(seed, TruckStyle::Japanese, TrafficLevel::FULL);
            b.set_level(TrafficLevel::FULL);
            for _ in 0..4000 {
                a.update(DT, OWN_SPEED);
                b.update(DT, OWN_SPEED);
            }
            let key = |t: &Traffic| {
                t.lights()
                    .iter()
                    .map(|l| (l.d, l.lateral, l.height, l.alpha))
                    .collect::<Vec<_>>()
            };
            assert_eq!(key(&a), key(&b));
            assert!(a.cars.iter().all(|c| c.active));
        }
    }

    #[test]
    fn cars_that_do_not_run_take_nothing_from_the_others() {
        // The roll for running has a stream of its own: a quiet road is the
        // full road with some cars left out, and the others drive the same.
        for seed in 0..10 {
            let mut full = Traffic::new(seed, TruckStyle::None);
            let mut quiet = Traffic::with_level(
                seed,
                TruckStyle::None,
                TrafficLevel {
                    active: 0.4,
                    trucks: 0.2,
                },
            );
            quiet.set_level(TrafficLevel {
                active: 0.4,
                trucks: 0.2,
            });
            for _ in 0..4000 {
                full.update(DT, OWN_SPEED);
                quiet.update(DT, OWN_SPEED);
                for (a, b) in full.cars.iter().zip(&quiet.cars) {
                    assert_eq!(
                        (a.rel, a.generation, a.lateral),
                        (b.rel, b.generation, b.lateral)
                    );
                }
            }
            let drawn = |t: &Traffic| t.lights().len();
            assert!(drawn(&quiet) <= drawn(&full));
        }
    }

    #[test]
    fn a_car_that_does_not_run_draws_no_light() {
        let mut traffic = Traffic::with_level(
            3,
            TruckStyle::Japanese,
            TrafficLevel {
                active: 0.0,
                trucks: 0.4,
            },
        );
        run(&mut traffic, 2000, |t| assert!(t.lights().is_empty()));
    }

    #[test]
    fn there_are_no_trucks_at_any_level_without_the_setting() {
        for hour in [2.0, 6.0, 12.0] {
            let level = TrafficLevel::at_minute(hour * 60.0);
            let (_, trucks) = shares(level, TruckStyle::None);
            assert_eq!(trucks, 0.0);
        }
    }

    #[test]
    fn the_traffic_changes_as_cars_are_placed_again() {
        // Going from the night to the day, the cars that run grow in number
        // as the old ones leave the view.
        let mut traffic =
            Traffic::with_level(8, TruckStyle::Japanese, TrafficLevel::at_minute(2.0 * 60.0));
        let running = |t: &Traffic| t.cars.iter().filter(|c| c.active).count();
        let before = running(&traffic);
        traffic.set_level(TrafficLevel::FULL);
        run(&mut traffic, 20 * 60 * 20, |_| {});
        assert!(running(&traffic) > before, "{before}");
        assert_eq!(running(&traffic), traffic.cars.len());
    }

    #[test]
    fn there_are_no_trucks_without_the_setting() {
        for seed in 0..20 {
            let mut traffic = Traffic::new(seed, TruckStyle::None);
            run(&mut traffic, 6000, |t| {
                assert!(t.cars.iter().all(|c| !c.truck));
            });
        }
    }

    #[test]
    fn cars_that_are_not_trucks_drive_as_without_trucks() {
        // The truck roll has a stream of its own, so a car that stays a car
        // is placed exactly as when trucks are disabled.
        for seed in 0..20 {
            let with = Traffic::new(seed, TruckStyle::NorthAmerican);
            let without = Traffic::new(seed, TruckStyle::None);
            let mut compared = 0;
            for (a, b) in with.cars.iter().zip(&without.cars) {
                if a.truck {
                    continue;
                }
                compared += 1;
                assert_eq!(
                    (a.lateral, a.rel, a.ratio, a.speed, a.phase, a.escape_after),
                    (b.lateral, b.rel, b.ratio, b.speed, b.phase, b.escape_after)
                );
            }
            assert!(compared > 10);
        }
    }

    #[test]
    fn lights_without_trucks_are_those_of_cars() {
        // Without trucks every car has 2 lights, a third while braking, or
        // 2 or 3 for an oncoming car (the glare).
        let mut traffic = Traffic::new(11, TruckStyle::None);
        run(&mut traffic, 6000, |t| {
            let lights = t.lights();
            let max: usize = t.cars.iter().map(|c| 3 + usize::from(c.braking())).sum();
            assert!(lights.len() <= max);
            assert!(lights.iter().all(|l| l.size <= 1.2));
        });
    }

    #[test]
    fn trucks_are_slower_than_cars() {
        for seed in 0..50 {
            let traffic = Traffic::new(seed, TruckStyle::Japanese);
            for car in traffic.cars.iter().filter(|c| c.truck) {
                match car.kind {
                    Kind::Lead => assert!((0.80..0.90).contains(&car.ratio), "{}", car.ratio),
                    Kind::Passing => assert!((1.0..5.0).contains(&car.speed), "{}", car.speed),
                    Kind::Oncoming => assert!((24.0..32.0).contains(&car.speed)),
                }
            }
        }
    }

    #[test]
    fn traffic_with_trucks_stays_healthy_for_an_hour() {
        for style in [TruckStyle::Japanese, TruckStyle::NorthAmerican] {
            let mut traffic = Traffic::new(5, style);
            let mut seen = 0;
            run(&mut traffic, 72_000, |t| {
                for car in &t.cars {
                    assert!(car.rel.is_finite());
                    if car.kind == Kind::Lead {
                        assert!(
                            (FOLLOW_MIN - 1.0..=FAR_AHEAD).contains(&car.rel),
                            "{}",
                            car.rel
                        );
                    }
                }
                for light in t.lights() {
                    assert!((NEAR_DISTANCE..VIEW_DISTANCE).contains(&light.d));
                    assert!(light.alpha > 0.0 && light.alpha <= 1.0, "{light:?}");
                }
                seen += t.cars.iter().filter(|c| c.truck).count();
            });
            assert!(seen > 0);
        }
    }

    #[test]
    fn a_japanese_truck_seen_from_behind_has_tail_end_and_side_lamps() {
        for id in REAR_IDS {
            let car = truck_of(id, TruckStyle::Japanese, 100.0);
            let lights = lights_of(&car, TruckStyle::Japanese);
            assert_eq!(lights.len(), 2 + 2 + 7);
            assert_eq!(count(&lights, 1.0, TAIL_RED), 2);
            assert_eq!(count(&lights, 3.4, TAIL_RED), 2);
            assert_eq!(count(&lights, 0.9, MARKER_AMBER), 7);
            let tails: Vec<f32> = lights
                .iter()
                .filter(|l| l.height == 1.0)
                .map(|l| l.lateral - car.lateral)
                .collect();
            assert!(tails.contains(&-1.05) && tails.contains(&1.05));
        }
    }

    #[test]
    fn a_north_american_truck_seen_from_behind_has_identification_lamps() {
        for id in REAR_IDS {
            let car = truck_of(id, TruckStyle::NorthAmerican, 100.0);
            let lights = lights_of(&car, TruckStyle::NorthAmerican);
            assert_eq!(lights.len(), 2 + 3 + 2 + 2);
            assert_eq!(count(&lights, 1.0, TAIL_RED), 3);
            assert_eq!(count(&lights, 1.0, MARKER_AMBER), 1);
            assert_eq!(count(&lights, 3.5, TAIL_RED), 5);
            let mut offsets: Vec<f32> = lights
                .iter()
                .filter(|l| l.height == 3.5)
                .map(|l| l.lateral - car.lateral)
                .collect();
            offsets.sort_by(f32::total_cmp);
            for (offset, expected) in offsets.iter().zip([-1.2, -0.3, 0.0, 0.3, 1.2]) {
                assert!((offset - expected).abs() < 1e-5, "{offsets:?}");
            }
        }
    }

    #[test]
    fn an_oncoming_truck_shows_headlights_roof_and_side_lamps() {
        let car = truck_of(ONCOMING_ID, TruckStyle::Japanese, 200.0);
        let lights = lights_of(&car, TruckStyle::Japanese);
        assert_eq!(lights.len(), 2 + 2 + 7);
        assert_eq!(count(&lights, 0.9, HEADLIGHT_WHITE), 2);
        assert_eq!(count(&lights, 3.3, MARKER_AMBER), 2);
        assert_eq!(count(&lights, 0.9, MARKER_AMBER), 7);

        let car = truck_of(ONCOMING_ID, TruckStyle::NorthAmerican, 200.0);
        let lights = lights_of(&car, TruckStyle::NorthAmerican);
        assert_eq!(lights.len(), 2 + 2 + 5);
        assert_eq!(count(&lights, 0.9, HEADLIGHT_WHITE), 2);
        assert_eq!(count(&lights, 3.0, MARKER_AMBER), 5);
        // One amber lamp near the front (close to the camera) and one red
        // near the rear (far from it).
        let amber = lights
            .iter()
            .find(|l| l.height == 1.0 && l.color == MARKER_AMBER)
            .unwrap();
        let red = lights
            .iter()
            .find(|l| l.height == 1.0 && l.color == TAIL_RED)
            .unwrap();
        assert!(amber.d < red.d);
        let mut offsets: Vec<f32> = lights
            .iter()
            .filter(|l| l.height == 3.0)
            .map(|l| l.lateral - car.lateral)
            .collect();
        offsets.sort_by(f32::total_cmp);
        for (offset, expected) in offsets.iter().zip([-0.9, -0.45, 0.0, 0.45, 0.9]) {
            assert!((offset - expected).abs() < 1e-5, "{offsets:?}");
        }
    }

    #[test]
    fn side_lamps_are_on_the_side_facing_the_camera() {
        for style in [TruckStyle::Japanese, TruckStyle::NorthAmerican] {
            for (id, lateral, expected) in [
                (PASSING_ID, PASSING_LANE, PASSING_LANE - TRUCK_HALF_WIDTH),
                (ONCOMING_ID, ONCOMING_LANES[0], ONCOMING_LANES[0] - 1.25),
                (ONCOMING_ID, ONCOMING_LANES[1], ONCOMING_LANES[1] - 1.25),
            ] {
                let mut car = truck_of(id, style, 100.0);
                car.lateral = lateral;
                let lights = lights_of(&car, style);
                let sides: Vec<&CarLight> = lights
                    .iter()
                    .filter(|l| l.color == MARKER_AMBER && l.height <= 1.0)
                    .collect();
                assert!(!sides.is_empty());
                assert!(sides.iter().all(|l| (l.lateral - expected).abs() < 1e-5));
            }
        }
    }

    #[test]
    fn side_lamps_line_up_along_the_body() {
        let car = truck_of(PASSING_ID, TruckStyle::Japanese, 100.0);
        let mut distances: Vec<f32> = lights_of(&car, TruckStyle::Japanese)
            .iter()
            .filter(|l| l.color == MARKER_AMBER)
            .map(|l| l.d)
            .collect();
        distances.sort_by(f32::total_cmp);
        assert_eq!(distances.len(), 7);
        for (index, d) in distances.iter().enumerate() {
            assert!((d - (100.0 + 1.8 * index as f32)).abs() < 1e-3, "{d}");
        }
    }

    #[test]
    fn a_braking_truck_has_brighter_tail_lamps_and_no_high_mount_lamp() {
        let mut car = truck_of(LEAD_ID, TruckStyle::Japanese, 100.0);
        let tails = |car: &Car| {
            lights_of(car, TruckStyle::Japanese)
                .into_iter()
                .filter(|l| l.height == 1.0)
                .collect::<Vec<_>>()
        };
        let normal = tails(&car);
        let total = lights_of(&car, TruckStyle::Japanese).len();
        car.braking_left = 1.0;
        let braking = tails(&car);
        assert!(braking[0].alpha > normal[0].alpha && braking[0].size > normal[0].size);
        assert_eq!(lights_of(&car, TruckStyle::Japanese).len(), total);
    }

    #[test]
    fn upper_lamps_are_dimmer_and_smaller_than_tail_lamps() {
        let car = truck_of(LEAD_ID, TruckStyle::Japanese, 100.0);
        let lights = lights_of(&car, TruckStyle::Japanese);
        let tail = lights.iter().find(|l| l.height == 1.0).unwrap();
        let upper = lights.iter().find(|l| l.height == 3.4).unwrap();
        assert!(upper.size < tail.size && upper.alpha < tail.alpha);
    }

    #[test]
    fn an_oncoming_truck_dazzles_like_a_car() {
        let mut car = truck_of(ONCOMING_ID, TruckStyle::Japanese, 200.0);
        let glare = |car: &Car| lights_of(car, TruckStyle::Japanese).iter().any(|l| l.glare);
        assert!(!glare(&car));
        car.rel = 50.0;
        assert!(glare(&car));
    }

    #[test]
    fn lamps_past_the_view_distance_are_left_out() {
        let car = truck_of(ONCOMING_ID, TruckStyle::Japanese, VIEW_DISTANCE - 3.0);
        let lights = lights_of(&car, TruckStyle::Japanese);
        assert!(lights.iter().all(|l| l.d < VIEW_DISTANCE));
        assert!(count(&lights, 0.9, MARKER_AMBER) < 7);
    }

    #[test]
    fn invalid_steps_are_ignored() {
        let mut traffic = Traffic::new(0, TruckStyle::None);
        let before = snapshot(&traffic);
        for dt in [f32::NAN, -1.0, 0.0, f32::INFINITY] {
            traffic.update(dt, OWN_SPEED);
        }
        assert_eq!(snapshot(&traffic), before);
        traffic.update(DT, f32::NAN);
        traffic.update(DT, -5.0);
        assert!(traffic.cars.iter().all(|c| c.rel.is_finite()));
    }

    fn body_of(id: u32, truck: bool, rel: f32) -> Vec<BodyFace> {
        let mut car = Car::new(0, id, TruckStyle::Japanese, TrafficLevel::FULL);
        car.truck = truck;
        car.active = true;
        car.rel = rel;
        let mut faces = Vec::new();
        car.push_body(&mut faces);
        faces
    }

    fn span(faces: &[BodyFace], axis: fn(&(f32, f32, f32)) -> f32) -> f32 {
        let values = faces.iter().flat_map(|f| f.corners.iter().map(axis));
        values.clone().fold(f32::MIN, f32::max) - values.fold(f32::MAX, f32::min)
    }

    #[test]
    fn the_end_face_of_a_car_is_a_car_wide_and_high() {
        for id in [LEAD_ID, PASSING_ID, ONCOMING_ID] {
            let faces = body_of(id, false, 50.0);
            let end = faces[0];
            assert!(end.corners.iter().all(|c| c.0 == 50.0), "{id}");
            assert!((span(&faces[..1], |c| c.1) - 1.8).abs() < 1e-4, "{id}");
            assert!((span(&faces[..1], |c| c.2) - 1.45).abs() < 1e-4, "{id}");
            // The face stands on the road, at the end of the body.
            assert_eq!(
                end.corners.iter().map(|c| c.2).fold(f32::MAX, f32::min),
                0.0
            );
            assert!(end.corners.iter().all(|c| c.0 == 50.0), "{id}");
        }
    }

    #[test]
    fn the_end_face_of_a_truck_is_a_truck_wide_and_high() {
        for id in [LEAD_ID, PASSING_ID, ONCOMING_ID] {
            let faces = body_of(id, true, 50.0);
            assert!((span(&faces[..1], |c| c.1) - 2.5).abs() < 1e-4, "{id}");
            assert!((span(&faces[..1], |c| c.2) - 3.6).abs() < 1e-4, "{id}");
        }
    }

    #[test]
    fn a_car_in_another_lane_shows_the_side_toward_the_own_lane() {
        for (truck, length, half) in [(false, 4.5, 0.9), (true, 12.0, 1.25)] {
            for id in [PASSING_ID, ONCOMING_ID] {
                let faces = body_of(id, truck, 50.0);
                assert_eq!(faces.len(), 2, "{id}");
                let (end, side) = (faces[0], faces[1]);
                let lateral = end.lateral;
                assert!(lateral > OWN_LANE);
                // The side reaches the length of the car away from the camera,
                // along the edge of the body that is nearer to the own lane.
                assert!(side.corners.iter().all(|c| c.1 == lateral - half), "{id}");
                assert_eq!(span(&faces[1..], |c| c.0), length, "{id}");
                assert_eq!(side.corners[0].0, 50.0, "{id}");
            }
        }
    }

    #[test]
    fn a_car_in_the_own_lane_shows_only_its_end() {
        for truck in [false, true] {
            assert_eq!(body_of(LEAD_ID, truck, 50.0).len(), 1);
        }
    }

    #[test]
    fn a_car_that_does_not_run_has_no_body() {
        let mut traffic = Traffic::with_level(
            2,
            TruckStyle::Japanese,
            TrafficLevel {
                active: 0.0,
                trucks: 0.0,
            },
        );
        assert!(traffic.bodies().is_empty());
        traffic.set_level(TrafficLevel::FULL);
        traffic.cars.iter_mut().for_each(|c| c.active = true);
        assert!(!traffic.bodies().is_empty());
    }

    #[test]
    fn the_body_of_a_passing_car_is_cut_off_at_the_camera() {
        // The rear is behind the camera: only the side that is ahead of it is left.
        let faces = body_of(PASSING_ID, false, -2.0);
        assert_eq!(faces.len(), 1);
        let near = faces[0]
            .corners
            .iter()
            .map(|c| c.0)
            .fold(f32::MAX, f32::min);
        assert_eq!(near, NEAR_DISTANCE);
        assert_eq!(faces[0].corners[1].0, 2.5);
        // Once the whole car is behind the camera nothing is left.
        assert!(body_of(PASSING_ID, false, -3.6).is_empty());
        assert_eq!(body_of(PASSING_ID, true, -10.0).len(), 1);
        assert!(body_of(PASSING_ID, true, -12.0).is_empty());
    }

    #[test]
    fn the_body_stops_before_the_end_of_the_view() {
        for rel in [VIEW_DISTANCE - 3.0, VIEW_DISTANCE - 0.005, VIEW_DISTANCE] {
            for truck in [false, true] {
                for id in [LEAD_ID, PASSING_ID, ONCOMING_ID] {
                    for face in body_of(id, truck, rel) {
                        assert!(
                            face.corners
                                .iter()
                                .all(|c| (NEAR_DISTANCE..VIEW_DISTANCE).contains(&c.0)),
                            "{rel} {id}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn the_bodies_are_finite_for_an_hour_of_driving() {
        let mut traffic = Traffic::new(4, TruckStyle::NorthAmerican);
        let mut seen = 0;
        run(&mut traffic, 72_000, |t| {
            for face in t.bodies() {
                seen += 1;
                assert!(
                    face.corners
                        .iter()
                        .all(|c| c.0.is_finite() && c.1.is_finite() && c.2.is_finite())
                );
            }
        });
        assert!(seen > 10_000);
    }

    #[test]
    fn car_body_choices_give_their_opacity() {
        assert_eq!(CarBodies::Transparent.opacity(), None);
        assert_eq!(CarBodies::TranslucentBlack.opacity(), Some(0.9));
        assert_eq!(CarBodies::Black.opacity(), Some(1.0));
    }
}
