//! User settings: values, defaults and validation.
//!
//! Settings come from storage that the user (or another program) can edit,
//! so every raw value is checked on its own; a value that is missing or not
//! valid falls back to the default for that setting only.

use crate::i18n::DisplayLanguage;
use crate::scene::SceneConfig;
use crate::traffic::{CarColor, CarShapes, LaneChanges, TruckStyle};
use crate::tunnel::TunnelLighting;

pub const MIN_SPEED_KMH: u32 = 30;
pub const MAX_SPEED_KMH: u32 = 160;
pub const SPEED_STEP_KMH: u32 = 10;
pub const DEFAULT_SPEED_KMH: u32 = 100;

/// Average time between two UFO visits, in seconds, when they are enabled.
pub const UFO_MEAN_INTERVAL_SECONDS: f32 = 600.0;

/// Number of speeds the user can choose from, 30, 40, ... 160.
pub const SPEED_CHOICES: u32 = (MAX_SPEED_KMH - MIN_SPEED_KMH) / SPEED_STEP_KMH + 1;

/// The language the user chose for the settings dialog and messages.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Language {
    /// Follow the display language of Windows.
    #[default]
    Auto,
    English,
    Japanese,
}

impl Language {
    /// Every choice, in the order of the stored values 0, 1, 2.
    pub const ALL: [Language; 3] = [Language::Auto, Language::English, Language::Japanese];

    /// The language for a stored value; `None` when the value is not valid.
    pub fn from_raw(raw: Option<u32>) -> Option<Self> {
        Self::ALL.get(usize::try_from(raw?).ok()?).copied()
    }

    /// The value to store; `from_raw` reads it back.
    pub fn to_raw(self) -> u32 {
        match self {
            Language::Auto => 0,
            Language::English => 1,
            Language::Japanese => 2,
        }
    }
}

/// How the speedometer is shown.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SpeedometerStyle {
    #[default]
    None,
    /// The speed as a number in the lower left corner.
    Digits,
    /// A gauge with the speed in its middle.
    SimpleDashboard,
    /// A dashboard with a speedometer, a tachometer and a row of information.
    DetailedDashboard,
}

impl SpeedometerStyle {
    /// Every choice, in the order of the stored values 0, 1, 2, 3.
    pub const ALL: [SpeedometerStyle; 4] = [
        SpeedometerStyle::None,
        SpeedometerStyle::Digits,
        SpeedometerStyle::SimpleDashboard,
        SpeedometerStyle::DetailedDashboard,
    ];

    /// The choice for a stored value; `None` when the value is not valid.
    pub fn from_raw(raw: Option<u32>) -> Option<Self> {
        Self::ALL.get(usize::try_from(raw?).ok()?).copied()
    }

    /// The value to store; `from_raw` reads it back.
    pub fn to_raw(self) -> u32 {
        match self {
            SpeedometerStyle::None => 0,
            SpeedometerStyle::Digits => 1,
            SpeedometerStyle::SimpleDashboard => 2,
            SpeedometerStyle::DetailedDashboard => 3,
        }
    }

    /// The value of the setting of versions before 1.10, which only said
    /// whether to show the speedometer; `ShowSpeedometer` is stored for them.
    pub fn to_legacy_flag(self) -> bool {
        self != SpeedometerStyle::None
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Settings {
    /// Cruising speed in km/h, a multiple of `SPEED_STEP_KMH` within the range.
    pub speed_kmh: u32,
    pub left_hand_traffic: bool,
    pub speedometer: SpeedometerStyle,
    /// Whether a UFO now and then crosses the sky.
    pub show_ufo: bool,
    /// Whether mountains stand on the horizon.
    pub show_mountains: bool,
    /// Whether some of the other cars are trucks, and how they are lit.
    pub trucks: TruckStyle,
    /// Whether the road runs through tunnels, and how they are lit.
    pub tunnels: TunnelLighting,
    /// How the bodies of the other cars are drawn.
    pub car_shapes: CarShapes,
    /// The color of the bodies.
    pub car_color: CarColor,
    pub language: Language,
    /// Whether the speed drifts slowly around `speed_kmh`.
    pub vary_speed: bool,
    /// Whether the traffic follows the time of day.
    pub traffic_by_time: bool,
    /// Whether the road alternates between mountains and plains.
    pub scenery_zones: bool,
    /// Who changes lanes.
    pub lane_changes: LaneChanges,
    /// Whether all the monitors are drawn as one screen, with one scene
    /// across them (docs/adr/0008-span-monitors-as-one-scene.md).
    pub span_monitors: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            speed_kmh: DEFAULT_SPEED_KMH,
            left_hand_traffic: true,
            speedometer: SpeedometerStyle::None,
            show_ufo: false,
            show_mountains: true,
            trucks: TruckStyle::Japanese,
            tunnels: TunnelLighting::Mixed,
            car_shapes: CarShapes::None,
            car_color: CarColor::TranslucentBlack,
            language: Language::Auto,
            vary_speed: true,
            traffic_by_time: true,
            scenery_zones: true,
            lane_changes: LaneChanges::All,
            span_monitors: false,
        }
    }
}

