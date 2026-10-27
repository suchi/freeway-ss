//! Windows-specific screensaver host: windows, message loop and rendering.

mod dashboard;
mod renderer;
mod settings_dialog;
mod settings_store;
mod window;

use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use freeway_core::{DisplayLanguage, Language, Scene, Settings, SpeedometerStyle, Text};

use windows::Win32::Foundation::HWND;
use windows::Win32::Globalization::GetUserDefaultUILanguage;
use windows::Win32::System::SystemInformation::GetLocalTime;
use windows::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext,
};
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, IsWindow, MB_ICONERROR, MB_OK, MSG, MessageBoxW, PM_REMOVE, PeekMessageW,
    TranslateMessage, WM_QUIT,
};
use windows::core::{HSTRING, Result, w};

use crate::args::Mode;
use renderer::{Renderer, Target, is_device_lost};

/// Runs the screensaver in the requested mode until it should exit.
pub fn run(mode: Mode) -> Result<()> {
    // Why: render at the monitor's physical resolution. This must happen
    // before any window exists; a failure only means the awareness was
    // already set, so it is ignored.
    let _ = unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };

    match mode {
        Mode::Fullscreen => animate(&window::create_fullscreen_windows()?, None),
        Mode::Preview { parent } => {
            let parent = window::hwnd_from_handle(parent);
            match window::create_preview_window(parent)? {
                Some(hwnd) => animate(&[hwnd], Some(parent)),
                None => Ok(()),
            }
        }
        Mode::Configure { parent } => configure(parent.map(window::hwnd_from_handle)),
    }
}

/// Shows the settings dialog and saves the result when the user chose OK.
fn configure(owner: Option<HWND>) -> Result<()> {
    if let Some(settings) = settings_dialog::show(owner, settings_store::load())? {
        // Why: a save failure is shown here, with its cause, instead of
        // ending the program with the generic message of `main`. It uses the
        // language that was just chosen.
        if let Err(error) = settings_store::save(&settings) {
            let language = display_language(&settings);
            show_error(&Text::SaveFailed.with_error(language, error));
        }
    }
    Ok(())
}

/// The language to show `settings` in; `Auto` follows the Windows display
/// language.
pub fn display_language(settings: &Settings) -> DisplayLanguage {
    resolve_language(settings.language)
}

/// The language to show for the choice `language`.
pub fn resolve_language(language: Language) -> DisplayLanguage {
    DisplayLanguage::resolve(language, unsafe { GetUserDefaultUILanguage() })
}

/// The message for a run that ended with `error`, in the saved language.
pub fn stopped_message(error: &windows::core::Error) -> String {
    // Why: the error may happen before the settings are read, so they are read
    // here; a missing or invalid value gives the default language.
    let language = display_language(&settings_store::load());
    Text::Stopped.with_error(language, error)
}

/// Shows an error to the user; a screensaver has no console to print to.
pub fn show_error(message: &str) {
    unsafe {
        MessageBoxW(
            None,
            &HSTRING::from(message),
            w!("Night Freeway SS"),
            MB_OK | MB_ICONERROR,
        )
    };
}

fn animate(windows: &[HWND], preview_parent: Option<HWND>) -> Result<()> {
    let settings = settings_store::load();
    let mut renderer = Renderer::new()?;
    let mut targets = create_targets(&renderer, windows)?;
    // Why: each window gets its own scene (docs/adr/0003); the scenes live
    // outside the device resources, so losing the device does not restart them.
    let mut scenes = create_scenes(windows.len(), &settings);
    let mut last_frame = Instant::now();
    loop {
        if !pump_messages() {
            return Ok(());
        }
        if let Some(parent) = preview_parent
            && !unsafe { IsWindow(Some(parent)) }.as_bool()
        {
            return Ok(());
        }

        let now = Instant::now();
        let dt = now.duration_since(last_frame).as_secs_f32();
        last_frame = now;
        for scene in &mut scenes {
            scene.update(dt);
        }
        match draw_frame(&mut renderer, &mut targets, &scenes, settings.speedometer) {
            Ok(true) => {}
            // Nothing is visible (for example, the session is locked).
            Ok(false) => std::thread::sleep(Duration::from_millis(100)),
            Err(error) if is_device_lost(&error) => {
                // A window can own only one flip-model swap chain, so release
                // the old ones before creating new ones.
                targets.clear();
                renderer = Renderer::new()?;
                targets = create_targets(&renderer, windows)?;
            }
            Err(error) => return Err(error),
        }
    }
}

/// Draws every window once and returns whether any of them is visible.
fn draw_frame(
    renderer: &mut Renderer,
    targets: &mut [Target],
    scenes: &[Scene],
    speedometer: SpeedometerStyle,
) -> Result<bool> {
    let last = targets.len().saturating_sub(1);
    let mut visible = false;
    for (index, (target, scene)) in targets.iter_mut().zip(scenes).enumerate() {
        // Why: waiting for vertical sync only on the last window paces the
        // loop once per frame; waiting on every window would divide the
        // frame rate by the number of monitors.
        // The speed of a scene drifts, so each frame shows its current speed.
        visible |= renderer.draw_scene(target, scene, speedometer, index == last)?;
    }
    Ok(visible)
}

