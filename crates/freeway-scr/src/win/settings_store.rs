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
/// Written by versions before 1.10, which only said whether to show the
/// speedometer. It is still written, so that going back to such a version
/// keeps the speedometer on or off, and read when `Speedometer` is missing.
const SHOW_SPEEDOMETER: PCWSTR = w!("ShowSpeedometer");
const SPEEDOMETER: PCWSTR = w!("Speedometer");
const SHOW_UFO: PCWSTR = w!("ShowUfo");
const SHOW_MOUNTAINS: PCWSTR = w!("ShowMountains");
const TRUCKS: PCWSTR = w!("Trucks");
const TUNNELS: PCWSTR = w!("Tunnels");
const LANGUAGE: PCWSTR = w!("Language");
const VARY_SPEED: PCWSTR = w!("VarySpeed");
const TRAFFIC_BY_TIME: PCWSTR = w!("TrafficByTime");
const SCENERY_ZONES: PCWSTR = w!("SceneryZones");
/// Written by versions before 1.11, which had one choice for the bodies of
/// the cars. It is still written, so that going back to such a version keeps
/// whether the cars have bodies, and read when `CarShapes` is missing.
const CAR_BODIES: PCWSTR = w!("CarBodies");
const CAR_SHAPES: PCWSTR = w!("CarShapes");
const CAR_COLOR: PCWSTR = w!("CarColor");
const LANE_CHANGES: PCWSTR = w!("LaneChanges");
const SPAN_MONITORS: PCWSTR = w!("SpanMonitors");

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
        read_dword(root, key, VARY_SPEED),
        read_dword(root, key, TRAFFIC_BY_TIME),
        read_dword(root, key, SCENERY_ZONES),
        read_dword(root, key, CAR_BODIES),
        read_dword(root, key, SPEEDOMETER),
        read_dword(root, key, CAR_SHAPES),
        read_dword(root, key, CAR_COLOR),
        read_dword(root, key, LANE_CHANGES),
    )
    .with_span_monitors_raw(read_dword(root, key, SPAN_MONITORS))
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
        u32::from(settings.speedometer.to_legacy_flag()),
    )?;
    write_dword(root, key, SPEEDOMETER, settings.speedometer.to_raw())?;
    write_dword(root, key, SHOW_UFO, u32::from(settings.show_ufo))?;
    write_dword(
        root,
        key,
        SHOW_MOUNTAINS,
        u32::from(settings.show_mountains),
    )?;
    write_dword(root, key, TRUCKS, settings.trucks.to_raw())?;
    write_dword(root, key, TUNNELS, settings.tunnels.to_raw())?;
    write_dword(root, key, LANGUAGE, settings.language.to_raw())?;
    write_dword(root, key, VARY_SPEED, u32::from(settings.vary_speed))?;
    write_dword(
        root,
        key,
        TRAFFIC_BY_TIME,
        u32::from(settings.traffic_by_time),
    )?;
    write_dword(root, key, SCENERY_ZONES, u32::from(settings.scenery_zones))?;
    write_dword(root, key, CAR_SHAPES, settings.car_shapes.to_raw())?;
    write_dword(root, key, CAR_COLOR, settings.car_color.to_raw())?;
    write_dword(root, key, LANE_CHANGES, settings.lane_changes.to_raw())?;
    write_dword(root, key, SPAN_MONITORS, u32::from(settings.span_monitors))?;
    write_dword(root, key, CAR_BODIES, settings.legacy_car_bodies())
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
    use freeway_core::{
        CarColor, CarShapes, LaneChanges, Language, SpeedometerStyle, TruckStyle, TunnelLighting,
    };
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
            speedometer: SpeedometerStyle::DetailedDashboard,
            show_ufo: true,
            show_mountains: false,
            trucks: TruckStyle::NorthAmerican,
            tunnels: TunnelLighting::Led,
            car_shapes: CarShapes::Blocks,
            car_color: CarColor::Black,
            lane_changes: LaneChanges::OwnCar,
            language: Language::Japanese,
            vary_speed: false,
            traffic_by_time: false,
            scenery_zones: false,
            span_monitors: true,
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
        write_dword(HKEY_CURRENT_USER, test.key(), SPEEDOMETER, 4)?;
        write_dword(HKEY_CURRENT_USER, test.key(), SHOW_UFO, 2)?;
        write_dword(HKEY_CURRENT_USER, test.key(), SHOW_MOUNTAINS, 2)?;
        write_dword(HKEY_CURRENT_USER, test.key(), TRUCKS, 3)?;
        write_dword(HKEY_CURRENT_USER, test.key(), TUNNELS, 4)?;
        write_dword(HKEY_CURRENT_USER, test.key(), LANGUAGE, 3)?;
        write_dword(HKEY_CURRENT_USER, test.key(), VARY_SPEED, 2)?;
        write_dword(HKEY_CURRENT_USER, test.key(), TRAFFIC_BY_TIME, 7)?;
        write_dword(HKEY_CURRENT_USER, test.key(), SCENERY_ZONES, u32::MAX)?;
        write_dword(HKEY_CURRENT_USER, test.key(), CAR_SHAPES, 4)?;
        write_dword(HKEY_CURRENT_USER, test.key(), CAR_COLOR, 2)?;
        write_dword(HKEY_CURRENT_USER, test.key(), LANE_CHANGES, 3)?;
        write_dword(HKEY_CURRENT_USER, test.key(), SPAN_MONITORS, 2)?;
        let settings = test.load();
        assert_eq!(settings.speed_kmh, DEFAULT_SPEED_KMH);
        assert!(!settings.left_hand_traffic);
        assert_eq!(settings.speedometer, SpeedometerStyle::None);
        assert!(!settings.show_ufo);
        assert!(settings.show_mountains);
        assert_eq!(settings.trucks, TruckStyle::Japanese);
        assert_eq!(settings.tunnels, TunnelLighting::Mixed);
        assert_eq!(settings.car_shapes, CarShapes::None);
        assert_eq!(settings.car_color, CarColor::TranslucentBlack);
        assert_eq!(settings.lane_changes, LaneChanges::All);
        assert_eq!(settings.language, Language::Auto);
        assert!(settings.vary_speed && settings.traffic_by_time && settings.scenery_zones);
        assert!(settings.span_monitors);
        Ok(())
    }

    #[test]
    fn spanning_the_monitors_is_read_back_and_on_by_default() -> Result<()> {
        let test = TestKey::new("span-monitors");
        assert!(test.load().span_monitors);
        for span_monitors in [true, false] {
            let settings = Settings {
                span_monitors,
                ..Settings::default()
            };
            save_to(HKEY_CURRENT_USER, test.key(), &settings)?;
            assert_eq!(test.load().span_monitors, span_monitors);
        }
        // A value that is not 0 or 1 is the default.
        for raw in [2, u32::MAX] {
            write_dword(HKEY_CURRENT_USER, test.key(), SPAN_MONITORS, raw)?;
            assert!(test.load().span_monitors, "{raw}");
        }
        Ok(())
    }

    #[test]
    fn every_lane_changes_choice_is_read_back() -> Result<()> {
        let test = TestKey::new("lane-changes");
        for lane_changes in LaneChanges::ALL {
            let settings = Settings {
                lane_changes,
                ..Settings::default()
            };
            save_to(HKEY_CURRENT_USER, test.key(), &settings)?;
            assert_eq!(test.load().lane_changes, lane_changes);
        }
        // Without the value, or with one that is not 0, 1 or 2, lane changes are on.
        let missing = TestKey::new("lane-changes-missing");
        write_dword(HKEY_CURRENT_USER, missing.key(), SPEED, 60)?;
        assert_eq!(missing.load().lane_changes, LaneChanges::All);
        let invalid = TestKey::new("lane-changes-invalid");
        for raw in [3, u32::MAX] {
            write_dword(HKEY_CURRENT_USER, invalid.key(), LANE_CHANGES, raw)?;
            assert_eq!(invalid.load().lane_changes, LaneChanges::All, "{raw}");
        }
        Ok(())
    }

    #[test]
    fn every_speedometer_style_is_read_back() -> Result<()> {
        let test = TestKey::new("speedometer");
        for speedometer in SpeedometerStyle::ALL {
            let settings = Settings {
                speedometer,
                ..Settings::default()
            };
            save_to(HKEY_CURRENT_USER, test.key(), &settings)?;
            assert_eq!(test.load().speedometer, speedometer);
        }
        Ok(())
    }

    #[test]
    fn the_old_flag_is_written_for_earlier_versions() -> Result<()> {
        let test = TestKey::new("speedometer-flag");
        for (speedometer, flag) in SpeedometerStyle::ALL.into_iter().zip([0, 1, 1, 1]) {
            let settings = Settings {
                speedometer,
                ..Settings::default()
            };
            save_to(HKEY_CURRENT_USER, test.key(), &settings)?;
            assert_eq!(
                read_dword(HKEY_CURRENT_USER, test.key(), SHOW_SPEEDOMETER),
                Some(flag),
                "{speedometer:?}"
            );
        }
        Ok(())
    }

    #[test]
    fn the_old_flag_alone_carries_over_as_digits_only() -> Result<()> {
        // Settings saved by a version before 1.10 have ShowSpeedometer only.
        let on = TestKey::new("speedometer-old-on");
        write_dword(HKEY_CURRENT_USER, on.key(), SHOW_SPEEDOMETER, 1)?;
        assert_eq!(on.load().speedometer, SpeedometerStyle::Digits);

        let off = TestKey::new("speedometer-old-off");
        write_dword(HKEY_CURRENT_USER, off.key(), SHOW_SPEEDOMETER, 0)?;
        assert_eq!(off.load().speedometer, SpeedometerStyle::None);

        let invalid = TestKey::new("speedometer-old-invalid");
        write_dword(HKEY_CURRENT_USER, invalid.key(), SHOW_SPEEDOMETER, 2)?;
        assert_eq!(invalid.load().speedometer, SpeedometerStyle::None);

        // The new value wins when both are stored.
        let both = TestKey::new("speedometer-both");
        write_dword(HKEY_CURRENT_USER, both.key(), SHOW_SPEEDOMETER, 1)?;
        write_dword(HKEY_CURRENT_USER, both.key(), SPEEDOMETER, 0)?;
        assert_eq!(both.load().speedometer, SpeedometerStyle::None);
        write_dword(HKEY_CURRENT_USER, both.key(), SPEEDOMETER, 2)?;
        assert_eq!(both.load().speedometer, SpeedometerStyle::SimpleDashboard);

        // A value out of range gives the default, whatever the old flag says.
        let out_of_range = TestKey::new("speedometer-out-of-range");
        write_dword(HKEY_CURRENT_USER, out_of_range.key(), SHOW_SPEEDOMETER, 1)?;
        for raw in [4, u32::MAX] {
            write_dword(HKEY_CURRENT_USER, out_of_range.key(), SPEEDOMETER, raw)?;
            assert_eq!(out_of_range.load().speedometer, SpeedometerStyle::None);
        }
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
    fn the_variation_flags_are_read_back_one_by_one() -> Result<()> {
        let test = TestKey::new("variations");
        for (vary_speed, traffic_by_time, scenery_zones) in [
            (false, false, false),
            (true, false, false),
            (false, true, false),
            (false, false, true),
            (true, true, true),
        ] {
            let settings = Settings {
                vary_speed,
                traffic_by_time,
                scenery_zones,
                ..Settings::default()
            };
            save_to(HKEY_CURRENT_USER, test.key(), &settings)?;
            let loaded = test.load();
            assert_eq!(loaded.vary_speed, vary_speed);
            assert_eq!(loaded.traffic_by_time, traffic_by_time);
            assert_eq!(loaded.scenery_zones, scenery_zones);
        }
        // Without the values, or with one that is not 0 or 1, each is on.
        let missing = TestKey::new("variations-missing");
        write_dword(HKEY_CURRENT_USER, missing.key(), SPEED, 60)?;
        let loaded = missing.load();
        assert!(loaded.vary_speed && loaded.traffic_by_time && loaded.scenery_zones);
        let invalid = TestKey::new("variations-invalid");
        for raw in [2, u32::MAX] {
            for name in [VARY_SPEED, TRAFFIC_BY_TIME, SCENERY_ZONES] {
                write_dword(HKEY_CURRENT_USER, invalid.key(), name, raw)?;
            }
            let loaded = invalid.load();
            assert!(loaded.vary_speed && loaded.traffic_by_time && loaded.scenery_zones);
        }
        Ok(())
    }

    #[test]
    fn every_car_shape_and_color_is_read_back() -> Result<()> {
        let test = TestKey::new("car-shapes");
        for car_shapes in CarShapes::ALL {
            for car_color in CarColor::ALL {
                let settings = Settings {
                    car_shapes,
                    car_color,
                    ..Settings::default()
                };
                save_to(HKEY_CURRENT_USER, test.key(), &settings)?;
                let loaded = test.load();
                assert_eq!(
                    (loaded.car_shapes, loaded.car_color),
                    (car_shapes, car_color)
                );
            }
        }
        // Without the values, the bodies are off and translucent black.
        let missing = TestKey::new("car-shapes-missing");
        write_dword(HKEY_CURRENT_USER, missing.key(), SPEED, 60)?;
        assert_eq!(missing.load().car_shapes, CarShapes::None);
        assert_eq!(missing.load().car_color, CarColor::TranslucentBlack);
        // With one out of range, each falls back by itself.
        let invalid = TestKey::new("car-shapes-invalid");
        for raw in [4, u32::MAX] {
            write_dword(HKEY_CURRENT_USER, invalid.key(), CAR_SHAPES, raw)?;
            write_dword(HKEY_CURRENT_USER, invalid.key(), CAR_COLOR, 1)?;
            let loaded = invalid.load();
            assert_eq!(loaded.car_shapes, CarShapes::None, "{raw}");
            assert_eq!(loaded.car_color, CarColor::Black, "{raw}");
            write_dword(HKEY_CURRENT_USER, invalid.key(), CAR_SHAPES, 2)?;
            write_dword(HKEY_CURRENT_USER, invalid.key(), CAR_COLOR, raw)?;
            let loaded = invalid.load();
            assert_eq!(loaded.car_shapes, CarShapes::Blocks, "{raw}");
            assert_eq!(loaded.car_color, CarColor::TranslucentBlack, "{raw}");
        }
        Ok(())
    }

    #[test]
    fn the_old_car_bodies_value_alone_carries_over() -> Result<()> {
        // Settings saved by a version before 1.11 have CarBodies only.
        for (raw, shapes, color) in [
            (0, CarShapes::None, CarColor::TranslucentBlack),
            (1, CarShapes::Blocks, CarColor::TranslucentBlack),
            (2, CarShapes::Blocks, CarColor::Black),
        ] {
            let test = TestKey::new(&format!("car-bodies-old-{raw}"));
            write_dword(HKEY_CURRENT_USER, test.key(), CAR_BODIES, raw)?;
            let loaded = test.load();
            assert_eq!(
                (loaded.car_shapes, loaded.car_color),
                (shapes, color),
                "{raw}"
            );
        }
        // A value out of range gives the defaults.
        let invalid = TestKey::new("car-bodies-old-invalid");
        for raw in [3, u32::MAX] {
            write_dword(HKEY_CURRENT_USER, invalid.key(), CAR_BODIES, raw)?;
            let loaded = invalid.load();
            assert_eq!(loaded.car_shapes, CarShapes::None, "{raw}");
            assert_eq!(loaded.car_color, CarColor::TranslucentBlack, "{raw}");
        }
        // The new values win when both are stored.
        let both = TestKey::new("car-bodies-both");
        write_dword(HKEY_CURRENT_USER, both.key(), CAR_BODIES, 2)?;
        write_dword(HKEY_CURRENT_USER, both.key(), CAR_SHAPES, 0)?;
        assert_eq!(both.load().car_shapes, CarShapes::None);
        write_dword(HKEY_CURRENT_USER, both.key(), CAR_SHAPES, 1)?;
        write_dword(HKEY_CURRENT_USER, both.key(), CAR_COLOR, 0)?;
        let loaded = both.load();
        assert_eq!(
            (loaded.car_shapes, loaded.car_color),
            (CarShapes::Boxes, CarColor::TranslucentBlack)
        );
        Ok(())
    }

    #[test]
    fn the_old_car_bodies_value_is_written_for_earlier_versions() -> Result<()> {
        let test = TestKey::new("car-bodies-written");
        for (car_shapes, car_color, expected) in [
            (CarShapes::None, CarColor::TranslucentBlack, 0),
            (CarShapes::None, CarColor::Black, 0),
            (CarShapes::Boxes, CarColor::TranslucentBlack, 1),
            (CarShapes::Blocks, CarColor::TranslucentBlack, 1),
            (CarShapes::Boxes, CarColor::Black, 2),
            (CarShapes::Blocks, CarColor::Black, 2),
        ] {
            let settings = Settings {
                car_shapes,
                car_color,
                ..Settings::default()
            };
            save_to(HKEY_CURRENT_USER, test.key(), &settings)?;
            assert_eq!(
                read_dword(HKEY_CURRENT_USER, test.key(), CAR_BODIES),
                Some(expected),
                "{car_shapes:?} {car_color:?}"
            );
        }
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
