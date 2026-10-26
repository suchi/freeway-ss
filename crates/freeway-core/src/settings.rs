//! User settings: values, defaults and validation.
//!
//! Settings come from storage that the user (or another program) can edit,
//! so every raw value is checked on its own; a value that is missing or not
//! valid falls back to the default for that setting only.

use crate::i18n::DisplayLanguage;
use crate::scene::SceneConfig;
use crate::traffic::TruckStyle;
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Settings {
    /// Cruising speed in km/h, a multiple of `SPEED_STEP_KMH` within the range.
    pub speed_kmh: u32,
    pub left_hand_traffic: bool,
    pub show_speedometer: bool,
    /// Whether a UFO now and then crosses the sky.
    pub show_ufo: bool,
    /// Whether mountains stand on the horizon.
    pub show_mountains: bool,
    /// Whether some of the other cars are trucks, and how they are lit.
    pub trucks: TruckStyle,
    /// Whether the road runs through tunnels, and how they are lit.
    pub tunnels: TunnelLighting,
    pub language: Language,
    /// Whether the speed drifts slowly around `speed_kmh`.
    pub vary_speed: bool,
    /// Whether the traffic follows the time of day.
    pub traffic_by_time: bool,
    /// Whether the road alternates between mountains and plains.
    pub scenery_zones: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            speed_kmh: DEFAULT_SPEED_KMH,
            left_hand_traffic: true,
            show_speedometer: false,
            show_ufo: false,
            show_mountains: true,
            trucks: TruckStyle::Japanese,
            tunnels: TunnelLighting::Mixed,
            language: Language::Auto,
            vary_speed: true,
            traffic_by_time: true,
            scenery_zones: true,
        }
    }
}

impl Settings {
    /// Builds settings from stored values, where `None` means "not stored".
    ///
    /// A speed outside the range or off the step is not rounded to a nearby
    /// choice: it is a sign of a damaged or hand-edited value, so the default
    /// is used. A flag is valid only as 0 or 1, a truck style or language only as 0, 1 or 2,
    /// a tunnel choice only as 0, 1, 2 or 3.
    #[allow(clippy::too_many_arguments)]
    pub fn from_raw(
        speed_kmh: Option<u32>,
        left_hand_traffic: Option<u32>,
        show_speedometer: Option<u32>,
        show_ufo: Option<u32>,
        show_mountains: Option<u32>,
        trucks: Option<u32>,
        tunnels: Option<u32>,
        language: Option<u32>,
        vary_speed: Option<u32>,
        traffic_by_time: Option<u32>,
        scenery_zones: Option<u32>,
    ) -> Self {
        let defaults = Self::default();
        Self {
            speed_kmh: speed_kmh
                .filter(|&speed| is_valid_speed(speed))
                .unwrap_or(defaults.speed_kmh),
            left_hand_traffic: flag(left_hand_traffic).unwrap_or(defaults.left_hand_traffic),
            show_speedometer: flag(show_speedometer).unwrap_or(defaults.show_speedometer),
            show_ufo: flag(show_ufo).unwrap_or(defaults.show_ufo),
            show_mountains: flag(show_mountains).unwrap_or(defaults.show_mountains),
            trucks: TruckStyle::from_raw(trucks).unwrap_or(defaults.trucks),
            tunnels: TunnelLighting::from_raw(tunnels).unwrap_or(defaults.tunnels),
            language: Language::from_raw(language).unwrap_or(defaults.language),
            vary_speed: flag(vary_speed).unwrap_or(defaults.vary_speed),
            traffic_by_time: flag(traffic_by_time).unwrap_or(defaults.traffic_by_time),
            scenery_zones: flag(scenery_zones).unwrap_or(defaults.scenery_zones),
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
            vary_speed: self.vary_speed,
            traffic_by_time: self.traffic_by_time,
            scenery_zones: self.scenery_zones,
            seed,
            ..SceneConfig::default()
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
        assert_eq!(
            Settings::from_raw(
                None, None, None, None, None, None, None, None, None, None, None
            ),
            settings
        );
    }

    #[test]
    fn valid_values_are_kept() {
        let settings = Settings::from_raw(
            Some(160),
            Some(0),
            Some(1),
            Some(1),
            Some(0),
            Some(2),
            Some(3),
            Some(2),
            None,
            None,
            None,
        );
        assert_eq!(
            settings,
            Settings {
                speed_kmh: 160,
                left_hand_traffic: false,
                show_speedometer: true,
                show_ufo: true,
                show_mountains: false,
                trucks: TruckStyle::NorthAmerican,
                tunnels: TunnelLighting::Led,
                language: Language::Japanese,
                vary_speed: true,
                traffic_by_time: true,
                scenery_zones: true,
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
        );
        assert_eq!(settings.speed_kmh, DEFAULT_SPEED_KMH);
        assert!(!settings.left_hand_traffic);
        assert!(!settings.show_speedometer);
        assert!(settings.show_ufo);
        assert!(settings.show_mountains);
        assert_eq!(settings.language, Language::English);
    }

    #[test]
    fn scene_config_carries_the_settings() {
        let settings = Settings {
            speed_kmh: 60,
            left_hand_traffic: false,
            show_speedometer: true,
            show_ufo: false,
            show_mountains: false,
            trucks: TruckStyle::None,
            tunnels: TunnelLighting::None,
            language: Language::Auto,
            vary_speed: false,
            traffic_by_time: false,
            scenery_zones: false,
        };
        let config = settings.scene_config(42);
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
            None, None, None, None, None, None, None, None, None, None, None,
        );
        assert!(none.vary_speed && none.traffic_by_time && none.scenery_zones);
    }

    #[test]
    fn the_variation_flags_are_read_and_checked_one_by_one() {
        let read = |vary, traffic, zones| {
            let s = Settings::from_raw(
                None, None, None, None, None, None, None, None, vary, traffic, zones,
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
    fn trucks_are_japanese_style_by_default() {
        assert_eq!(Settings::default().trucks, TruckStyle::Japanese);
        let settings = Settings::from_raw(
            None, None, None, None, None, None, None, None, None, None, None,
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
            None, None, None, None, None, None, None, None, None, None, None,
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
}
