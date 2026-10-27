//! Windows-specific screensaver host: windows, message loop and rendering.

mod dashboard;
mod hint;
mod renderer;
mod settings_dialog;
mod settings_store;
mod window;

use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use freeway_core::{DisplayLanguage, Language, Scene, Settings, SpeedometerStyle, Text};

use crate::input::{Command, Hint};

use windows::Win32::Foundation::HWND;
use windows::Win32::Globalization::GetUserDefaultUILanguage;
use windows::Win32::System::Power::{
    ES_CONTINUOUS, ES_DISPLAY_REQUIRED, ES_SYSTEM_REQUIRED, SetThreadExecutionState,
};
use windows::Win32::System::SystemInformation::GetLocalTime;
use windows::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext,
};
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, IsIconic, IsWindow, MB_ICONERROR, MB_OK, MSG, MessageBoxW, PM_REMOVE,
    PeekMessageW, TranslateMessage, WM_QUIT,
};
use windows::core::{HSTRING, Result, w};

use crate::args::Mode;
use hint::Overlay;
use renderer::{Renderer, Target, is_device_lost};
use settings_dialog::Outcome;

/// Runs the screensaver in the requested mode until it should exit.
pub fn run(mode: Mode) -> Result<()> {
    // Why: render at the monitor's physical resolution. This must happen
    // before any window exists; a failure only means the awareness was
    // already set, so it is ignored.
    let _ = unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };

    match mode {
        Mode::Fullscreen { .. } => animate(&window::create_fullscreen_windows()?, Run::default()),
        Mode::Preview { parent } => {
            let parent = window::hwnd_from_handle(parent);
            match window::create_preview_window(parent)? {
                Some(hwnd) => animate(
                    &[hwnd],
                    Run {
                        preview_parent: Some(parent),
                        ..Run::default()
                    },
                ),
                None => Ok(()),
            }
        }
        Mode::Configure { parent } => configure(parent.map(window::hwnd_from_handle)),
        Mode::Window { minutes } => animate(&[window::create_normal_window()?], Run::keep(minutes)),
        Mode::Play { minutes, .. } => animate(&window::create_play_windows()?, Run::keep(minutes)),
    }
}

/// How a run differs from the screensaver.
#[derive(Default)]
struct Run {
    /// The window of the settings dialog that the preview lives in; the run
    /// ends when it is gone.
    preview_parent: Option<HWND>,
    /// How long the run lasts, if it ends by itself.
    duration: Option<Duration>,
    /// Whether the display and the system must not go to sleep meanwhile.
    keep_awake: bool,
    /// Whether the keys of a run work and the hint about them is shown.
    controls: bool,
}

impl Run {
    /// A run that is meant to be left on, for the `/window` and `/play`
    /// modes: it keeps the machine awake, and ends after `minutes` if given.
    fn keep(minutes: Option<u32>) -> Self {
        Self {
            duration: minutes.map(|minutes| Duration::from_secs(u64::from(minutes) * 60)),
            keep_awake: true,
            controls: true,
            ..Self::default()
        }
    }
}

/// Keeps the display and the system awake while it lives.
struct KeepAwake;

impl KeepAwake {
    fn new() -> Self {
        // Why: a screensaver is started by the idle time, and sleeps with the
        // rest of the display when it ends; a run that is left on has to ask
        // Windows to stay awake. The request belongs to this thread, which
        // runs the loop, and `ES_CONTINUOUS` makes it last until it is reset.
        unsafe {
            SetThreadExecutionState(ES_CONTINUOUS | ES_DISPLAY_REQUIRED | ES_SYSTEM_REQUIRED)
        };
        Self
    }
}

impl Drop for KeepAwake {
    fn drop(&mut self) {
        unsafe { SetThreadExecutionState(ES_CONTINUOUS) };
    }
}

