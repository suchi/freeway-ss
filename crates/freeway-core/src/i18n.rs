//! The strings shown to the user, in every language that is supported
//! (docs/adr/0007-localize-ui-strings-in-code.md).
//!
//! Each key lists all of its translations in one place, and the `match` has
//! no wildcard, so a key without a translation does not compile.

use std::fmt::Display;

use crate::settings::Language;

/// The primary language ID of Japanese (`LANG_JAPANESE`), the low 10 bits of
/// a Windows language ID.
const LANG_JAPANESE: u16 = 0x11;

/// The placeholder that `Text::with_error` replaces with the error.
const ERROR_PLACEHOLDER: &str = "{error}";

/// A language the strings are written in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DisplayLanguage {
    English,
    Japanese,
}

impl DisplayLanguage {
    /// Decides the language from the choice and the Windows display language.
    ///
    /// `windows_ui_language` is a Windows language ID such as 0x0411; only
    /// its primary language matters, so every Japanese locale gives Japanese.
    /// An explicit choice does not depend on Windows.
    pub fn resolve(language: Language, windows_ui_language: u16) -> Self {
        match language {
            Language::English => Self::English,
            Language::Japanese => Self::Japanese,
            Language::Auto if windows_ui_language & 0x3FF == LANG_JAPANESE => Self::Japanese,
            Language::Auto => Self::English,
        }
    }
}

/// A string of the settings dialog or of a message.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Text {
    DialogTitle,
    SpeedLabel,
    DrivingSideLabel,
    LeftHandTraffic,
    RightHandTraffic,
    Speedometer,
    Ufo,
    Mountains,
    VarySpeed,
    TrafficByTime,
    SceneryZones,
    TrucksLabel,
    TrucksNone,
    TrucksJapanese,
    TrucksNorthAmerican,
    TunnelsLabel,
    TunnelsNone,
    TunnelsMixed,
    TunnelsSodium,
    TunnelsLed,
    CarBodiesLabel,
    CarBodiesTransparent,
    CarBodiesTranslucent,
    CarBodiesBlack,
    LanguageLabel,
    LanguageAuto,
    LanguageEnglish,
    LanguageJapanese,
    Ok,
    Cancel,
    /// Contains `{error}`; use `with_error`.
    SaveFailed,
    /// Contains `{error}`; use `with_error`.
    Stopped,
}

impl Text {
    /// Every key, so that tests can check all of them.
    pub const ALL: [Text; 32] = [
        Text::DialogTitle,
        Text::SpeedLabel,
        Text::DrivingSideLabel,
        Text::LeftHandTraffic,
        Text::RightHandTraffic,
        Text::Speedometer,
        Text::Ufo,
        Text::Mountains,
        Text::VarySpeed,
        Text::TrafficByTime,
        Text::SceneryZones,
        Text::TrucksLabel,
        Text::TrucksNone,
        Text::TrucksJapanese,
        Text::TrucksNorthAmerican,
        Text::TunnelsLabel,
        Text::TunnelsNone,
        Text::TunnelsMixed,
        Text::TunnelsSodium,
        Text::TunnelsLed,
        Text::CarBodiesLabel,
        Text::CarBodiesTransparent,
        Text::CarBodiesTranslucent,
        Text::CarBodiesBlack,
        Text::LanguageLabel,
        Text::LanguageAuto,
        Text::LanguageEnglish,
        Text::LanguageJapanese,
        Text::Ok,
        Text::Cancel,
        Text::SaveFailed,
        Text::Stopped,
    ];

