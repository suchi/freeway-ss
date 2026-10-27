//! Tunnels: stretches of the road that run through a hill, lit by lamps.
//!
//! A tunnel is a stretch of the road between two distances travelled. They
//! are made one after another from the seed, only as far ahead as the camera
//! can see, and forgotten once passed, so hours of driving cost nothing. The
//! distance between tunnels and their length are in meters, not seconds: the
//! road is the same at any speed.

use std::collections::VecDeque;

use crate::canvas::{Canvas, Rgb};
use crate::projection::{CAMERA_HEIGHT, NEAR_DISTANCE, Projector, VIEW_DISTANCE};
use crate::rng::hash01;
use crate::road::RoadView;
use crate::scenery::{BLEND_HALF, ZoneKind, Zones};
use crate::sky::{BAND_BOTTOM, REF_FOCAL};

/// Length of a tunnel, in meters: 20 to 60 seconds at 100 km/h.
const LENGTH: (f64, f64) = (550.0, 1650.0);
/// The road between two tunnels is at least this long, in meters.
const MIN_GAP: f64 = 1000.0;
/// Mean of the part of the gap beyond `MIN_GAP`, in meters. With the length,
/// about one tunnel in three minutes at 100 km/h.
const GAP_EXCESS_MEAN: f64 = 3000.0;
/// In the mountains of the scenery zones the road between two tunnels is at
/// least `ZONED_MIN_GAP` long, and `ZONED_GAP_EXCESS_MEAN` longer on average.
/// The plains in between have no tunnels, so over a whole journey there are
/// about as many tunnels as without zones.
const ZONED_MIN_GAP: f64 = 1000.0;
const ZONED_GAP_EXCESS_MEAN: f64 = 500.0;
/// Tunnels are made this far ahead of the camera, in meters.
const LOOKAHEAD: f64 = VIEW_DISTANCE as f64 + 100.0;

/// Half the width of the hill the entrance is cut into, in meters.
const HILL_HALF_WIDTH: (f32, f32) = (100.0, 200.0);
/// Height of the hill above the road at its middle, in meters.
const HILL_HEIGHT: (f32, f32) = (25.0, 60.0);

/// What fits through a tunnel, in meters: the lights of the road outside
/// (the median's street lamps, the oncoming lanes) do not shine into it.
/// The walls stand just beyond the delineators at -4.0 and 3.9 meters, where
/// the lamps of the tunnel hang.
const CLEARANCE_LATERAL: (f32, f32) = (-4.5, 4.5);
const CLEARANCE_HEIGHT: f32 = 7.0;

/// The entrance appears out of the night between these distances, in meters.
const FADE_FAR: f32 = VIEW_DISTANCE;
const FADE_NEAR: f32 = 450.0;
/// The night sky behind everything (the renderer's background, #02030a).
const BACKGROUND: Rgb = Rgb::new(2, 3, 10);
/// The hill, as dark as the mountains on the horizon (#05070f).
const HILL_COLOR: Rgb = Rgb::new(5, 7, 15);
/// Inside the opening, darker still and tinted by the lamps.
const SODIUM_DARK: Rgb = Rgb::new(10, 7, 5);
const LED_DARK: Rgb = Rgb::new(6, 8, 12);

/// The opening of a tunnel as seen from the front: lateral position and
/// height of its corners, in meters. The top is rounded off.
pub(crate) const OPENING: [(f32, f32); 7] = [
    (-5.0, 0.0),
    (-5.0, 5.6),
    (-3.6, 6.6),
    (0.25, 7.0),
    (4.1, 6.6),
    (5.5, 5.6),
    (5.5, 0.0),
];
/// Number of steps from the foot of the hill to its top.
const HILL_STEPS: usize = 16;

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
    /// The color of the dark inside of the opening seen from afar.
    pub fn opening_color(&self) -> Rgb {
        if self.lamp == SODIUM_LAMP {
            SODIUM_DARK
        } else {
            LED_DARK
        }
    }

    /// Whether the point `s` meters along the road is inside the tunnel.
    pub fn contains(&self, s: f64) -> bool {
        self.start <= s && s < self.end
    }
}

/// Whether a light of the road outside, at the given lateral position and
/// height, can be seen through a tunnel.
pub fn fits_through(lateral: f32, height: f32) -> bool {
    (CLEARANCE_LATERAL.0..=CLEARANCE_LATERAL.1).contains(&lateral)
        && (0.0..=CLEARANCE_HEIGHT).contains(&height)
}

