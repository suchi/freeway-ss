//! The car of the camera: it catches up with slower cars, passes them in the
//! other lane and returns, or keeps behind them when the other lane is full.
//!
//! Positions across the road are those of the traffic (`OWN_LANE`,
//! `PASSING_LANE`); the projection mirrors them for right-hand traffic.

use crate::traffic::{
    AHEAD_CLEAR, BEHIND_CLEAR, EGO_LENGTH, LANE_GAP, MOVE_SECONDS, OWN_LANE, PASSING_LANE,
    RETURN_AHEAD_CLEAR, RETURN_GAP, SIGNAL_SECONDS, Seconds, Traffic, smooth, smooth_slope,
    tunnel_in_the_way,
};

/// The car looks for a way past a slower car when it is this many seconds of
/// driving away from it.
const CATCH_SECONDS: f32 = 4.0;
/// A car ahead has to be at least this much slower, in meters per second,
/// for the car to pass it; with less, passing takes minutes and the car
/// only keeps behind it.
const MIN_PASS_CLOSING: f32 = 2.0;
/// ... and never from nearer than this, in meters, however slow it drives.
/// It lets go of the car ahead at this many seconds of driving.
const CATCH_MIN: f32 = 40.0;
const RELEASE_SECONDS: f32 = 6.0;
/// The car keeps this many seconds of driving behind a car it cannot pass,
/// and no less than `FOLLOW_MIN` meters.
const FOLLOW_SECONDS: f32 = 2.0;
const FOLLOW_MIN: f32 = 20.0;
/// How fast the speed follows the gap to a car ahead, per second.
const FOLLOW_GAIN: f32 = 0.2;
/// The car slows down at the rate of `LANE_DECEL` of the traffic at most, in
/// meters per second squared, and speeds up at this rate. The safe speed
/// near a car ahead is figured at `SAFE_DECEL`, so that the car has room to
/// spare.
const BRAKE: f32 = 3.0;
const SAFE_DECEL: f32 = 2.5;
const ACCELERATION: f32 = 1.5;
/// ... and it passes cars together that are this many seconds of driving apart.
const PLATOON_SECONDS: f32 = 3.0;
/// A car this near behind in the passing lane is held up by the car.
const TAILGATE_DISTANCE: f32 = 60.0;
/// A lane change given up is not tried again before this many seconds.
const GIVE_UP_SECONDS: f32 = 5.0;
/// The car does not return before it has been in the other lane this long, in seconds.
const MIN_PASS_SECONDS: f32 = 3.0;
/// The camera turns toward the lane it moves to by at most this much, in radians.
pub const MAX_YAW: f32 = 1.5 * std::f32::consts::PI / 180.0;
/// The speed of the move across the road is at its largest in the middle.
const PEAK_SLOPE: f32 = 1.875;

/// What the car is doing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    /// Driving in the own lane at its own speed.
    Cruise,
    /// Behind a slower car it cannot pass yet.
    Follow,
    /// Signaling before moving out to the passing lane.
    SignalOut,
    MoveOut,
    /// Driving in the passing lane.
    Pass,
    /// Signaling before moving back to the own lane.
    SignalIn,
    MoveIn,
}

impl Phase {
    /// Whether the turn signal blinks.
    pub fn is_signaling(self) -> bool {
        !matches!(self, Phase::Cruise | Phase::Follow | Phase::Pass)
    }
}

/// The car of the camera.
#[derive(Clone, Copy, Debug)]
pub struct Ego {
    phase: Phase,
    /// Seconds in the phase; the signal and the move are one stretch of time.
    time: f32,
    /// Seconds since the lane change began, for the blink.
    blink: f32,
    speed: f32,
    /// The speed the car would drive at the step before; its change is the
    /// rate at which the speed keys work.
    target: f32,
    lateral: f32,
    yaw: f32,
    /// Seconds before the car tries a lane change again.
    pause: f32,
}

impl Ego {
    /// A car in the own lane, driving at `speed` meters per second.
    pub fn new(speed: f32) -> Self {
        Self {
            phase: Phase::Cruise,
            time: 0.0,
            blink: 0.0,
            speed,
            target: speed,
            lateral: OWN_LANE,
            yaw: 0.0,
            pause: 0.0,
        }
    }