/// Average seconds between UFO visits in a build with the `ufo-demo` feature.
#[cfg(feature = "ufo-demo")]
const UFO_DEMO_MEAN_SECONDS: f32 = 20.0;

/// The local time of day in minutes after midnight, for the traffic.
fn local_minute_of_day() -> u32 {
    let time = unsafe { GetLocalTime() };
    u32::from(time.wHour) * 60 + u32::from(time.wMinute)
}

/// One scene per window, each with its own road.
fn create_scenes(count: usize, settings: &Settings) -> Vec<Scene> {
    let start_minute = local_minute_of_day();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_nanos() as u64);
    (0..count)
        .map(|index| {
            let seed = nanos ^ (index as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15);
            let config = freeway_core::SceneConfig {
                start_minute,
                ..settings.scene_config(seed)
            };
            #[cfg(feature = "ufo-demo")]
            let config = freeway_core::SceneConfig {
                ufo_mean_interval: config.ufo_mean_interval.map(|_| UFO_DEMO_MEAN_SECONDS),
                ..config
            };
            Scene::new(config)
        })
        .collect()
}

fn create_targets(renderer: &Renderer, windows: &[HWND]) -> Result<Vec<Target>> {
    windows
        .iter()
        .map(|&hwnd| renderer.create_target(hwnd))
        .collect()
}

