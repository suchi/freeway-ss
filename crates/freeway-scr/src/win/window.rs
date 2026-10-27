//! Screensaver windows and the input handling that ends the screensaver.

use std::cell::{Cell, RefCell};
use std::ffi::c_void;
use std::time::{Duration, Instant};

use windows::Win32::Foundation::{
    E_FAIL, ERROR_CLASS_ALREADY_EXISTS, HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM,
};
use windows::Win32::Graphics::Gdi::{
    BLACK_BRUSH, EnumDisplayMonitors, GetMonitorInfoW, GetStockObject, HBRUSH, HDC, HMONITOR,
    MONITORINFO, SC_SCREENSAVE,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::{AdjustWindowRectExForDpi, GetDpiForSystem};
use windows::Win32::UI::Input::KeyboardAndMouse::VK_ESCAPE;
use windows::Win32::UI::WindowsAndMessaging::{
    CS_HREDRAW, CS_VREDRAW, CreateWindowExW, DefWindowProcW, GetClientRect, GetCursorPos,
    IDC_ARROW, IsWindow, LoadCursorW, PostQuitMessage, RegisterClassExW, SPI_GETWORKAREA,
    SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, SetCursor, SetForegroundWindow, SetTimer,
    SystemParametersInfoW, WINDOW_EX_STYLE, WM_ACTIVATEAPP, WM_DESTROY, WM_ERASEBKGND, WM_KEYDOWN,
    WM_LBUTTONDOWN, WM_MBUTTONDOWN, WM_MOUSEHWHEEL, WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_RBUTTONDOWN,
    WM_SETCURSOR, WM_SYSCOMMAND, WM_SYSKEYDOWN, WM_TIMER, WM_XBUTTONDOWN, WNDCLASSEXW, WS_CHILD,
    WS_EX_TOPMOST, WS_OVERLAPPEDWINDOW, WS_POPUP, WS_VISIBLE,
};
use windows::core::{BOOL, Error, PCWSTR, Result, w};

use crate::input::MouseExitTracker;
use crate::placement::{self, Rect as Placement};

const CLASS_NAME: PCWSTR = w!("NightFreewaySSWindow");
const TITLE: PCWSTR = w!("Night Freeway SS");
/// Pixels the cursor may move on either axis before the screensaver ends.
const MOUSE_EXIT_THRESHOLD: u32 = 10;
/// How long the cursor stays in sight after the mouse moved in the `/play`
/// mode, and how often the windows look whether that time is over.
const CURSOR_IDLE: Duration = Duration::from_secs(2);
const CURSOR_TIMER: usize = 1;
const CURSOR_TIMER_MILLISECONDS: u32 = 500;

/// What the windows do with input.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Behavior {
    /// The screensaver: keyboard and mouse input end the program, and the
    /// cursor is hidden.
    Saver,
    /// The preview inside the settings dialog, which ignores input.
    Preview,
    /// The `/window` mode: an ordinary window that only Esc and its own close
    /// button, or Alt+F4, end.
    Window,
    /// The `/play` mode: full screen, ended by Esc or Alt+F4 only. The cursor
    /// is seen for a while after the mouse moved.
    Play,
}

thread_local! {
    static BEHAVIOR: Cell<Behavior> = const { Cell::new(Behavior::Preview) };
    static MOUSE: RefCell<MouseExitTracker> =
        const { RefCell::new(MouseExitTracker::new(MOUSE_EXIT_THRESHOLD)) };
    /// When the mouse last moved in the `/play` mode.
    static LAST_MOVE: Cell<Option<Instant>> = const { Cell::new(None) };
}

/// Converts a window handle received on the command line.
pub fn hwnd_from_handle(handle: usize) -> HWND {
    HWND(handle as *mut c_void)
}

/// Creates a topmost borderless window covering each monitor
/// (docs/adr/0003-render-independent-scene-per-monitor.md).
pub fn create_fullscreen_windows() -> Result<Vec<HWND>> {
    create_monitor_windows(Behavior::Saver)
}

/// Creates the windows of the `/play` mode, which look like those of the
/// screensaver but are not ended by input.
pub fn create_play_windows() -> Result<Vec<HWND>> {
    let windows = create_monitor_windows(Behavior::Play)?;
    for &hwnd in &windows {
        // Why: no message tells that the cursor has stood still, so each
        // window looks at the time now and then and hides the cursor.
        if unsafe { SetTimer(Some(hwnd), CURSOR_TIMER, CURSOR_TIMER_MILLISECONDS, None) } == 0 {
            return Err(Error::from_thread());
        }
    }
    Ok(windows)
}