    pub fn phase(&self) -> Phase {
        self.phase
    }

    /// The speed now, in meters per second.
    pub fn speed(&self) -> f32 {
        self.speed
    }

    /// Where the car is across the road.
    pub fn lateral(&self) -> f32 {
        self.lateral
    }

    /// How far the camera is turned toward larger lateral positions, in radians.
    pub fn yaw(&self) -> f32 {
        self.yaw
    }

    /// Seconds since the car began to signal for the lane change, while it does.
    pub fn signal_time(&self) -> Option<f32> {
        self.phase.is_signaling().then_some(self.blink)
    }

    /// The lanes the car takes up: two while it signals and moves, since the
    /// other cars are to keep clear of the lane it is about to take.
    pub fn lanes(&self) -> [f32; 2] {
        match self.phase {
            Phase::Cruise | Phase::Follow => [OWN_LANE; 2],
            Phase::Pass => [PASSING_LANE; 2],
            Phase::SignalOut | Phase::MoveOut | Phase::SignalIn | Phase::MoveIn => {
                [OWN_LANE, PASSING_LANE]
            }
        }
    }

    /// Moves the car for `dt` seconds. `target` is the speed it would drive
    /// at on an empty road, `tunnels` are the tunnels as (entrance, exit) in
    /// meters ahead of the camera.
    pub fn update(&mut self, dt: f32, target: f32, traffic: &Traffic, tunnels: &[(f32, f32)]) {
        if !(dt.is_finite() && dt > 0.0) {
            return;
        }
        // The speed keys change the target at a rate of their own, which the
        // speed has to be able to keep up with.
        let target_rate = (target - self.target) / dt;
        self.target = target;
        self.time += dt;
        self.pause = (self.pause - dt).max(0.0);
        self.decide(dt, target, traffic, tunnels);
        self.drive(dt, target, target_rate, traffic);
        self.place();
    }

    fn enter(&mut self, phase: Phase) {
        self.phase = phase;
        self.time = 0.0;
    }

    /// The phase after this step.
    fn decide(&mut self, dt: f32, target: f32, traffic: &Traffic, tunnels: &[(f32, f32)]) {
        let speed = self.speed;
        let tunnel_ahead = || tunnel_in_the_way(tunnels, -EGO_LENGTH, EGO_LENGTH, speed);
        match self.phase {
            Phase::Cruise | Phase::Follow => {
                let lead = traffic.scan(&[OWN_LANE]).ahead;
                let lead_speed = lead.map(|a| self.speed + a.velocity);
                let slower = lead_speed.is_some_and(|v| target - v > MIN_PASS_CLOSING);
                let caught = lead.is_some_and(|a| slower && a.gap < self.catch_distance());
                let held = lead.is_some_and(|a| slower && a.gap < self.speed * RELEASE_SECONDS);
                if self.phase == Phase::Follow && !held {
                    self.enter(Phase::Cruise);
                }
                if caught && self.phase == Phase::Cruise {
                    self.enter(Phase::Follow);
                }
                if self.phase == Phase::Follow
                    && self.pause <= 0.0
                    && !tunnel_ahead()
                    && traffic.scan(&[PASSING_LANE]).is_open(
                        Seconds::OUT,
                        AHEAD_CLEAR,
                        BEHIND_CLEAR,
                    )
                {
                    self.enter(Phase::SignalOut);
                    self.blink = 0.0;
                }
            }
            Phase::SignalOut => {
                self.blink += dt;
                if self.time >= SIGNAL_SECONDS {
                    // Look again before moving.
                    if traffic.scan(&[PASSING_LANE]).is_open(
                        Seconds::MOVE,
                        LANE_GAP * 2.0,
                        LANE_GAP * 2.0,
                    ) {
                        self.enter(Phase::MoveOut);
                    } else {
                        self.pause = GIVE_UP_SECONDS;
                        self.enter(Phase::Follow);
                    }
                }
            }
            Phase::MoveOut => {
                self.blink += dt;
                if self.time >= MOVE_SECONDS {
                    self.enter(Phase::Pass);
                }
            }
            Phase::Pass => {
                // Return to the own lane when the cars that were passed are
                // far enough behind, and there is room ahead of them. Not
                // while there is a slower car in the own lane that it would
                // have to pass again, unless a car is held up behind it.
                let own = traffic.scan(&[OWN_LANE]);
                // A car right behind in the passing lane, held up, wants it back.
                let tailgated = traffic
                    .scan(&[PASSING_LANE])
                    .behind
                    .is_some_and(|b| b.gap < TAILGATE_DISTANCE);
                let held_again = !tailgated
                    && own.ahead.is_some_and(|a| {
                        -a.velocity > MIN_PASS_CLOSING && a.gap < self.platoon_distance()
                    });
                let behind_clear = if tunnel_ahead_soon(tunnels, self.speed) {
                    LANE_GAP * 2.0
                } else {
                    RETURN_GAP
                };
                if self.time >= MIN_PASS_SECONDS
                    && self.pause <= 0.0
                    && !held_again
                    && !tunnel_ahead()
                    && own.is_open(Seconds::RETURN, RETURN_AHEAD_CLEAR, behind_clear)
                {
                    self.enter(Phase::SignalIn);
                    self.blink = 0.0;
                }
            }
            Phase::SignalIn => {
                self.blink += dt;
                if self.time >= SIGNAL_SECONDS {
                    if traffic.scan(&[OWN_LANE]).is_open(
                        Seconds::MOVE,
                        LANE_GAP * 2.0,
                        LANE_GAP * 2.0,
                    ) {
                        self.enter(Phase::MoveIn);
                    } else {
                        self.pause = GIVE_UP_SECONDS;
                        self.enter(Phase::Pass);
                        self.time = MIN_PASS_SECONDS;
                    }
                }
            }
            Phase::MoveIn => {
                self.blink += dt;
                if self.time >= MOVE_SECONDS {
                    self.enter(Phase::Cruise);
                    self.pause = GIVE_UP_SECONDS;
                }
            }
        }
    }

