//! Windows-specific screensaver host: windows, message loop and rendering.

mod renderer;
mod window;

use std::time::{Duration, Instant};

use windows::Win32::Foundation::HWND;
use windows::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext,
};
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, IsWindow, MB_ICONERROR, MB_ICONINFORMATION, MB_OK, MSG, MessageBoxW,
    PM_REMOVE, PeekMessageW, TranslateMessage, WM_QUIT,
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
        Mode::Configure { parent } => {
            // The settings dialog is implemented in Phase 5.
            let owner = parent.map(window::hwnd_from_handle);
            unsafe {
                MessageBoxW(
                    owner,
                    w!("Night Freeway SS has no settings yet."),
                    w!("Night Freeway SS"),
                    MB_OK | MB_ICONINFORMATION,
                )
            };
            Ok(())
        }
    }
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
    let mut renderer = Renderer::new()?;
    let mut targets = create_targets(&renderer, windows)?;
    let start = Instant::now();
    loop {
        if !pump_messages() {
            return Ok(());
        }
        if let Some(parent) = preview_parent
            && !unsafe { IsWindow(Some(parent)) }.as_bool()
        {
            return Ok(());
        }

        let seconds = start.elapsed().as_secs_f32();
        match draw_frame(&renderer, &mut targets, seconds) {
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
fn draw_frame(renderer: &Renderer, targets: &mut [Target], seconds: f32) -> Result<bool> {
    let last = targets.len().saturating_sub(1);
    let mut visible = false;
    for (index, target) in targets.iter_mut().enumerate() {
        // Why: waiting for vertical sync only on the last window paces the
        // loop once per frame; waiting on every window would divide the
        // frame rate by the number of monitors.
        visible |= renderer.draw_test_frame(target, seconds, index == last)?;
    }
    Ok(visible)
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

    #[test]
    fn renders_and_resizes_a_hidden_window() -> Result<()> {
        let hwnd = window::create_hidden_window(64, 48)?;
        let renderer = Renderer::new()?;
        let mut target = renderer.create_target(hwnd)?;
        for frame in 0..3 {
            renderer.draw_test_frame(&mut target, frame as f32 * 0.25, true)?;
        }

        unsafe { SetWindowPos(hwnd, None, 0, 0, 80, 60, SWP_NOMOVE | SWP_NOZORDER)? };
        renderer.draw_test_frame(&mut target, 1.0, true)?;
        Ok(())
    }
}