    /// The string in `language`.
    pub fn get(self, language: DisplayLanguage) -> &'static str {
        let (english, japanese) = self.translations();
        match language {
            DisplayLanguage::English => english,
            DisplayLanguage::Japanese => japanese,
        }
    }

    /// The string in `language` with `{error}` replaced by `error`.
    pub fn with_error(self, language: DisplayLanguage, error: impl Display) -> String {
        self.get(language)
            .replace(ERROR_PLACEHOLDER, &error.to_string())
    }

    /// The English and Japanese strings.
    fn translations(self) -> (&'static str, &'static str) {
        match self {
            Text::DialogTitle => ("Night Freeway SS Settings", "Night Freeway SS の設定"),
            Text::SpeedLabel => ("Cruising speed:", "巡航速度:"),
            Text::DrivingSideLabel => ("Driving side:", "通行方向:"),
            Text::LeftHandTraffic => ("Left-hand traffic", "左側通行"),
            Text::RightHandTraffic => ("Right-hand traffic", "右側通行"),
            Text::Speedometer => ("Show speedometer", "速度計を表示する"),
            Text::Ufo => ("Show a UFO now and then", "ときどき UFO を表示する"),
            Text::Mountains => ("Show distant mountains", "遠くの山並みを表示する"),
            Text::VarySpeed => ("Vary the cruising speed", "巡航速度をゆっくり変える"),
            Text::TrafficByTime => (
                "Traffic follows the time of day",
                "時刻に合わせて交通量を変える",
            ),
            Text::SceneryZones => ("Alternate mountains and plains", "山あいと平野を交互に走る"),
            Text::TrucksLabel => ("Trucks:", "トラック:"),
            Text::TrucksNone => ("None", "なし"),
            Text::TrucksJapanese => ("Japanese style", "日本風"),
            Text::TrucksNorthAmerican => ("North American style", "北米風"),
            Text::TunnelsLabel => ("Tunnels:", "トンネル:"),
            Text::TunnelsNone => ("None", "なし"),
            Text::TunnelsMixed => ("Mixed lighting", "混在"),
            Text::TunnelsSodium => ("Sodium (orange)", "ナトリウム灯(橙)"),
            Text::TunnelsLed => ("LED (white)", "LED(白)"),
            Text::CarBodiesLabel => ("Car bodies:", "他車の車体:"),
            Text::CarBodiesTransparent => ("Transparent (lights only)", "透明(灯火だけ)"),
            Text::CarBodiesTranslucent => ("Translucent black", "半透明の黒"),
            Text::CarBodiesBlack => ("Black", "黒"),
            Text::LanguageLabel => ("Language:", "言語:"),
            Text::LanguageAuto => (
                "Automatic (Windows display language)",
                "自動(Windows の表示言語)",
            ),
            // The names of the languages are written in the languages
            // themselves, whichever language is shown.
            Text::LanguageEnglish => ("English", "English"),
            Text::LanguageJapanese => ("日本語", "日本語"),
            Text::Ok => ("OK", "OK"),
            Text::Cancel => ("Cancel", "キャンセル"),
            Text::SaveFailed => (
                "The settings could not be saved: {error}",
                "設定を保存できませんでした: {error}",
            ),
            Text::Stopped => (
                "Night Freeway SS stopped: {error}",
                "Night Freeway SS が停止しました: {error}",
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LANGUAGES: [DisplayLanguage; 2] = [DisplayLanguage::English, DisplayLanguage::Japanese];

    #[test]
    fn automatic_follows_the_windows_language() {
        // Japanese (Japan), English (US), German, and Japanese with a
        // sub-language other than the default.
        assert_eq!(
            DisplayLanguage::resolve(Language::Auto, 0x0411),
            DisplayLanguage::Japanese
        );
        assert_eq!(
            DisplayLanguage::resolve(Language::Auto, 0x0011),
            DisplayLanguage::Japanese
        );
        for id in [0x0409, 0x0407, 0x0809, 0] {
            assert_eq!(
                DisplayLanguage::resolve(Language::Auto, id),
                DisplayLanguage::English,
                "{id:#06x}"
            );
        }
    }

    #[test]
    fn an_explicit_language_ignores_windows() {
        for id in [0x0411, 0x0409, 0x0407, 0] {
            assert_eq!(
                DisplayLanguage::resolve(Language::English, id),
                DisplayLanguage::English
            );
            assert_eq!(
                DisplayLanguage::resolve(Language::Japanese, id),
                DisplayLanguage::Japanese
            );
        }
    }

    #[test]
    fn every_key_has_a_string_in_every_language() {
        for text in Text::ALL {
            for language in LANGUAGES {
                assert!(!text.get(language).is_empty(), "{text:?} {language:?}");
            }
        }
    }

    #[test]
    fn all_lists_every_key_once() {
        for (index, text) in Text::ALL.iter().enumerate() {
            assert!(!Text::ALL[..index].contains(text), "{text:?}");
        }
    }

    #[test]
    fn messages_with_an_error_have_one_placeholder() {
        for text in Text::ALL {
            for language in LANGUAGES {
                let count = text.get(language).matches(ERROR_PLACEHOLDER).count();
                let expected = usize::from(matches!(text, Text::SaveFailed | Text::Stopped));
                assert_eq!(count, expected, "{text:?} {language:?}");
            }
        }
    }

    #[test]
    fn the_error_is_put_into_the_message() {
        assert_eq!(
            Text::SaveFailed.with_error(DisplayLanguage::English, "denied"),
            "The settings could not be saved: denied"
        );
        assert_eq!(
            Text::Stopped.with_error(DisplayLanguage::Japanese, "denied"),
            "Night Freeway SS が停止しました: denied"
        );
    }

    #[test]
    fn language_names_are_written_in_their_own_language() {
        for language in LANGUAGES {
            assert_eq!(Text::LanguageEnglish.get(language), "English");
            assert_eq!(Text::LanguageJapanese.get(language), "日本語");
        }
    }
}