fn create_monitor_windows(behavior: Behavior) -> Result<Vec<HWND>> {
    BEHAVIOR.set(behavior);
    let instance = register_class()?;
    let monitors = monitor_rects()?;
    if monitors.is_empty() {
        // Without a window nothing could receive the input that ends the program.
        return Err(Error::new(E_FAIL, "No display monitor was found"));
    }
    let mut windows = Vec::new();
    for rect in monitors {
        let hwnd = unsafe {
            CreateWindowExW(
                WS_EX_TOPMOST,
                CLASS_NAME,
                TITLE,
                WS_POPUP | WS_VISIBLE,
                rect.left,
                rect.top,
                rect.right - rect.left,
                rect.bottom - rect.top,
                None,
                None,
                Some(instance),
                None,
            )?
        };
        windows.push(hwnd);
    }
    if let Some(&first) = windows.first() {
        // Keyboard input only reaches the foreground window.
        let _ = unsafe { SetForegroundWindow(first) };
    }
    Ok(windows)
}

/// Creates an ordinary window, with a title bar and borders, for the
/// `/window` mode: 1280 by 720 pixels at 96 DPI, in the middle of the work
/// area of the primary monitor (see `placement::window_rect`).
pub fn create_normal_window() -> Result<HWND> {
    BEHAVIOR.set(Behavior::Window);
    let instance = register_class()?;
    let style = WS_OVERLAPPEDWINDOW | WS_VISIBLE;
    let dpi = unsafe { GetDpiForSystem() };
    // The frame is what the window adds around its picture at that DPI.
    let mut frame = RECT::default();
    unsafe { AdjustWindowRectExForDpi(&mut frame, style, false, WINDOW_EX_STYLE(0), dpi)? };
    let mut work = RECT::default();
    unsafe {
        SystemParametersInfoW(
            SPI_GETWORKAREA,
            0,
            Some((&raw mut work).cast()),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )?
    };
    let rect = placement::window_rect(
        Placement {
            left: work.left,
            top: work.top,
            right: work.right,
            bottom: work.bottom,
        },
        dpi,
        (frame.right - frame.left, frame.bottom - frame.top),
    );
    unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            CLASS_NAME,
            TITLE,
            style,
            rect.left,
            rect.top,
            rect.width(),
            rect.height(),
            None,
            None,
            Some(instance),
            None,
        )
    }
}

/// Creates a child window filling the preview area of the settings dialog.
/// Returns `None` when the parent window no longer exists.
pub fn create_preview_window(parent: HWND) -> Result<Option<HWND>> {
    if !unsafe { IsWindow(Some(parent)) }.as_bool() {
        return Ok(None);
    }
    let instance = register_class()?;
    let mut rect = RECT::default();
    unsafe { GetClientRect(parent, &mut rect)? };
    let hwnd = unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            CLASS_NAME,
            TITLE,
            WS_CHILD | WS_VISIBLE,
            0,
            0,
            rect.right - rect.left,
            rect.bottom - rect.top,
            Some(parent),
            None,
            Some(instance),
            None,
        )?
    };
    Ok(Some(hwnd))
}

/// Creates an invisible window for rendering tests.
#[cfg(test)]
pub fn create_hidden_window(width: i32, height: i32) -> Result<HWND> {
    let instance = register_class()?;
    unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            CLASS_NAME,
            TITLE,
            WS_POPUP,
            0,
            0,
            width,
            height,
            None,
            None,
            Some(instance),
            None,
        )
    }
}

fn register_class() -> Result<HINSTANCE> {
    let instance: HINSTANCE = unsafe { GetModuleHandleW(None)? }.into();
    let class = WNDCLASSEXW {
        cbSize: size_of::<WNDCLASSEXW>() as u32,
        style: CS_HREDRAW | CS_VREDRAW,
        lpfnWndProc: Some(window_proc),
        hInstance: instance,
        // Shown over the preview; the full-screen windows hide the cursor.
        hCursor: unsafe { LoadCursorW(None, IDC_ARROW)? },
        hbrBackground: HBRUSH(unsafe { GetStockObject(BLACK_BRUSH) }.0),
        lpszClassName: CLASS_NAME,
        ..Default::default()
    };
    if unsafe { RegisterClassExW(&class) } == 0 {
        let error = Error::from_thread();
        if error.code() != ERROR_CLASS_ALREADY_EXISTS.to_hresult() {
            return Err(error);
        }
    }
    Ok(instance)
}

