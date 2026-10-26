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
/// Tail lights fade out at this distance.
const TAIL_RANGE: f32 = 650.0;
/// Glare of an oncoming car is seen within this distance.
const GLARE_RANGE: f32 = 90.0;
const GLARE_ALPHA: f32 = 0.25;

// Salts keep the random values of one car independent of each other.
const SALT_POSITION: u32 = 10;
const SALT_SPEED: u32 = 11;
const SALT_LANE: u32 = 12;
const SALT_BRAKE_GAP: u32 = 13;
const SALT_BRAKE_LENGTH: u32 = 14;
const SALT_ESCAPE: u32 = 15;
const SALT_PHASE: u32 = 16;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    /// Same direction, same lane, a little slower than the camera.
    Lead,
    /// Same direction, passing lane, faster than the camera.
    Passing,
    Oncoming,
}

/// A light of a car, positioned like the lights of the road.
#[derive(Clone, Copy, Debug)]
pub struct CarLight {
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
    /// How often the car has braked.
    brakes: u32,
    until_brake: f32,
    braking_left: f32,
}

impl Car {
    fn new(seed: u64, id: u32) -> Self {
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
            brakes: 0,
            until_brake: 0.0,
            braking_left: 0.0,
        };
        car.place(seed, true);
        car.until_brake = car.brake_interval(seed);
        car
    }

    /// Random number for this car and its current placement.
    fn roll(&self, seed: u64, salt: u32) -> f32 {
        hash01(
            seed,
            salt,
            i64::from(self.id) << 32 | i64::from(self.generation),
        )
    }

    /// Sets position, speed and lane. `initial` spreads the cars over the
    /// whole view; later they enter where they cannot be seen, except that
    /// lead cars keep appearing ahead.
    fn place(&mut self, seed: u64, initial: bool) {
        let position = self.roll(seed, SALT_POSITION);
        let speed = self.roll(seed, SALT_SPEED);
        self.age = 0.0;
        self.phase = self.roll(seed, SALT_PHASE) * std::f32::consts::TAU;
        self.escape_after = 40.0 + 80.0 * self.roll(seed, SALT_ESCAPE);
        match self.kind {
            Kind::Lead => {
                self.lateral = OWN_LANE;
                self.rel = 80.0 + 450.0 * position;
                self.ratio = 0.85 + 0.10 * speed;
            }
            Kind::Passing => {
                self.lateral = PASSING_LANE;
                self.rel = if initial {
                    20.0 + 500.0 * position
                } else {
                    -(30.0 + 150.0 * position)
                };
                self.speed = 2.0 + 8.0 * speed;
            }
            Kind::Oncoming => {
                self.lateral = ONCOMING_LANES[usize::from(self.roll(seed, SALT_LANE) >= 0.5)];
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
            SALT_BRAKE_GAP,
            i64::from(self.id) << 32 | i64::from(self.brakes),
        );
        // The gap between random events is exponentially distributed.
        (-MEAN_BRAKE_INTERVAL * (1.0 - gap).ln()).max(MIN_BRAKE_INTERVAL)
    }

    fn brake_length(&self, seed: u64) -> f32 {
        let length = hash01(
            seed,
            SALT_BRAKE_LENGTH,
            i64::from(self.id) << 32 | i64::from(self.brakes),
        );
        0.8 + length
    }

    fn braking(&self) -> bool {
        self.braking_left > 0.0
    }

    /// How fast the car approaches the camera, in meters per second
    /// (negative while it pulls away).
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

    fn step(&mut self, seed: u64, dt: f32, own_speed: f32) {
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
            self.place(seed, false);
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

    fn push_lights(&self, out: &mut Vec<CarLight>) {
        let d = self.rel;
        if !(NEAR_DISTANCE..VIEW_DISTANCE).contains(&d) {
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

pub struct Traffic {
    seed: u64,
    cars: Vec<Car>,
}

impl Traffic {
    pub fn new(seed: u64) -> Self {
        let count = LEAD_CARS + PASSING_CARS + ONCOMING_CARS;
        Self {
            seed,
            cars: (0..count).map(|id| Car::new(seed, id)).collect(),
        }
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
            car.step(self.seed, dt, own_speed);
        }
    }

    /// The lights in front of the camera, in no particular order.
    pub fn lights(&self) -> Vec<CarLight> {
        let mut lights = Vec::new();
        for car in &self.cars {
            car.push_lights(&mut lights);
        }
        lights
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
        let (mut a, mut b) = (Traffic::new(5), Traffic::new(5));
        run(&mut a, 6000, |_| {});
        run(&mut b, 6000, |_| {});
        assert_eq!(snapshot(&a), snapshot(&b));
        let mut c = Traffic::new(6);
        run(&mut c, 6000, |_| {});
        assert_ne!(snapshot(&a), snapshot(&c));
    }

    #[test]
    fn traffic_stays_healthy_for_an_hour() {
        for seed in [0, 1, 2] {
            let mut traffic = Traffic::new(seed);
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
        let mut traffic = Traffic::new(3);
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
        let mut traffic = Traffic::new(1);
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
        let mut traffic = Traffic::new(2);
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
        let mut traffic = Traffic::new(4);
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
        let mut traffic = Traffic::new(7);
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
        let mut car = Car::new(0, 0);
        car.rel = 100.0;
        let lights = |car: &Car| {
            let mut out = Vec::new();
            car.push_lights(&mut out);
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
        let mut car = Car::new(0, LEAD_CARS + PASSING_CARS);
        let glare = |car: &mut Car, d: f32| {
            car.rel = d;
            let mut out = Vec::new();
            car.push_lights(&mut out);
            out.iter().find(|l| l.glare).map(|l| l.alpha)
        };
        assert_eq!(glare(&mut car, 200.0), None);
        let far = glare(&mut car, 80.0).unwrap();
        let near = glare(&mut car, 10.0).unwrap();
        assert!(near > far && near <= GLARE_ALPHA);
    }

    #[test]
    fn invalid_steps_are_ignored() {
        let mut traffic = Traffic::new(0);
        let before = snapshot(&traffic);
        for dt in [f32::NAN, -1.0, 0.0, f32::INFINITY] {
            traffic.update(dt, OWN_SPEED);
        }
        assert_eq!(snapshot(&traffic), before);
        traffic.update(DT, f32::NAN);
        traffic.update(DT, -5.0);
        assert!(traffic.cars.iter().all(|c| c.rel.is_finite()));
    }
}
