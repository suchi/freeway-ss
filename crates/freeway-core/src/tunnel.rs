//! Tunnels: stretches of the road that run through a hill, lit by lamps.
//!
//! A tunnel is a stretch of the road between two distances travelled. They
//! are made one after another from the seed, only as far ahead as the camera
//! can see, and forgotten once passed, so hours of driving cost nothing. The
//! distance between tunnels and their length are in meters, not seconds: the
//! road is the same at any speed.

use std::collections::VecDeque;

use crate::canvas::Rgb;
use crate::projection::VIEW_DISTANCE;
use crate::rng::hash01;

/// Length of a tunnel, in meters: 20 to 60 seconds at 100 km/h.
const LENGTH: (f64, f64) = (550.0, 1650.0);
/// The road between two tunnels is at least this long, in meters.
const MIN_GAP: f64 = 1000.0;
/// Mean of the part of the gap beyond `MIN_GAP`, in meters. With the length,
/// about one tunnel in three minutes at 100 km/h.
const GAP_EXCESS_MEAN: f64 = 3000.0;
/// Tunnels are made this far ahead of the camera, in meters.
const LOOKAHEAD: f64 = VIEW_DISTANCE as f64 + 100.0;

/// Half the width of the hill the entrance is cut into, in meters.
const HILL_HALF_WIDTH: (f32, f32) = (100.0, 200.0);
/// Height of the hill above the road at its middle, in meters.
const HILL_HEIGHT: (f32, f32) = (25.0, 60.0);

const SODIUM_LAMP: Rgb = Rgb::new(255, 150, 40);
const LED_LAMP: Rgb = Rgb::new(225, 235, 255);

// Stream IDs tell the random values of a tunnel apart; they have nothing to
// do with cryptography.
const STREAM_TUNNEL: u32 = 0x60;
const PART_GAP: u32 = 0;
const PART_LENGTH: u32 = 1;
const PART_LAMP: u32 = 2;
const PART_HILL_WIDTH: u32 = 3;
const PART_HILL_HEIGHT: u32 = 4;

/// How the lamps of the tunnels are colored, or whether there are tunnels.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TunnelLighting {
    /// There are no tunnels.
    None,
    /// Each tunnel has sodium lamps or LED lamps.
    #[default]
    Mixed,
    /// Every tunnel has orange sodium lamps.
    Sodium,
    /// Every tunnel has white LED lamps.
    Led,
}

impl TunnelLighting {
    /// Every choice, in the order of the stored values 0, 1, 2, 3.
    pub const ALL: [TunnelLighting; 4] = [
        TunnelLighting::None,
        TunnelLighting::Mixed,
        TunnelLighting::Sodium,
        TunnelLighting::Led,
    ];

    /// The choice for a stored value; `None` when the value is not valid.
    pub fn from_raw(raw: Option<u32>) -> Option<Self> {
        Self::ALL.get(usize::try_from(raw?).ok()?).copied()
    }

    /// The value to store; `from_raw` reads it back.
    pub fn to_raw(self) -> u32 {
        match self {
            TunnelLighting::None => 0,
            TunnelLighting::Mixed => 1,
            TunnelLighting::Sodium => 2,
            TunnelLighting::Led => 3,
        }
    }
}

/// One tunnel.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tunnel {
    /// Distance travelled at which the entrance lies.
    pub start: f64,
    /// Distance travelled at which the exit lies.
    pub end: f64,
    /// The color of the lamps.
    pub lamp: Rgb,
    /// Half the width of the hill around the entrance, in meters.
    pub hill_half_width: f32,
    /// Height of the hill around the entrance at its middle, in meters.
    pub hill_height: f32,
}

impl Tunnel {
    /// Whether the point `s` meters along the road is inside the tunnel.
    pub fn contains(&self, s: f64) -> bool {
        self.start <= s && s < self.end
    }
}

/// The tunnels of a road, from just behind the camera to just beyond its view.
pub struct Tunnels {
    seed: u64,
    lighting: TunnelLighting,
    /// Tunnels in order, those the camera has not left yet.
    ahead: VecDeque<Tunnel>,
    /// Number of tunnels made so far.
    made: i64,
    /// The first tunnel not in `ahead` yet; `None` when there are no tunnels.
    next: Option<Tunnel>,
}

