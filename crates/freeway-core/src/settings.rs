//! User settings: values, defaults and validation.
//!
//! Settings come from storage that the user (or another program) can edit,
//! so every raw value is checked on its own; a value that is missing or not
//! valid falls back to the default for that setting only.

use crate::scene::SceneConfig;

pub const MIN_SPEED_KMH: u32 = 30;
pub const MAX_SPEED_KMH: u32 = 160;
pub const SPEED_STEP_KMH: u32 = 10;
pub const DEFAULT_SPEED_KMH: u32 = 100;

/// Average time between two UFO visits, in seconds, when they are enabled.
pub const UFO_MEAN_INTERVAL_SECONDS: f32 = 600.0;

/// Number of speeds the user can choose from, 30, 40, ... 160.
pub const SPEED_CHOICES: u32 = (MAX_SPEED_KMH - MIN_SPEED_KMH) / SPEED_STEP_KMH + 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Settings {
    /// Cruising speed in km/h, a multiple of `SPEED_STEP_KMH` within the range.
    pub speed_kmh: u32,
    pub left_hand_traffic: bool,
    pub show_speedometer: bool,
    /// Whether a UFO now and then crosses the sky.
    pub show_ufo: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            speed_kmh: DEFAULT_SPEED_KMH,
            left_hand_traffic: true,
            show_speedometer: false,
            show_ufo: false,
        }
    }
}

impl Settings {
    /// Builds settings from stored values, where `None` means "not stored".
    ///
    /// A speed outside the range or off the step is not rounded to a nearby
    /// choice: it is a sign of a damaged or hand-edited value, so the default
    /// is used. A flag is valid only as 0 or 1.
    pub fn from_raw(
        speed_kmh: Option<u32>,
        left_hand_traffic: Option<u32>,
        show_speedometer: Option<u32>,
        show_ufo: Option<u32>,
    ) -> Self {
        let defaults = Self::default();
        Self {
            speed_kmh: speed_kmh
                .filter(|&speed| is_valid_speed(speed))
                .unwrap_or(defaults.speed_kmh),
            left_hand_traffic: flag(left_hand_traffic).unwrap_or(defaults.left_hand_traffic),
            show_speedometer: flag(show_speedometer).unwrap_or(defaults.show_speedometer),
            show_ufo: flag(show_ufo).unwrap_or(defaults.show_ufo),
        }
    }

    /// The configuration of one scene; each window gets its own `seed`.
    pub fn scene_config(&self, seed: u64) -> SceneConfig {
        SceneConfig {
            speed_kmh: self.speed_kmh as f32,
            left_hand_traffic: self.left_hand_traffic,
            ufo_mean_interval: self.show_ufo.then_some(UFO_MEAN_INTERVAL_SECONDS),
            seed,
        }
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
        assert!(!settings.show_speedometer);
        assert_eq!(Settings::from_raw(None, None, None, None), settings);
    }

    #[test]
    fn valid_values_are_kept() {
        let settings = Settings::from_raw(Some(160), Some(0), Some(1), Some(1));
        assert_eq!(
            settings,
            Settings {
                speed_kmh: 160,
                left_hand_traffic: false,
                show_speedometer: true,
                show_ufo: true,
            }
        );
        assert_eq!(Settings::from_raw(Some(30), None, None, None).speed_kmh, 30);
    }

    #[test]
    fn invalid_speeds_fall_back_to_the_default() {
        for speed in [0, 20, 29, 35, 105, 161, 170, 1000, u32::MAX] {
            assert_eq!(
                Settings::from_raw(Some(speed), None, None, None).speed_kmh,
                DEFAULT_SPEED_KMH,
                "{speed}"
            );
        }
    }

    #[test]
    fn invalid_flags_fall_back_to_the_default() {
        for raw in [2, 3, u32::MAX] {
            let settings = Settings::from_raw(None, Some(raw), Some(raw), Some(raw));
            assert_eq!(settings, Settings::default(), "{raw}");
        }
    }

    #[test]
    fn each_setting_is_checked_on_its_own() {
        let settings = Settings::from_raw(Some(999), Some(0), Some(7), Some(1));
        assert_eq!(settings.speed_kmh, DEFAULT_SPEED_KMH);
        assert!(!settings.left_hand_traffic);
        assert!(!settings.show_speedometer);
        assert!(settings.show_ufo);
    }

    #[test]
    fn scene_config_carries_the_settings() {
        let settings = Settings {
            speed_kmh: 60,
            left_hand_traffic: false,
            show_speedometer: true,
            show_ufo: false,
        };
        let config = settings.scene_config(42);
        assert_eq!(config.speed_kmh, 60.0);
        assert!(!config.left_hand_traffic);
        assert_eq!(config.seed, 42);
    }

    #[test]
    fn ufo_is_off_by_default_and_the_flag_is_checked() {
        assert!(!Settings::default().show_ufo);
        assert!(Settings::from_raw(None, None, None, Some(1)).show_ufo);
        assert!(!Settings::from_raw(None, None, None, Some(0)).show_ufo);
        for raw in [2, 3, u32::MAX] {
            assert!(
                !Settings::from_raw(None, None, None, Some(raw)).show_ufo,
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
}