impl Settings {
    /// Builds settings from stored values, where `None` means "not stored".
    ///
    /// A speed outside the range or off the step is not rounded to a nearby
    /// choice: it is a sign of a damaged or hand-edited value, so the default
    /// is used. A flag is valid only as 0 or 1, a truck style or language only as 0, 1 or 2,
    /// a tunnel choice only as 0, 1, 2 or 3, a car shape choice only as 0, 1 or 2,
    /// a car color choice only as 0 or 1, a speedometer choice only as 0, 1, 2
    /// or 3.
    ///
    /// Versions before 1.10 stored only whether to show the speedometer
    /// (`legacy_show_speedometer`). It is used when `speedometer` is not
    /// stored: 1 means the number only, anything else no speedometer.
    ///
    /// Versions before 1.11 stored one choice for the bodies of the cars
    /// (`legacy_car_bodies`). It is used when `car_shapes` is not stored:
    /// 0 means no bodies, 1 realistic shapes in translucent black, 2 realistic
    /// shapes in black (see `legacy_car_bodies`).
    #[allow(clippy::too_many_arguments)]
    pub fn from_raw(
        speed_kmh: Option<u32>,
        left_hand_traffic: Option<u32>,
        legacy_show_speedometer: Option<u32>,
        show_ufo: Option<u32>,
        show_mountains: Option<u32>,
        trucks: Option<u32>,
        tunnels: Option<u32>,
        language: Option<u32>,
        vary_speed: Option<u32>,
        traffic_by_time: Option<u32>,
        scenery_zones: Option<u32>,
        legacy_car_bodies: Option<u32>,
        speedometer: Option<u32>,
        car_shapes: Option<u32>,
        car_color: Option<u32>,
        lane_changes: Option<u32>,
    ) -> Self {
        let defaults = Self::default();
        let (car_shapes, car_color) = match car_shapes {
            Some(_) => (
                CarShapes::from_raw(car_shapes).unwrap_or(defaults.car_shapes),
                CarColor::from_raw(car_color).unwrap_or(defaults.car_color),
            ),
            None => from_legacy_car_bodies(legacy_car_bodies)
                .unwrap_or((defaults.car_shapes, defaults.car_color)),
        };
        Self {
            speed_kmh: speed_kmh
                .filter(|&speed| is_valid_speed(speed))
                .unwrap_or(defaults.speed_kmh),
            left_hand_traffic: flag(left_hand_traffic).unwrap_or(defaults.left_hand_traffic),
            speedometer: match speedometer {
                Some(_) => SpeedometerStyle::from_raw(speedometer).unwrap_or(defaults.speedometer),
                None if flag(legacy_show_speedometer) == Some(true) => SpeedometerStyle::Digits,
                None => defaults.speedometer,
            },
            show_ufo: flag(show_ufo).unwrap_or(defaults.show_ufo),
            show_mountains: flag(show_mountains).unwrap_or(defaults.show_mountains),
            trucks: TruckStyle::from_raw(trucks).unwrap_or(defaults.trucks),
            tunnels: TunnelLighting::from_raw(tunnels).unwrap_or(defaults.tunnels),
            language: Language::from_raw(language).unwrap_or(defaults.language),
            vary_speed: flag(vary_speed).unwrap_or(defaults.vary_speed),
            traffic_by_time: flag(traffic_by_time).unwrap_or(defaults.traffic_by_time),
            scenery_zones: flag(scenery_zones).unwrap_or(defaults.scenery_zones),
            car_shapes,
            car_color,
            lane_changes: LaneChanges::from_raw(lane_changes).unwrap_or(defaults.lane_changes),
            span_monitors: defaults.span_monitors,
        }
    }

    /// These settings with `span_monitors` read from its stored value: valid
    /// only as 0 or 1, and the default (off) where it is not stored.
    ///
    /// Why: a separate step, not another argument of `from_raw`, which has
    /// more arguments than it can have in a readable call already.
    pub fn with_span_monitors_raw(self, span_monitors: Option<u32>) -> Self {
        Self {
            span_monitors: flag(span_monitors).unwrap_or(Self::default().span_monitors),
            ..self
        }
    }

    /// The value of the choice for the bodies of the cars that versions
    /// before 1.11 read, so that going back to such a version keeps whether
    /// the cars have bodies: 0 for none, 1 for translucent black, 2 for black.
    pub fn legacy_car_bodies(&self) -> u32 {
        match (self.car_shapes, self.car_color) {
            (CarShapes::None, _) => 0,
            (_, CarColor::TranslucentBlack) => 1,
            (_, CarColor::Black) => 2,
        }
    }