/// Draws the hill around the entrance of `tunnel`, which is `d` meters ahead,
/// and the opening in it. It stands in front of the sky and behind the lights.
pub fn draw_entrance(
    tunnel: &Tunnel,
    d: f32,
    projector: &Projector,
    view: &RoadView,
    canvas: &mut dyn Canvas,
) {
    let (bend, rise) = view.offset(d);
    let project = |lateral: f32, height: f32| {
        projector
            .project(d, lateral + bend, height + rise)
            .map(|p| (p.x, p.y))
    };
    // Fades in from the dark of the night so that it does not pop up far away.
    let fade = ((FADE_FAR - d) / (FADE_FAR - FADE_NEAR)).clamp(0.0, 1.0);
    // The foot of the hill reaches down to the bottom of the glow on the
    // horizon (as far below the horizon as the mountains), so that no gap of
    // sky shows under it where the road climbs to the entrance. Its height
    // above the road is negative then, and the hill widens with it.
    let foot = (CAMERA_HEIGHT - rise - BAND_BOTTOM / REF_FOCAL * d).min(0.0);
    let height_above_foot = tunnel.hill_height - foot;
    let half_width = tunnel.hill_half_width * height_above_foot / tunnel.hill_height;
    let hill: Option<Vec<(f32, f32)>> = (0..=2 * HILL_STEPS)
        .map(|step| {
            let across = step as f32 / HILL_STEPS as f32 - 1.0;
            // A bell that levels off at the foot of the hill on both sides.
            let bell = 0.5 * (1.0 + (across * std::f32::consts::PI).cos());
            project(across * half_width, foot + height_above_foot * bell)
        })
        .collect();
    let opening: Option<Vec<(f32, f32)>> = OPENING
        .iter()
        .map(|&(lateral, height)| project(lateral, height))
        .collect();
    let (Some(hill), Some(opening)) = (hill, opening) else {
        return;
    };
    canvas.silhouette(&hill, mix(BACKGROUND, HILL_COLOR, fade), 1.0);
    canvas.silhouette(&opening, mix(BACKGROUND, tunnel.opening_color(), fade), 1.0);
}

fn mix(from: Rgb, to: Rgb, t: f32) -> Rgb {
    let channel = |a: u8, b: u8| (f32::from(a) + (f32::from(b) - f32::from(a)) * t).round() as u8;
    Rgb::new(
        channel(from.r, to.r),
        channel(from.g, to.g),
        channel(from.b, to.b),
    )
}

/// Distance between the points of a line of sight that are checked against
/// the walls of a tunnel, in meters.
const SIGHT_STEP: f32 = 2.0;
/// The walls are checked this much wider than the opening, in meters: the
/// lamps and the emergency lights hang on the walls, at the edge of it.
pub(crate) const SIGHT_MARGIN: f32 = 0.3;

/// One tunnel as seen from the camera, in meters ahead of it.
struct Span {
    start: f32,
    end: f32,
    /// The bend and the rise of the road every `SIGHT_STEP` meters from
    /// `start` on, as (distance, bend, rise).
    samples: Vec<(f32, f32, f32)>,
}

/// The edges of `OPENING` as (lateral, height, offset) of the line
/// `lateral * x + height * y + offset = 0`, normalized so that points inside
/// of the opening have a value of zero or more.
fn opening_edges() -> [(f32, f32, f32); OPENING.len()] {
    std::array::from_fn(|i| {
        let (x1, y1) = OPENING[i];
        let (x2, y2) = OPENING[(i + 1) % OPENING.len()];
        // The corners run clockwise (up the left wall, over the top, down
        // the right wall), so the inside lies to the right of each edge.
        let (nx, ny) = (y2 - y1, x1 - x2);
        let length = nx.hypot(ny);
        (nx / length, ny / length, -(nx * x1 + ny * y1) / length)
    })
}

/// What the camera can see through the tunnels ahead: a light is hidden
/// when the line from the eye to it leaves the opening of a tunnel on its
/// way (the road bends and rises, so a wall or the hill stands in the way).
pub struct Sight<'a> {
    view: &'a RoadView,
    /// Lateral position of the eye.
    eye: f32,
    spans: Vec<Span>,
    edges: [(f32, f32, f32); OPENING.len()],
}