/// Shows the settings dialog and saves the result when the user chose OK or
/// Start. Start then runs this program again in the mode that was chosen.
fn configure(owner: Option<HWND>) -> Result<()> {
    let (settings, start) = match settings_dialog::show(owner, settings_store::load())? {
        Outcome::Cancel => return Ok(()),
        Outcome::Save(settings) => (settings, None),
        Outcome::Start(settings, mode) => (settings, Some(mode)),
    };
    // Why: a save failure is shown here, with its cause, instead of
    // ending the program with the generic message of `main`. It uses the
    // language that was just chosen.
    if let Err(error) = settings_store::save(&settings) {
        let language = display_language(&settings);
        show_error(&Text::SaveFailed.with_error(language, error));
        // The new run reads the saved settings: it would show the old ones.
        return Ok(());
    }
    if let Some(mode) = start
        && let Err(error) = start_again(mode)
    {
        show_error(&Text::RunFailed.with_error(display_language(&settings), error));
    }
    Ok(())
}

/// Runs this program again in `mode`, as a process of its own that goes on
/// when this one ends.
fn start_again(mode: Mode) -> std::io::Result<()> {
    // Why: the program starts itself, by the path Windows reports for the
    // running executable, with arguments that `args::Mode::arguments` makes
    // and nothing from outside.
    std::process::Command::new(std::env::current_exe()?)
        .args(mode.arguments())
        .spawn()
        .map(|_| ())
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

fn animate(windows: &[HWND], run: Run) -> Result<()> {
    let _keep_awake = run.keep_awake.then(KeepAwake::new);
    let deadline = run.duration.map(|duration| Instant::now() + duration);
    let settings = settings_store::load();
    let language = display_language(&settings);
    let mut windows = windows.to_vec();
    let mut renderer = Renderer::new()?;
    let mut targets = create_targets(&renderer, &windows)?;
    // Why: each window gets its own scene (docs/adr/0003); the scenes live
    // outside the device resources, so losing the device does not restart them.
    let mut scenes = create_scenes(windows.len(), &settings);
    // The state that the keys of a run change. It is not saved: the next run
    // begins with the settings again.
    let mut paused = false;
    let mut left_hand_traffic = settings.left_hand_traffic;
    let mut hint = Hint::default();
    let mut last_frame = Instant::now();
    loop {
        if !pump_messages() {
            return Ok(());
        }
        if let Some(parent) = run.preview_parent
            && !unsafe { IsWindow(Some(parent)) }.as_bool()
        {
            return Ok(());
        }
        let now = Instant::now();
        if deadline.is_some_and(|deadline| now >= deadline) {
            return Ok(());
        }
        // Why: a minimized window has no picture to draw; the road stands
        // still meanwhile, and the time limit goes on.
        if windows
            .iter()
            .all(|&hwnd| unsafe { IsIconic(hwnd) }.as_bool())
        {
            std::thread::sleep(Duration::from_millis(100));
            last_frame = Instant::now();
            continue;
        }

        let dt = now.duration_since(last_frame).as_secs_f32();
        last_frame = now;
        let mut fullscreen_request = None;
        if run.controls {
            let input = window::take_input();
            if input.activity || input.speed_direction != 0.0 {
                hint.touch();
            }
            for (command, hwnd) in input.commands {
                match command {
                    Command::TogglePause => paused = !paused,
                    Command::ToggleSide => left_hand_traffic = !left_hand_traffic,
                    Command::CallUfo => scenes.iter_mut().for_each(Scene::call_ufo),
                    Command::ToggleFullscreen => fullscreen_request = Some(hwnd),
                }
            }
            for scene in &mut scenes {
                scene.set_paused(paused);
                scene.set_left_hand_traffic(left_hand_traffic);
                scene.change_speed(input.speed_direction, dt);
            }
            hint.update(dt);
        }
        for scene in &mut scenes {
            scene.update(dt);
        }
        if let Some(hwnd) = fullscreen_request {
            switch_fullscreen(hwnd, &mut windows, &mut targets, &mut scenes)?;
        }
        let overlay = run.controls.then(|| Overlay {
            text: if window::is_fullscreen() {
                Text::HintFullscreen
            } else {
                Text::HintInWindow
            }
            .get(language),
            paused: paused.then(|| Text::Paused.get(language)),
            opacity: hint.opacity(paused),
            language,
        });
        match draw_frame(
            &mut renderer,
            &mut targets,
            &scenes,
            settings.speedometer,
            overlay.as_ref(),
        ) {
            Ok(true) => {}
            // Nothing is visible (for example, the session is locked).
            Ok(false) => std::thread::sleep(Duration::from_millis(100)),
            Err(error) if is_device_lost(&error) => {
                // A window can own only one flip-model swap chain, so release
                // the old ones before creating new ones.
                targets.clear();
                renderer = Renderer::new()?;
                targets = create_targets(&renderer, &windows)?;
            }
            Err(error) => return Err(error),
        }
    }
}

/// Switches between the window and the full screen (the F key) in the window
/// `hwnd` that got the key, keeping its road. From the full screen of several
/// monitors only that window is kept, as a window; the others are closed.
fn switch_fullscreen(
    hwnd: HWND,
    windows: &mut Vec<HWND>,
    targets: &mut Vec<Target>,
    scenes: &mut Vec<Scene>,
) -> Result<()> {
    let Some(index) = windows.iter().position(|&window| window == hwnd) else {
        return Ok(());
    };
    if window::is_fullscreen() && windows.len() > 1 {
        let target = targets.swap_remove(index);
        let scene = scenes.swap_remove(index);
        windows.swap_remove(index);
        // Why: the swap chain of a window is released before the window is
        // destroyed.
        targets.clear();
        scenes.clear();
        for other in windows.drain(..) {
            window::close(other);
        }
        windows.push(hwnd);
        targets.push(target);
        scenes.push(scene);
    }
    window::toggle_fullscreen(hwnd)
}

/// Draws every window once and returns whether any of them is visible.
fn draw_frame(
    renderer: &mut Renderer,
    targets: &mut [Target],
    scenes: &[Scene],
    speedometer: SpeedometerStyle,
    overlay: Option<&Overlay>,
) -> Result<bool> {
    let last = targets.len().saturating_sub(1);
    let mut visible = false;
    for (index, (target, scene)) in targets.iter_mut().zip(scenes).enumerate() {
        // Why: waiting for vertical sync only on the last window paces the
        // loop once per frame; waiting on every window would divide the
        // frame rate by the number of monitors.
        // The speed of a scene drifts, so each frame shows its current speed.
        visible |=
            renderer.draw_scene_with_hint(target, scene, speedometer, overlay, index == last)?;
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
    fn a_run_with_a_time_limit_ends_by_itself() -> Result<()> {
        let hwnd = window::create_hidden_window(640, 360)?;
        let started = Instant::now();
        animate(
            &[hwnd],
            Run {
                duration: Some(Duration::from_millis(300)),
                keep_awake: true,
                ..Run::default()
            },
        )?;
        let lasted = started.elapsed();
        assert!(lasted >= Duration::from_millis(300), "{lasted:?}");
        assert!(lasted < Duration::from_secs(30), "{lasted:?}");
        Ok(())
    }

    #[test]
    fn the_minutes_of_a_run_are_a_duration() {
        assert_eq!(Run::keep(None).duration, None);
        assert_eq!(
            Run::keep(Some(30)).duration,
            Some(Duration::from_secs(30 * 60))
        );
        assert!(Run::keep(None).keep_awake);
        assert!(!Run::default().keep_awake);
    }

    #[test]
    fn the_display_stays_awake_only_while_the_guard_lives() {
        let guard = KeepAwake::new();
        // Setting a state returns the one before it.
        let during = unsafe {
            SetThreadExecutionState(ES_CONTINUOUS | ES_DISPLAY_REQUIRED | ES_SYSTEM_REQUIRED)
        };
        assert_ne!(during.0 & ES_DISPLAY_REQUIRED.0, 0);
        assert_ne!(during.0 & ES_SYSTEM_REQUIRED.0, 0);
        drop(guard);
        let after = unsafe { SetThreadExecutionState(ES_CONTINUOUS) };
        assert_eq!(after.0 & (ES_DISPLAY_REQUIRED.0 | ES_SYSTEM_REQUIRED.0), 0);
    }

    #[test]
    fn renders_in_the_window_of_the_window_mode_and_resizes_it() -> Result<()> {
        use windows::Win32::UI::WindowsAndMessaging::{DestroyWindow, SWP_NOMOVE, SWP_NOZORDER};
        let hwnd = window::create_normal_window()?;
        let mut renderer = Renderer::new()?;
        let mut target = renderer.create_target(hwnd)?;
        let scene = create_scenes(1, &Settings::default()).remove(0);
        for style in SpeedometerStyle::ALL {
            renderer.draw_scene(&mut target, &scene, style, false)?;
        }
        unsafe { SetWindowPos(hwnd, None, 0, 0, 900, 600, SWP_NOMOVE | SWP_NOZORDER)? };
        renderer.draw_scene(&mut target, &scene, SpeedometerStyle::None, false)?;
        unsafe { DestroyWindow(hwnd)? };
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

    #[test]
    fn draws_the_hint_in_both_languages_and_every_strength() -> Result<()> {
        let mut renderer = Renderer::new()?;
        let scene = create_scenes(1, &Settings::default()).remove(0);
        // The narrow window makes the text shrink to fit; the tiny one has
        // no room for text at all.
        for (width, height) in [(1280, 720), (320, 180), (152, 112)] {
            let hwnd = window::create_hidden_window(width, height)?;
            let mut target = renderer.create_target(hwnd)?;
            for language in [DisplayLanguage::English, DisplayLanguage::Japanese] {
                for (paused, opacity) in [(false, 1.0), (false, 0.4), (true, 1.0), (false, 0.0)] {
                    for text in [Text::HintInWindow, Text::HintFullscreen] {
                        let overlay = Overlay {
                            text: text.get(language),
                            paused: paused.then(|| Text::Paused.get(language)),
                            opacity,
                            language,
                        };
                        for style in SpeedometerStyle::ALL {
                            renderer.draw_scene_with_hint(
                                &mut target,
                                &scene,
                                style,
                                Some(&overlay),
                                false,
                            )?;
                        }
                    }
                }
            }
        }
        Ok(())
    }

    #[test]
    fn the_f_key_keeps_the_road_and_draws_in_the_window_and_in_the_full_screen() -> Result<()> {
        let hwnd = window::create_normal_window()?;
        let mut renderer = Renderer::new()?;
        let mut windows = vec![hwnd];
        let mut targets = vec![renderer.create_target(hwnd)?];
        let mut scenes = create_scenes(1, &Settings::default());
        for _ in 0..40 {
            scenes[0].update(0.05);
        }
        let travelled = scenes[0].track_s();
        for fullscreen in [true, false, true] {
            switch_fullscreen(hwnd, &mut windows, &mut targets, &mut scenes)?;
            assert_eq!(window::is_fullscreen(), fullscreen);
            assert_eq!((windows.len(), targets.len(), scenes.len()), (1, 1, 1));
            // The same road goes on in the new size.
            assert_eq!(scenes[0].track_s(), travelled);
            draw_frame(
                &mut renderer,
                &mut targets,
                &scenes,
                SpeedometerStyle::None,
                None,
            )?;
        }
        // A window that is not one of the run is left alone.
        let other = window::create_hidden_window(320, 180)?;
        switch_fullscreen(other, &mut windows, &mut targets, &mut scenes)?;
        assert!(window::is_fullscreen());
        unsafe { windows::Win32::UI::WindowsAndMessaging::DestroyWindow(other)? };
        Ok(())
    }
}