    /// The language to display, given the Windows display language ID.
    pub fn display_language(&self, windows_ui_language: u16) -> DisplayLanguage {
        DisplayLanguage::resolve(self.language, windows_ui_language)
    }

    /// The configuration of one scene; each window gets its own `seed`. The
    /// host sets `start_minute` to the local time.
    pub fn scene_config(&self, seed: u64) -> SceneConfig {
        SceneConfig {
            speed_kmh: self.speed_kmh as f32,
            left_hand_traffic: self.left_hand_traffic,
            ufo_mean_interval: self.show_ufo.then_some(UFO_MEAN_INTERVAL_SECONDS),
            mountains: self.show_mountains,
            trucks: self.trucks,
            tunnels: self.tunnels,
            car_shapes: self.car_shapes,
            car_color: self.car_color,
            vary_speed: self.vary_speed,
            traffic_by_time: self.traffic_by_time,
            scenery_zones: self.scenery_zones,
            lane_changes: self.lane_changes,
            seed,
            ..SceneConfig::default()
        }
    }
}

/// The shapes and the color for the choice for the bodies of the cars of
/// versions before 1.11; `None` when the value is not valid. Realistic shapes
/// are the closest: those versions drew trucks in their own shapes.
fn from_legacy_car_bodies(raw: Option<u32>) -> Option<(CarShapes, CarColor)> {
    match raw? {
        0 => Some((CarShapes::None, CarColor::TranslucentBlack)),
        1 => Some((CarShapes::Blocks, CarColor::TranslucentBlack)),
        2 => Some((CarShapes::Blocks, CarColor::Black)),
        _ => None,
    }
}

fn flag(raw: Option<u32>) -> Option<bool> {
    match raw {
        Some(0) => Some(false),
        Some(1) => Some(true),
        _ => None,
    }
}

fn is_valid_speed(speed_kmh: u32) -> bool {
    (MIN_SPEED_KMH..=MAX_SPEED_KMH).contains(&speed_kmh)
        && (speed_kmh - MIN_SPEED_KMH).is_multiple_of(SPEED_STEP_KMH)
}

/// Position of `speed_kmh` on a slider with `SPEED_CHOICES` positions.
/// A speed that is not a choice maps to the nearest position inside the range.
pub fn speed_to_position(speed_kmh: u32) -> u32 {
    let clamped = speed_kmh.clamp(MIN_SPEED_KMH, MAX_SPEED_KMH);
    (clamped - MIN_SPEED_KMH + SPEED_STEP_KMH / 2) / SPEED_STEP_KMH
}