impl Sight<'_> {
    /// Whether a light of the road outside, `d` meters ahead, at the given
    /// lateral position and height above the road, is hidden by a tunnel.
    pub fn hides(&self, d: f32, lateral: f32, height: f32) -> bool {
        // The lights of the road outside do not stand in a tunnel.
        let inside = self.spans.iter().any(|span| span.start < d && d < span.end);
        (inside && !fits_through(lateral, height)) || self.blocked(d, lateral, height)
    }

    /// Whether a wall or the hill is in the way of the line of sight to the
    /// light, which may stand in a tunnel (a lamp, for example).
    pub fn blocked(&self, d: f32, lateral: f32, height: f32) -> bool {
        let (bend, rise) = self.view.offset(d);
        let target = (lateral + bend, height + rise);
        let eye = (self.eye, CAMERA_HEIGHT);
        self.spans.iter().any(|span| {
            if span.start >= d {
                return false;
            }
            // Where the line of sight, straight in the space of the
            // unbent road, stands outside the tunnel at the distance `at`.
            let outside = |at: f32, bend: f32, rise: f32| {
                let t = at / d;
                let x = eye.0 + (target.0 - eye.0) * t - bend;
                let y = eye.1 + (target.1 - eye.1) * t - rise;
                self.edges
                    .iter()
                    .any(|&(nx, ny, c)| nx * x + ny * y + c < -SIGHT_MARGIN)
            };
            let last = d.min(span.end);
            span.samples
                .iter()
                .take_while(|s| s.0 < last)
                .any(|&(at, bend, rise)| outside(at, bend, rise))
                || {
                    let (bend, rise) = self.view.offset(last);
                    outside(last, bend, rise)
                }
        })
    }
}

/// The tunnels of a road, from just behind the camera to just beyond its view.
pub struct Tunnels {
    seed: u64,
    lighting: TunnelLighting,
    /// Where tunnels may be; the same zones as the scene sees.
    zones: Zones,
    /// Tunnels in order, those the camera has not left yet.
    ahead: VecDeque<Tunnel>,
    /// Number of tunnels tried so far; also the number of the next one.
    made: i64,
    /// The first tunnel not in `ahead` yet; `None` when there are no tunnels.
    next: Option<Tunnel>,
}

impl Tunnels {
    pub fn new(seed: u64, lighting: TunnelLighting) -> Self {
        Self::with_zones(seed, lighting, false)
    }

    /// Tunnels that, with `scenery_zones`, are made only in the mountains.
    pub fn with_zones(seed: u64, lighting: TunnelLighting, scenery_zones: bool) -> Self {
        let mut tunnels = Self {
            seed,
            lighting,
            zones: Zones::new(seed, scenery_zones),
            ahead: VecDeque::new(),
            made: 0,
            next: None,
        };
        if lighting != TunnelLighting::None {
            tunnels.next = Some(tunnels.make_next(0.0));
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
            self.next = Some(self.make_next(next.end));
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

    /// Whether the camera at `track_s` is in a tunnel, or so close to the
    /// entrance that the entrance is no longer drawn.
    pub fn sky_hidden(&self, track_s: f64) -> bool {
        self.ahead
            .iter()
            .any(|t| t.start < track_s + f64::from(NEAR_DISTANCE) && track_s < t.end)
    }

    /// Whether the point `s` meters along the road is inside a tunnel.
    pub fn contains(&self, s: f64) -> bool {
        self.ahead.iter().any(|t| t.contains(s))
    }

    /// What the camera at `track_s` can see through the tunnels ahead: the
    /// road ahead is `view`, and the eye stands `eye` to the side of its
    /// reference line.
    pub fn sight<'a>(&self, track_s: f64, view: &'a RoadView, eye: f32) -> Sight<'a> {
        let spans = self
            .ahead
            .iter()
            .filter(|t| t.end > track_s)
            .filter_map(|t| {
                let start = ((t.start - track_s) as f32).max(0.0);
                let end = (t.end - track_s) as f32;
                (start < VIEW_DISTANCE).then(|| {
                    let mut samples = Vec::new();
                    let mut d = start;
                    while d < end.min(VIEW_DISTANCE) {
                        let (bend, rise) = view.offset(d);
                        samples.push((d, bend, rise));
                        d += SIGHT_STEP;
                    }
                    Span {
                        start,
                        end,
                        samples,
                    }
                })
            })
            .collect();
        Sight {
            view,
            eye,
            spans,
            edges: opening_edges(),
        }
    }