impl Tunnels {
    pub fn new(seed: u64, lighting: TunnelLighting) -> Self {
        let mut tunnels = Self {
            seed,
            lighting,
            ahead: VecDeque::new(),
            made: 0,
            next: None,
        };
        if lighting != TunnelLighting::None {
            tunnels.next = Some(tunnels.make(0, 0.0));
            tunnels.made = 1;
        }
        tunnels.update(0.0);
        tunnels
    }

    /// Makes the tunnels the camera at `track_s` can see and drops those it
    /// has passed.
    pub fn update(&mut self, track_s: f64) {
        while let Some(next) = self
            .next
            .filter(|tunnel| tunnel.start < track_s + LOOKAHEAD)
        {
            self.ahead.push_back(next);
            self.next = Some(self.make(self.made, next.end));
            self.made += 1;
        }
        while self
            .ahead
            .front()
            .is_some_and(|tunnel| tunnel.end <= track_s)
        {
            self.ahead.pop_front();
        }
    }

    /// The tunnels that have not been passed and begin within sight.
    pub fn ahead(&self) -> impl Iterator<Item = &Tunnel> {
        self.ahead.iter()
    }

    /// The tunnel number `index`, which comes after the road up to `after`.
    fn make(&self, index: i64, after: f64) -> Tunnel {
        let roll = |part: u32| hash01(self.seed, STREAM_TUNNEL | (part << 8), index);
        // An exponential distribution gives irregular gaps; `1 - roll` is above 0.
        let gap = MIN_GAP - GAP_EXCESS_MEAN * f64::from(1.0 - roll(PART_GAP)).ln();
        let start = after + gap;
        let length = LENGTH.0 + f64::from(roll(PART_LENGTH)) * (LENGTH.1 - LENGTH.0);
        let lamp = match self.lighting {
            TunnelLighting::Sodium => SODIUM_LAMP,
            TunnelLighting::Led => LED_LAMP,
            TunnelLighting::None | TunnelLighting::Mixed if roll(PART_LAMP) < 0.5 => SODIUM_LAMP,
            TunnelLighting::None | TunnelLighting::Mixed => LED_LAMP,
        };
        Tunnel {
            start,
            end: start + length,
            lamp,
            hill_half_width: lerp(HILL_HALF_WIDTH, roll(PART_HILL_WIDTH)),
            hill_height: lerp(HILL_HEIGHT, roll(PART_HILL_HEIGHT)),
        }
    }
}

