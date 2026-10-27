//! Other cars on the road: tail lights ahead, passing cars and oncoming headlights.
//!
//! Cars are kept relative to the camera (`rel`, meters ahead) so that hours of
//! driving never lose precision. Everything about a car (where it appears, how
//! fast it is, when it brakes) is hashed from the seed, the car's number and
//! how often it has been placed again, so the same seed gives the same traffic.

use crate::canvas::Rgb;
use crate::projection::{NEAR_DISTANCE, VIEW_DISTANCE};
use crate::rng::hash01;
use crate::solid::{Solid, SolidSpec, Surface};

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
const STREAM_SHAPE: u32 = 19;
const STREAM_MODEL: u32 = 20;

/// The share of cars that are trucks, when trucks are enabled.
pub const TRUCK_RATIO: f32 = 0.2;
const TRUCK_HALF_WIDTH: f32 = 1.25;
/// Cars farther than this are drawn as blocks even where the shapes are
/// detailed: they are a few pixels on the screen.
const DETAIL_RANGE: f32 = 150.0;
/// Boxes of a body no higher than this cast no shadow of their own.
pub(crate) const LOW_BOX: f32 = 1.25;
/// The wheels of a car are boxes of this width at both sides, this much in
/// from the side of the body.
const CAR_WHEEL_INSET: f32 = 0.05;
const CAR_WHEEL_WIDTH: f32 = 0.22;
/// The cabin of a car is this much narrower than its body on each side.
const CABIN_INSET: f32 = 0.1;
/// The lamps of a car stand this far in from the side of the body.
const LAMP_INSET: f32 = 0.15;
/// The wheels of a truck are boxes of this length and height along the
/// sides, reaching this far in from the edge of the body.
const WHEEL_LENGTH: f32 = 1.0;
const WHEEL_HEIGHT: f32 = 1.0;
const WHEEL_WIDTH: f32 = 0.6;
/// Wheels that are less than this far apart are drawn as one.
const TANDEM_GAP: f32 = 0.5;
/// Cars of one lane keep at least this far apart where bodies are drawn.
const LANE_GAP: f32 = 10.0;
/// A car that closes in on the one ahead of it in its lane eases off at this
/// rate, in meters per second squared, so that it has the speed of the other
/// when the gap is `LANE_GAP`.
const LANE_DECEL: f32 = 3.0;
/// Where cars keep clear of each other, no car changes its speed faster than
/// this, in meters per second squared. A car that is let go (the one ahead of it
/// has left) speeds up to its own speed as slowly as it slowed down.
const LANE_SLEW: f32 = 2.0 * LANE_DECEL;
/// Speeds are matched to settle this far beyond `LANE_GAP`, so that rounding
/// in the steps never leaves a car closer than `LANE_GAP`.
const LANE_CUSHION: f32 = 1.0;
/// A car that is placed keeps this far from the others of its lane: enough to
/// ease off from the largest difference of speeds, 8 meters per second, to
/// the speed of the car ahead before the gap is `LANE_GAP`.
const PLACE_GAP: f32 = LANE_GAP + 8.0 * 8.0 / (2.0 * LANE_DECEL);
/// Free places for a car that is placed are searched at this interval.
const SLOT_STEP: f32 = 5.0;
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

/// The shapes of trucks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TruckShape {
    /// A cab-over truck with a box on its frame.
    JapaneseBox,
    /// A cab-over tractor with a container trailer.
    JapaneseTrailer,
    /// A tractor with a hood and a sleeper cab, with a trailer.
    AmericanSemi,
    /// A short hood, a cab and a box.
    AmericanBox,
}

/// A box of a truck: along the road from its front end, across from its
/// middle line and above the road, in meters.
struct Block {
    along: (f32, f32),
    half_width: f32,
    up: (f32, f32),
}

const fn block(along: (f32, f32), half_width: f32, up: (f32, f32)) -> Block {
    Block {
        along,
        half_width,
        up,
    }
}

/// What a truck is made of and where its lamps stand.
struct ShapeSpec {
    length: f32,
    height: f32,
    blocks: &'static [Block],
    /// Where the axles are along the truck from its front end; each has a
    /// wheel at both sides.
    axles: &'static [f32],
    /// Where the front face of the cab stands along the truck and how high
    /// its roof is.
    cab: (f32, f32),
    /// Where the box or the trailer stands along the truck, and its lower
    /// and upper edge.
    cargo: (f32, f32),
    cargo_up: (f32, f32),
    /// Half the distance between the headlights.
    headlight: f32,
    /// The parts of the cab and the hood with outlines, and how many of the
    /// first of `blocks` they stand in for when the shapes are detailed.
    solids: &'static [SolidSpec],
    solid_blocks: usize,
    /// Where the lamps on the roof of the cab stand when the shapes are
    /// detailed: along the truck from its front end, and their height.
    roof_lamp: (f32, f32),
}

/// The cab of the Japanese trucks: the front leans back a little, the corners
/// of the front and the edge of the roof are cut off, and a window runs along
/// the side of the roof.
const JAPANESE_CAB: SolidSpec = SolidSpec {
    outline: &[
        (0.0, 0.6, B),
        (0.06, 1.9, G),
        (0.15, 2.9, B),
        (0.4, 3.2, C),
        (2.3, 3.2, B),
    ],
    half_width: TRUCK_HALF_WIDTH,
    bottom: 0.6,
    shoulder: 2.2,
    inset: 0.0,
    chamfer: 0.2,
};

/// The deflector on the roof of a Japanese cab rises toward the rear to the
/// height of the box.
const fn japanese_deflector(top: f32) -> SolidSpec {
    SolidSpec {
        outline: if top > 4.0 {
            &[(1.1, 3.2, B), (2.3, 4.1, B)]
        } else {
            &[(1.1, 3.2, B), (2.3, 3.8, B)]
        },
        half_width: 1.1,
        bottom: 3.2,
        shoulder: 9.0,
        inset: 0.0,
        chamfer: 0.0,
    }
}

const AMERICAN_HOOD: SolidSpec = SolidSpec {
    outline: &[(0.0, 1.2, B), (0.25, 1.6, B), (1.8, 1.9, B)],
    half_width: 1.0,
    bottom: 0.6,
    shoulder: 9.0,
    inset: 0.0,
    chamfer: 0.0,
};

const AMERICAN_SLEEPER: SolidSpec = SolidSpec {
    outline: &[(1.8, 1.9, G), (2.6, 3.3, C), (3.4, 4.0, B), (5.5, 4.0, B)],
    half_width: TRUCK_HALF_WIDTH,
    bottom: 0.6,
    shoulder: 2.3,
    inset: 0.0,
    chamfer: 0.0,
};

/// The deflector over the sleeper rises to the height of the trailer.
const AMERICAN_DEFLECTOR: SolidSpec = SolidSpec {
    outline: &[(3.6, 4.0, B), (5.5, 4.1, B)],
    half_width: 1.1,
    bottom: 4.0,
    shoulder: 9.0,
    inset: 0.0,
    chamfer: 0.0,
};

const AMERICAN_BOX_HOOD: SolidSpec = SolidSpec {
    outline: &[(0.0, 0.9, B), (0.15, 1.5, B), (1.2, 1.6, B)],
    half_width: 1.0,
    bottom: 0.6,
    shoulder: 9.0,
    inset: 0.0,
    chamfer: 0.0,
};

const AMERICAN_BOX_CAB: SolidSpec = SolidSpec {
    outline: &[(1.2, 1.6, G), (1.75, 2.5, C), (2.7, 2.6, B)],
    half_width: TRUCK_HALF_WIDTH,
    bottom: 0.6,
    shoulder: 2.0,
    inset: 0.0,
    chamfer: 0.0,
};

const JAPANESE_BOX: ShapeSpec = ShapeSpec {
    length: 12.0,
    height: 3.8,
    blocks: &[
        block((0.0, 2.3), TRUCK_HALF_WIDTH, (0.6, 3.2)),
        block((2.3, 11.8), 0.6, (0.6, 1.2)),
        block((2.6, 12.0), TRUCK_HALF_WIDTH, (1.2, 3.8)),
        block((11.8, 12.0), TRUCK_HALF_WIDTH, (0.6, 1.2)),
    ],
    axles: &[1.2, 9.4, 10.6],
    cab: (0.0, 3.2),
    cargo: (2.6, 12.0),
    cargo_up: (1.2, 3.8),
    headlight: 0.9,
    solids: &[JAPANESE_CAB, japanese_deflector(3.8)],
    solid_blocks: 1,
    roof_lamp: (0.3, 3.1),
};

const JAPANESE_TRAILER: ShapeSpec = ShapeSpec {
    length: 16.5,
    height: 4.1,
    blocks: &[
        block((0.0, 2.3), TRUCK_HALF_WIDTH, (0.6, 3.2)),
        block((2.3, 5.8), 0.6, (0.6, 1.2)),
        block((3.3, 16.5), TRUCK_HALF_WIDTH, (1.2, 4.1)),
        block((16.3, 16.5), TRUCK_HALF_WIDTH, (0.6, 1.2)),
    ],
    axles: &[1.2, 4.0, 5.2, 13.0, 14.2, 15.4],
    cab: (0.0, 3.2),
    cargo: (3.3, 16.5),
    cargo_up: (1.2, 4.1),
    headlight: 0.9,
    solids: &[JAPANESE_CAB, japanese_deflector(4.1)],
    solid_blocks: 1,
    roof_lamp: (0.3, 3.1),
};

const AMERICAN_SEMI: ShapeSpec = ShapeSpec {
    length: 22.0,
    height: 4.1,
    blocks: &[
        block((0.0, 1.8), 1.0, (0.6, 1.9)),
        block((1.8, 5.5), TRUCK_HALF_WIDTH, (0.6, 4.0)),
        block((5.5, 8.0), 0.6, (0.6, 1.2)),
        block((5.9, 22.0), TRUCK_HALF_WIDTH, (1.2, 4.1)),
        block((21.8, 22.0), TRUCK_HALF_WIDTH, (0.6, 1.2)),
    ],
    axles: &[1.0, 5.0, 6.2, 19.2, 20.4],
    cab: (1.8, 4.0),
    cargo: (5.9, 22.0),
    cargo_up: (1.2, 4.1),
    headlight: 0.8,
    solids: &[AMERICAN_HOOD, AMERICAN_SLEEPER, AMERICAN_DEFLECTOR],
    solid_blocks: 2,
    roof_lamp: (2.6, 3.3),
};

const AMERICAN_BOX: ShapeSpec = ShapeSpec {
    length: 9.5,
    height: 3.9,
    blocks: &[
        block((0.0, 1.2), 1.0, (0.6, 1.6)),
        block((1.2, 2.7), TRUCK_HALF_WIDTH, (0.6, 2.6)),
        block((2.7, 9.3), 0.6, (0.6, 1.1)),
        block((2.9, 9.5), TRUCK_HALF_WIDTH, (1.1, 3.9)),
        block((9.3, 9.5), TRUCK_HALF_WIDTH, (0.6, 1.1)),
    ],
    axles: &[0.8, 7.8],
    cab: (1.2, 2.6),
    cargo: (2.9, 9.5),
    cargo_up: (1.1, 3.9),
    headlight: 0.8,
    solids: &[AMERICAN_BOX_HOOD, AMERICAN_BOX_CAB],
    solid_blocks: 2,
    roof_lamp: (1.75, 2.5),
};

impl TruckShape {
    fn spec(self) -> &'static ShapeSpec {
        match self {
            TruckShape::JapaneseBox => &JAPANESE_BOX,
            TruckShape::JapaneseTrailer => &JAPANESE_TRAILER,
            TruckShape::AmericanSemi => &AMERICAN_SEMI,
            TruckShape::AmericanBox => &AMERICAN_BOX,
        }
    }

    /// The length of the truck in meters.
    pub fn length(self) -> f32 {
        self.spec().length
    }

    /// The shape of a truck of the given style for a random `roll` in
    /// `0..1`; `None` where the style has no trucks.
    fn pick(style: TruckStyle, roll: f32) -> Option<Self> {
        match style {
            TruckStyle::None => None,
            TruckStyle::Japanese if roll < 0.6 => Some(TruckShape::JapaneseBox),
            TruckStyle::Japanese => Some(TruckShape::JapaneseTrailer),
            TruckStyle::NorthAmerican if roll < 0.7 => Some(TruckShape::AmericanSemi),
            TruckStyle::NorthAmerican => Some(TruckShape::AmericanBox),
        }
    }
}

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

/// How the bodies of the other cars are drawn.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CarShapes {
    /// Only the lights of the cars are drawn.
    #[default]
    None,
    /// Each car is one box.
    Boxes,
    /// Cars have the shapes of sedans, minivans and so on, and trucks their
    /// own shapes, made of boxes.
    Blocks,
    /// The same shapes with the outlines of real cars: a hood, a windshield, a
    /// roof that tapers, windows.
    Detailed,
}

impl CarShapes {
    /// Every choice, in the order of the stored values 0, 1, 2, 3.
    pub const ALL: [CarShapes; 4] = [
        CarShapes::None,
        CarShapes::Boxes,
        CarShapes::Blocks,
        CarShapes::Detailed,
    ];

    /// The choice for a stored value; `None` when the value is not valid.
    pub fn from_raw(raw: Option<u32>) -> Option<Self> {
        Self::ALL.get(usize::try_from(raw?).ok()?).copied()
    }

    /// The value to store; `from_raw` reads it back.
    pub fn to_raw(self) -> u32 {
        match self {
            CarShapes::None => 0,
            CarShapes::Boxes => 1,
            CarShapes::Blocks => 2,
            CarShapes::Detailed => 3,
        }
    }

    /// Whether the cars have bodies at all.
    pub fn has_bodies(self) -> bool {
        self != CarShapes::None
    }
}

/// The color of the bodies of the other cars.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CarColor {
    /// Black bodies that let a tenth of the light behind them through.
    #[default]
    TranslucentBlack,
    /// Black bodies that hide everything behind them.
    Black,
}

impl CarColor {
    /// Every choice, in the order of the stored values 0, 1.
    pub const ALL: [CarColor; 2] = [CarColor::TranslucentBlack, CarColor::Black];

    /// The choice for a stored value; `None` when the value is not valid.
    pub fn from_raw(raw: Option<u32>) -> Option<Self> {
        Self::ALL.get(usize::try_from(raw?).ok()?).copied()
    }

    /// How much a body hides what is behind it, from 0 to 1.
    pub fn opacity(self) -> f32 {
        match self {
            CarColor::TranslucentBlack => 0.9,
            CarColor::Black => 1.0,
        }
    }

    /// The value to store; `from_raw` reads it back.
    pub fn to_raw(self) -> u32 {
        match self {
            CarColor::TranslucentBlack => 0,
            CarColor::Black => 1,
        }
    }
}