fn monitor_rects() -> Result<Vec<RECT>> {
    unsafe extern "system" fn collect(
        monitor: HMONITOR,
        _: HDC,
        _: *mut RECT,
        data: LPARAM,
    ) -> BOOL {
        // SAFETY: `data` is the address of the vector below, which outlives
        // the synchronous EnumDisplayMonitors call.
        let rects = unsafe { &mut *(data.0 as *mut Vec<RECT>) };
        let mut info = MONITORINFO {
            cbSize: size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if unsafe { GetMonitorInfoW(monitor, &mut info) }.as_bool() {
            rects.push(info.rcMonitor);
        }
        true.into()
    }

    let mut rects: Vec<RECT> = Vec::new();
    unsafe {
        EnumDisplayMonitors(
            None,
            None,
            Some(collect),
            LPARAM(&mut rects as *mut Vec<RECT> as isize),
        )
    }
    .ok()?;
    Ok(rects)
}

/// Whether the mouse has stood still for long enough to hide the cursor of
/// the `/play` mode.
fn cursor_is_idle() -> bool {
    LAST_MOVE
        .get()
        .is_none_or(|moved| moved.elapsed() >= CURSOR_IDLE)
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let behavior = BEHAVIOR.get();
    let exit_on_input = behavior == Behavior::Saver;
    let keeps_running = matches!(behavior, Behavior::Window | Behavior::Play);
    match message {
        WM_SETCURSOR if exit_on_input => {
            unsafe { SetCursor(None) };
            return LRESULT(1);
        }
        WM_SETCURSOR if behavior == Behavior::Play && cursor_is_idle() => {
            unsafe { SetCursor(None) };
            return LRESULT(1);
        }
        WM_KEYDOWN | WM_SYSKEYDOWN | WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_MBUTTONDOWN
        | WM_XBUTTONDOWN | WM_MOUSEWHEEL | WM_MOUSEHWHEEL
            if exit_on_input =>
        {
            unsafe { PostQuitMessage(0) };
            return LRESULT(0);
        }
        // Alt+F4 and the close button are not keys here: Windows closes the
        // window, which ends the program (`WM_DESTROY`).
        WM_KEYDOWN if keeps_running && wparam.0 == usize::from(VK_ESCAPE.0) => {
            unsafe { PostQuitMessage(0) };
            return LRESULT(0);
        }
        WM_MOUSEMOVE if exit_on_input => {
            // Screen coordinates, so that every monitor's window shares one origin.
            let mut point = POINT::default();
            if unsafe { GetCursorPos(&mut point) }.is_ok()
                && MOUSE.with_borrow_mut(|mouse| mouse.should_exit(point.x, point.y))
            {
                unsafe { PostQuitMessage(0) };
            }
            return LRESULT(0);
        }
        WM_MOUSEMOVE if behavior == Behavior::Play => {
            // Show the cursor again; no `WM_SETCURSOR` comes before the next move.
            LAST_MOVE.set(Some(Instant::now()));
            if let Ok(arrow) = unsafe { LoadCursorW(None, IDC_ARROW) } {
                unsafe { SetCursor(Some(arrow)) };
            }
            return LRESULT(0);
        }
        WM_TIMER if behavior == Behavior::Play && wparam.0 == CURSOR_TIMER => {
            if cursor_is_idle() {
                unsafe { SetCursor(None) };
            }
            return LRESULT(0);
        }
        WM_ACTIVATEAPP if exit_on_input && wparam.0 == 0 => {
            unsafe { PostQuitMessage(0) };
            return LRESULT(0);
        }
        // Do not let Windows start another screensaver on top of this one.
        WM_SYSCOMMAND if (wparam.0 & 0xFFF0) as u32 == SC_SCREENSAVE => return LRESULT(0),
        // Every frame repaints the whole window.
        WM_ERASEBKGND => return LRESULT(1),
        // The preview window is destroyed with its parent dialog.
        WM_DESTROY => {
            unsafe { PostQuitMessage(0) };
            return LRESULT(0);
        }
        _ => {}
    }
    unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
}

#[cfg(test)]
mod tests {
    use windows::Win32::UI::WindowsAndMessaging::{
        DestroyWindow, MSG, PM_REMOVE, PeekMessageW, SendMessageW, WM_QUIT,
    };

    use super::*;

    const KEY_A: usize = 0x41;

    /// Sends `message` to a new hidden window of the thread, whose program
    /// behaves as `behavior`, and returns whether the window asked the thread
    /// to quit.
    fn quits_on(behavior: Behavior, message: u32, wparam: usize) -> bool {
        BEHAVIOR.set(behavior);
        let hwnd = create_hidden_window(320, 180).expect("a hidden window");
        unsafe { SendMessageW(hwnd, message, Some(WPARAM(wparam)), Some(LPARAM(0))) };
        let mut queued = MSG::default();
        let quit = unsafe { PeekMessageW(&mut queued, None, WM_QUIT, WM_QUIT, PM_REMOVE) };
        // Destroying the window posts its own quit message, which is not asked about.
        unsafe { DestroyWindow(hwnd).expect("the window is destroyed") };
        let _ = unsafe { PeekMessageW(&mut queued, None, WM_QUIT, WM_QUIT, PM_REMOVE) };
        quit.as_bool()
    }

    #[test]
    fn only_escape_ends_the_window_and_play_modes() {
        for behavior in [Behavior::Window, Behavior::Play] {
            assert!(
                quits_on(behavior, WM_KEYDOWN, usize::from(VK_ESCAPE.0)),
                "{behavior:?}"
            );
            for (message, wparam) in [
                (WM_KEYDOWN, KEY_A),
                (WM_SYSKEYDOWN, KEY_A),
                (WM_LBUTTONDOWN, 0),
                (WM_RBUTTONDOWN, 0),
                (WM_MOUSEMOVE, 0),
                (WM_MOUSEWHEEL, 0),
                (WM_ACTIVATEAPP, 0),
            ] {
                assert!(
                    !quits_on(behavior, message, wparam),
                    "{behavior:?} {message:#x}"
                );
            }
        }
    }

    #[test]
    fn any_key_ends_the_screensaver_but_not_the_preview() {
        assert!(quits_on(Behavior::Saver, WM_KEYDOWN, KEY_A));
        assert!(quits_on(Behavior::Saver, WM_LBUTTONDOWN, 0));
        assert!(!quits_on(Behavior::Preview, WM_KEYDOWN, KEY_A));
        assert!(!quits_on(
            Behavior::Preview,
            WM_KEYDOWN,
            usize::from(VK_ESCAPE.0)
        ));
    }

    #[test]
    fn the_cursor_of_the_play_mode_is_seen_only_after_the_mouse_moved() {
        BEHAVIOR.set(Behavior::Play);
        LAST_MOVE.set(None);
        assert!(cursor_is_idle());
        let hwnd = create_hidden_window(320, 180).expect("a hidden window");
        unsafe { SendMessageW(hwnd, WM_MOUSEMOVE, Some(WPARAM(0)), Some(LPARAM(0))) };
        assert!(!cursor_is_idle());
        // The time is over.
        LAST_MOVE.set(Instant::now().checked_sub(CURSOR_IDLE));
        assert!(cursor_is_idle());
        unsafe { DestroyWindow(hwnd).expect("the window is destroyed") };
    }

    #[test]
    fn the_window_of_the_window_mode_opens_inside_the_work_area() {
        let hwnd = create_normal_window().expect("the window of the window mode");
        let mut outer = RECT::default();
        let mut client = RECT::default();
        let mut work = RECT::default();
        unsafe {
            windows::Win32::UI::WindowsAndMessaging::GetWindowRect(hwnd, &mut outer)
                .expect("the rectangle of the window");
            GetClientRect(hwnd, &mut client).expect("the client area");
            SystemParametersInfoW(
                SPI_GETWORKAREA,
                0,
                Some((&raw mut work).cast()),
                SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
            )
            .expect("the work area");
            DestroyWindow(hwnd).expect("the window is destroyed");
        }
        assert!(client.right > 0 && client.bottom > 0);
        // It is a window with a title bar around its picture.
        assert!(outer.bottom - outer.top > client.bottom);
        assert!(outer.left >= work.left && outer.top >= work.top);
        assert!(outer.right <= work.right && outer.bottom <= work.bottom);
    }
}