/// Dispatches pending messages; returns `false` once the program should exit.
fn pump_messages() -> bool {
    let mut message = MSG::default();
    while unsafe { PeekMessageW(&mut message, None, 0, 0, PM_REMOVE) }.as_bool() {
        if message.message == WM_QUIT {
            return false;
        }
        unsafe {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use windows::Win32::UI::WindowsAndMessaging::{SWP_NOMOVE, SWP_NOZORDER, SetWindowPos};

    use super::*;
    use crate::speedometer;

    #[test]
    fn renders_and_resizes_a_hidden_window() -> Result<()> {
        // Large enough for stars and city lights to land inside.
        let hwnd = window::create_hidden_window(640, 360)?;
        let mut renderer = Renderer::new()?;
        let mut target = renderer.create_target(hwnd)?;
        let mut scene = create_scenes(1, &Settings::default()).remove(0);
        // Run until a shooting star burns, so that every kind of sky command
        // (dot, sky band, streak) reaches Direct2D.
        let mut steps = 0;
        while scene.meteors_visible() == 0 {
            scene.update(0.05);
            steps += 1;
            assert!(steps < 2000, "no shooting star within 100 seconds");
        }
        for _ in 0..3 {
            renderer.draw_scene(&mut target, &scene, SpeedometerStyle::None, true)?;
        }
        // Every speedometer needs its text to be laid out and drawn.
        for style in SpeedometerStyle::ALL {
            renderer.draw_scene(&mut target, &scene, style, true)?;
        }

        unsafe { SetWindowPos(hwnd, None, 0, 0, 800, 450, SWP_NOMOVE | SWP_NOZORDER)? };
        for style in SpeedometerStyle::ALL {
            renderer.draw_scene(&mut target, &scene, style, true)?;
        }
        Ok(())
    }

    #[test]
    fn draws_every_speedometer_in_windows_of_any_size() -> Result<()> {
        // The preview of the settings is the smallest window; the narrow one
        // is too small for the detailed dashboard, which is then left out.
        let mut renderer = Renderer::new()?;
        let scene = create_scenes(1, &Settings::default()).remove(0);
        for (width, height) in [(152, 112), (80, 600), (1920, 1080)] {
            let hwnd = window::create_hidden_window(width, height)?;
            let mut target = renderer.create_target(hwnd)?;
            for style in SpeedometerStyle::ALL {
                renderer.draw_scene(&mut target, &scene, style, false)?;
            }
        }
        Ok(())
    }

    #[test]
    fn draws_the_speedometer_at_the_speed_of_a_drifting_scene() -> Result<()> {
        let hwnd = window::create_hidden_window(640, 360)?;
        let mut renderer = Renderer::new()?;
        let mut target = renderer.create_target(hwnd)?;
        let mut scene = Scene::new(freeway_core::SceneConfig {
            seed: 3,
            vary_speed: true,
            ..Default::default()
        });
        // Ten minutes of driving, drawn every five seconds: the speed shown
        // changes, and a new speed needs its text to be laid out.
        let mut shown = std::collections::BTreeSet::new();
        for step in 0..12_000 {
            scene.update(0.05);
            if step % 100 == 99 {
                let speed = speedometer::displayed_speed(scene.speed_kmh());
                assert!((80..=120).contains(&speed), "{speed}");
                shown.insert(speed);
                for style in SpeedometerStyle::ALL {
                    renderer.draw_scene(&mut target, &scene, style, false)?;
                }
            }
        }
        assert!(shown.len() > 1, "{shown:?}");
        Ok(())
    }

    #[test]
    fn the_local_time_is_a_minute_of_the_day() {
        assert!(local_minute_of_day() < 24 * 60);
    }

    #[test]
    fn scenes_start_at_the_local_time() {
        let scene = create_scenes(1, &Settings::default()).remove(0);
        let minute = scene.minute_of_day();
        let now = local_minute_of_day() as f32;
        // Within a minute of now, also across midnight.
        let apart = (minute - now).abs().min(1440.0 - (minute - now).abs());
        assert!(apart <= 1.0, "{minute} {now}");
    }

    #[test]
    fn renders_mountains_with_their_lights() -> Result<()> {
        let hwnd = window::create_hidden_window(640, 360)?;
        let mut renderer = Renderer::new()?;
        let mut target = renderer.create_target(hwnd)?;
        // A fixed seed whose car faces a range within a few seconds.
        let mut scene = Scene::new(freeway_core::SceneConfig {
            seed: 8,
            ..Default::default()
        });
        // Drive until the car faces a range, so that silhouettes reach
        // Direct2D, which switches the blend mode for them.
        let mut steps = 0;
        while !scene.mountains_ahead() {
            scene.update(0.05);
            steps += 1;
            assert!(steps < 4000, "no mountains within 200 seconds");
        }
        // The lights blink every 2.5 seconds; draw through a whole period
        // so that lights and silhouettes are drawn together as well as alone.
        for _ in 0..6 {
            for _ in 0..10 {
                scene.update(0.05);
            }
            renderer.draw_scene(&mut target, &scene, SpeedometerStyle::None, true)?;
        }
        Ok(())
    }

    #[test]
    fn renders_a_tunnel_from_far_to_near_and_from_inside() -> Result<()> {
        let hwnd = window::create_hidden_window(640, 360)?;
        let mut renderer = Renderer::new()?;
        let mut target = renderer.create_target(hwnd)?;
        let mut scene = Scene::new(freeway_core::SceneConfig {
            seed: 11,
            speed_kmh: 160.0,
            ..Default::default()
        });
        // Draw while the entrance comes closer, so that the hill and its
        // opening (silhouettes) are drawn together with the lights, and then
        // from inside the tunnel, where there is no sky.
        let mut steps = 0;
        let mut drawn = (0, 0);
        while !(drawn.0 >= 20 && drawn.1 >= 20) {
            scene.update(0.05);
            steps += 1;
            assert!(steps < 40_000, "no tunnel within 2000 seconds");
            let near = scene.tunnel_entrance_ahead().is_some_and(|d| d < 400.0);
            if (near && drawn.0 < 20 && steps % 4 == 0) || (scene.in_tunnel() && drawn.1 < 20) {
                renderer.draw_scene(&mut target, &scene, SpeedometerStyle::None, true)?;
                if scene.in_tunnel() {
                    drawn.1 += 1;
                } else {
                    drawn.0 += 1;
                }
            }
        }
        Ok(())
    }

    #[test]
    fn renders_a_ufo_with_its_beam() -> Result<()> {
        let hwnd = window::create_hidden_window(640, 360)?;
        let mut renderer = Renderer::new()?;
        let mut target = renderer.create_target(hwnd)?;
        let mut scene = Scene::new(freeway_core::SceneConfig {
            seed: 2,
            ufo_mean_interval: Some(5.0),
            ..Default::default()
        });
        let mut steps = 0;
        while !scene.ufo_visible() {
            scene.update(0.05);
            steps += 1;
            assert!(steps < 40_000, "no UFO within 2000 seconds");
        }
        // 7 seconds into the visit the beam is down; draw before and after.
        for step in 0..150 {
            scene.update(0.05);
            if step % 50 == 49 {
                renderer.draw_scene(&mut target, &scene, SpeedometerStyle::None, true)?;
            }
        }
        Ok(())
    }

    #[test]
    fn renders_car_bodies_over_the_lights() -> Result<()> {
        use freeway_core::{CarColor, CarShapes};
        let hwnd = window::create_hidden_window(640, 360)?;
        let mut renderer = Renderer::new()?;
        let mut target = renderer.create_target(hwnd)?;
        let choices = CarShapes::ALL
            .into_iter()
            .flat_map(|shapes| CarColor::ALL.into_iter().map(move |color| (shapes, color)));
        for (car_shapes, car_color) in choices {
            let mut scene = Scene::new(freeway_core::SceneConfig {
                seed: 4,
                car_shapes,
                car_color,
                ..Default::default()
            });
            // Silhouettes of the bodies are drawn among the lights, with
            // their own opacity, and the additive blend must be back after
            // each of them.
            for _ in 0..20 {
                for _ in 0..10 {
                    scene.update(0.05);
                }
                renderer.draw_scene(&mut target, &scene, SpeedometerStyle::None, true)?;
            }
        }
        Ok(())
    }
}