/// The shapes of cars that are not trucks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CarModel {
    Sedan,
    Hatchback,
    Minivan,
    Suv,
    /// A small Japanese car.
    Kei,
    /// A North American pickup truck.
    Pickup,
}

/// What a car is made of, in meters from its front end and above the road:
/// a body, a cabin on it and the wheels.
struct ModelSpec {
    length: f32,
    half_width: f32,
    /// The lower and upper edge of the body, which spans the whole car.
    body: (f32, f32),
    /// Where the cabin stands along the car, and the height of its roof. It
    /// rises from the upper edge of the body.
    cabin: (f32, f32),
    roof: f32,
    /// The diameter of the wheels, and where the front and the rear axle are.
    wheel: f32,
    axles: (f32, f32),
    /// The upper outline of the detailed shape, from the front end, and how
    /// much narrower its roof is than its body on each side.
    outline: &'static [(f32, f32, Surface)],
    inset: f32,
}

impl ModelSpec {
    /// Where the lamp that shows when the car brakes stands on the rear window
    /// of the detailed shape: how far from the rear end, and how high.
    fn rear_window_lamp(&self) -> (f32, f32) {
        let outline = self.outline;
        let window = (0..outline.len() - 1)
            .rev()
            .find(|&i| outline[i].2 == Surface::Glass)
            .unwrap_or(outline.len() - 2);
        let (a, b) = (outline[window], outline[window + 1]);
        // A fifth of the way down from the roof.
        let (x, z) = (a.0 + (b.0 - a.0) * 0.2, a.1 + (b.1 - a.1) * 0.2);
        (self.length - x, z)
    }
}

const B: Surface = Surface::Body;
const G: Surface = Surface::Glass;
const C: Surface = Surface::Cabin;

impl CarModel {
    /// Every model, in the order the shares are given in.
    pub const ALL: [CarModel; 6] = [
        CarModel::Sedan,
        CarModel::Hatchback,
        CarModel::Minivan,
        CarModel::Suv,
        CarModel::Kei,
        CarModel::Pickup,
    ];

    fn spec(self) -> &'static ModelSpec {
        match self {
            CarModel::Sedan => &SEDAN,
            CarModel::Hatchback => &HATCHBACK,
            CarModel::Minivan => &MINIVAN,
            CarModel::Suv => &SUV,
            CarModel::Kei => &KEI,
            CarModel::Pickup => &PICKUP,
        }
    }

    /// The length of the car in meters.
    pub fn length(self) -> f32 {
        self.spec().length
    }

    /// The model for a random `roll` in `0..1`, by the shares of the models
    /// among the cars that are not trucks. The Japanese small car and the
    /// pickup truck belong to the truck style of their country.
    fn pick(style: TruckStyle, roll: f32) -> Self {
        let shares: [(CarModel, f32); 5] = match style {
            TruckStyle::None => [
                (CarModel::Sedan, 0.30),
                (CarModel::Hatchback, 0.25),
                (CarModel::Minivan, 0.25),
                (CarModel::Suv, 0.20),
                (CarModel::Suv, 0.0),
            ],
            TruckStyle::Japanese => [
                (CarModel::Sedan, 0.25),
                (CarModel::Hatchback, 0.20),
                (CarModel::Minivan, 0.25),
                (CarModel::Suv, 0.15),
                (CarModel::Kei, 0.15),
            ],
            TruckStyle::NorthAmerican => [
                (CarModel::Sedan, 0.30),
                (CarModel::Hatchback, 0.10),
                (CarModel::Minivan, 0.15),
                (CarModel::Suv, 0.30),
                (CarModel::Pickup, 0.15),
            ],
        };
        let mut edge = 0.0;
        for (model, share) in shares {
            edge += share;
            if roll < edge {
                return model;
            }
        }
        // Rounding may leave a sliver at the end.
        shares[4].0
    }
}

const SEDAN: ModelSpec = ModelSpec {
    length: 4.7,
    half_width: 0.9,
    body: (0.15, 0.95),
    cabin: (1.6, 3.7),
    roof: 1.45,
    wheel: 0.65,
    axles: (0.95, 3.75),
    outline: &[
        (0.0, 0.55, B),
        (0.15, 0.75, B),
        (1.3, 0.85, B),
        (1.5, 0.9, G),
        (2.3, 1.45, C),
        (3.3, 1.45, G),
        (3.95, 1.0, B),
        (4.6, 0.98, B),
        (4.7, 0.6, B),
    ],
    inset: 0.17,
};

const HATCHBACK: ModelSpec = ModelSpec {
    length: 4.0,
    half_width: 0.875,
    body: (0.15, 0.95),
    cabin: (1.4, 3.9),
    roof: 1.5,
    wheel: 0.6,
    axles: (0.8, 3.2),
    outline: &[
        (0.0, 0.5, B),
        (0.12, 0.7, B),
        (1.1, 0.8, B),
        (1.25, 0.85, G),
        (2.0, 1.5, C),
        (2.9, 1.5, G),
        (3.85, 0.97, B),
        (4.0, 0.6, B),
    ],
    inset: 0.15,
};

const MINIVAN: ModelSpec = ModelSpec {
    length: 4.8,
    half_width: 0.9,
    body: (0.15, 1.0),
    cabin: (1.0, 4.8),
    roof: 1.85,
    wheel: 0.65,
    axles: (0.95, 3.85),
    outline: &[
        (0.0, 0.55, B),
        (0.15, 0.8, B),
        (0.9, 0.95, B),
        (1.1, 1.0, G),
        (1.9, 1.85, C),
        (4.3, 1.85, G),
        (4.72, 1.1, B),
        (4.8, 0.65, B),
    ],
    inset: 0.15,
};

const SUV: ModelSpec = ModelSpec {
    length: 4.6,
    half_width: 0.925,
    body: (0.2, 1.1),
    cabin: (1.5, 4.4),
    roof: 1.75,
    wheel: 0.75,
    axles: (0.95, 3.65),
    outline: &[
        (0.0, 0.65, B),
        (0.15, 0.95, B),
        (1.25, 1.05, B),
        (1.5, 1.1, G),
        (2.2, 1.75, C),
        (3.95, 1.75, G),
        (4.5, 1.3, B),
        (4.6, 0.75, B),
    ],
    inset: 0.18,
};

const KEI: ModelSpec = ModelSpec {
    length: 3.4,
    half_width: 0.74,
    body: (0.15, 0.9),
    cabin: (0.8, 3.4),
    roof: 1.75,
    wheel: 0.55,
    axles: (0.6, 2.8),
    outline: &[
        (0.0, 0.5, B),
        (0.1, 0.75, B),
        (0.75, 0.85, B),
        (0.9, 0.9, G),
        (1.35, 1.75, C),
        (3.2, 1.75, G),
        (3.35, 1.0, B),
        (3.4, 0.6, B),
    ],
    inset: 0.15,
};

const PICKUP: ModelSpec = ModelSpec {
    length: 5.9,
    half_width: 1.0,
    body: (0.22, 1.2),
    cabin: (1.9, 3.6),
    roof: 1.95,
    wheel: 0.8,
    axles: (1.0, 4.6),
    outline: &[
        (0.0, 0.75, B),
        (0.15, 1.0, B),
        (1.6, 1.15, B),
        (1.8, 1.2, G),
        (2.5, 1.95, C),
        (3.4, 1.95, G),
        (3.55, 1.45, B),
        (3.6, 1.2, B),
        (5.8, 1.2, B),
        (5.9, 0.75, B),
    ],
    inset: 0.2,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    /// Same direction, same lane, a little slower than the camera.
    Lead,
    /// Same direction, passing lane, faster than the camera.
    Passing,
    Oncoming,
}

/// Which face of the box of a body a `BodyFace` is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FacePart {
    /// The end that faces the camera.
    End,
    /// One of the two sides.
    Side,
    Roof,
    /// The windshield, the rear window and the side windows of a detailed body.
    Glass,
}

/// A flat face of the body of a car: a quadrilateral whose corners are
/// positioned like the lights of the road, as (distance ahead of the camera,
/// lateral position, height above the road).
#[derive(Clone, Copy, Debug)]
pub struct BodyFace {
    /// The car the face belongs to, which is also that of its lights.
    pub car: u32,
    pub part: FacePart,
    /// Lateral position of the car.
    pub lateral: f32,
    /// The point of the car whose view through a tunnel tells whether the
    /// car is seen, as (distance, lateral position, height).
    pub sight: (f32, f32, f32),
    pub corners: [(f32, f32, f32); 4],
}

/// A box of the body of a car, cut off at the near and far end of the view.
/// A car is one box; a truck is several, wheels included.
#[derive(Clone, Copy, Debug)]
pub struct BodyBox {
    /// The car the box belongs to, which is also that of its lights.
    pub car: u32,
    pub lateral: f32,
    /// See `BodyFace::sight`; the same for every box of the car.
    pub sight: (f32, f32, f32),
    /// The extent of the box along the road.
    pub near: f32,
    pub far: f32,
    /// The extent across the road, as lateral positions.
    pub left: f32,
    pub right: f32,
    /// The lowest and highest height of the box.
    pub bottom: f32,
    pub top: f32,
    /// Whether the end that faces the camera is in view.
    pub has_end: bool,
    /// The middle and half the length of the whole car.
    pub center: f32,
    pub half_length: f32,
    /// The lowest and highest height of the box that casts the shadow of this
    /// box; `None` where the box casts none. The shadow of a car is that of
    /// the whole car, cast by one of its boxes.
    pub shadow: Option<(f32, f32)>,
    /// The part along the road where another box of the car stands on the top
    /// of this one, which hides the roof there.
    pub covered: Option<(f32, f32)>,
    /// The solid with the outline of a car that the box is the extent of, if
    /// it is one: its faces and shadow are those of the solid.
    pub solid: Option<Solid>,
}

impl BodyBox {
    /// The corners of the top, as (distance, lateral position, height).
    pub fn top_corners(&self) -> [(f32, f32, f32); 4] {
        [
            (self.near, self.left, self.top),
            (self.near, self.right, self.top),
            (self.far, self.right, self.top),
            (self.far, self.left, self.top),
        ]
    }

    /// The faces that the camera can see: the end that faces it, both
    /// sides and the roof. Which of them it does depend on the bends and
    /// hills of the road, so they are all given (the end only where it is in
    /// view) and the scene keeps those that turn toward it. Each runs the
    /// way that makes the face turn toward the camera, as seen from the
    /// outside, with the corners in the same sense.
    pub fn faces(&self) -> Vec<BodyFace> {
        if let Some(solid) = &self.solid {
            return solid
                .faces()
                .into_iter()
                .map(|(part, corners)| BodyFace {
                    car: self.car,
                    part,
                    lateral: self.lateral,
                    sight: self.sight,
                    corners,
                })
                .collect();
        }
        let (near, far, left, right) = (self.near, self.far, self.left, self.right);
        let (bottom, top) = (self.bottom, self.top);
        let end = self.has_end.then_some((
            FacePart::End,
            [
                (near, left, bottom),
                (near, right, bottom),
                (near, right, top),
                (near, left, top),
            ],
        ));
        let rest = [
            (
                FacePart::Side,
                [
                    (near, left, bottom),
                    (near, left, top),
                    (far, left, top),
                    (far, left, bottom),
                ],
            ),
            (
                FacePart::Side,
                [
                    (near, right, bottom),
                    (far, right, bottom),
                    (far, right, top),
                    (near, right, top),
                ],
            ),
        ];
        // The roof, but for the part that another box stands on.
        let (hole_near, hole_far) = self.covered.unwrap_or((near, near));
        let roofs = [
            (near, hole_near.clamp(near, far)),
            (hole_far.clamp(near, far), far),
        ]
        .into_iter()
        .filter(|&(from, to)| to - from > 1e-3)
        .map(move |(from, to)| {
            (
                FacePart::Roof,
                [
                    (from, left, top),
                    (from, right, top),
                    (to, right, top),
                    (to, left, top),
                ],
            )
        });
        end.into_iter()
            .chain(rest)
            .chain(roofs)
            .map(|(part, corners)| BodyFace {
                car: self.car,
                part,
                lateral: self.lateral,
                sight: self.sight,
                corners,
            })
            .collect()
    }
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

/// Where a car stands on the road, for keeping clear of it.
#[derive(Clone, Copy)]
struct Occupant {
    lateral: f32,
    rel: f32,
    length: f32,
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
    /// The shape of the car if it is a truck; decided each time the car is
    /// placed.
    truck: Option<TruckShape>,
    /// The shape of the car if it is not a truck; decided each time the car
    /// is placed.
    model: CarModel,
    /// Where cars keep clear of each other: the velocity the car drives at,
    /// which follows what it should be at a limited rate. `None` after the car
    /// has been placed, when it takes up the velocity it should have at once.
    drive: Option<f32>,
    /// How often the car has braked.
    brakes: u32,
    until_brake: f32,
    braking_left: f32,
}

impl Car {
    #[cfg(test)]
    fn new(seed: u64, id: u32, style: TruckStyle, level: TrafficLevel) -> Self {
        Self::among(seed, id, style, level, None)
    }

