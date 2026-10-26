//! Settings in the registry, under `HKEY_CURRENT_USER\Software\NightFreewaySS`
//! (docs/adr/0004-store-settings-in-registry.md). Every setting is a
//! `REG_DWORD` value.

use std::ffi::c_void;

use freeway_core::Settings;
use windows::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, REG_DWORD, RRF_RT_REG_DWORD, RegGetValueW, RegSetKeyValueW,
};
use windows::core::{PCWSTR, Result, w};

const KEY: PCWSTR = w!(r"Software\NightFreewaySS");
const SPEED: PCWSTR = w!("SpeedKmh");
const LEFT_HAND_TRAFFIC: PCWSTR = w!("LeftHandTraffic");
const SHOW_SPEEDOMETER: PCWSTR = w!("ShowSpeedometer");
const SHOW_UFO: PCWSTR = w!("ShowUfo");
const SHOW_MOUNTAINS: PCWSTR = w!("ShowMountains");
const TRUCKS: PCWSTR = w!("Trucks");
const TUNNELS: PCWSTR = w!("Tunnels");
const LANGUAGE: PCWSTR = w!("Language");

/// Reads the saved settings; anything missing or invalid is the default.
pub fn load() -> Settings {
    load_from(HKEY_CURRENT_USER, KEY)
}

/// Saves `settings`, creating the key when it does not exist yet.
pub fn save(settings: &Settings) -> Result<()> {
    save_to(HKEY_CURRENT_USER, KEY, settings)
}

fn load_from(root: HKEY, key: PCWSTR) -> Settings {
    Settings::from_raw(
        read_dword(root, key, SPEED),
        read_dword(root, key, LEFT_HAND_TRAFFIC),
        read_dword(root, key, SHOW_SPEEDOMETER),
        read_dword(root, key, SHOW_UFO),
        read_dword(root, key, SHOW_MOUNTAINS),
        read_dword(root, key, TRUCKS),
        read_dword(root, key, TUNNELS),
        read_dword(root, key, LANGUAGE),
    )
}

fn save_to(root: HKEY, key: PCWSTR, settings: &Settings) -> Result<()> {
    write_dword(root, key, SPEED, settings.speed_kmh)?;
    write_dword(
        root,
        key,
        LEFT_HAND_TRAFFIC,
        u32::from(settings.left_hand_traffic),
    )?;
    write_dword(
        root,
        key,
        SHOW_SPEEDOMETER,
        u32::from(settings.show_speedometer),
    )?;
    write_dword(root, key, SHOW_UFO, u32::from(settings.show_ufo))?;
    write_dword(
        root,
        key,
        SHOW_MOUNTAINS,
        u32::from(settings.show_mountains),
    )?;
    write_dword(root, key, TRUCKS, settings.trucks.to_raw())?;
    write_dword(root, key, TUNNELS, settings.tunnels.to_raw())?;
    write_dword(root, key, LANGUAGE, settings.language.to_raw())
}

/// Returns `None` when the key or value is missing, or is not a `REG_DWORD`.
/// Why: no error is shown for a bad value, the screensaver just runs with
/// the default (ADR-0004), so the cause of the failure is not needed.
fn read_dword(root: HKEY, key: PCWSTR, name: PCWSTR) -> Option<u32> {
    let mut value = 0u32;
    let mut size = size_of::<u32>() as u32;
    // RRF_RT_REG_DWORD makes the call fail for any other type of value.
    let status = unsafe {
        RegGetValueW(
            root,
            key,
            name,
            RRF_RT_REG_DWORD,
            None,
            Some((&raw mut value).cast::<c_void>()),
            Some(&raw mut size),
        )
    };
    status.is_ok().then_some(value)
}

fn write_dword(root: HKEY, key: PCWSTR, name: PCWSTR, value: u32) -> Result<()> {
    unsafe {
        RegSetKeyValueW(
            root,
            key,
            name,
            REG_DWORD.0,
            Some((&raw const value).cast::<c_void>()),
            size_of::<u32>() as u32,
        )
        .ok()
    }
}

#[cfg(test)]
mod tests {
    use freeway_core::settings::DEFAULT_SPEED_KMH;
    use freeway_core::{Language, TruckStyle, TunnelLighting};
    use windows::Win32::System::Registry::{REG_SZ, RegDeleteTreeW};
    use windows::core::HSTRING;

    use super::*;

    /// A key of its own, so that the real settings are never touched; it is
    /// deleted again when the test ends, even when the test fails.
    struct TestKey {
        path: HSTRING,
    }

    impl TestKey {
        fn new(test: &str) -> Self {
            let path = format!(
                r"Software\NightFreewaySS-Test-{}-{test}",
                std::process::id()
            );
            Self {
                path: HSTRING::from(path),
            }
        }

        fn key(&self) -> PCWSTR {
            PCWSTR(self.path.as_ptr())
        }

        fn load(&self) -> Settings {
            load_from(HKEY_CURRENT_USER, self.key())
        }
    }

    impl Drop for TestKey {
        fn drop(&mut self) {
            // The key is missing when the test never wrote to it.
            let _ = unsafe { RegDeleteTreeW(HKEY_CURRENT_USER, self.key()) };
        }
    }

    #[test]
    fn a_missing_key_gives_the_defaults() {
        let test = TestKey::new("missing");
        assert_eq!(test.load(), Settings::default());
    }