/// Speed at a slider position; a position past the end gives the top speed.
pub fn position_to_speed(position: u32) -> u32 {
    MIN_SPEED_KMH + position.min(SPEED_CHOICES - 1) * SPEED_STEP_KMH
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_the_plan() {
        let settings = Settings::default();
        assert_eq!(settings.speed_kmh, 100);
        assert!(settings.left_hand_traffic);
        assert_eq!(settings.speedometer, SpeedometerStyle::None);
        assert_eq!(
            Settings::from_raw(
                None, None, None, None, None, None, None, None, None, None, None, None, None, None,
                None, None
            ),
            settings
        );
    }

    #[test]
    fn valid_values_are_kept() {
        let settings = Settings::from_raw(
            Some(160),
            Some(0),
            Some(0),
            Some(1),
            Some(0),
            Some(2),
            Some(3),
            Some(2),
            None,
            None,
            None,
            Some(2),
            Some(3),
            Some(1),
            Some(1),
            Some(1),
        );
        assert_eq!(
            settings,
            Settings {
                speed_kmh: 160,
                left_hand_traffic: false,
                speedometer: SpeedometerStyle::DetailedDashboard,
                show_ufo: true,
                show_mountains: false,
                trucks: TruckStyle::NorthAmerican,
                tunnels: TunnelLighting::Led,
                car_shapes: CarShapes::Boxes,
                car_color: CarColor::Black,
                language: Language::Japanese,
                vary_speed: true,
                traffic_by_time: true,
                scenery_zones: true,
                lane_changes: LaneChanges::OwnCar,
                span_monitors: false,
            }
        );
        assert_eq!(
            Settings::from_raw(
                Some(30),
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None
            )
            .speed_kmh,
            30
        );
    }

    #[test]
    fn invalid_speeds_fall_back_to_the_default() {
        for speed in [0, 20, 29, 35, 105, 161, 170, 1000, u32::MAX] {
            assert_eq!(
                Settings::from_raw(
                    Some(speed),
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None
                )
                .speed_kmh,
                DEFAULT_SPEED_KMH,
                "{speed}"
            );
        }
    }

    #[test]
    fn invalid_flags_fall_back_to_the_default() {
        for raw in [2, 3, u32::MAX] {
            let settings = Settings::from_raw(
                None,
                Some(raw),
                Some(raw),
                Some(raw),
                Some(raw),
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
            );
            assert_eq!(settings, Settings::default(), "{raw}");
        }
    }

    #[test]
    fn each_setting_is_checked_on_its_own() {
        let settings = Settings::from_raw(
            Some(999),
            Some(0),
            Some(7),
            Some(1),
            Some(9),
            None,
            None,
            Some(1),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        );
        assert_eq!(settings.speed_kmh, DEFAULT_SPEED_KMH);
        assert!(!settings.left_hand_traffic);
        assert_eq!(settings.speedometer, SpeedometerStyle::None);
        assert!(settings.show_ufo);
        assert!(settings.show_mountains);
        assert_eq!(settings.language, Language::English);
    }

    #[test]
    fn scene_config_carries_the_settings() {
        let settings = Settings {
            speed_kmh: 60,
            left_hand_traffic: false,
            speedometer: SpeedometerStyle::SimpleDashboard,
            show_ufo: false,
            show_mountains: false,
            trucks: TruckStyle::None,
            tunnels: TunnelLighting::None,
            car_shapes: CarShapes::Blocks,
            car_color: CarColor::Black,
            language: Language::Auto,
            vary_speed: false,
            traffic_by_time: false,
            scenery_zones: false,
            lane_changes: LaneChanges::None,
            span_monitors: true,
        };
        let config = settings.scene_config(42);
        assert_eq!(config.lane_changes, LaneChanges::None);
        assert_eq!(config.speed_kmh, 60.0);
        assert!(!config.left_hand_traffic);
        assert_eq!(config.seed, 42);
        assert!(!config.mountains);
    }

    #[test]
    fn ufo_is_off_by_default_and_the_flag_is_checked() {
        assert!(!Settings::default().show_ufo);
        assert!(
            Settings::from_raw(
                None,
                None,
                None,
                Some(1),
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None
            )
            .show_ufo
        );
        assert!(
            !Settings::from_raw(
                None,
                None,
                None,
                Some(0),
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None
            )
            .show_ufo
        );
        for raw in [2, 3, u32::MAX] {
            assert!(
                !Settings::from_raw(
                    None,
                    None,
                    None,
                    Some(raw),
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None
                )
                .show_ufo,
                "{raw}"
            );
        }
    }

    #[test]
    fn scene_config_enables_the_ufo_only_when_asked() {
        let on = Settings {
            show_ufo: true,
            ..Settings::default()
        };
        assert_eq!(on.scene_config(0).ufo_mean_interval, Some(600.0));
        assert_eq!(Settings::default().scene_config(0).ufo_mean_interval, None);
    }

    #[test]
    fn mountains_are_on_by_default_and_the_flag_is_checked() {
        assert!(Settings::default().show_mountains);
        assert!(Settings::default().scene_config(0).mountains);
        assert!(
            Settings::from_raw(
                None,
                None,
                None,
                None,
                Some(1),
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None
            )
            .show_mountains
        );
        assert!(
            !Settings::from_raw(
                None,
                None,
                None,
                None,
                Some(0),
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None
            )
            .show_mountains
        );
        for raw in [2, 3, u32::MAX] {
            assert!(
                Settings::from_raw(
                    None,
                    None,
                    None,
                    None,
                    Some(raw),
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None
                )
                .show_mountains,
                "{raw}"
            );
        }
    }

    #[test]
    fn the_variations_are_on_by_default() {
        let settings = Settings::default();
        assert!(settings.vary_speed && settings.traffic_by_time && settings.scenery_zones);
        let none = Settings::from_raw(
            None, None, None, None, None, None, None, None, None, None, None, None, None, None,
            None, None,
        );
        assert!(none.vary_speed && none.traffic_by_time && none.scenery_zones);
    }

    #[test]
    fn the_variation_flags_are_read_and_checked_one_by_one() {
        let read = |vary, traffic, zones| {
            let s = Settings::from_raw(
                None, None, None, None, None, None, None, None, vary, traffic, zones, None, None,
                None, None, None,
            );
            (s.vary_speed, s.traffic_by_time, s.scenery_zones)
        };
        assert_eq!(read(Some(0), Some(0), Some(0)), (false, false, false));
        assert_eq!(read(Some(1), Some(0), Some(1)), (true, false, true));
        assert_eq!(read(Some(0), None, Some(0)), (false, true, false));
        // A value other than 0 and 1 is the default (on) for that flag only.
        for raw in [2, 3, u32::MAX] {
            assert_eq!(
                read(Some(raw), Some(0), Some(0)),
                (true, false, false),
                "{raw}"
            );
            assert_eq!(
                read(Some(0), Some(raw), Some(0)),
                (false, true, false),
                "{raw}"
            );
            assert_eq!(
                read(Some(0), Some(0), Some(raw)),
                (false, false, true),
                "{raw}"
            );
        }
    }

    #[test]
    fn the_variations_do_not_disturb_the_other_settings() {
        let settings = Settings::from_raw(
            Some(60),
            None,
            None,
            None,
            None,
            None,
            None,
            Some(1),
            Some(0),
            Some(0),
            Some(0),
            None,
            None,
            None,
            None,
            None,
        );
        assert_eq!(settings.speed_kmh, 60);
        assert_eq!(settings.language, Language::English);
    }

    #[test]
    fn scene_config_carries_the_variations() {
        for (vary, traffic, zones) in [
            (true, true, true),
            (false, false, false),
            (true, false, true),
        ] {
            let settings = Settings {
                vary_speed: vary,
                traffic_by_time: traffic,
                scenery_zones: zones,
                ..Settings::default()
            };
            let config = settings.scene_config(9);
            assert_eq!(
                (
                    config.vary_speed,
                    config.traffic_by_time,
                    config.scenery_zones
                ),
                (vary, traffic, zones)
            );
            assert_eq!(config.seed, 9);
        }
    }

    #[test]
    fn lane_changes_are_on_for_everyone_by_default_and_off_in_the_bare_scene() {
        assert_eq!(Settings::default().lane_changes, LaneChanges::All);
        assert_eq!(SceneConfig::default().lane_changes, LaneChanges::None);
        assert_eq!(
            Settings::default().scene_config(1).lane_changes,
            LaneChanges::All
        );
    }

    #[test]
    fn lane_changes_are_read_back_and_a_bad_value_is_the_default() {
        let read = |raw: Option<u32>| {
            Settings::from_raw(
                None, None, None, None, None, None, None, None, None, None, None, None, None, None,
                None, raw,
            )
            .lane_changes
        };
        for choice in LaneChanges::ALL {
            assert_eq!(read(Some(choice.to_raw())), choice);
        }
        assert_eq!(read(None), LaneChanges::All);
        for raw in [3, 4, 100, u32::MAX] {
            assert_eq!(read(Some(raw)), LaneChanges::All, "{raw}");
        }
    }

    #[test]
    fn trucks_are_japanese_style_by_default() {
        assert_eq!(Settings::default().trucks, TruckStyle::Japanese);
        let settings = Settings::from_raw(
            None, None, None, None, None, None, None, None, None, None, None, None, None, None,
            None, None,
        );
        assert_eq!(settings.trucks, TruckStyle::Japanese);
    }

    #[test]
    fn truck_values_map_to_the_choices() {
        assert_eq!(TruckStyle::from_raw(Some(0)), Some(TruckStyle::None));
        assert_eq!(TruckStyle::from_raw(Some(1)), Some(TruckStyle::Japanese));
        assert_eq!(
            TruckStyle::from_raw(Some(2)),
            Some(TruckStyle::NorthAmerican)
        );
        for style in TruckStyle::ALL {
            assert_eq!(TruckStyle::from_raw(Some(style.to_raw())), Some(style));
        }
    }

    #[test]
    fn an_invalid_truck_style_falls_back_to_the_default() {
        assert_eq!(TruckStyle::from_raw(None), None);
        for raw in [3, 4, 100, u32::MAX] {
            assert_eq!(TruckStyle::from_raw(Some(raw)), None, "{raw}");
            let settings = Settings::from_raw(
                None,
                None,
                None,
                None,
                None,
                Some(raw),
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
            );
            assert_eq!(settings.trucks, TruckStyle::Japanese, "{raw}");
        }
        // The other settings are not affected.
        let settings = Settings::from_raw(
            Some(60),
            None,
            None,
            None,
            None,
            Some(9),
            None,
            Some(1),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        );
        assert_eq!(settings.speed_kmh, 60);
        assert_eq!(settings.language, Language::English);
    }

    #[test]
    fn scene_config_carries_the_truck_style() {
        for trucks in TruckStyle::ALL {
            let settings = Settings {
                trucks,
                ..Settings::default()
            };
            assert_eq!(settings.scene_config(1).trucks, trucks);
        }
    }

    #[test]
    fn tunnels_are_mixed_by_default() {
        assert_eq!(Settings::default().tunnels, TunnelLighting::Mixed);
        let settings = Settings::from_raw(
            None, None, None, None, None, None, None, None, None, None, None, None, None, None,
            None, None,
        );
        assert_eq!(settings.tunnels, TunnelLighting::Mixed);
        assert_eq!(
            Settings::default().scene_config(0).tunnels,
            TunnelLighting::Mixed
        );
    }

    #[test]
    fn tunnel_values_map_to_the_choices() {
        assert_eq!(
            TunnelLighting::from_raw(Some(0)),
            Some(TunnelLighting::None)
        );
        assert_eq!(
            TunnelLighting::from_raw(Some(1)),
            Some(TunnelLighting::Mixed)
        );
        assert_eq!(
            TunnelLighting::from_raw(Some(2)),
            Some(TunnelLighting::Sodium)
        );
        assert_eq!(TunnelLighting::from_raw(Some(3)), Some(TunnelLighting::Led));
        for lighting in TunnelLighting::ALL {
            assert_eq!(
                TunnelLighting::from_raw(Some(lighting.to_raw())),
                Some(lighting)
            );
        }
    }

    #[test]
    fn an_invalid_tunnel_choice_falls_back_to_the_default() {
        assert_eq!(TunnelLighting::from_raw(None), None);
        for raw in [4, 5, 100, u32::MAX] {
            assert_eq!(TunnelLighting::from_raw(Some(raw)), None, "{raw}");
            let settings = Settings::from_raw(
                None,
                None,
                None,
                None,
                None,
                None,
                Some(raw),
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
            );
            assert_eq!(settings.tunnels, TunnelLighting::Mixed, "{raw}");
        }
        // The other settings are not affected.
        let settings = Settings::from_raw(
            Some(60),
            None,
            None,
            None,
            None,
            None,
            Some(9),
            Some(1),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        );
        assert_eq!(settings.speed_kmh, 60);
        assert_eq!(settings.language, Language::English);
    }

    #[test]
    fn scene_config_carries_the_tunnel_choice() {
        for tunnels in TunnelLighting::ALL {
            let settings = Settings {
                tunnels,
                ..Settings::default()
            };
            assert_eq!(settings.scene_config(1).tunnels, tunnels);
        }
    }

    /// The settings read from stored values of the bodies of the cars only:
    /// the legacy choice, the shapes and the color.
    fn bodies_from_raw(
        legacy: Option<u32>,
        shapes: Option<u32>,
        color: Option<u32>,
    ) -> (CarShapes, CarColor) {
        let settings = Settings::from_raw(
            None, None, None, None, None, None, None, None, None, None, None, legacy, None, shapes,
            color, None,
        );
        (settings.car_shapes, settings.car_color)
    }

    #[test]
    fn car_bodies_are_off_and_translucent_black_by_default() {
        assert_eq!(Settings::default().car_shapes, CarShapes::None);
        assert_eq!(Settings::default().car_color, CarColor::TranslucentBlack);
        assert_eq!(SceneConfig::default().car_shapes, CarShapes::None);
        assert_eq!(SceneConfig::default().car_color, CarColor::TranslucentBlack);
        assert_eq!(
            bodies_from_raw(None, None, None),
            (CarShapes::None, CarColor::TranslucentBlack)
        );
        let config = Settings::default().scene_config(0);
        assert_eq!(config.car_shapes, CarShapes::None);
        assert_eq!(config.car_color, CarColor::TranslucentBlack);
    }

    #[test]
    fn car_shape_and_color_values_map_to_the_choices() {
        assert_eq!(CarShapes::from_raw(Some(0)), Some(CarShapes::None));
        assert_eq!(CarShapes::from_raw(Some(1)), Some(CarShapes::Boxes));
        assert_eq!(CarShapes::from_raw(Some(2)), Some(CarShapes::Blocks));
        assert_eq!(CarShapes::from_raw(Some(3)), Some(CarShapes::Detailed));
        assert_eq!(
            CarColor::from_raw(Some(0)),
            Some(CarColor::TranslucentBlack)
        );
        assert_eq!(CarColor::from_raw(Some(1)), Some(CarColor::Black));
        for shapes in CarShapes::ALL {
            assert_eq!(CarShapes::from_raw(Some(shapes.to_raw())), Some(shapes));
        }
        for color in CarColor::ALL {
            assert_eq!(CarColor::from_raw(Some(color.to_raw())), Some(color));
        }
        assert_eq!(CarColor::TranslucentBlack.opacity(), 0.9);
        assert_eq!(CarColor::Black.opacity(), 1.0);
    }

    #[test]
    fn an_invalid_car_shape_or_color_falls_back_to_the_default() {
        assert_eq!(CarShapes::from_raw(None), None);
        assert_eq!(CarColor::from_raw(None), None);
        for raw in [4, 5, 100, u32::MAX] {
            assert_eq!(CarShapes::from_raw(Some(raw)), None, "{raw}");
            assert_eq!(CarColor::from_raw(Some(raw)), None, "{raw}");
            // Each is judged by itself: the other one is kept.
            assert_eq!(
                bodies_from_raw(None, Some(raw), Some(1)),
                (CarShapes::None, CarColor::Black),
                "{raw}"
            );
            assert_eq!(
                bodies_from_raw(None, Some(2), Some(raw)),
                (CarShapes::Blocks, CarColor::TranslucentBlack),
                "{raw}"
            );
            let settings = Settings::from_raw(
                Some(60),
                None,
                None,
                None,
                None,
                None,
                None,
                Some(1),
                None,
                None,
                None,
                None,
                None,
                Some(raw),
                Some(raw),
                None,
            );
            assert_eq!(settings.car_shapes, CarShapes::None, "{raw}");
            assert_eq!(settings.car_color, CarColor::TranslucentBlack, "{raw}");
            // The other settings are not affected.
            assert_eq!(settings.speed_kmh, 60);
            assert_eq!(settings.language, Language::English);
        }
    }

    #[test]
    fn the_car_bodies_of_versions_before_1_11_are_carried_over() {
        // Those versions drew trucks in their own shapes: realistic shapes.
        assert_eq!(
            bodies_from_raw(Some(0), None, None),
            (CarShapes::None, CarColor::TranslucentBlack)
        );
        assert_eq!(
            bodies_from_raw(Some(1), None, None),
            (CarShapes::Blocks, CarColor::TranslucentBlack)
        );
        assert_eq!(
            bodies_from_raw(Some(2), None, None),
            (CarShapes::Blocks, CarColor::Black)
        );
        // A value that is not valid is the default, as for the other settings.
        for raw in [3, 100, u32::MAX] {
            assert_eq!(
                bodies_from_raw(Some(raw), None, None),
                (CarShapes::None, CarColor::TranslucentBlack),
                "{raw}"
            );
        }
        // The old color is not read when the new shapes are stored, and the
        // color is not read from the old choice then either.
        assert_eq!(
            bodies_from_raw(Some(2), Some(1), None),
            (CarShapes::Boxes, CarColor::TranslucentBlack)
        );
        assert_eq!(
            bodies_from_raw(Some(2), Some(0), Some(0)),
            (CarShapes::None, CarColor::TranslucentBlack)
        );
    }

    #[test]
    fn the_legacy_car_bodies_value_keeps_whether_there_are_bodies() {
        for color in CarColor::ALL {
            let none = Settings {
                car_shapes: CarShapes::None,
                car_color: color,
                ..Settings::default()
            };
            assert_eq!(none.legacy_car_bodies(), 0);
        }
        for shapes in [CarShapes::Boxes, CarShapes::Blocks, CarShapes::Detailed] {
            let settings = |car_color| Settings {
                car_shapes: shapes,
                car_color,
                ..Settings::default()
            };
            assert_eq!(settings(CarColor::TranslucentBlack).legacy_car_bodies(), 1);
            assert_eq!(settings(CarColor::Black).legacy_car_bodies(), 2);
        }
    }

    #[test]
    fn spanning_the_monitors_is_off_unless_stored_as_one() {
        assert!(!Settings::default().span_monitors);
        let read = |raw| {
            Settings::default()
                .with_span_monitors_raw(raw)
                .span_monitors
        };
        assert!(read(Some(1)));
        assert!(!read(Some(0)));
        // Missing or out of range is the default, not "on".
        for raw in [None, Some(2), Some(u32::MAX)] {
            assert!(!read(raw), "{raw:?}");
        }
        // Other settings are left as they are.
        let before = Settings {
            speed_kmh: 60,
            ..Settings::default()
        };
        assert_eq!(before.with_span_monitors_raw(Some(1)).speed_kmh, 60);
    }

    #[test]
    fn scene_config_carries_the_car_shape_and_color() {
        for car_shapes in CarShapes::ALL {
            for car_color in CarColor::ALL {
                let settings = Settings {
                    car_shapes,
                    car_color,
                    ..Settings::default()
                };
                let config = settings.scene_config(1);
                assert_eq!(
                    (config.car_shapes, config.car_color),
                    (car_shapes, car_color)
                );
            }
        }
    }

    #[test]
    fn default_settings_give_the_default_scene_config() {
        let config = Settings::default().scene_config(0);
        let expected = SceneConfig::default();
        assert_eq!(config.speed_kmh, expected.speed_kmh);
        assert_eq!(config.left_hand_traffic, expected.left_hand_traffic);
    }

    #[test]
    fn slider_positions_cover_the_range() {
        assert_eq!(SPEED_CHOICES, 14);
        assert_eq!(position_to_speed(0), 30);
        assert_eq!(position_to_speed(SPEED_CHOICES - 1), 160);
        assert_eq!(position_to_speed(SPEED_CHOICES + 5), 160);
        assert_eq!(speed_to_position(100), 7);
    }

    #[test]
    fn slider_positions_round_trip_for_every_choice() {
        for position in 0..SPEED_CHOICES {
            let speed = position_to_speed(position);
            assert!(is_valid_speed(speed), "{speed}");
            assert_eq!(speed_to_position(speed), position, "{speed}");
        }
    }

    #[test]
    fn off_range_speeds_map_inside_the_slider() {
        assert_eq!(speed_to_position(0), 0);
        assert_eq!(speed_to_position(1000), SPEED_CHOICES - 1);
    }

    #[test]
    fn language_is_automatic_by_default() {
        assert_eq!(Language::default(), Language::Auto);
        assert_eq!(Settings::default().language, Language::Auto);
    }

    #[test]
    fn language_values_map_to_the_choices() {
        assert_eq!(Language::from_raw(Some(0)), Some(Language::Auto));
        assert_eq!(Language::from_raw(Some(1)), Some(Language::English));
        assert_eq!(Language::from_raw(Some(2)), Some(Language::Japanese));
        for language in Language::ALL {
            assert_eq!(Language::from_raw(Some(language.to_raw())), Some(language));
        }
    }

    #[test]
    fn an_invalid_language_falls_back_to_the_default() {
        assert_eq!(Language::from_raw(None), None);
        for raw in [3, 4, 100, u32::MAX] {
            assert_eq!(Language::from_raw(Some(raw)), None, "{raw}");
            let settings = Settings::from_raw(
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                Some(raw),
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
            );
            assert_eq!(settings.language, Language::Auto, "{raw}");
        }
    }

    #[test]
    fn display_language_follows_the_setting() {
        let japanese_windows = 0x0411;
        let english_windows = 0x0409;
        let auto = Settings::default();
        assert_eq!(
            auto.display_language(japanese_windows),
            DisplayLanguage::Japanese
        );
        assert_eq!(
            auto.display_language(english_windows),
            DisplayLanguage::English
        );
        let english = Settings {
            language: Language::English,
            ..auto
        };
        assert_eq!(
            english.display_language(japanese_windows),
            DisplayLanguage::English
        );
    }

    /// `Settings::from_raw` with only the speedometer values.
    fn speedometer_from_raw(legacy: Option<u32>, speedometer: Option<u32>) -> SpeedometerStyle {
        Settings::from_raw(
            None,
            None,
            legacy,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            speedometer,
            None,
            None,
            None,
        )
        .speedometer
    }

    #[test]
    fn the_speedometer_style_maps_to_stored_values_0_to_3() {
        assert_eq!(SpeedometerStyle::default(), SpeedometerStyle::None);
        assert_eq!(
            SpeedometerStyle::ALL.map(SpeedometerStyle::to_raw),
            [0, 1, 2, 3]
        );
        for style in SpeedometerStyle::ALL {
            assert_eq!(
                SpeedometerStyle::from_raw(Some(style.to_raw())),
                Some(style)
            );
            assert_eq!(speedometer_from_raw(None, Some(style.to_raw())), style);
        }
        assert_eq!(SpeedometerStyle::from_raw(None), None);
    }

    #[test]
    fn a_speedometer_value_out_of_range_gives_the_default() {
        for raw in [4, 5, 100, u32::MAX] {
            assert_eq!(
                speedometer_from_raw(None, Some(raw)),
                SpeedometerStyle::None,
                "{raw}"
            );
            // The stored value is wrong, not missing, so the old flag does
            // not apply.
            assert_eq!(
                speedometer_from_raw(Some(1), Some(raw)),
                SpeedometerStyle::None,
                "{raw}"
            );
        }
    }

    #[test]
    fn the_old_flag_carries_over_as_digits_only() {
        // A version before 1.10 stored only ShowSpeedometer.
        assert_eq!(
            speedometer_from_raw(Some(1), None),
            SpeedometerStyle::Digits
        );
        assert_eq!(speedometer_from_raw(Some(0), None), SpeedometerStyle::None);
        assert_eq!(speedometer_from_raw(None, None), SpeedometerStyle::None);
        for raw in [2, 3, u32::MAX] {
            assert_eq!(
                speedometer_from_raw(Some(raw), None),
                SpeedometerStyle::None,
                "{raw}"
            );
        }
    }

    #[test]
    fn the_new_value_wins_over_the_old_flag() {
        assert_eq!(
            speedometer_from_raw(Some(1), Some(0)),
            SpeedometerStyle::None
        );
        assert_eq!(
            speedometer_from_raw(Some(0), Some(2)),
            SpeedometerStyle::SimpleDashboard
        );
    }

    #[test]
    fn the_legacy_flag_is_on_for_every_style_but_none() {
        assert_eq!(
            SpeedometerStyle::ALL.map(SpeedometerStyle::to_legacy_flag),
            [false, true, true, true]
        );
    }
}