    /// A car placed clear of `others`, if they are given.
    fn among(
        seed: u64,
        id: u32,
        style: TruckStyle,
        level: TrafficLevel,
        others: Option<&[Occupant]>,
    ) -> Self {
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
            truck: None,
            model: CarModel::Sedan,
            drive: None,
            brakes: 0,
            until_brake: 0.0,
            braking_left: 0.0,
        };
        car.place(seed, style, level, true, others);
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
    ///
    /// With `others`, the places of the other cars, the car is put where it
    /// keeps clear of the cars in its lane.
    fn place(
        &mut self,
        seed: u64,
        style: TruckStyle,
        level: TrafficLevel,
        initial: bool,
        others: Option<&[Occupant]>,
    ) {
        let position = self.roll(seed, STREAM_POSITION);
        let speed = self.roll(seed, STREAM_SPEED);
        // New streams keep the other random values of the car as they were.
        self.active = self.roll(seed, STREAM_ACTIVE) < level.active;
        self.truck = if self.roll(seed, STREAM_TRUCK) < level.trucks {
            TruckShape::pick(style, self.roll(seed, STREAM_SHAPE))
        } else {
            None
        };
        self.model = CarModel::pick(style, self.roll(seed, STREAM_MODEL));
        self.age = 0.0;
        self.drive = None;
        self.phase = self.roll(seed, STREAM_PHASE) * std::f32::consts::TAU;
        self.escape_after = 40.0 + 80.0 * self.roll(seed, STREAM_ESCAPE);
        match self.kind {
            Kind::Lead => {
                self.lateral = OWN_LANE;
                self.rel = 80.0 + 450.0 * position;
                self.ratio = if self.truck.is_some() {
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
                if self.truck.is_some() {
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
        if let Some(others) = others {
            self.keep_clear(initial, others);
        }
    }

    /// Moves a car that is newly placed along the part of the road it may be
    /// placed in until it is clear of the cars in its lane.
    fn keep_clear(&mut self, initial: bool, others: &[Occupant]) {
        let (first, span) = match (self.kind, initial) {
            (Kind::Lead, _) => (80.0, 450.0),
            (Kind::Passing, true) => (20.0, 500.0),
            (Kind::Passing, false) => (-FAR_BEHIND + 10.0, 220.0),
            (Kind::Oncoming, true) => (0.0, 600.0),
            (Kind::Oncoming, false) => (600.0, 400.0),
        };
        let length = self.length();
        let lane = |o: &&Occupant| (o.lateral - self.lateral).abs() < 0.5;
        let clear = |at: f32| {
            others
                .iter()
                .filter(lane)
                .all(|o| at >= o.rel + o.length + PLACE_GAP || at + length + PLACE_GAP <= o.rel)
        };
        let start = self.rel;
        let tries = (span / SLOT_STEP) as usize;
        for step in 0..tries {
            let at = first + (start - first + step as f32 * SLOT_STEP).rem_euclid(span);
            if clear(at) {
                self.rel = at;
                return;
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

    /// Moves the car for `dt` seconds at `velocity`, meters per second ahead
    /// of the camera.
    fn advance(&mut self, seed: u64, dt: f32, velocity: f32) {
        self.rel += velocity * dt;
        if self.kind != Kind::Oncoming {
            self.step_brake(seed, dt);
        }
    }

    /// Whether the car has left the part of the road it drives on.
    fn is_gone(&self) -> bool {
        match self.kind {
            Kind::Oncoming => self.rel < ONCOMING_GONE,
            _ => self.rel > FAR_AHEAD || self.rel < -FAR_BEHIND,
        }
    }

    fn occupies(&self) -> Occupant {
        Occupant {
            lateral: self.lateral,
            rel: self.rel,
            length: self.length(),
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

    fn push_lights(&self, style: TruckStyle, shapes: CarShapes, out: &mut Vec<CarLight>) {
        let d = self.rel;
        if !self.active || !(NEAR_DISTANCE..VIEW_DISTANCE).contains(&d) {
            return;
        }
        if let Some(shape) = self.truck {
            self.push_truck_lights(shape, style, self.detailed(shapes), out);
            return;
        }
        if shapes.has_bodies() {
            self.push_model_lights(self.detailed(shapes), out);
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
    /// The length of the body of the car in meters.
    fn length(&self) -> f32 {
        self.truck
            .map_or(self.model.length(), |shape| shape.spec().length)
    }

    /// The height of the body of the car in meters.
    fn height(&self) -> f32 {
        self.truck
            .map_or(self.model.spec().roof, |shape| shape.spec().height)
    }

    /// The boxes of the body, cut off where the view ends. `rel` is the end
    /// that faces the camera, and the body reaches the length of the car
    /// farther away. A body that has partly passed the camera is cut off at
    /// `NEAR_DISTANCE`, so that it does not vanish all at once.
    fn push_boxes(&self, shapes: CarShapes, out: &mut Vec<BodyBox>) {
        let length = self.length();
        let end_of_view = VIEW_DISTANCE - VIEW_MARGIN;
        if !self.active || self.rel >= end_of_view || self.rel + length <= NEAR_DISTANCE {
            return;
        }
        let height = self.height();
        let sight = (self.rel.max(NEAR_DISTANCE), self.lateral, height * 0.5);
        let center = self.rel + length * 0.5;
        let push = |out: &mut Vec<BodyBox>, part: Part| {
            let (start, stop) = (
                self.rel + part.along.0,
                (self.rel + part.along.1).min(end_of_view),
            );
            if start >= end_of_view || stop <= NEAR_DISTANCE {
                return;
            }
            out.push(BodyBox {
                car: self.id,
                lateral: self.lateral,
                sight,
                near: start.max(NEAR_DISTANCE),
                far: stop,
                left: self.lateral + part.across.0,
                right: self.lateral + part.across.1,
                bottom: part.up.0,
                top: part.up.1,
                has_end: start >= NEAR_DISTANCE,
                center,
                half_length: length * 0.5,
                shadow: part.shadow,
                covered: part
                    .covered
                    .map(|(from, to)| (self.rel + from, self.rel + to)),
                solid: None,
            });
        };
        // A part with the outline of a car; `shadow` tells whether it casts
        // the shadow of the car.
        let push_solid = |out: &mut Vec<BodyBox>, spec: &SolidSpec, shadow: bool| {
            let front_first = self.kind == Kind::Oncoming;
            let Some(solid) = spec.place(
                self.rel,
                length,
                front_first,
                self.lateral,
                (NEAR_DISTANCE, end_of_view),
            ) else {
                return;
            };
            let (near, far, left, right, bottom, top) = solid.bounds();
            out.push(BodyBox {
                car: self.id,
                lateral: self.lateral,
                sight,
                near,
                far,
                left,
                right,
                bottom,
                top,
                has_end: solid.has_end(),
                center,
                half_length: length * 0.5,
                shadow: shadow.then_some((0.0, top)),
                covered: None,
                solid: Some(solid),
            });
        };
        // The shape is given from the front; the end that faces the camera is
        // the front of an oncoming car and the rear of the others.
        let along = |from: f32, to: f32| {
            if self.kind == Kind::Oncoming {
                (from, to)
            } else {
                (length - to, length - from)
            }
        };
        let solid = |across: f32, up: (f32, f32)| Part {
            along: (0.0, length),
            across: (-across, across),
            up,
            shadow: Some(up),
            covered: None,
        };
        if shapes == CarShapes::Boxes {
            // One box that holds the whole shape.
            let across = self
                .truck
                .map_or(self.model.spec().half_width, |_| TRUCK_HALF_WIDTH);
            push(out, solid(across, (0.0, height)));
            return;
        }
        let detailed = self.detailed(shapes);
        let Some(shape) = self.truck else {
            let spec = self.model.spec();
            let half = spec.half_width;
            if detailed {
                // The body and the cabin are one solid, which casts the shadow
                // of the whole car.
                push_solid(
                    out,
                    &SolidSpec {
                        outline: spec.outline,
                        half_width: half,
                        bottom: spec.body.0,
                        shoulder: spec.body.1,
                        inset: spec.inset,
                        chamfer: 0.0,
                    },
                    true,
                );
            } else {
                let (cabin_from, cabin_to) = spec.cabin;
                // The body spans the car and casts the shadow of the whole car.
                push(
                    out,
                    Part {
                        shadow: Some((0.0, spec.roof)),
                        covered: Some(along(cabin_from, cabin_to)),
                        ..solid(half, spec.body)
                    },
                );
                push(
                    out,
                    Part {
                        along: along(cabin_from, cabin_to),
                        across: (-(half - CABIN_INSET), half - CABIN_INSET),
                        up: (spec.body.1, spec.roof),
                        shadow: None,
                        covered: None,
                    },
                );
            }
            // A wheel at each side of each axle; the light of the road shows
            // between them.
            let edge = half - CAR_WHEEL_INSET;
            for axle in [spec.axles.0, spec.axles.1] {
                for across in [
                    (-edge, CAR_WHEEL_WIDTH - edge),
                    (edge - CAR_WHEEL_WIDTH, edge),
                ] {
                    push(
                        out,
                        Part {
                            along: along(axle - spec.wheel * 0.5, axle + spec.wheel * 0.5),
                            across,
                            up: (0.0, spec.wheel),
                            shadow: None,
                            covered: None,
                        },
                    );
                }
            }
            return;
        };
        let spec = shape.spec();
        // The low parts, wheels and frames, cast shadows that the body's hide.
        let shadow = |up: (f32, f32)| (up.1 > LOW_BOX).then_some(up);
        // Where the cab and the hood have outlines, the boxes that stand for
        // them are left out.
        let skipped = if detailed { spec.solid_blocks } else { 0 };
        for block in spec.blocks.iter().skip(skipped) {
            let half = block.half_width;
            push(
                out,
                Part {
                    along: along(block.along.0, block.along.1),
                    across: (-half, half),
                    up: block.up,
                    shadow: shadow(block.up),
                    covered: None,
                },
            );
        }
        if detailed {
            for solid in spec.solids {
                push_solid(out, solid, true);
            }
        }
        // Axles that are closer than `TANDEM_GAP` are one set with one wheel
        // each side: the gaps between their wheels are too small to be seen.
        let mut sets: Vec<(f32, f32)> = Vec::new();
        for &axle in spec.axles {
            let (from, to) = (axle - WHEEL_LENGTH * 0.5, axle + WHEEL_LENGTH * 0.5);
            match sets.last_mut() {
                Some(last) if from - last.1 < TANDEM_GAP => last.1 = to,
                _ => sets.push((from, to)),
            }
        }
        let edge = TRUCK_HALF_WIDTH;
        for (from, to) in sets {
            for across in [(-edge, WHEEL_WIDTH - edge), (edge - WHEEL_WIDTH, edge)] {
                push(
                    out,
                    Part {
                        along: along(from, to),
                        across,
                        up: (0.0, WHEEL_HEIGHT),
                        shadow: shadow((0.0, WHEEL_HEIGHT)),
                        covered: None,
                    },
                );
            }
        }
    }

    /// Whether the body of the car has the outline of a real car: the shapes
    /// are detailed and the car is near enough to show it.
    fn detailed(&self, shapes: CarShapes) -> bool {
        shapes == CarShapes::Detailed && self.rel < DETAIL_RANGE
    }

    /// Lights of a car that has a body, which stand on its surface. `rel` is
    /// the end of the body that faces the camera. The tail lamps and the
    /// headlights are on the body, near its upper edge; the lamp that shows
    /// when the car brakes is on the cabin.
    fn push_model_lights(&self, detailed: bool, out: &mut Vec<CarLight>) {
        let spec = self.model.spec();
        let d = self.rel;
        let across = spec.half_width - LAMP_INSET;
        match self.kind {
            Kind::Lead | Kind::Passing => {
                let fade = (1.0 - d / TAIL_RANGE).max(0.0);
                let (size, alpha) = if self.braking() {
                    (0.09, fade)
                } else {
                    (0.07, 0.6 * fade)
                };
                for side in [-across, across] {
                    out.push(CarLight {
                        car: self.id,
                        d,
                        lateral: self.lateral + side,
                        height: spec.body.1 - 0.15,
                        size,
                        color: TAIL_RED,
                        alpha,
                        glare: false,
                    });
                }
                // The lamp stands on the rear face of the cabin, or on the rear
                // window of a car with an outline, near its top.
                let (from_rear, height) = if detailed {
                    spec.rear_window_lamp()
                } else {
                    (spec.length - spec.cabin.1, spec.body.1 + 0.25)
                };
                let behind = d + from_rear;
                if self.braking() && behind < VIEW_DISTANCE {
                    out.push(CarLight {
                        car: self.id,
                        d: behind,
                        lateral: self.lateral,
                        height,
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
                let height = spec.body.1 - 0.25;
                for side in [-across, across] {
                    out.push(CarLight {
                        car: self.id,
                        d,
                        lateral: self.lateral + side,
                        height,
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
                        height,
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

/// A box of a car: along the road as distances from the end that faces the
/// camera, across it from the middle of the car, and above the road.
struct Part {
    along: (f32, f32),
    across: (f32, f32),
    up: (f32, f32),
    shadow: Option<(f32, f32)>,
    covered: Option<(f32, f32)>,
}

impl Car {
    /// Lights of a truck. `rel` is the end of the body that faces the
    /// camera (the rear of a car ahead, the front of an oncoming car).
    /// Lamps stand on the surface of the shape of the truck.
    fn push_truck_lights(
        &self,
        shape: TruckShape,
        style: TruckStyle,
        detailed: bool,
        out: &mut Vec<CarLight>,
    ) {
        let spec = shape.spec();
        let oncoming = self.kind == Kind::Oncoming;
        // The side that faces the camera. The camera is in the own lane, so
        // a truck in that lane has no near side; it gets the right one.
        let side = if self.lateral > OWN_LANE { -1.0 } else { 1.0 };
        // Distance from the end that faces the camera, for a place counted
        // from the front of the truck.
        let from_camera = |front: f32| if oncoming { front } else { spec.length - front };
        let braking = self.braking();
        let mut lamp =
            |front: f32, offset: f32, height: f32, size: f32, color: Rgb, strength: f32| {
                let d = self.rel + from_camera(front);
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
        let (front, rear) = (0.0, spec.length);
        // The end that faces the camera, as a place counted from the front.
        let end = if oncoming { front } else { rear };
        if oncoming {
            for lateral in [-spec.headlight, spec.headlight] {
                lamp(front, lateral, 0.9, 0.13, HEADLIGHT_WHITE, 1.0);
            }
        } else {
            let (size, strength) = if braking { (0.09, 1.0) } else { (0.07, 0.6) };
            for lateral in [-1.05, 1.05] {
                lamp(rear, lateral, 1.0, size, TAIL_RED, strength);
            }
        }
        let edge = side * TRUCK_HALF_WIDTH;
        let (cargo_from, cargo_to) = spec.cargo;
        let lower = spec.cargo_up.0 + 0.15;
        let upper = spec.cargo_up.1 - 0.1;
        match style {
            TruckStyle::None => {}
            TruckStyle::Japanese => {
                // End lamps on the top edge of the cab or of the box, and
                // marker lamps along the lower edge of the box.
                let (color, at, height) = match (oncoming, detailed) {
                    // The front of the roof of a cab that has an outline.
                    (true, true) => (MARKER_AMBER, spec.roof_lamp.0, spec.roof_lamp.1),
                    (true, false) => (MARKER_AMBER, end, spec.cab.1 - 0.1),
                    (false, _) => (TAIL_RED, end, upper),
                };
                for lateral in [-1.15, 1.15] {
                    lamp(at, lateral, height, small_size, color, small_strength);
                }
                let count = ((cargo_to - cargo_from) / 1.8).round() as usize;
                for index in 0..=count {
                    let t = index as f32 / count as f32;
                    let z = cargo_from + 0.3 + (cargo_to - cargo_from - 0.6) * t;
                    lamp(z, edge, lower, small_size, MARKER_AMBER, small_strength);
                }
            }
            TruckStyle::NorthAmerican => {
                // Markers along the lower edge of the trailer or the box:
                // amber toward the front, red toward the rear.
                let count = ((cargo_to - cargo_from) / 4.0).round().max(1.0) as usize;
                for index in 0..=count {
                    let t = index as f32 / count as f32;
                    let z = cargo_from + 0.3 + (cargo_to - cargo_from - 0.6) * t;
                    let color = if t <= 0.5 { MARKER_AMBER } else { TAIL_RED };
                    lamp(z, edge, lower, small_size, color, small_strength);
                }
                if oncoming {
                    // Three amber identification lamps on the roof of the cab.
                    let (at, height) = if detailed {
                        spec.roof_lamp
                    } else {
                        (spec.cab.0, spec.cab.1 - 0.05)
                    };
                    for lateral in [-0.3, 0.0, 0.3] {
                        lamp(
                            at,
                            lateral,
                            height,
                            small_size,
                            MARKER_AMBER,
                            small_strength,
                        );
                    }
                } else {
                    // Three identification lamps and two clearance lamps on
                    // the top edge of the rear.
                    for lateral in [-0.3, 0.0, 0.3, -1.2, 1.2] {
                        lamp(rear, lateral, upper, small_size, TAIL_RED, small_strength);
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
    /// How the bodies of the cars are drawn. The cars keep clear of each other
    /// where they are drawn, since bodies that pass through each other are
    /// seen; with only lights, that is not worth the change in the traffic.
    shapes: CarShapes,
    cars: Vec<Car>,
}

impl Traffic {
    pub fn new(seed: u64, trucks: TruckStyle) -> Self {
        Self::with_level(seed, trucks, TrafficLevel::FULL)
    }

    /// Traffic that is as busy as `level` from the start.
    pub fn with_level(seed: u64, trucks: TruckStyle, level: TrafficLevel) -> Self {
        Self::build(seed, trucks, level, CarShapes::None)
    }

    /// Like `with_level`, and the cars of a lane keep clear of each other if
    /// they have bodies.
    pub fn with_shapes(
        seed: u64,
        trucks: TruckStyle,
        level: TrafficLevel,
        shapes: CarShapes,
    ) -> Self {
        Self::build(seed, trucks, level, shapes)
    }

    fn build(seed: u64, trucks: TruckStyle, level: TrafficLevel, shapes: CarShapes) -> Self {
        let keep_apart = shapes.has_bodies();
        let count = LEAD_CARS + PASSING_CARS + ONCOMING_CARS;
        let mut cars: Vec<Car> = Vec::new();
        for id in 0..count {
            let placed: Vec<Occupant> = cars.iter().map(Car::occupies).collect();
            cars.push(Car::among(
                seed,
                id,
                trucks,
                level,
                keep_apart.then_some(&placed[..]),
            ));
        }
        Self {
            seed,
            trucks,
            level,
            shapes,
            cars,
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
        // A car's speed depends on how long it has been placed, which counts this step.
        for car in &mut self.cars {
            car.age += dt;
        }
        let mut velocity: Vec<f32> = self
            .cars
            .iter()
            .map(|c| c.rel_velocity(own_speed))
            .collect();
        if self.shapes.has_bodies() {
            self.match_speeds(&mut velocity, dt);
        }
        for (car, &velocity) in self.cars.iter_mut().zip(&velocity) {
            car.advance(self.seed, dt, velocity);
        }
        if self.shapes.has_bodies() {
            self.push_apart();
        }
        for index in 0..self.cars.len() {
            if !self.cars[index].is_gone() {
                continue;
            }
            let others: Vec<Occupant> = self
                .cars
                .iter()
                .enumerate()
                .filter(|&(other, _)| other != index)
                .map(|(_, c)| c.occupies())
                .collect();
            let car = &mut self.cars[index];
            car.generation = car.generation.wrapping_add(1);
            car.place(
                self.seed,
                self.trucks,
                self.level,
                false,
                self.shapes.has_bodies().then_some(&others[..]),
            );
        }
    }

    /// Matches the speeds of the cars of a lane so that they keep `LANE_GAP`
    /// apart without sudden changes. `velocity` is how fast each car would move
    /// ahead of the camera; it is changed to how fast it does.
    ///
    /// A car may be faster than the one ahead only by as much as it can lose
    /// at `LANE_DECEL` before the gap is `LANE_GAP`. In the lane of the camera
    /// this is done the other way round, with the car ahead speeding up and
    /// not the one behind slowing down: the nearest car must stay ahead of the
    /// camera, which would otherwise run into the queue. No speed changes
    /// faster than `LANE_SLEW`.
    fn match_speeds(&mut self, velocity: &mut [f32], dt: f32) {
        let allowed =
            |gap: f32| (2.0 * LANE_DECEL * (gap - LANE_GAP - LANE_CUSHION).max(0.0)).sqrt();
        let mut order: Vec<usize> = (0..self.cars.len()).collect();
        order.sort_by(|&a, &b| self.cars[a].rel.total_cmp(&self.cars[b].rel));
        // The speed a car drives at, going from what it did to the target. It
        // changes faster to keep clear of another car than on its own.
        let follow = |cars: &[Car], index: usize, target: f32| match cars[index].drive {
            Some(drive) => {
                let rate = if target == velocity[index] {
                    LANE_DECEL
                } else {
                    LANE_SLEW
                };
                drive + (target - drive).clamp(-rate * dt, rate * dt)
            }
            None => target,
        };
        let mut driven = velocity.to_vec();
        for lane in [OWN_LANE, PASSING_LANE, ONCOMING_LANES[0], ONCOMING_LANES[1]] {
            let in_lane: Vec<usize> = order
                .iter()
                .copied()
                .filter(|&i| self.cars[i].lateral == lane)
                .collect();
            // The space between a car and the one farther from the camera.
            let gap = |near: usize, far: usize| {
                self.cars[far].rel - (self.cars[near].rel + self.cars[near].length())
            };
            if lane == OWN_LANE {
                // From the nearest car on, the one ahead keeps up with the one behind.
                for (index, &car) in in_lane.iter().enumerate() {
                    let mut target = velocity[car];
                    if index > 0 {
                        let behind = in_lane[index - 1];
                        target = target.max(driven[behind] - allowed(gap(behind, car)));
                    }
                    driven[car] = follow(&self.cars, car, target);
                }
            } else if lane == PASSING_LANE {
                // From the farthest car on, the one behind keeps to the one ahead.
                for (index, &car) in in_lane.iter().enumerate().rev() {
                    let mut target = velocity[car];
                    if let Some(&ahead) = in_lane.get(index + 1) {
                        target = target.min(driven[ahead] + allowed(gap(car, ahead)));
                    }
                    driven[car] = follow(&self.cars, car, target);
                }
            } else {
                // Oncoming cars drive toward the camera: the one ahead is the
                // nearer one, and the one behind it is faster toward the
                // camera if its velocity is lower.
                for (index, &car) in in_lane.iter().enumerate() {
                    let mut target = velocity[car];
                    if index > 0 {
                        let ahead = in_lane[index - 1];
                        target = target.max(driven[ahead] - allowed(gap(ahead, car)));
                    }
                    driven[car] = follow(&self.cars, car, target);
                }
            }
        }
        for (car, &drive) in self.cars.iter_mut().zip(&driven) {
            car.drive = Some(drive);
        }
        velocity.copy_from_slice(&driven);
    }

    /// Where speeds were not matched in time (a car that is placed close
    /// behind another, or a step that is long), a car that has caught up with
    /// the one ahead of it in its lane pushes it on: no car passes another or
    /// comes closer than `LANE_GAP`.
    fn push_apart(&mut self) {
        let mut order: Vec<usize> = (0..self.cars.len()).collect();
        order.sort_by(|&a, &b| self.cars[a].rel.total_cmp(&self.cars[b].rel));
        for lane in [OWN_LANE, PASSING_LANE, ONCOMING_LANES[0], ONCOMING_LANES[1]] {
            let in_lane: Vec<usize> = order
                .iter()
                .copied()
                .filter(|&i| self.cars[i].lateral == lane)
                .collect();
            if lane == OWN_LANE || lane == PASSING_LANE {
                // Same direction as the camera: the farther car is ahead.
                for pair in in_lane.windows(2) {
                    let (behind, ahead) = (self.cars[pair[0]].occupies(), pair[1]);
                    let least = behind.rel + behind.length + LANE_GAP;
                    let car = &mut self.cars[ahead];
                    car.rel = car.rel.max(least);
                }
            } else {
                // Oncoming: the nearer car is ahead.
                for pair in in_lane.windows(2).rev() {
                    let (behind, ahead) = (self.cars[pair[1]].occupies(), pair[0]);
                    let car = &mut self.cars[ahead];
                    let most = behind.rel - LANE_GAP - car.length();
                    car.rel = car.rel.min(most);
                }
            }
        }
    }

    /// The lights in front of the camera, in no particular order.
    pub fn lights(&self) -> Vec<CarLight> {
        let mut lights = Vec::new();
        for car in &self.cars {
            car.push_lights(self.trucks, self.shapes, &mut lights);
        }
        lights
    }

    /// The boxes of the bodies of the cars in front of the camera, in no
    /// particular order.
    pub fn boxes(&self) -> Vec<BodyBox> {
        let mut boxes = Vec::new();
        for car in &self.cars {
            car.push_boxes(self.shapes, &mut boxes);
        }
        boxes
    }

    /// The faces of the bodies of the cars in front of the camera, in no
    /// particular order.
    pub fn bodies(&self) -> Vec<BodyFace> {
        self.boxes().iter().flat_map(BodyBox::faces).collect()
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
            car.push_lights(TruckStyle::None, CarShapes::None, &mut out);
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
            car.push_lights(TruckStyle::None, CarShapes::None, &mut out);
            out.iter().find(|l| l.glare).map(|l| l.alpha)
        };
        assert_eq!(glare(&mut car, 200.0), None);
        let far = glare(&mut car, 80.0).unwrap();
        let near = glare(&mut car, 10.0).unwrap();
        assert!(near > far && near <= GLARE_ALPHA);
    }

    fn truck_of(kind_id: u32, style: TruckStyle, rel: f32) -> Car {
        let shape = match style {
            TruckStyle::NorthAmerican => TruckShape::AmericanSemi,
            _ => TruckShape::JapaneseBox,
        };
        shaped(kind_id, shape, rel)
    }

    fn style_of(shape: TruckShape) -> TruckStyle {
        match shape {
            TruckShape::JapaneseBox | TruckShape::JapaneseTrailer => TruckStyle::Japanese,
            TruckShape::AmericanSemi | TruckShape::AmericanBox => TruckStyle::NorthAmerican,
        }
    }

    fn shaped(kind_id: u32, shape: TruckShape, rel: f32) -> Car {
        let mut car = Car::new(0, kind_id, style_of(shape), TrafficLevel::FULL);
        car.truck = Some(shape);
        car.active = true;
        car.rel = rel;
        car.braking_left = 0.0;
        car
    }

    fn lights_of(car: &Car, style: TruckStyle) -> Vec<CarLight> {
        let mut out = Vec::new();
        car.push_lights(style, CarShapes::Blocks, &mut out);
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
                        slot.0 += u32::from(car.truck.is_some());
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
                        trucks += u32::from(car.active && car.truck.is_some());
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
                assert!(t.cars.iter().all(|c| c.truck.is_none()));
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
                if a.truck.is_some() {
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
            for car in traffic.cars.iter().filter(|c| c.truck.is_some()) {
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
                seen += t.cars.iter().filter(|c| c.truck.is_some()).count();
            });
            assert!(seen > 0);
        }
    }

    /// The height of the marker lamps along a truck of the shape.
    fn marker_height(shape: TruckShape) -> f32 {
        shape.spec().cargo_up.0 + 0.15
    }

    /// The height of the lamps on the top edge of the cargo.
    fn top_height(shape: TruckShape) -> f32 {
        shape.spec().cargo_up.1 - 0.1
    }

    const JAPANESE: [TruckShape; 2] = [TruckShape::JapaneseBox, TruckShape::JapaneseTrailer];
    const AMERICAN: [TruckShape; 2] = [TruckShape::AmericanSemi, TruckShape::AmericanBox];

    #[test]
    fn a_japanese_truck_seen_from_behind_has_tail_end_and_side_lamps() {
        for id in REAR_IDS {
            for shape in JAPANESE {
                let car = shaped(id, shape, 100.0);
                let lights = lights_of(&car, TruckStyle::Japanese);
                assert_eq!(count(&lights, 1.0, TAIL_RED), 2, "{shape:?}");
                assert_eq!(count(&lights, top_height(shape), TAIL_RED), 2, "{shape:?}");
                let markers = count(&lights, marker_height(shape), MARKER_AMBER);
                assert!(markers >= 6, "{shape:?}: {markers}");
                assert_eq!(lights.len(), 2 + 2 + markers, "{shape:?}");
                let tails: Vec<f32> = lights
                    .iter()
                    .filter(|l| l.height == 1.0)
                    .map(|l| l.lateral - car.lateral)
                    .collect();
                assert!(tails.contains(&-1.05) && tails.contains(&1.05));
            }
        }
    }

    #[test]
    fn a_north_american_truck_seen_from_behind_has_identification_lamps() {
        for id in REAR_IDS {
            for shape in AMERICAN {
                let car = shaped(id, shape, 100.0);
                let lights = lights_of(&car, TruckStyle::NorthAmerican);
                let top = top_height(shape);
                assert_eq!(count(&lights, 1.0, TAIL_RED), 2, "{shape:?}");
                assert_eq!(count(&lights, top, TAIL_RED), 5, "{shape:?}");
                let (amber, red) = (
                    count(&lights, marker_height(shape), MARKER_AMBER),
                    count(&lights, marker_height(shape), TAIL_RED),
                );
                assert!(amber >= 2 && red >= 1, "{shape:?}: {amber} {red}");
                assert_eq!(lights.len(), 2 + 5 + amber + red, "{shape:?}");
                let mut offsets: Vec<f32> = lights
                    .iter()
                    .filter(|l| l.height == top)
                    .map(|l| l.lateral - car.lateral)
                    .collect();
                offsets.sort_by(f32::total_cmp);
                for (offset, expected) in offsets.iter().zip([-1.2, -0.3, 0.0, 0.3, 1.2]) {
                    assert!((offset - expected).abs() < 1e-5, "{offsets:?}");
                }
                // The amber markers are toward the front of the truck, which is
                // far from the camera, and the red ones toward the rear.
                let at = |color| {
                    lights
                        .iter()
                        .filter(|l| l.color == color && l.height == marker_height(shape))
                        .map(|l| l.d)
                        .fold((f32::MAX, f32::MIN), |(lo, hi), d| (lo.min(d), hi.max(d)))
                };
                assert!(at(MARKER_AMBER).0 > at(TAIL_RED).1, "{shape:?}");
            }
        }
    }

    #[test]
    fn an_oncoming_truck_shows_headlights_roof_and_side_lamps() {
        for shape in JAPANESE {
            let car = shaped(ONCOMING_ID, shape, 200.0);
            let lights = lights_of(&car, TruckStyle::Japanese);
            let cab_top = shape.spec().cab.1 - 0.1;
            assert_eq!(count(&lights, 0.9, HEADLIGHT_WHITE), 2, "{shape:?}");
            assert_eq!(count(&lights, cab_top, MARKER_AMBER), 2, "{shape:?}");
            let markers = count(&lights, marker_height(shape), MARKER_AMBER);
            assert!(markers >= 6, "{shape:?}");
            assert_eq!(lights.len(), 2 + 2 + markers, "{shape:?}");
        }
        for shape in AMERICAN {
            let car = shaped(ONCOMING_ID, shape, 200.0);
            let lights = lights_of(&car, TruckStyle::NorthAmerican);
            let roof = shape.spec().cab.1 - 0.05;
            assert_eq!(count(&lights, 0.9, HEADLIGHT_WHITE), 2, "{shape:?}");
            assert_eq!(count(&lights, roof, MARKER_AMBER), 3, "{shape:?}");
            let marker = marker_height(shape);
            // Amber near the front, which faces the camera, red toward the rear.
            let amber = lights
                .iter()
                .filter(|l| l.height == marker && l.color == MARKER_AMBER);
            let red = lights
                .iter()
                .filter(|l| l.height == marker && l.color == TAIL_RED);
            let (amber, red): (Vec<_>, Vec<_>) = (amber.collect(), red.collect());
            assert!(amber.len() >= 2 && !red.is_empty(), "{shape:?}");
            assert!(
                amber.iter().all(|a| red.iter().all(|r| a.d < r.d)),
                "{shape:?}"
            );
            let mut offsets: Vec<f32> = lights
                .iter()
                .filter(|l| l.height == roof)
                .map(|l| l.lateral - car.lateral)
                .collect();
            offsets.sort_by(f32::total_cmp);
            for (offset, expected) in offsets.iter().zip([-0.3, 0.0, 0.3]) {
                assert!((offset - expected).abs() < 1e-5, "{offsets:?}");
            }
        }
    }

    #[test]
    fn side_lamps_are_on_the_side_facing_the_camera() {
        for shape in JAPANESE.into_iter().chain(AMERICAN) {
            for (id, lateral, expected) in [
                (PASSING_ID, PASSING_LANE, PASSING_LANE - TRUCK_HALF_WIDTH),
                (ONCOMING_ID, ONCOMING_LANES[0], ONCOMING_LANES[0] - 1.25),
                (ONCOMING_ID, ONCOMING_LANES[1], ONCOMING_LANES[1] - 1.25),
            ] {
                let mut car = shaped(id, shape, 100.0);
                car.lateral = lateral;
                let lights = lights_of(&car, style_of(shape));
                let sides: Vec<&CarLight> = lights
                    .iter()
                    .filter(|l| l.height == marker_height(shape))
                    .collect();
                assert!(!sides.is_empty());
                assert!(sides.iter().all(|l| (l.lateral - expected).abs() < 1e-5));
            }
        }
    }

    #[test]
    fn side_lamps_line_up_along_the_cargo() {
        for shape in JAPANESE {
            let car = shaped(PASSING_ID, shape, 100.0);
            let mut distances: Vec<f32> = lights_of(&car, TruckStyle::Japanese)
                .iter()
                .filter(|l| l.color == MARKER_AMBER)
                .map(|l| l.d)
                .collect();
            distances.sort_by(f32::total_cmp);
            let (from, to) = shape.spec().cargo;
            // The lamps run along the cargo, in even steps of about two meters.
            let length = shape.length();
            assert!(distances[0] >= 100.0 + length - to, "{shape:?}");
            assert!(
                *distances.last().unwrap() <= 100.0 + length - from,
                "{shape:?}"
            );
            for pair in distances.windows(2) {
                assert!((1.5..2.1).contains(&(pair[1] - pair[0])), "{shape:?}");
            }
        }
    }

    /// The distance of a point from a box of a truck, and whether the point is
    /// inside it. The box is given from the front of the truck of `shape`
    /// placed at `rel`, seen from `kind` of car.
    fn lamp_clearance(boxes: &[BodyBox], light: &CarLight) -> (f32, bool) {
        let mut nearest = f32::MAX;
        let mut inside = false;
        for b in boxes {
            let gap = |value: f32, low: f32, high: f32| (low - value).max(value - high).max(0.0);
            let (dx, dy, dz) = (
                gap(light.d, b.near, b.far),
                gap(light.lateral, b.left, b.right),
                gap(light.height, b.bottom, b.top),
            );
            nearest = nearest.min((dx * dx + dy * dy + dz * dz).sqrt());
            let within =
                |value: f32, low: f32, high: f32| value > low + 1e-3 && value < high - 1e-3;
            inside |= within(light.d, b.near, b.far)
                && within(light.lateral, b.left, b.right)
                && within(light.height, b.bottom, b.top);
        }
        (nearest, inside)
    }

    #[test]
    fn lamps_stand_on_the_surface_of_the_shape_and_are_not_inside_it() {
        for shape in JAPANESE.into_iter().chain(AMERICAN) {
            for id in [LEAD_ID, PASSING_ID, ONCOMING_ID] {
                for lateral in [None, Some(ONCOMING_LANES[1])] {
                    let mut car = shaped(id, shape, 100.0);
                    if let Some(lateral) = lateral {
                        car.lateral = lateral;
                    }
                    let mut boxes = Vec::new();
                    car.push_boxes(CarShapes::Blocks, &mut boxes);
                    for light in lights_of(&car, style_of(shape)) {
                        let (distance, inside) = lamp_clearance(&boxes, &light);
                        assert!(!inside, "{shape:?} {id}: {light:?}");
                        assert!(distance < 0.12, "{shape:?} {id}: {distance} {light:?}");
                    }
                }
            }
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
        let upper = lights
            .iter()
            .find(|l| l.height == top_height(TruckShape::JapaneseBox))
            .unwrap();
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
        let full = lights_of(
            &truck_of(ONCOMING_ID, TruckStyle::Japanese, 200.0),
            TruckStyle::Japanese,
        );
        assert!(lights.len() < full.len());
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

    /// The faces of a car of `id`, drawn as `shapes`; a sedan unless it is a truck.
    fn body_of(id: u32, truck: Option<TruckShape>, shapes: CarShapes, rel: f32) -> Vec<BodyFace> {
        let mut car = Car::new(0, id, TruckStyle::Japanese, TrafficLevel::FULL);
        car.truck = truck;
        car.model = CarModel::Sedan;
        car.active = true;
        car.rel = rel;
        let mut boxes = Vec::new();
        car.push_boxes(shapes, &mut boxes);
        boxes.iter().flat_map(BodyBox::faces).collect()
    }

    fn boxes_of(id: u32, shape: TruckShape, rel: f32) -> Vec<BodyBox> {
        let mut car = shaped(id, shape, rel);
        car.lateral = match car.kind {
            Kind::Lead => OWN_LANE,
            Kind::Passing => PASSING_LANE,
            Kind::Oncoming => ONCOMING_LANES[0],
        };
        let mut boxes = Vec::new();
        car.push_boxes(CarShapes::Blocks, &mut boxes);
        boxes
    }

    fn span(faces: &[BodyFace], axis: fn(&(f32, f32, f32)) -> f32) -> f32 {
        let values = faces.iter().flat_map(|f| f.corners.iter().map(axis));
        values.clone().fold(f32::MIN, f32::max) - values.fold(f32::MAX, f32::min)
    }

    const ALL_SHAPES: [TruckShape; 4] = [
        TruckShape::JapaneseBox,
        TruckShape::JapaneseTrailer,
        TruckShape::AmericanSemi,
        TruckShape::AmericanBox,
    ];

    #[test]
    fn the_end_face_of_a_car_is_a_car_wide_and_high() {
        for id in [LEAD_ID, PASSING_ID, ONCOMING_ID] {
            let faces = body_of(id, None, CarShapes::Boxes, 50.0);
            let end = faces[0];
            assert!(end.corners.iter().all(|c| c.0 == 50.0), "{id}");
            assert!((span(&faces[..1], |c| c.1) - 1.8).abs() < 1e-4, "{id}");
            assert!((span(&faces[..1], |c| c.2) - 1.45).abs() < 1e-4, "{id}");
            // The face stands on the road, at the end of the body.
            assert_eq!(
                end.corners.iter().map(|c| c.2).fold(f32::MAX, f32::min),
                0.0
            );
        }
    }

    #[test]
    fn a_car_is_a_box_of_an_end_two_sides_and_a_roof() {
        for id in [LEAD_ID, PASSING_ID, ONCOMING_ID] {
            let faces = body_of(id, None, CarShapes::Boxes, 50.0);
            let parts: Vec<FacePart> = faces.iter().map(|f| f.part).collect();
            assert_eq!(
                parts,
                [
                    FacePart::End,
                    FacePart::Side,
                    FacePart::Side,
                    FacePart::Roof
                ],
                "{id}"
            );
            let spec = CarModel::Sedan.spec();
            let size = (spec.half_width * 2.0, spec.roof, spec.length);
            let lateral = faces[0].lateral;
            assert!(faces.iter().all(|f| f.lateral == lateral), "{id}");
            // The sides stand at the edges of the width and reach the length of the car.
            let (left, right) = (lateral - size.0 * 0.5, lateral + size.0 * 0.5);
            assert!(faces[1].corners.iter().all(|c| c.1 == left), "{id}");
            assert!(faces[2].corners.iter().all(|c| c.1 == right), "{id}");
            assert!((span(&faces[1..2], |c| c.0) - size.2).abs() < 1e-4, "{id}");
            assert!((span(&faces[1..2], |c| c.2) - size.1).abs() < 1e-4, "{id}");
            assert_eq!(faces[1].corners[0].0, 50.0, "{id}");
            // The roof is at the height of the body and covers its whole top.
            assert!(faces[3].corners.iter().all(|c| c.2 == size.1), "{id}");
            assert!((span(&faces[3..], |c| c.0) - size.2).abs() < 1e-4, "{id}");
            assert!((span(&faces[3..], |c| c.1) - size.0).abs() < 1e-4, "{id}");
        }
    }

    #[test]
    fn the_shapes_of_trucks_have_the_designed_sizes() {
        let sizes = [
            (TruckShape::JapaneseBox, 12.0, 3.8),
            (TruckShape::JapaneseTrailer, 16.5, 4.1),
            (TruckShape::AmericanSemi, 22.0, 4.1),
            (TruckShape::AmericanBox, 9.5, 3.9),
        ];
        for (shape, length, height) in sizes {
            let spec = shape.spec();
            assert_eq!((spec.length, spec.height), (length, height), "{shape:?}");
            assert_eq!(shape.length(), length);
            // The blocks fit in the length and the width and the height, touch
            // the front and the rear, and the tallest one is as high as the truck.
            for block in spec.blocks {
                assert!(block.along.0 >= 0.0 && block.along.1 <= length, "{shape:?}");
                assert!(block.along.0 < block.along.1 && block.up.0 < block.up.1);
                assert!(block.half_width <= TRUCK_HALF_WIDTH, "{shape:?}");
                assert!(block.up.1 <= height, "{shape:?}");
            }
            assert_eq!(spec.blocks.first().unwrap().along.0, 0.0, "{shape:?}");
            assert_eq!(spec.blocks.last().unwrap().along.1, length, "{shape:?}");
            assert!(spec.blocks.iter().any(|b| b.up.1 == height), "{shape:?}");
            for &axle in spec.axles {
                let reach = WHEEL_LENGTH * 0.5;
                assert!(axle - reach >= 0.0 && axle + reach <= length, "{shape:?}");
            }
        }
        // The cabs and the cargo of the designed table.
        assert_eq!(JAPANESE_BOX.blocks[0].along, (0.0, 2.3));
        assert_eq!(JAPANESE_BOX.cargo, (2.6, 12.0));
        assert_eq!(JAPANESE_TRAILER.cargo, (3.3, 16.5));
        assert_eq!(AMERICAN_SEMI.cab, (1.8, 4.0));
        assert_eq!(AMERICAN_SEMI.cargo, (5.9, 22.0));
        assert_eq!(AMERICAN_BOX.cab, (1.2, 2.6));
        assert_eq!(AMERICAN_BOX.cargo, (2.9, 9.5));
        // Axles: the wheels of the tractor and of the trailer.
        let axles = |shape: TruckShape| shape.spec().axles.len();
        assert_eq!(axles(TruckShape::JapaneseBox), 3);
        assert_eq!(axles(TruckShape::JapaneseTrailer), 6);
        assert_eq!(axles(TruckShape::AmericanSemi), 5);
        assert_eq!(axles(TruckShape::AmericanBox), 2);
    }

    #[test]
    fn a_truck_is_made_of_its_blocks_and_a_wheel_each_side_for_each_set_of_axles() {
        // The axles of a set, that are less than half a meter apart, are drawn as one.
        let sets = [
            (TruckShape::JapaneseBox, 2),
            (TruckShape::JapaneseTrailer, 3),
            (TruckShape::AmericanSemi, 3),
            (TruckShape::AmericanBox, 2),
        ];
        for (shape, sets) in sets {
            let spec = shape.spec();
            // Far from the camera, so that nothing is cut off.
            let boxes = boxes_of(PASSING_ID, shape, 100.0);
            assert_eq!(boxes.len(), spec.blocks.len() + 2 * sets, "{shape:?}");
            let wheels: Vec<&BodyBox> = boxes.iter().filter(|b| b.top == WHEEL_HEIGHT).collect();
            assert_eq!(wheels.len(), 2 * sets, "{shape:?}");
            for wheel in wheels {
                assert!(wheel.far - wheel.near >= WHEEL_LENGTH - 1e-4);
                assert!(wheel.far - wheel.near < 3.5, "{shape:?}");
                assert!((wheel.right - wheel.left - WHEEL_WIDTH).abs() < 1e-4);
                // Each wheel is flush with a side of the body.
                let edge = (wheel.left - PASSING_LANE)
                    .abs()
                    .max((wheel.right - PASSING_LANE).abs());
                assert!((edge - TRUCK_HALF_WIDTH).abs() < 1e-4, "{shape:?}");
            }
            // All of the truck lies within its length and its width.
            for b in &boxes {
                assert!(b.near >= 100.0 - 1e-4 && b.far <= 100.0 + spec.length + 1e-4);
                assert!(b.left >= PASSING_LANE - TRUCK_HALF_WIDTH - 1e-4);
                assert!(b.right <= PASSING_LANE + TRUCK_HALF_WIDTH + 1e-4);
                assert!(b.bottom >= 0.0 && b.top <= spec.height);
            }
        }
    }

    #[test]
    fn the_cab_faces_the_camera_on_an_oncoming_truck_and_the_rear_on_the_others() {
        for shape in ALL_SHAPES {
            let spec = shape.spec();
            let front_top = spec.blocks[0].up.1;
            for (id, front_first) in [(ONCOMING_ID, true), (LEAD_ID, false), (PASSING_ID, false)] {
                let boxes = boxes_of(id, shape, 100.0);
                let tops = |edge: f32, of: fn(&BodyBox) -> f32| {
                    boxes
                        .iter()
                        .filter(|b| (of(b) - edge).abs() < 1e-4)
                        .map(|b| b.top)
                        .fold((f32::MAX, f32::MIN), |(lo, hi), t| (lo.min(t), hi.max(t)))
                };
                let (nearest, farthest) = (
                    tops(100.0, |b| b.near),
                    tops(100.0 + spec.length, |b| b.far),
                );
                if front_first {
                    // The hood or the cab is nearest, and the rear of the trailer farthest.
                    assert_eq!(nearest.1, front_top, "{shape:?}");
                    assert_eq!(farthest.0, spec.blocks.last().unwrap().up.1, "{shape:?}");
                } else {
                    // The truck is seen from behind: the rear, low where the
                    // lamps stand, is nearest and the cab is farthest.
                    assert!(nearest.0 <= 1.2, "{shape:?} {id}: {}", nearest.0);
                    assert_eq!(
                        farthest.1,
                        front_top.max(spec.blocks[0].up.1),
                        "{shape:?} {id}"
                    );
                }
            }
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
        // The rear is behind the camera: the end is gone, and what is left of
        // the sides and the roof starts at the nearest distance.
        let faces = body_of(PASSING_ID, None, CarShapes::Boxes, -2.0);
        let parts: Vec<FacePart> = faces.iter().map(|f| f.part).collect();
        assert_eq!(parts, [FacePart::Side, FacePart::Side, FacePart::Roof]);
        for face in &faces {
            let near = face.corners.iter().map(|c| c.0).fold(f32::MAX, f32::min);
            let far = face.corners.iter().map(|c| c.0).fold(f32::MIN, f32::max);
            assert_eq!(
                (near, far),
                (NEAR_DISTANCE, -2.0 + CarModel::Sedan.length())
            );
        }
        // Once the whole car is behind the camera nothing is left.
        assert!(
            body_of(
                PASSING_ID,
                None,
                CarShapes::Boxes,
                -(CarModel::Sedan.length() - NEAR_DISTANCE + 0.1)
            )
            .is_empty()
        );
        let truck = Some(TruckShape::JapaneseBox);
        assert!(!body_of(PASSING_ID, truck, CarShapes::Blocks, -10.0).is_empty());
        assert!(body_of(PASSING_ID, truck, CarShapes::Blocks, -12.0).is_empty());
        // The parts of a truck that are past the camera are gone, the others cut.
        let boxes_at = |rel: f32| {
            let mut car = shaped(PASSING_ID, TruckShape::JapaneseBox, rel);
            let mut boxes = Vec::new();
            car.lateral = PASSING_LANE;
            car.push_boxes(CarShapes::Blocks, &mut boxes);
            boxes
        };
        let (whole, partly) = (boxes_at(100.0), boxes_at(-8.0));
        assert!(partly.len() < whole.len());
        assert!(
            partly
                .iter()
                .all(|b| b.near >= NEAR_DISTANCE && (b.has_end || b.near == NEAR_DISTANCE))
        );
    }

    #[test]
    fn the_body_stops_before_the_end_of_the_view() {
        for rel in [VIEW_DISTANCE - 3.0, VIEW_DISTANCE - 0.005, VIEW_DISTANCE] {
            for truck in [None, Some(TruckShape::AmericanSemi)] {
                for id in [LEAD_ID, PASSING_ID, ONCOMING_ID] {
                    for face in body_of(id, truck, CarShapes::Blocks, rel) {
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
    fn every_box_of_a_car_has_the_same_sight_point() {
        for shape in ALL_SHAPES {
            let boxes = boxes_of(PASSING_ID, shape, 100.0);
            assert!(boxes.iter().all(|b| b.sight == boxes[0].sight));
            let faces = boxes.iter().flat_map(BodyBox::faces).collect::<Vec<_>>();
            assert!(faces.iter().all(|f| f.sight == boxes[0].sight));
            // The middle of the end that faces the camera, at half the height.
            assert_eq!(
                boxes[0].sight,
                (100.0, PASSING_LANE, shape.spec().height * 0.5)
            );
        }
    }

    fn lane_of(car: &Car) -> u32 {
        (car.lateral * 100.0) as i32 as u32
    }

    /// The smallest space between two cars in a lane, in meters; negative where
    /// the bodies overlap.
    fn least_gap(traffic: &Traffic) -> f32 {
        let mut least = f32::MAX;
        for (i, a) in traffic.cars.iter().enumerate() {
            for b in &traffic.cars[i + 1..] {
                if lane_of(a) != lane_of(b) {
                    continue;
                }
                let (first, second) = if a.rel < b.rel { (a, b) } else { (b, a) };
                least = least.min(second.rel - (first.rel + first.length()));
            }
        }
        least
    }

    #[test]
    fn cars_that_keep_apart_never_overlap_in_a_lane() {
        for style in [
            TruckStyle::None,
            TruckStyle::Japanese,
            TruckStyle::NorthAmerican,
        ] {
            for seed in 0..4 {
                let mut traffic =
                    Traffic::with_shapes(seed, style, TrafficLevel::FULL, CarShapes::Blocks);
                assert!(
                    least_gap(&traffic) >= LANE_GAP - 1e-3,
                    "{style:?} {seed} start"
                );
                let mut worst = f32::MAX;
                run(&mut traffic, 72_000, |t| {
                    worst = worst.min(least_gap(t));
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
                });
                assert!(worst >= LANE_GAP - 1e-3, "{style:?} {seed}: {worst}");
            }
        }
    }

    #[test]
    fn cars_that_keep_apart_neither_jump_nor_change_speed_suddenly_in_view() {
        // Seen from the camera, a car moves by its speed times the step, and
        // its speed changes by `LANE_SLEW` per second at most. (A car that is
        // placed again is not seen to jump: it was not in view, or it is the
        // lead car that is placed in the distance, as ever.) The limit allows
        // for the rounding of positions of hundreds of meters.
        let limit = LANE_SLEW * DT * 1.05 + 0.02;
        let (mut checked, mut steepest) = (0, 0.0f32);
        for style in [TruckStyle::None, TruckStyle::Japanese] {
            for seed in 0..3 {
                let mut traffic =
                    Traffic::with_shapes(seed, style, TrafficLevel::FULL, CarShapes::Blocks);
                // The position, the speed (0 if not known) and the generation of each car.
                let mut last: Vec<(f32, f32, u32)> = traffic
                    .cars
                    .iter()
                    .map(|c| (c.rel, 0.0, c.generation))
                    .collect();
                run(&mut traffic, 72_000, |t| {
                    for (car, last) in t.cars.iter().zip(&mut last) {
                        let velocity = (car.rel - last.0) / DT;
                        let in_view =
                            car.rel + car.length() > NEAR_DISTANCE && car.rel < VIEW_DISTANCE;
                        let same = car.generation == last.2;
                        if same && in_view && last.1 != 0.0 {
                            let change = (velocity - last.1).abs();
                            steepest = steepest.max(change);
                            assert!(
                                change <= limit,
                                "{style:?} {seed}: car {} {change} at {}",
                                car.id,
                                car.rel
                            );
                            checked += 1;
                        }
                        // A car that has been placed again has no speed to compare with.
                        *last = (car.rel, if same { velocity } else { 0.0 }, car.generation);
                    }
                });
            }
        }
        assert!(checked > 100_000, "{checked}");
        assert!(steepest > 0.0);
    }

    #[test]
    fn cars_keep_apart_even_when_every_car_is_a_long_truck() {
        let level = TrafficLevel {
            active: 1.0,
            trucks: 1.0,
        };
        for style in [TruckStyle::Japanese, TruckStyle::NorthAmerican] {
            for seed in 0..2 {
                let mut traffic = Traffic::with_shapes(seed, style, level, CarShapes::Blocks);
                assert!(traffic.cars.iter().all(|c| c.truck.is_some()));
                let mut worst = f32::MAX;
                run(&mut traffic, 72_000, |t| worst = worst.min(least_gap(t)));
                assert!(worst >= LANE_GAP - 1e-3, "{style:?} {seed}: {worst}");
            }
        }
    }

    #[test]
    fn cars_that_do_not_keep_apart_overlap_as_before() {
        // Without bodies the traffic is not changed: its cars pass through each other.
        let mut traffic = Traffic::new(3, TruckStyle::Japanese);
        let mut overlapped = false;
        run(&mut traffic, 72_000, |t| overlapped |= least_gap(t) < 0.0);
        assert!(overlapped);
    }

    #[test]
    fn keeping_apart_keeps_the_traffic_busy() {
        // The convoys that form are not a few cars in a row: most of the cars
        // stay in view as before.
        let mut visible = 0;
        let mut samples = 0;
        let mut traffic = Traffic::with_shapes(
            3,
            TruckStyle::Japanese,
            TrafficLevel::FULL,
            CarShapes::Blocks,
        );
        run(&mut traffic, 72_000, |t| {
            visible += t
                .cars
                .iter()
                .filter(|c| (NEAR_DISTANCE..VIEW_DISTANCE).contains(&c.rel))
                .count();
            samples += 1;
        });
        let mean = visible as f32 / samples as f32;
        assert!((8.0..22.0).contains(&mean), "{mean}");
    }

    #[test]
    fn shapes_follow_the_style_and_the_designed_shares() {
        for (style, shapes, share) in [
            (TruckStyle::Japanese, JAPANESE, 0.6),
            (TruckStyle::NorthAmerican, AMERICAN, 0.7),
        ] {
            let mut counts = [0u32; 4];
            for seed in 0..200 {
                let traffic = Traffic::with_level(
                    seed,
                    style,
                    TrafficLevel {
                        active: 1.0,
                        trucks: 1.0,
                    },
                );
                for shape in traffic.cars.iter().filter_map(|c| c.truck) {
                    assert!(shapes.contains(&shape), "{style:?} {shape:?}");
                    counts[ALL_SHAPES.iter().position(|&s| s == shape).unwrap()] += 1;
                }
            }
            let total: u32 = counts.iter().sum();
            let first = shapes[0];
            let ratio =
                counts[ALL_SHAPES.iter().position(|&s| s == first).unwrap()] as f32 / total as f32;
            assert!(
                total == 200 * 25 && (ratio - share).abs() < 0.03,
                "{style:?} {ratio}"
            );
        }
        assert_eq!(TruckShape::pick(TruckStyle::None, 0.0), None);
    }

    #[test]
    fn car_body_choices_give_their_opacity() {
        assert_eq!(CarColor::TranslucentBlack.opacity(), 0.9);
        assert_eq!(CarColor::Black.opacity(), 1.0);
    }

    /// A car that is not a truck of `model`, as the kind of car of `id`, at `rel`.
    fn model_car(id: u32, model: CarModel, rel: f32) -> Car {
        let mut car = Car::new(0, id, TruckStyle::Japanese, TrafficLevel::FULL);
        car.truck = None;
        car.model = model;
        car.active = true;
        car.rel = rel;
        car.lateral = match car.kind {
            Kind::Lead => OWN_LANE,
            Kind::Passing => PASSING_LANE,
            Kind::Oncoming => ONCOMING_LANES[0],
        };
        car.braking_left = 0.0;
        car
    }

    fn model_boxes(car: &Car, shapes: CarShapes) -> Vec<BodyBox> {
        let mut boxes = Vec::new();
        car.push_boxes(shapes, &mut boxes);
        boxes
    }

    #[test]
    fn the_models_have_the_designed_sizes() {
        // (length, width, body from, body to, cabin from, cabin to, roof, wheel)
        let designed = [
            (CarModel::Sedan, 4.7, 1.8, 0.15, 0.95, 1.6, 3.7, 1.45, 0.65),
            (
                CarModel::Hatchback,
                4.0,
                1.75,
                0.15,
                0.95,
                1.4,
                3.9,
                1.5,
                0.6,
            ),
            (CarModel::Minivan, 4.8, 1.8, 0.15, 1.0, 1.0, 4.8, 1.85, 0.65),
            (CarModel::Suv, 4.6, 1.85, 0.2, 1.1, 1.5, 4.4, 1.75, 0.75),
            (CarModel::Kei, 3.4, 1.48, 0.15, 0.9, 0.8, 3.4, 1.75, 0.55),
            (CarModel::Pickup, 5.9, 2.0, 0.22, 1.2, 1.9, 3.6, 1.95, 0.8),
        ];
        for (model, length, width, from, to, cabin_from, cabin_to, roof, wheel) in designed {
            let spec = model.spec();
            assert_eq!(model.length(), length, "{model:?}");
            assert!((spec.half_width * 2.0 - width).abs() < 1e-4, "{model:?}");
            assert_eq!(spec.body, (from, to), "{model:?}");
            assert_eq!(spec.cabin, (cabin_from, cabin_to), "{model:?}");
            assert_eq!((spec.roof, spec.wheel), (roof, wheel), "{model:?}");
        }
    }

    #[test]
    fn a_block_car_is_a_body_a_cabin_and_four_wheels_inside_its_size() {
        for model in CarModel::ALL {
            let spec = model.spec();
            for id in [LEAD_ID, PASSING_ID, ONCOMING_ID] {
                let car = model_car(id, model, 100.0);
                let boxes = model_boxes(&car, CarShapes::Blocks);
                assert_eq!(boxes.len(), 6, "{model:?} {id}");
                for b in &boxes {
                    let values = [b.near, b.far, b.left, b.right, b.bottom, b.top];
                    assert!(values.iter().all(|v| v.is_finite()), "{model:?} {id}");
                    assert!(b.near >= 100.0 - 1e-4 && b.far <= 100.0 + spec.length + 1e-4);
                    assert!(b.left >= car.lateral - spec.half_width - 1e-4);
                    assert!(b.right <= car.lateral + spec.half_width + 1e-4);
                    assert!(b.bottom >= 0.0 && b.top <= spec.roof + 1e-4);
                    assert!(b.near < b.far && b.left < b.right && b.bottom < b.top);
                    assert!(b.has_end, "{model:?} {id}");
                    for face in b.faces() {
                        let all = face.corners.iter().flat_map(|c| [c.0, c.1, c.2]);
                        assert!(all.into_iter().all(f32::is_finite), "{model:?} {id}");
                    }
                }
                // The body spans the car, the cabin stands on it and the
                // wheels reach the road.
                let body = &boxes[0];
                assert_eq!((body.near, body.far), (100.0, 100.0 + spec.length));
                assert_eq!((body.bottom, body.top), spec.body);
                let cabin = &boxes[1];
                assert_eq!(cabin.bottom, spec.body.1);
                assert_eq!(cabin.top, spec.roof);
                assert!(
                    (cabin.right - cabin.left - (spec.half_width - CABIN_INSET) * 2.0).abs() < 1e-4
                );
                assert!(
                    boxes[2..]
                        .iter()
                        .all(|w| w.bottom == 0.0 && w.top == spec.wheel)
                );
                // The two wheels of an axle leave the road between them open.
                for pair in boxes[2..].chunks(2) {
                    assert_eq!((pair[0].near, pair[0].far), (pair[1].near, pair[1].far));
                    assert!(pair[1].left - pair[0].right > 0.5, "{model:?} {id}");
                    assert!((pair[0].right - pair[0].left - CAR_WHEEL_WIDTH).abs() < 1e-4);
                }
                // Only the body casts the shadow, and it is the shadow of the whole car.
                assert_eq!(body.shadow, Some((0.0, spec.roof)));
                assert!(boxes[1..].iter().all(|b| b.shadow.is_none()));
            }
        }
    }

    #[test]
    fn the_cabin_faces_the_camera_the_way_the_car_does() {
        // The pickup truck has its cabin 1.9 to 3.6 meters from the front and
        // its bed behind it: an oncoming one shows the front, the others the bed.
        let spec = CarModel::Pickup.spec();
        let oncoming = model_boxes(
            &model_car(ONCOMING_ID, CarModel::Pickup, 100.0),
            CarShapes::Blocks,
        );
        let (near, far) = (oncoming[1].near, oncoming[1].far);
        assert!((near - 101.9).abs() < 1e-4 && (far - 103.6).abs() < 1e-4);
        assert_eq!(oncoming[0].covered, Some((101.9, 103.6)));
        for id in [LEAD_ID, PASSING_ID] {
            let boxes = model_boxes(&model_car(id, CarModel::Pickup, 100.0), CarShapes::Blocks);
            let (near, far) = (boxes[1].near, boxes[1].far);
            let (from, to) = (100.0 + spec.length - 3.6, 100.0 + spec.length - 1.9);
            assert!(
                (near - from).abs() < 1e-4 && (far - to).abs() < 1e-4,
                "{id}"
            );
            assert_eq!(boxes[0].covered, Some((from, to)));
        }
    }

    #[test]
    fn the_roof_of_the_body_is_left_out_where_the_cabin_stands() {
        for model in CarModel::ALL {
            let spec = model.spec();
            let car = model_car(PASSING_ID, model, 100.0);
            let boxes = model_boxes(&car, CarShapes::Blocks);
            let (hole_from, hole_to) = boxes[0].covered.unwrap();
            let roofs: Vec<BodyFace> = boxes[0]
                .faces()
                .into_iter()
                .filter(|f| f.part == FacePart::Roof)
                .collect();
            let mut covered = 0.0;
            for roof in &roofs {
                let (near, far) = (
                    roof.corners.iter().map(|c| c.0).fold(f32::MAX, f32::min),
                    roof.corners.iter().map(|c| c.0).fold(f32::MIN, f32::max),
                );
                // No piece of the roof reaches into the hole.
                assert!(
                    far <= hole_from + 1e-4 || near >= hole_to - 1e-4,
                    "{model:?}"
                );
                covered += far - near;
            }
            let expected = spec.length - (spec.cabin.1 - spec.cabin.0);
            assert!((covered - expected).abs() < 1e-3, "{model:?} {covered}");
            // A box that has nothing on it has its whole roof.
            let solid = model_boxes(&car, CarShapes::Boxes);
            assert_eq!(solid.len(), 1);
            let whole = solid[0]
                .faces()
                .into_iter()
                .filter(|f| f.part == FacePart::Roof)
                .count();
            assert_eq!(whole, 1, "{model:?}");
        }
    }

    #[test]
    fn a_simple_box_holds_the_whole_shape_of_a_car_or_a_truck() {
        for model in CarModel::ALL {
            let spec = model.spec();
            let boxes = model_boxes(&model_car(LEAD_ID, model, 100.0), CarShapes::Boxes);
            assert_eq!(boxes.len(), 1, "{model:?}");
            let b = &boxes[0];
            assert_eq!((b.near, b.far), (100.0, 100.0 + spec.length));
            assert!((b.right - b.left - spec.half_width * 2.0).abs() < 1e-4);
            assert_eq!((b.bottom, b.top), (0.0, spec.roof));
            assert_eq!(b.shadow, Some((0.0, spec.roof)));
            assert_eq!(b.covered, None);
        }
        for shape in ALL_SHAPES {
            let mut car = shaped(LEAD_ID, shape, 100.0);
            car.lateral = OWN_LANE;
            let boxes = model_boxes(&car, CarShapes::Boxes);
            assert_eq!(boxes.len(), 1, "{shape:?}");
            let b = &boxes[0];
            assert_eq!((b.near, b.far), (100.0, 100.0 + shape.length()));
            assert!((b.right - b.left - TRUCK_HALF_WIDTH * 2.0).abs() < 1e-4);
            assert_eq!((b.bottom, b.top), (0.0, shape.spec().height));
        }
    }

    #[test]
    fn every_truck_is_the_same_with_the_shapes_but_for_the_shadow_rule() {
        // The shadow of a part of a truck is cast by it if it is high enough.
        for shape in ALL_SHAPES {
            let boxes = boxes_of(PASSING_ID, shape, 100.0);
            for b in &boxes {
                assert_eq!(b.shadow.is_some(), b.top > LOW_BOX, "{shape:?}");
                if let Some(heights) = b.shadow {
                    assert_eq!(heights, (b.bottom, b.top));
                }
            }
        }
    }

    #[test]
    fn car_models_follow_the_designed_shares() {
        // (style, shares of sedan, hatchback, minivan, SUV, kei, pickup)
        let designed = [
            (TruckStyle::Japanese, [0.25, 0.20, 0.25, 0.15, 0.15, 0.0]),
            (
                TruckStyle::NorthAmerican,
                [0.30, 0.10, 0.15, 0.30, 0.0, 0.15],
            ),
            (TruckStyle::None, [0.30, 0.25, 0.25, 0.20, 0.0, 0.0]),
        ];
        for (style, shares) in designed {
            let mut counts = [0u32; 6];
            for seed in 0..300 {
                let traffic = Traffic::with_level(
                    seed,
                    style,
                    TrafficLevel {
                        active: 1.0,
                        trucks: 0.0,
                    },
                );
                for car in &traffic.cars {
                    assert!(car.truck.is_none());
                    counts[CarModel::ALL.iter().position(|&m| m == car.model).unwrap()] += 1;
                }
            }
            let total: u32 = counts.iter().sum();
            assert_eq!(total, 300 * 25);
            for (index, share) in shares.into_iter().enumerate() {
                let ratio = counts[index] as f32 / total as f32;
                assert!(
                    (ratio - share).abs() < 0.03,
                    "{style:?} {:?}: {ratio}",
                    CarModel::ALL[index]
                );
                if share == 0.0 {
                    assert_eq!(counts[index], 0, "{style:?} {:?}", CarModel::ALL[index]);
                }
            }
        }
    }

    #[test]
    fn the_models_are_picked_by_the_cumulative_shares() {
        assert_eq!(CarModel::pick(TruckStyle::None, 0.0), CarModel::Sedan);
        assert_eq!(CarModel::pick(TruckStyle::None, 0.29), CarModel::Sedan);
        assert_eq!(CarModel::pick(TruckStyle::None, 0.31), CarModel::Hatchback);
        assert_eq!(CarModel::pick(TruckStyle::None, 0.999), CarModel::Suv);
        assert_eq!(CarModel::pick(TruckStyle::Japanese, 0.999), CarModel::Kei);
        assert_eq!(
            CarModel::pick(TruckStyle::NorthAmerican, 0.999),
            CarModel::Pickup
        );
        // The roll just below 1 is the last model, even where rounding of the
        // shares leaves a sliver.
        let last = f32::from_bits(1.0f32.to_bits() - 1);
        assert_eq!(CarModel::pick(TruckStyle::None, last), CarModel::Suv);
        assert_eq!(CarModel::pick(TruckStyle::Japanese, last), CarModel::Kei);
        assert_eq!(
            CarModel::pick(TruckStyle::NorthAmerican, last),
            CarModel::Pickup
        );
        // Whatever the roll, the model exists.
        for style in TruckStyle::ALL {
            for step in 0..=100 {
                let _ = CarModel::pick(style, step as f32 / 100.0);
            }
        }
    }

    #[test]
    fn the_shapes_do_not_change_where_the_cars_are() {
        for style in TruckStyle::ALL {
            for seed in 0..3 {
                let mut boxes =
                    Traffic::with_shapes(seed, style, TrafficLevel::FULL, CarShapes::Boxes);
                let mut detailed =
                    Traffic::with_shapes(seed, style, TrafficLevel::FULL, CarShapes::Blocks);
                for step in 0..4000 {
                    boxes.update(DT, OWN_SPEED);
                    detailed.update(DT, OWN_SPEED);
                    if step % 20 == 0 {
                        assert_eq!(
                            snapshot(&boxes),
                            snapshot(&detailed),
                            "{style:?} {seed} {step}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn lights_of_a_car_stand_on_its_surface_and_not_inside_it() {
        for model in CarModel::ALL {
            let spec = model.spec();
            for id in [LEAD_ID, PASSING_ID, ONCOMING_ID] {
                for braking in [0.0, 1.0] {
                    let mut car = model_car(id, model, 20.0);
                    car.braking_left = braking;
                    let boxes = model_boxes(&car, CarShapes::Blocks);
                    let mut lights = Vec::new();
                    car.push_lights(TruckStyle::Japanese, CarShapes::Blocks, &mut lights);
                    assert!(lights.len() >= 2, "{model:?} {id}");
                    for light in &lights {
                        let (distance, inside) = lamp_clearance(&boxes, light);
                        assert!(!inside, "{model:?} {id}: {light:?}");
                        assert!(distance < 1e-3, "{model:?} {id}: {distance} {light:?}");
                        // The lamps are over the body, not under it.
                        assert!(light.height > spec.body.0 && light.height < spec.roof);
                        assert!(
                            (light.lateral - car.lateral).abs()
                                <= spec.half_width - LAMP_INSET + 1e-4
                        );
                    }
                    // Lamps of a car are at its end that faces the camera, but the high lamp.
                    let high = lights.iter().filter(|l| l.height > spec.body.1).count();
                    assert_eq!(
                        high,
                        usize::from(braking > 0.0 && id != ONCOMING_ID),
                        "{model:?} {id}"
                    );
                }
            }
        }
    }

    #[test]
    fn lights_of_a_car_without_bodies_are_where_they_were() {
        for id in [LEAD_ID, PASSING_ID, ONCOMING_ID] {
            let mut car = model_car(id, CarModel::Minivan, 100.0);
            car.braking_left = 1.0;
            let mut lights = Vec::new();
            car.push_lights(TruckStyle::Japanese, CarShapes::None, &mut lights);
            let heights: Vec<f32> = lights.iter().map(|l| l.height).collect();
            let expected: &[f32] = if id == ONCOMING_ID {
                &[0.65, 0.65]
            } else {
                &[0.8, 0.8, 1.15]
            };
            assert_eq!(heights, expected, "{id}");
            let sides: Vec<f32> = lights
                .iter()
                .take(2)
                .map(|l| l.lateral - car.lateral)
                .collect();
            assert!((sides[0] + 0.7).abs() < 1e-5 && (sides[1] - 0.7).abs() < 1e-5);
        }
    }

    #[test]
    fn the_bodies_of_cars_in_a_lane_never_overlap() {
        for style in TruckStyle::ALL {
            for seed in 0..3 {
                let mut traffic =
                    Traffic::with_shapes(seed, style, TrafficLevel::FULL, CarShapes::Blocks);
                run(&mut traffic, 12_000, |t| {
                    let mut lanes: Vec<Vec<(f32, f32)>> = vec![Vec::new(); 4];
                    for car in t.cars.iter().filter(|c| c.active) {
                        let lane = [OWN_LANE, PASSING_LANE, ONCOMING_LANES[0], ONCOMING_LANES[1]]
                            .iter()
                            .position(|&l| l == car.lateral)
                            .unwrap();
                        let boxes = model_boxes(car, CarShapes::Blocks);
                        if boxes.is_empty() {
                            continue;
                        }
                        let near = boxes.iter().map(|b| b.near).fold(f32::MAX, f32::min);
                        let far = boxes.iter().map(|b| b.far).fold(f32::MIN, f32::max);
                        lanes[lane].push((near, far));
                    }
                    for lane in &mut lanes {
                        lane.sort_by(|a, b| a.0.total_cmp(&b.0));
                        for pair in lane.windows(2) {
                            assert!(pair[0].1 <= pair[1].0, "{style:?} {seed}: {pair:?}");
                        }
                    }
                });
            }
        }
    }

    /// Whether the four corners of a face lie in one plane.
    fn is_flat(corners: &[(f32, f32, f32); 4]) -> bool {
        let sub = |a: (f32, f32, f32), b: (f32, f32, f32)| (b.0 - a.0, b.1 - a.1, b.2 - a.2);
        let cross = |a: (f32, f32, f32), b: (f32, f32, f32)| {
            (
                a.1 * b.2 - a.2 * b.1,
                a.2 * b.0 - a.0 * b.2,
                a.0 * b.1 - a.1 * b.0,
            )
        };
        let length = |v: (f32, f32, f32)| (v.0 * v.0 + v.1 * v.1 + v.2 * v.2).sqrt();
        // The widest triangle of three corners gives the plane.
        let triples = [[0, 1, 2], [0, 1, 3], [0, 2, 3], [1, 2, 3]];
        let widest = triples
            .iter()
            .map(|t| {
                cross(
                    sub(corners[t[0]], corners[t[1]]),
                    sub(corners[t[0]], corners[t[2]]),
                )
            })
            .max_by(|a, b| length(*a).total_cmp(&length(*b)))
            .unwrap();
        let size = length(widest);
        if size < 1e-6 {
            // All on a line: nothing to see.
            return true;
        }
        let normal = (widest.0 / size, widest.1 / size, widest.2 / size);
        corners.iter().all(|c| {
            let v = sub(corners[0], *c);
            (v.0 * normal.0 + v.1 * normal.1 + v.2 * normal.2).abs() < 1e-3
        })
    }

    fn all_faces_are_flat_and_finite(boxes: &[BodyBox], what: &str) {
        for b in boxes {
            for face in b.faces() {
                let all = face.corners.iter().flat_map(|c| [c.0, c.1, c.2]);
                assert!(all.into_iter().all(f32::is_finite), "{what}");
                assert!(is_flat(&face.corners), "{what}: {face:?}");
            }
        }
    }

    #[test]
    fn a_detailed_car_is_one_solid_and_four_wheels_inside_its_size() {
        for model in CarModel::ALL {
            let spec = model.spec();
            for id in [LEAD_ID, PASSING_ID, ONCOMING_ID] {
                let what = format!("{model:?} {id}");
                let car = model_car(id, model, 100.0);
                let boxes = model_boxes(&car, CarShapes::Detailed);
                assert_eq!(boxes.len(), 5, "{what}");
                assert!(boxes[0].solid.is_some(), "{what}");
                assert!(boxes[1..].iter().all(|b| b.solid.is_none()), "{what}");
                all_faces_are_flat_and_finite(&boxes, &what);
                let body = &boxes[0];
                assert_eq!(
                    (body.near, body.far),
                    (100.0, 100.0 + spec.length),
                    "{what}"
                );
                assert!((body.top - spec.roof).abs() < 1e-4, "{what}");
                assert_eq!(body.bottom, spec.body.0, "{what}");
                assert!(body.left >= car.lateral - spec.half_width - 1e-4, "{what}");
                assert!(body.right <= car.lateral + spec.half_width + 1e-4, "{what}");
                // It stands on the underside: the outline never goes below it.
                assert!(spec.outline.iter().all(|p| p.1 > spec.body.0), "{what}");
                assert!(spec.outline.windows(2).all(|w| w[0].0 <= w[1].0), "{what}");
                assert_eq!(spec.outline[0].0, 0.0);
                assert_eq!(spec.outline[spec.outline.len() - 1].0, spec.length);
                assert!(body.shadow.is_some(), "{what}");
            }
        }
    }

    #[test]
    fn the_end_of_a_detailed_car_that_faces_the_camera_is_its_front_or_its_rear() {
        for model in CarModel::ALL {
            let spec = model.spec();
            let (front, rear) = (spec.outline[0].1, spec.outline[spec.outline.len() - 1].1);
            for (id, expected) in [(LEAD_ID, rear), (PASSING_ID, rear), (ONCOMING_ID, front)] {
                let car = model_car(id, model, 100.0);
                let boxes = model_boxes(&car, CarShapes::Detailed);
                let ends: Vec<BodyFace> = boxes[0]
                    .faces()
                    .into_iter()
                    .filter(|f| f.part == FacePart::End && f.corners.iter().all(|c| c.0 == 100.0))
                    .collect();
                assert_eq!(ends.len(), 1, "{model:?} {id}");
                let top = ends[0].corners.iter().map(|c| c.2).fold(f32::MIN, f32::max);
                assert!(
                    (top - expected).abs() < 1e-4,
                    "{model:?} {id}: {top} {expected}"
                );
            }
        }
    }

    #[test]
    fn a_detailed_car_has_windows_and_a_roof_narrower_than_the_body() {
        for model in CarModel::ALL {
            let car = model_car(PASSING_ID, model, 100.0);
            let boxes = model_boxes(&car, CarShapes::Detailed);
            let faces = boxes[0].faces();
            assert!(faces.iter().any(|f| f.part == FacePart::Glass), "{model:?}");
            assert!(faces.iter().any(|f| f.part == FacePart::Roof), "{model:?}");
            assert!(faces.len() > 15, "{model:?}: {}", faces.len());
            // Above the shoulder the sides lean in: the highest corners are
            // nearer the middle than the lowest.
            let half = |height: f32| {
                faces
                    .iter()
                    .flat_map(|f| f.corners)
                    .filter(|c| (c.2 - height).abs() < 1e-3)
                    .map(|c| (c.1 - car.lateral).abs())
                    .fold(0.0, f32::max)
            };
            let spec = model.spec();
            assert!(half(spec.roof) < half(spec.body.1) - 0.1, "{model:?}");
        }
    }

    #[test]
    fn detailed_trucks_have_outlines_for_the_cab_and_boxes_for_the_rest() {
        for shape in [
            TruckShape::JapaneseBox,
            TruckShape::JapaneseTrailer,
            TruckShape::AmericanSemi,
            TruckShape::AmericanBox,
        ] {
            let spec = shape.spec();
            for id in [LEAD_ID, PASSING_ID, ONCOMING_ID] {
                let what = format!("{shape:?} {id}");
                let mut car = shaped(id, shape, 100.0);
                car.lateral = match car.kind {
                    Kind::Lead => OWN_LANE,
                    Kind::Passing => PASSING_LANE,
                    Kind::Oncoming => ONCOMING_LANES[0],
                };
                let mut boxes = Vec::new();
                car.push_boxes(CarShapes::Detailed, &mut boxes);
                let blocks = boxes.iter().filter(|b| b.solid.is_none()).count();
                let solids = boxes.iter().filter(|b| b.solid.is_some()).count();
                assert_eq!(solids, spec.solids.len(), "{what}");
                let mut plain = Vec::new();
                car.push_boxes(CarShapes::Blocks, &mut plain);
                assert_eq!(blocks + spec.solid_blocks, plain.len(), "{what}");
                all_faces_are_flat_and_finite(&boxes, &what);
                for b in &boxes {
                    assert!(
                        b.near >= 100.0 - 1e-4 && b.far <= 100.0 + spec.length + 1e-4,
                        "{what}"
                    );
                    assert!(b.top <= spec.height + 1e-4 && b.bottom >= 0.0, "{what}");
                    assert!(b.left >= car.lateral - TRUCK_HALF_WIDTH - 1e-4, "{what}");
                    assert!(b.right <= car.lateral + TRUCK_HALF_WIDTH + 1e-4, "{what}");
                }
                // The shapes keep the lengths and the heights of the boxes.
                let (near, far) = boxes.iter().fold((f32::MAX, f32::MIN), |(n, f), b| {
                    (n.min(b.near), f.max(b.far))
                });
                assert!((near, far) == (100.0, 100.0 + spec.length), "{what}");
                assert!(
                    (boxes.iter().map(|b| b.top).fold(0.0, f32::max) - spec.height).abs() < 1e-4,
                    "{what}"
                );
            }
        }
    }

    #[test]
    fn the_front_of_a_japanese_cab_leans_back_and_its_corners_and_roof_edge_are_cut() {
        let car = {
            let mut car = shaped(ONCOMING_ID, TruckShape::JapaneseBox, 100.0);
            car.lateral = ONCOMING_LANES[0];
            car
        };
        let mut boxes = Vec::new();
        car.push_boxes(CarShapes::Detailed, &mut boxes);
        let cab = boxes.iter().find_map(|b| b.solid).unwrap();
        let faces = cab.faces();
        // The front face stands at a slant: its top is farther than its foot.
        let front: Vec<_> = faces
            .iter()
            .filter(|(part, c)| {
                *part == FacePart::Glass && c.iter().all(|p| (p.1 - ONCOMING_LANES[0]).abs() < 1.2)
            })
            .collect();
        assert!(!front.is_empty());
        let lean = front[0].1.iter().map(|c| c.0).fold(f32::MIN, f32::max) - 100.0;
        assert!(lean > 0.05 && lean < 0.25, "{lean}");
        // The vertical corners are cut: the front is narrower than the body.
        let width = |d: f32| {
            faces
                .iter()
                .flat_map(|f| f.1)
                .filter(|c| (c.0 - d).abs() < 1e-3)
                .map(|c| (c.1 - car.lateral).abs())
                .fold(0.0, f32::max)
        };
        assert!(width(100.0) < TRUCK_HALF_WIDTH - 0.1, "{}", width(100.0));
        assert!(width(100.4) > TRUCK_HALF_WIDTH - 0.01);
        // The deflector on the roof rises toward the rear to the height of the box.
        let top = |solid: &BodyBox| solid.top;
        assert!((boxes.iter().map(top).fold(0.0, f32::max) - 3.8).abs() < 1e-4);
    }

    #[test]
    fn cars_far_away_are_blocks_and_near_ones_have_outlines() {
        for (rel, detailed) in [
            (60.0, true),
            (DETAIL_RANGE - 1.0, true),
            (DETAIL_RANGE + 1.0, false),
        ] {
            let car = model_car(LEAD_ID, CarModel::Sedan, rel);
            let boxes = model_boxes(&car, CarShapes::Detailed);
            assert_eq!(boxes.iter().any(|b| b.solid.is_some()), detailed, "{rel}");
            assert_eq!(boxes.len(), if detailed { 5 } else { 6 }, "{rel}");
            let mut truck = shaped(LEAD_ID, TruckShape::JapaneseBox, rel);
            truck.lateral = OWN_LANE;
            let mut out = Vec::new();
            truck.push_boxes(CarShapes::Detailed, &mut out);
            assert_eq!(out.iter().any(|b| b.solid.is_some()), detailed, "{rel}");
        }
        // Without detail the shapes are the blocks, whatever the distance.
        let car = model_car(LEAD_ID, CarModel::Sedan, 60.0);
        assert!(
            model_boxes(&car, CarShapes::Blocks)
                .iter()
                .all(|b| b.solid.is_none())
        );
    }

    /// Whether a point is inside a body, in a box or in a solid.
    fn inside_body(boxes: &[BodyBox], d: f32, lateral: f32, height: f32) -> bool {
        boxes.iter().any(|b| match &b.solid {
            Some(solid) => solid.contains(d, lateral, height),
            None => {
                let within =
                    |value: f32, low: f32, high: f32| value > low + 1e-3 && value < high - 1e-3;
                within(d, b.near, b.far)
                    && within(lateral, b.left, b.right)
                    && within(height, b.bottom, b.top)
            }
        })
    }

    #[test]
    fn the_lamps_of_detailed_cars_stand_on_the_outline_and_not_inside_it() {
        for model in CarModel::ALL {
            for id in [LEAD_ID, PASSING_ID, ONCOMING_ID] {
                let mut car = model_car(id, model, 100.0);
                car.braking_left = 1.0;
                let boxes = model_boxes(&car, CarShapes::Detailed);
                let mut lights = Vec::new();
                car.push_lights(TruckStyle::Japanese, CarShapes::Detailed, &mut lights);
                assert!(lights.len() >= 2);
                for light in lights.iter().filter(|l| !l.glare) {
                    let what = format!("{model:?} {id} {light:?}");
                    // A lamp on the surface is a hair in front of it, as seen from
                    // the camera.
                    assert!(
                        !inside_body(&boxes, light.d - 0.003, light.lateral, light.height),
                        "{what}"
                    );
                    // Just under the lamp, into the car, is the body.
                    assert!(
                        inside_body(&boxes, light.d + 0.2, light.lateral, light.height),
                        "{what}"
                    );
                }
            }
        }
    }

    #[test]
    fn the_lamps_of_detailed_trucks_are_not_inside_them_and_not_far_from_them() {
        for shape in [
            TruckShape::JapaneseBox,
            TruckShape::JapaneseTrailer,
            TruckShape::AmericanSemi,
            TruckShape::AmericanBox,
        ] {
            for id in [LEAD_ID, PASSING_ID, ONCOMING_ID] {
                let mut car = shaped(id, shape, 100.0);
                car.lateral = match car.kind {
                    Kind::Lead => OWN_LANE,
                    Kind::Passing => PASSING_LANE,
                    Kind::Oncoming => ONCOMING_LANES[0],
                };
                let mut boxes = Vec::new();
                car.push_boxes(CarShapes::Detailed, &mut boxes);
                let style = style_of(shape);
                let mut lights = Vec::new();
                car.push_lights(style, CarShapes::Detailed, &mut lights);
                for light in lights.iter().filter(|l| !l.glare) {
                    let what = format!("{shape:?} {id} {light:?}");
                    // A lamp on the surface is a hair in front of it, as seen from
                    // the camera.
                    assert!(
                        !inside_body(&boxes, light.d - 0.003, light.lateral, light.height),
                        "{what}"
                    );
                    let (gap, _) = lamp_clearance(&boxes, light);
                    assert!(gap < 0.35, "{what}: {gap}");
                }
            }
        }
    }

    #[test]
    fn the_detailed_shapes_are_finite_for_an_hour_of_driving() {
        for seed in 1..=3 {
            for style in [
                TruckStyle::None,
                TruckStyle::Japanese,
                TruckStyle::NorthAmerican,
            ] {
                let mut traffic =
                    Traffic::with_shapes(seed, style, TrafficLevel::FULL, CarShapes::Detailed);
                let mut solids = 0;
                for step in 0..7200 {
                    traffic.update(0.5, OWN_SPEED);
                    if step % 4 != 0 {
                        continue;
                    }
                    let boxes = traffic.boxes();
                    solids += boxes.iter().filter(|b| b.solid.is_some()).count();
                    all_faces_are_flat_and_finite(&boxes, &format!("{seed} {step}"));
                }
                assert!(solids > 200, "{solids}");
            }
        }
    }
}

#[cfg(test)]
mod golden {
    use super::*;

    /// A hash of where the cars of the traffic are, over a drive. The traffic
    /// of cars that do not keep apart is that of v1.8.0, which the hash below was
    /// taken from; if this fails, a transparent picture is no longer the old one.
    fn positions_hash(style: TruckStyle, seed: u64) -> u64 {
        let mut traffic = Traffic::new(seed, style);
        let mut hash = 0xcbf2_9ce4_8422_2325u64;
        for step in 0..6000 {
            traffic.update(0.05, 100.0 / 3.6);
            if step % 100 == 99 {
                for car in &traffic.cars {
                    hash = (hash ^ u64::from(car.rel.to_bits())).wrapping_mul(0x100_0000_01b3);
                    hash = (hash ^ u64::from(car.lateral.to_bits())).wrapping_mul(0x100_0000_01b3);
                    hash = (hash ^ u64::from(car.braking())).wrapping_mul(0x100_0000_01b3);
                }
            }
        }
        hash
    }

    #[test]
    fn the_traffic_of_cars_that_do_not_keep_apart_is_that_of_v1_8_0() {
        for (style, golden) in [
            (TruckStyle::None, GOLDEN_NONE),
            (TruckStyle::Japanese, GOLDEN_JAPANESE),
            (TruckStyle::NorthAmerican, GOLDEN_NORTH_AMERICAN),
        ] {
            let hashes: Vec<u64> = (0..4).map(|seed| positions_hash(style, seed)).collect();
            assert_eq!(hashes, golden, "{style:?}");
        }
    }

    const GOLDEN_NONE: [u64; 4] = [
        6_172_463_116_687_344_504,
        12_380_605_061_641_358_935,
        3_654_283_632_733_167_293,
        8_442_962_960_027_808_954,
    ];
    // The shape of a truck does not change where it is: the two styles agree.
    const GOLDEN_JAPANESE: [u64; 4] = [
        5_058_894_947_907_202_109,
        1_888_216_671_477_799_124,
        11_186_385_611_266_846_748,
        2_237_644_391_015_639_066,
    ];
    const GOLDEN_NORTH_AMERICAN: [u64; 4] = GOLDEN_JAPANESE;
}