    #[test]
    fn saved_settings_are_read_back() -> Result<()> {
        let test = TestKey::new("round-trip");
        let settings = Settings {
            speed_kmh: 60,
            left_hand_traffic: false,
            show_speedometer: true,
            show_ufo: true,
            show_mountains: false,
            trucks: TruckStyle::NorthAmerican,
            tunnels: TunnelLighting::Led,
            language: Language::Japanese,
        };
        save_to(HKEY_CURRENT_USER, test.key(), &settings)?;
        assert_eq!(test.load(), settings);

        // Saving again replaces the values.
        save_to(HKEY_CURRENT_USER, test.key(), &Settings::default())?;
        assert_eq!(test.load(), Settings::default());
        Ok(())
    }

    #[test]
    fn invalid_values_fall_back_one_by_one() -> Result<()> {
        let test = TestKey::new("invalid");
        write_dword(HKEY_CURRENT_USER, test.key(), SPEED, 12345)?;
        write_dword(HKEY_CURRENT_USER, test.key(), LEFT_HAND_TRAFFIC, 0)?;
        write_dword(HKEY_CURRENT_USER, test.key(), SHOW_SPEEDOMETER, 9)?;
        write_dword(HKEY_CURRENT_USER, test.key(), SHOW_UFO, 2)?;
        write_dword(HKEY_CURRENT_USER, test.key(), SHOW_MOUNTAINS, 2)?;
        write_dword(HKEY_CURRENT_USER, test.key(), TRUCKS, 3)?;
        write_dword(HKEY_CURRENT_USER, test.key(), TUNNELS, 4)?;
        write_dword(HKEY_CURRENT_USER, test.key(), LANGUAGE, 3)?;
        let settings = test.load();
        assert_eq!(settings.speed_kmh, DEFAULT_SPEED_KMH);
        assert!(!settings.left_hand_traffic);
        assert!(!settings.show_speedometer);
        assert!(!settings.show_ufo);
        assert!(settings.show_mountains);
        assert_eq!(settings.trucks, TruckStyle::Japanese);
        assert_eq!(settings.tunnels, TunnelLighting::Mixed);
        assert_eq!(settings.language, Language::Auto);
        Ok(())
    }

    #[test]
    fn the_mountains_flag_is_read_back() -> Result<()> {
        let test = TestKey::new("mountains");
        for show_mountains in [false, true] {
            let settings = Settings {
                show_mountains,
                ..Settings::default()
            };
            save_to(HKEY_CURRENT_USER, test.key(), &settings)?;
            assert_eq!(test.load().show_mountains, show_mountains);
        }
        // Without the value the default (on) is used.
        let missing = TestKey::new("mountains-missing");
        write_dword(HKEY_CURRENT_USER, missing.key(), SPEED, 60)?;
        assert!(missing.load().show_mountains);
        Ok(())
    }

    #[test]
    fn every_truck_style_is_read_back() -> Result<()> {
        let test = TestKey::new("trucks");
        for trucks in TruckStyle::ALL {
            let settings = Settings {
                trucks,
                ..Settings::default()
            };
            save_to(HKEY_CURRENT_USER, test.key(), &settings)?;
            assert_eq!(test.load().trucks, trucks);
        }
        // Without the value, or with one out of range, the default (Japanese
        // style) is used.
        let missing = TestKey::new("trucks-missing");
        write_dword(HKEY_CURRENT_USER, missing.key(), SPEED, 60)?;
        assert_eq!(missing.load().trucks, TruckStyle::Japanese);
        let invalid = TestKey::new("trucks-invalid");
        for raw in [3, u32::MAX] {
            write_dword(HKEY_CURRENT_USER, invalid.key(), TRUCKS, raw)?;
            assert_eq!(invalid.load().trucks, TruckStyle::Japanese, "{raw}");
        }
        Ok(())
    }

    #[test]
    fn every_tunnel_choice_is_read_back() -> Result<()> {
        let test = TestKey::new("tunnels");
        for tunnels in TunnelLighting::ALL {
            let settings = Settings {
                tunnels,
                ..Settings::default()
            };
            save_to(HKEY_CURRENT_USER, test.key(), &settings)?;
            assert_eq!(test.load().tunnels, tunnels);
        }
        // Without the value, or with one out of range, the default (mixed
        // lighting) is used.
        let missing = TestKey::new("tunnels-missing");
        write_dword(HKEY_CURRENT_USER, missing.key(), SPEED, 60)?;
        assert_eq!(missing.load().tunnels, TunnelLighting::Mixed);
        let invalid = TestKey::new("tunnels-invalid");
        for raw in [4, u32::MAX] {
            write_dword(HKEY_CURRENT_USER, invalid.key(), TUNNELS, raw)?;
            assert_eq!(invalid.load().tunnels, TunnelLighting::Mixed, "{raw}");
        }
        Ok(())
    }

    #[test]
    fn every_language_is_read_back() -> Result<()> {
        let test = TestKey::new("language");
        for language in Language::ALL {
            let settings = Settings {
                language,
                ..Settings::default()
            };
            save_to(HKEY_CURRENT_USER, test.key(), &settings)?;
            assert_eq!(test.load().language, language);
        }
        Ok(())
    }

    #[test]
    fn a_value_of_the_wrong_type_gives_the_default() -> Result<()> {
        let test = TestKey::new("wrong-type");
        write_dword(HKEY_CURRENT_USER, test.key(), LEFT_HAND_TRAFFIC, 0)?;
        // "60" as a UTF-16 string including the terminator.
        let text: Vec<u16> = "60\0".encode_utf16().collect();
        unsafe {
            RegSetKeyValueW(
                HKEY_CURRENT_USER,
                test.key(),
                SPEED,
                REG_SZ.0,
                Some(text.as_ptr().cast::<c_void>()),
                (text.len() * size_of::<u16>()) as u32,
            )
            .ok()?;
        }
        let settings = test.load();
        assert_eq!(settings.speed_kmh, DEFAULT_SPEED_KMH);
        assert!(!settings.left_hand_traffic);
        Ok(())
    }
}