    /// A slower car in the own lane this near, in meters, is passed too before
    /// the car returns: it would be caught up with at once.
    fn platoon_distance(&self) -> f32 {
        (PLATOON_SECONDS * self.speed).max(CATCH_MIN)
    }

    /// How near a slower car ahead must be before the car thinks of passing it.
    fn catch_distance(&self) -> f32 {
        (CATCH_SECONDS * self.speed).max(CATCH_MIN)
    }

    /// The speed after this step: the target, held back by the cars ahead.
    fn drive(&mut self, dt: f32, target: f32, target_rate: f32, traffic: &Traffic) {
        // The lane whose cars the car has to mind: the one it is nearer to.
        let lane = match self.phase {
            Phase::Cruise | Phase::Follow | Phase::SignalOut => OWN_LANE,
            Phase::MoveOut if self.time < MOVE_SECONDS * 0.5 => OWN_LANE,
            Phase::MoveOut | Phase::Pass | Phase::SignalIn => PASSING_LANE,
            Phase::MoveIn if self.time < MOVE_SECONDS * 0.5 => PASSING_LANE,
            Phase::MoveIn => OWN_LANE,
        };
        // Held up at a distance only where it drives along in a lane; on the
        // way to the other lane it only keeps clear.
        let settled = matches!(self.phase, Phase::Cruise | Phase::Follow | Phase::Pass);
        let mut wanted = target;
        if let Some(ahead) = traffic.scan(&[lane]).ahead {
            let lead = (self.speed + ahead.velocity).max(0.0);
            // The speed from which the car can slow down to the speed of the
            // car ahead by the time the gap is the least.
            let room = (ahead.gap - LANE_GAP - 1.0).max(0.0);
            let safe = lead + (2.0 * SAFE_DECEL * room).sqrt();
            // The speed that settles the gap at the distance of following.
            let gap = (FOLLOW_SECONDS * self.speed).max(FOLLOW_MIN);
            let follow = lead + FOLLOW_GAIN * (ahead.gap - gap);
            wanted = wanted.min(safe);
            if settled {
                wanted = wanted.min(follow.max(0.0));
            }
        }
        let up = ACCELERATION + target_rate.max(0.0);
        let down = BRAKE + (-target_rate).max(0.0);
        self.speed = (self.speed + (wanted - self.speed).clamp(-down * dt, up * dt)).max(0.0);
    }