fn lerp(range: (f32, f32), t: f32) -> f32 {
    range.0 + t * (range.1 - range.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every tunnel of the first `meters` of the road, in order.
    fn collect(seed: u64, lighting: TunnelLighting, meters: f64) -> Vec<Tunnel> {
        let mut tunnels = Tunnels::new(seed, lighting);
        let mut all: Vec<Tunnel> = Vec::new();
        let mut s = 0.0;
        while s < meters {
            tunnels.update(s);
            for tunnel in tunnels.ahead() {
                if all.last().is_none_or(|last| tunnel.start > last.start) {
                    all.push(*tunnel);
                }
            }
            s += 25.0;
        }
        all
    }

    #[test]
    fn the_same_seed_gives_the_same_tunnels() {
        let a = collect(5, TunnelLighting::Mixed, 100_000.0);
        assert_eq!(a, collect(5, TunnelLighting::Mixed, 100_000.0));
        assert_ne!(a, collect(6, TunnelLighting::Mixed, 100_000.0));
    }

    #[test]
    fn the_tunnels_do_not_depend_on_how_the_road_is_driven() {
        // Driving in small or in large steps meets the same tunnels.
        let mut fine = Tunnels::new(3, TunnelLighting::Mixed);
        let mut coarse = Tunnels::new(3, TunnelLighting::Mixed);
        let (mut a, mut b) = (Vec::new(), Vec::new());
        for step in 0..4000 {
            fine.update(f64::from(step) * 20.0);
            a.extend(fine.ahead().map(|t| t.start));
            if step % 5 == 0 {
                coarse.update(f64::from(step) * 20.0);
                b.extend(coarse.ahead().map(|t| t.start));
            }
        }
        a.sort_by(f64::total_cmp);
        a.dedup();
        b.sort_by(f64::total_cmp);
        b.dedup();
        assert_eq!(a, b);
    }

    #[test]
    fn tunnels_have_the_designed_length_and_gap() {
        let tunnels = collect(1, TunnelLighting::Mixed, 2_000_000.0);
        assert!(tunnels.len() > 300);
        for tunnel in &tunnels {
            let length = tunnel.end - tunnel.start;
            assert!((LENGTH.0..=LENGTH.1).contains(&length), "{length}");
            assert!(tunnel.hill_half_width >= HILL_HALF_WIDTH.0);
            assert!(tunnel.hill_half_width <= HILL_HALF_WIDTH.1);
            assert!(tunnel.hill_height >= HILL_HEIGHT.0 && tunnel.hill_height <= HILL_HEIGHT.1);
        }
        for pair in tunnels.windows(2) {
            let gap = pair[1].start - pair[0].end;
            assert!(gap >= MIN_GAP, "{gap}");
        }
        // The first tunnel is not at the very start.
        assert!(tunnels[0].start >= MIN_GAP);
    }

    #[test]
    fn the_mean_gap_is_about_four_kilometers() {
        let tunnels = collect(2, TunnelLighting::Mixed, 20_000_000.0);
        let gaps: Vec<f64> = tunnels.windows(2).map(|p| p[1].start - p[0].end).collect();
        let mean = gaps.iter().sum::<f64>() / gaps.len() as f64;
        assert!((mean - 4000.0).abs() < 250.0, "{mean}");
        // One tunnel in about three minutes at 100 km/h.
        let period =
            (tunnels.last().unwrap().start - tunnels[0].start) / (tunnels.len() - 1) as f64;
        let minutes = period / (100.0 / 3.6) / 60.0;
        assert!((2.6..3.4).contains(&minutes), "{minutes}");
    }

    #[test]
    fn hours_of_driving_keep_tunnels_in_order() {
        // 160 km/h for 24 hours.
        let meters = 160.0 / 3.6 * 86_400.0;
        let mut tunnels = Tunnels::new(9, TunnelLighting::Mixed);
        let mut s = 0.0;
        let mut previous_end = f64::MIN;
        let mut count = 0;
        while s < meters {
            tunnels.update(s);
            let list: Vec<&Tunnel> = tunnels.ahead().collect();
            // Few are kept at a time, in order and apart.
            assert!(list.len() <= 2, "{}", list.len());
            for pair in list.windows(2) {
                assert!(pair[0].end + MIN_GAP <= pair[1].start);
            }
            if let Some(first) = list.first() {
                if first.start > previous_end {
                    count += 1;
                    previous_end = first.end;
                }
                assert!(first.end > s);
            }
            s += 500.0;
        }
        // About one every 5,100 meters.
        assert!((650..850).contains(&count), "{count}");
    }

    #[test]
    fn mixed_lighting_has_both_colors_and_the_others_have_one() {
        let colors = |lighting| {
            let mut found: Vec<Rgb> = Vec::new();
            for tunnel in collect(4, lighting, 1_000_000.0) {
                if !found.contains(&tunnel.lamp) {
                    found.push(tunnel.lamp);
                }
            }
            found
        };
        let mixed = colors(TunnelLighting::Mixed);
        assert!(mixed.contains(&SODIUM_LAMP) && mixed.contains(&LED_LAMP));
        assert_eq!(colors(TunnelLighting::Sodium), [SODIUM_LAMP]);
        assert_eq!(colors(TunnelLighting::Led), [LED_LAMP]);
    }

    #[test]
    fn mixed_lighting_is_about_half_and_half() {
        let tunnels = collect(7, TunnelLighting::Mixed, 20_000_000.0);
        let sodium = tunnels.iter().filter(|t| t.lamp == SODIUM_LAMP).count();
        let share = sodium as f64 / tunnels.len() as f64;
        assert!((0.4..0.6).contains(&share), "{share}");
    }

    #[test]
    fn no_tunnels_are_made_when_they_are_off() {
        for seed in 0..20 {
            assert!(collect(seed, TunnelLighting::None, 500_000.0).is_empty());
        }
    }

    #[test]
    fn a_tunnel_contains_its_stretch_only() {
        let tunnel = collect(1, TunnelLighting::Led, 10_000.0)[0];
        assert!(tunnel.contains(tunnel.start));
        assert!(tunnel.contains(tunnel.end - 0.5));
        assert!(!tunnel.contains(tunnel.end));
        assert!(!tunnel.contains(tunnel.start - 0.5));
    }
}