    /// The tunnel whose entrance is in front of the camera at `track_s` and
    /// within sight, with the distance to it.
    pub fn entrance_ahead(&self, track_s: f64) -> Option<(&Tunnel, f32)> {
        self.ahead.iter().find_map(|t| {
            let d = (t.start - track_s) as f32;
            (NEAR_DISTANCE..VIEW_DISTANCE)
                .contains(&d)
                .then_some((t, d))
        })
    }

    /// The tunnel after the road up to `after`. With zones it lies in the
    /// mountains, clear of the stretch where the mountains rise or sink, so
    /// it never crosses the boundary of a zone.
    fn make_next(&mut self, after: f64) -> Tunnel {
        if !self.zones.enabled() {
            let index = self.made;
            self.made += 1;
            return self.make(index, after, (MIN_GAP, GAP_EXCESS_MEAN));
        }
        let mut cursor = after;
        loop {
            let zone = self.zones.zone_at(cursor);
            if zone.kind == ZoneKind::Plains {
                cursor = zone.end;
                continue;
            }
            let index = self.made;
            self.made += 1;
            let tunnel = self.make(
                index,
                cursor.max(zone.start + BLEND_HALF),
                (ZONED_MIN_GAP, ZONED_GAP_EXCESS_MEAN),
            );
            if tunnel.end <= zone.end - BLEND_HALF {
                return tunnel;
            }
            // It does not fit in this zone any more; try the next one.
            cursor = zone.end;
        }
    }

    /// The tunnel number `index`, which comes after the road up to `after`
    /// with a gap of `gap.0` meters and `gap.1` more on average.
    fn make(&self, index: i64, after: f64, gap: (f64, f64)) -> Tunnel {
        let roll = |part: u32| hash01(self.seed, STREAM_TUNNEL | (part << 8), index);
        // An exponential distribution gives irregular gaps; `1 - roll` is above 0.
        let gap = gap.0 - gap.1 * f64::from(1.0 - roll(PART_GAP)).ln();
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
        collect_from(Tunnels::new(seed, lighting), meters)
    }

    /// Every tunnel of the first `meters` of a road with scenery zones.
    fn collect_zoned(seed: u64, meters: f64) -> Vec<Tunnel> {
        collect_from(
            Tunnels::with_zones(seed, TunnelLighting::Mixed, true),
            meters,
        )
    }