    /// Where the car is across the road, and how the camera is turned.
    fn place(&mut self) {
        let across = |phase_time: f32| (phase_time / MOVE_SECONDS).clamp(0.0, 1.0);
        let (lateral, direction, progress) = match self.phase {
            Phase::Cruise | Phase::Follow | Phase::SignalOut => (OWN_LANE, 0.0, 0.0),
            Phase::MoveOut => {
                let x = across(self.time);
                (OWN_LANE + (PASSING_LANE - OWN_LANE) * smooth(x), 1.0, x)
            }
            Phase::Pass | Phase::SignalIn => (PASSING_LANE, 0.0, 0.0),
            Phase::MoveIn => {
                let x = across(self.time);
                (
                    PASSING_LANE + (OWN_LANE - PASSING_LANE) * smooth(x),
                    -1.0,
                    x,
                )
            }
        };
        self.lateral = lateral;
        // The camera turns the way the car moves, in proportion to the speed
        // across the road.
        self.yaw = direction * MAX_YAW * smooth_slope(progress) / PEAK_SLOPE;
    }
}

/// Whether a tunnel is near enough that the car should return to its lane
/// at the first chance.
fn tunnel_ahead_soon(tunnels: &[(f32, f32)], speed: f32) -> bool {
    let reach = speed * 30.0 + 100.0;
    tunnels
        .iter()
        .any(|&(start, end)| start < reach && end > 0.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traffic::{CarShapes, LANE_GAP, LANE_WIDTH, LaneChanges, Surroundings};

    const DT: f32 = 0.05;
    const TARGET: f32 = 100.0 / 3.6;

    struct World {
        ego: Ego,
        traffic: Traffic,
        tunnels: Vec<(f32, f32)>,
        time: f32,
    }

    impl World {
        fn new() -> Self {
            Self {
                ego: Ego::new(TARGET),
                traffic: Traffic::empty(1, CarShapes::None, LaneChanges::OwnCar),
                tunnels: Vec::new(),
                time: 0.0,
            }
        }

        fn step(&mut self) {
            self.ego.update(DT, TARGET, &self.traffic, &self.tunnels);
            self.traffic.update_around(
                DT,
                self.ego.speed(),
                &Surroundings {
                    target_speed: TARGET,
                    ego_lanes: &self.ego.lanes(),
                    ego_lateral: self.ego.lateral(),
                    tunnels: &self.tunnels,
                },
            );
            for tunnel in &mut self.tunnels {
                tunnel.0 -= self.ego.speed() * DT;
                tunnel.1 -= self.ego.speed() * DT;
            }
            self.time += DT;
        }

        fn run(&mut self, seconds: f32, mut each: impl FnMut(&World)) {
            for _ in 0..(seconds / DT) as usize {
                self.step();
                each(self);
            }
        }
    }

    #[test]
    fn it_catches_up_with_a_slower_car_passes_it_and_returns() {
        let mut world = World::new();
        // Only a little slower: the pass takes long, and the car stays out
        // until it is done.
        world.traffic.plant(0, OWN_LANE, 100.0, 0.9, false);
        let mut phases = vec![world.ego.phase()];
        let mut passed_at = None;
        let mut least_speed = f32::MAX;
        world.run(400.0, |w| {
            if phases.last() != Some(&w.ego.phase()) {
                phases.push(w.ego.phase());
                if w.ego.phase() == Phase::SignalIn {
                    passed_at = Some(w.traffic.rel_of(0));
                }
            }
            least_speed = least_speed.min(w.ego.speed());
        });
        use Phase::*;
        assert!(
            phases.starts_with(&[Cruise, SignalOut, MoveOut, Pass, SignalIn, MoveIn, Cruise]),
            "{phases:?}"
        );
        // The car it passed is well behind when it returns.
        let behind = passed_at.unwrap();
        assert!(behind < -(EGO_LENGTH + RETURN_GAP - 1.0), "{behind}");
        // It did not slow down for the car: the lane was free.
        assert!(least_speed > TARGET - 0.5, "{least_speed}");
        assert_eq!(world.ego.lateral(), OWN_LANE);
    }

    #[test]
    fn it_keeps_behind_a_car_while_the_other_lane_is_full_and_passes_when_it_is_free() {
        let mut world = World::new();
        world.traffic.plant(0, OWN_LANE, 100.0, 0.8, false);
        // A car in the passing lane that keeps alongside the lead car.
        world.traffic.plant(1, PASSING_LANE, 95.0, 0.8, false);
        let mut closest = f32::MAX;
        world.run(90.0, |w| {
            assert_eq!(w.ego.lateral(), OWN_LANE);
            assert!(!w.ego.phase().is_signaling(), "{:?}", w.ego.phase());
            closest = closest.min(w.traffic.rel_of(0));
        });
        assert_eq!(world.ego.phase(), Phase::Follow);
        // It follows at the speed of the car ahead, at a distance.
        let lead_speed = TARGET * 0.8;
        assert!(
            (world.ego.speed() - lead_speed).abs() < 0.5,
            "{}",
            world.ego.speed()
        );
        assert!(closest >= FOLLOW_MIN - 1.0, "{closest}");
        assert!(closest >= LANE_GAP + 1.0);
        // The passing lane is free again.
        world.traffic.unplant(1);
        let mut seen = Vec::new();
        world.run(40.0, |w| {
            if seen.last() != Some(&w.ego.phase()) {
                seen.push(w.ego.phase());
            }
        });
        assert!(
            seen.windows(3)
                .any(|w| w == [Phase::SignalOut, Phase::MoveOut, Phase::Pass]),
            "{seen:?}"
        );
    }

    #[test]
    fn it_does_not_pass_a_car_that_is_almost_as_fast() {
        let mut world = World::new();
        world.traffic.plant(0, OWN_LANE, 100.0, 0.98, false);
        world.run(120.0, |w| assert!(!w.ego.phase().is_signaling()));
        assert_eq!(world.ego.lateral(), OWN_LANE);
    }

    #[test]
    fn it_does_not_start_a_lane_change_in_or_before_a_tunnel() {
        // The tunnel starts 250 m ahead: a lane change takes more than that.
        let mut world = World::new();
        world.tunnels.push((250.0, 900.0));
        world.traffic.plant(0, OWN_LANE, 100.0, 0.8, false);
        world.run(60.0, |w| {
            let (start, end) = w.tunnels[0];
            if end > 0.0 {
                assert!(!w.ego.phase().is_signaling());
            }
            assert!(w.ego.lateral() == OWN_LANE || end <= 0.0 || start > 0.0);
        });
        // Inside the tunnel nothing starts either.
        let inside = World::new();
        let mut world = World {
            tunnels: vec![(-100.0, 2000.0)],
            ..inside
        };
        world.traffic.plant(0, OWN_LANE, 100.0, 0.8, false);
        world.run(60.0, |w| assert!(!w.ego.phase().is_signaling()));
        // Far ahead of it, a tunnel is no hindrance.
        let mut world = World::new();
        world.tunnels.push((650.0, 1200.0));
        world.traffic.plant(0, OWN_LANE, 100.0, 0.8, false);
        let mut signaled = false;
        world.run(20.0, |w| signaled |= w.ego.phase().is_signaling());
        assert!(signaled);
    }

    #[test]
    fn it_finishes_a_lane_change_before_a_tunnel_it_started_before() {
        let mut world = World::new();
        world.traffic.plant(0, OWN_LANE, 100.0, 0.8, false);
        // A tunnel as near as the rule allows.
        world.tunnels.push((600.0, 3000.0));
        let mut moved_in_tunnel = false;
        world.run(120.0, |w| {
            let inside = w.tunnels[0].0 <= 0.0 && w.tunnels[0].1 > 0.0;
            moved_in_tunnel |= inside && matches!(w.ego.phase(), Phase::MoveOut | Phase::MoveIn);
            moved_in_tunnel |= inside && w.ego.phase().is_signaling();
        });
        assert!(!moved_in_tunnel);
    }

    #[test]
    fn it_stays_in_the_passing_lane_through_a_tunnel_it_cannot_return_before() {
        let mut world = World::new();
        world.traffic.plant(0, OWN_LANE, 100.0, 0.8, false);
        world.run(20.0, |_| {});
        assert!(matches!(world.ego.phase(), Phase::Pass));
        // A tunnel too near for a lane change, which the car passes.
        world.tunnels.push((150.0, 600.0));
        let mut in_tunnel_lane = None;
        world.run(60.0, |w| {
            if w.tunnels[0].0 <= 0.0 && w.tunnels[0].1 > 0.0 {
                in_tunnel_lane = Some(w.ego.lateral());
                assert!(!w.ego.phase().is_signaling());
            }
        });
        assert_eq!(in_tunnel_lane, Some(PASSING_LANE));
    }

    #[test]
    fn the_camera_turns_with_the_move_and_not_otherwise() {
        let mut world = World::new();
        world.traffic.plant(0, OWN_LANE, 100.0, 0.8, false);
        let (mut largest, mut smallest) = (0.0f32, 0.0f32);
        let mut previous = world.ego.yaw();
        let mut greatest_step = 0.0f32;
        world.run(240.0, |w| {
            let yaw = w.ego.yaw();
            match w.ego.phase() {
                Phase::MoveOut | Phase::MoveIn => {}
                _ => assert_eq!(yaw, 0.0, "{:?}", w.ego.phase()),
            }
            assert!(yaw.abs() <= MAX_YAW + 1e-6);
            largest = largest.max(yaw);
            smallest = smallest.min(yaw);
            greatest_step = greatest_step.max((yaw - previous).abs());
            previous = yaw;
        });
        // Toward the passing lane when moving out, and back when moving in.
        assert!((largest - MAX_YAW).abs() < MAX_YAW * 0.02, "{largest}");
        assert!((smallest + MAX_YAW).abs() < MAX_YAW * 0.02, "{smallest}");
        // It builds up and dies away smoothly.
        assert!(greatest_step < MAX_YAW * 0.05, "{greatest_step}");
    }

    #[test]
    fn the_position_across_the_road_moves_smoothly_between_the_lanes() {
        let mut world = World::new();
        world.traffic.plant(0, OWN_LANE, 100.0, 0.8, false);
        let mut previous = (OWN_LANE, 0.0f32);
        let (mut greatest_speed, mut greatest_jerk) = (0.0f32, 0.0f32);
        world.run(240.0, |w| {
            let lateral = w.ego.lateral();
            assert!((OWN_LANE..=PASSING_LANE).contains(&lateral));
            let speed = (lateral - previous.0) / DT;
            greatest_speed = greatest_speed.max(speed.abs());
            greatest_jerk = greatest_jerk.max(((speed - previous.1) / DT).abs());
            previous = (lateral, speed);
        });
        // At most 1.875 times the average speed of the move.
        assert!(
            (greatest_speed - 1.875 * LANE_WIDTH / MOVE_SECONDS).abs() < 0.05,
            "{greatest_speed}"
        );
        // The jerk is at most 60 times the lane width over the cube of the time.
        assert!(greatest_jerk < 60.0 * LANE_WIDTH / MOVE_SECONDS.powi(3) * 1.2);
    }

    #[test]
    fn the_lanes_it_takes_up_include_the_one_it_is_about_to_take() {
        let mut world = World::new();
        world.traffic.plant(0, OWN_LANE, 100.0, 0.8, false);
        let mut seen = Vec::new();
        world.run(240.0, |w| {
            let entry = (w.ego.phase(), w.ego.lanes());
            if seen.last() != Some(&entry) {
                seen.push(entry);
            }
        });
        for (phase, lanes) in seen {
            let both = phase.is_signaling();
            assert_eq!(lanes[0] != lanes[1], both, "{phase:?}");
        }
    }

    #[test]
    fn it_keeps_clear_of_a_car_that_comes_up_fast_in_the_passing_lane() {
        let mut world = World::new();
        world.traffic.plant(0, OWN_LANE, 100.0, 0.8, false);
        // A fast car comes up from behind while the car signals.
        world.traffic.plant(5, PASSING_LANE, -60.0, 10.0, false);
        let mut least = f32::MAX;
        world.run(60.0, |w| least = least.min(w.traffic.least_gap()));
        assert!(least >= LANE_GAP - 0.01, "{least}");
    }
}