    fn collect_from(mut tunnels: Tunnels, meters: f64) -> Vec<Tunnel> {
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
    fn zoned_tunnels_stay_in_the_mountains_and_clear_of_the_boundaries() {
        for seed in 0..8 {
            let tunnels = collect_zoned(seed, 2_000_000.0);
            assert!(tunnels.len() > 100, "{}", tunnels.len());
            let mut zones = Zones::new(seed, true);
            for tunnel in &tunnels {
                let zone = zones.zone_at(tunnel.start);
                assert_eq!(zone.kind, ZoneKind::Mountains, "{tunnel:?}");
                assert!(tunnel.start >= zone.start + BLEND_HALF, "{tunnel:?}");
                assert!(tunnel.end <= zone.end - BLEND_HALF, "{tunnel:?}");
                let length = tunnel.end - tunnel.start;
                assert!((LENGTH.0..=LENGTH.1).contains(&length), "{length}");
            }
            for pair in tunnels.windows(2) {
                assert!(pair[1].start - pair[0].end >= ZONED_MIN_GAP);
            }
        }
    }

    #[test]
    fn there_are_no_tunnels_on_the_plains() {
        let mut zones = Zones::new(3, true);
        let tunnels = collect_zoned(3, 3_000_000.0);
        let mut s = 0.0;
        while s < 3_000_000.0 {
            let zone = zones.zone_at(s);
            if zone.kind == ZoneKind::Plains {
                assert!(
                    !tunnels
                        .iter()
                        .any(|t| t.start < zone.end && t.end > zone.start),
                    "{zone:?}"
                );
            }
            s = zone.end;
        }
    }

    #[test]
    fn the_mountains_have_more_tunnels_than_the_old_road() {
        // Inside a mountain zone the tunnels are close together.
        let tunnels = collect_zoned(2, 3_000_000.0);
        let close = tunnels
            .windows(2)
            .filter(|p| p[1].start - p[0].end < 3_000.0)
            .count();
        assert!(close * 2 > tunnels.len(), "{close} of {}", tunnels.len());
    }

    #[test]
    fn zones_keep_about_the_same_number_of_tunnels_over_a_whole_journey() {
        let meters = 20_000_000.0;
        let plain = collect(6, TunnelLighting::Mixed, meters).len() as f64;
        let zoned = collect_zoned(6, meters).len() as f64;
        // The mountains take half of the road, so tunnels come about as often
        // as without zones; the edges of the zones cost a little.
        let ratio = zoned / plain;
        assert!((0.8..1.1).contains(&ratio), "{ratio}");
    }

    #[test]
    fn zoned_tunnels_are_the_same_whichever_way_the_road_is_driven() {
        let mut fine = Tunnels::with_zones(5, TunnelLighting::Mixed, true);
        let mut coarse = Tunnels::with_zones(5, TunnelLighting::Mixed, true);
        let (mut a, mut b) = (Vec::new(), Vec::new());
        for step in 0..6000 {
            fine.update(f64::from(step) * 20.0);
            a.extend(fine.ahead().map(|t| t.start));
            if step % 7 == 0 {
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
    fn zoned_tunnels_are_not_made_when_they_are_off() {
        let mut tunnels = Tunnels::with_zones(1, TunnelLighting::None, true);
        for step in 0..2000 {
            tunnels.update(f64::from(step) * 100.0);
            assert_eq!(tunnels.ahead().count(), 0);
        }
    }

    /// Tunnels with the first one from 100 to 300 meters ahead of the camera
    /// at 0, on a road without bends and hills.
    fn straight_sight(view: &RoadView) -> Sight<'_> {
        let tunnel = Tunnel {
            start: 100.0,
            end: 300.0,
            lamp: LED_LAMP,
            hill_half_width: 150.0,
            hill_height: 40.0,
        };
        let tunnels = Tunnels {
            seed: 0,
            lighting: TunnelLighting::Led,
            zones: Zones::new(0, false),
            ahead: VecDeque::from([tunnel]),
            made: 1,
            next: None,
        };
        tunnels.sight(0.0, view, crate::projection::CAMERA_LATERAL)
    }

    #[test]
    fn on_a_straight_flat_road_what_fits_through_the_tunnel_is_seen_beyond_it() {
        let view = RoadView::flat();
        let sight = straight_sight(&view);
        // The delineators, the lanes and the lamps stand where the tunnel is.
        for (lateral, height) in [(-4.0, 0.9), (3.9, 0.9), (0.0, 0.0), (1.75, 0.9)] {
            for d in [150.0, 299.0, 301.0, 400.0, 590.0] {
                assert!(!sight.hides(d, lateral, height), "{d} {lateral} {height}");
            }
        }
        // Whatever stands outside, and beyond it, is hidden, but not before the entrance.
        for (lateral, height) in [
            (13.0, 0.9),
            (9.0, 0.0),
            (4.5, 10.0),
            (-9.0, 0.0),
            (0.0, 12.0),
        ] {
            assert!(sight.hides(400.0, lateral, height), "{lateral} {height}");
            assert!(!sight.hides(99.0, lateral, height), "{lateral} {height}");
        }
        // The walls leave the line of sight to the lamps hanging on them free.
        for (lateral, d) in [(-4.6, 150.0), (5.1, 150.0), (-4.6, 299.0), (5.1, 120.0)] {
            assert!(!sight.blocked(d, lateral, 5.0), "{d} {lateral}");
        }
    }

    #[test]
    fn a_bend_hides_the_lights_beyond_the_wall() {
        // A road that bends to the side: a straight line of sight reaches the
        // wall before the exit, where it can see nothing of the far lamps.
        let view = RoadView::new(&crate::road::RoadShape::new(21), 0.0);
        let (bend, _) = view.offset(300.0);
        assert!(bend.abs() > 10.0, "{bend}");
        let sight = straight_sight(&view);
        assert!(sight.hides(400.0, 0.0, 0.0));
        assert!(!sight.hides(101.0, 0.0, 0.0));
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
