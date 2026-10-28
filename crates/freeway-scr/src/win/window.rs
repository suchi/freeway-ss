//! Screensaver windows and the input handling that ends the screensaver.

use std::cell::{Cell, RefCell};
use std::ffi::c_void;
use std::time::{Duration, Instant};

use windows::Win32::Foundation::{
    E_FAIL, ERROR_CLASS_ALREADY_EXISTS, HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM,
};
use windows::Win32::Graphics::Gdi::{
    BLACK_BRUSH, EnumDisplayMonitors, GetMonitorInfoW, GetStockObject, HBRUSH, HDC, HMONITOR,
    MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow, SC_SCREENSAVE,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::{AdjustWindowRectExForDpi, GetDpiForSystem, GetDpiForWindow};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    VK_DOWN, VK_ESCAPE, VK_LEFT, VK_RIGHT, VK_SPACE, VK_UP,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CS_HREDRAW, CS_VREDRAW, CreateWindowExW, DefWindowProcW, DestroyWindow, GWL_EXSTYLE, GWL_STYLE,
    GetClientRect, GetCursorPos, GetWindowRect, HWND_NOTOPMOST, HWND_TOPMOST, IDC_ARROW, IsWindow,
    KillTimer, LoadCursorW, PostQuitMessage, RegisterClassExW, SPI_GETWORKAREA, SW_RESTORE,
    SWP_FRAMECHANGED, SWP_SHOWWINDOW, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, SetCursor,
    SetForegroundWindow, SetTimer, SetWindowLongPtrW, SetWindowPos, ShowWindow,
    SystemParametersInfoW, WINDOW_EX_STYLE, WM_ACTIVATEAPP, WM_DESTROY, WM_ERASEBKGND, WM_KEYDOWN,
    WM_KEYUP, WM_KILLFOCUS, WM_LBUTTONDOWN, WM_MBUTTONDOWN, WM_MOUSEHWHEEL, WM_MOUSEMOVE,
    WM_MOUSEWHEEL, WM_RBUTTONDOWN, WM_SETCURSOR, WM_SYSCOMMAND, WM_SYSKEYDOWN, WM_TIMER,
    WM_XBUTTONDOWN, WNDCLASSEXW, WS_CHILD, WS_EX_TOPMOST, WS_OVERLAPPEDWINDOW, WS_POPUP,
    WS_VISIBLE,
};
use windows::core::{BOOL, Error, PCWSTR, Result, w};

use crate::input::{Command, Controls, Key, MouseExitTracker};
use crate::placement::{self, Rect as Placement};

const CLASS_NAME: PCWSTR = w!("NightFreewaySSWindow");
const TITLE: PCWSTR = w!("Night Freeway SS");
/// Pixels the cursor may move on either axis before the screensaver ends.
const MOUSE_EXIT_THRESHOLD: u32 = 10;
/// How long the cursor stays in sight after the mouse moved in the `/play`
/// mode, and how often the windows look whether that time is over.
const CURSOR_IDLE: Duration = Duration::from_secs(2);
/// `MONITORINFO::dwFlags` has this bit for the primary monitor (the windows
/// crate does not define the constant).
const MONITORINFOF_PRIMARY: u32 = 1;
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
    /// button, or Alt+F4, end. The keys of a run work (`Controls`).
    Window,
    /// The `/play` mode: full screen, ended by Esc or Alt+F4 only. The cursor
    /// is seen for a while after the mouse moved. The keys of a run work. The
    /// F key switches between this and `Window`.
    Play,
}

thread_local! {
    static BEHAVIOR: Cell<Behavior> = const { Cell::new(Behavior::Preview) };
    static MOUSE: RefCell<MouseExitTracker> =
        const { RefCell::new(MouseExitTracker::new(MOUSE_EXIT_THRESHOLD)) };
    /// When the mouse last moved in the `/play` mode.
    static LAST_MOVE: Cell<Option<Instant>> = const { Cell::new(None) };
    /// The keys of a run that are held, and the sequence of the last keys.
    static CONTROLS: RefCell<Controls> = RefCell::new(Controls::default());
    /// The commands that keys asked for since the loop last took them, with
    /// the window that had the key.
    static COMMANDS: RefCell<Vec<(Command, HWND)>> = const { RefCell::new(Vec::new()) };
    /// Whether a key or the mouse was used since the loop last looked.
    static ACTIVITY: Cell<bool> = const { Cell::new(false) };
    /// Whether destroying a window ends the program. It does not while a
    /// window is closed on purpose, when the run goes on in another one.
    static QUIT_ON_DESTROY: Cell<bool> = const { Cell::new(true) };
}

/// What the user did since the last call to `take_input`.
#[derive(Debug, Default)]
pub struct Input {
    /// `1.0` while up is held, `-1.0` while down is held.
    pub speed_direction: f32,
    /// A key or the mouse was used.
    pub activity: bool,
    /// The commands of the keys pressed, in order, with the window that had
    /// the keyboard.
    pub commands: Vec<(Command, HWND)>,
}

/// Takes the input of the keys of a run. Nothing is there in the other modes.
pub fn take_input() -> Input {
    Input {
        speed_direction: CONTROLS.with_borrow(Controls::speed_direction),
        activity: ACTIVITY.replace(false),
        commands: COMMANDS.take(),
    }
}

/// Whether the windows are in full screen (the `/play` mode).
pub fn is_fullscreen() -> bool {
    BEHAVIOR.get() == Behavior::Play
}

/// The key that the virtual-key code `code` is.
fn key_of(code: usize) -> Key {
    const D: usize = 0x44;
    const F: usize = 0x46;
    const M: usize = 0x4D;
    const A: usize = 0x41;
    const B: usize = 0x42;
    match code {
        c if c == usize::from(VK_UP.0) => Key::Up,
        c if c == usize::from(VK_DOWN.0) => Key::Down,
        c if c == usize::from(VK_LEFT.0) => Key::Left,
        c if c == usize::from(VK_RIGHT.0) => Key::Right,
        c if c == usize::from(VK_SPACE.0) => Key::Space,
        D => Key::D,
        F => Key::F,
        M => Key::M,
        A => Key::A,
        B => Key::B,
        _ => Key::Other,
    }
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
        start_cursor_timer(hwnd)?;
    }
    Ok(windows)
}

/// Creates the full-screen windows of the `/play` mode on every monitor but
/// the one that `hwnd` is on, where the window `hwnd` is already full screen:
/// the F key brings the monitors back that it left when it made a window.
pub fn create_play_windows_besides(hwnd: HWND) -> Result<Vec<HWND>> {
    BEHAVIOR.set(Behavior::Play);
    let instance = register_class()?;
    let own = unsafe { MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST) };
    let mut windows = Vec::new();
    for (monitor, rect) in monitors()? {
        if monitor == own {
            continue;
        }
        let other = create_topmost_window(instance, rect)?;
        start_cursor_timer(other)?;
        windows.push(other);
    }
    Ok(windows)
}

fn start_cursor_timer(hwnd: HWND) -> Result<()> {
    // Why: no message tells that the cursor has stood still, so each
    // window looks at the time now and then and hides the cursor.
    if unsafe { SetTimer(Some(hwnd), CURSOR_TIMER, CURSOR_TIMER_MILLISECONDS, None) } == 0 {
        return Err(Error::from_thread());
    }
    Ok(())
}

fn create_monitor_windows(behavior: Behavior) -> Result<Vec<HWND>> {
    BEHAVIOR.set(behavior);
    let instance = register_class()?;
    let monitors = monitors()?;
    if monitors.is_empty() {
        // Without a window nothing could receive the input that ends the program.
        return Err(Error::new(E_FAIL, "No display monitor was found"));
    }
    let mut windows = Vec::new();
    for (_, rect) in monitors {
        windows.push(create_topmost_window(instance, rect)?);
    }
    if let Some(&first) = windows.first() {
        // Keyboard input only reaches the foreground window.
        let _ = unsafe { SetForegroundWindow(first) };
    }
    Ok(windows)
}

/// A topmost borderless window that covers `rect`.
fn create_topmost_window(instance: HINSTANCE, rect: RECT) -> Result<HWND> {
    unsafe {
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
        )
    }
}

/// Where `hwnd` is on the screen, in physical pixels; `None` if it is gone.
pub fn rect_of(hwnd: HWND) -> Option<Placement> {
    let mut rect = RECT::default();
    unsafe { GetWindowRect(hwnd, &mut rect) }.ok()?;
    Some(Placement {
        left: rect.left,
        top: rect.top,
        right: rect.right,
        bottom: rect.bottom,
    })
}

/// Whether `hwnd` is on the primary monitor, the one with Windows' main display.
pub fn is_on_primary_monitor(hwnd: HWND) -> bool {
    monitor_of(hwnd).is_ok_and(|info| info.dwFlags & MONITORINFOF_PRIMARY != 0)
}

/// The style of the window of the `/window` mode.
const NORMAL_STYLE: windows::Win32::UI::WindowsAndMessaging::WINDOW_STYLE = WS_OVERLAPPEDWINDOW;

/// The outer rectangle of the window of the `/window` mode inside `work`, at
/// `dpi` dots per inch (see `placement::window_rect`).
fn normal_window_rect(work: RECT, dpi: u32) -> Result<Placement> {
    // The frame is what the window adds around its picture at that DPI.
    let mut frame = RECT::default();
    unsafe { AdjustWindowRectExForDpi(&mut frame, NORMAL_STYLE, false, WINDOW_EX_STYLE(0), dpi)? };
    Ok(placement::window_rect(
        Placement {
            left: work.left,
            top: work.top,
            right: work.right,
            bottom: work.bottom,
        },
        dpi,
        (frame.right - frame.left, frame.bottom - frame.top),
    ))
}

/// Creates an ordinary window, with a title bar and borders, for the
/// `/window` mode: 1280 by 720 pixels at 96 DPI, in the middle of the work
/// area of the primary monitor (see `placement::window_rect`).
pub fn create_normal_window() -> Result<HWND> {
    BEHAVIOR.set(Behavior::Window);
    let instance = register_class()?;
    let mut work = RECT::default();
    unsafe {
        SystemParametersInfoW(
            SPI_GETWORKAREA,
            0,
            Some((&raw mut work).cast()),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )?
    };
    let rect = normal_window_rect(work, unsafe { GetDpiForSystem() })?;
    unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            CLASS_NAME,
            TITLE,
            NORMAL_STYLE | WS_VISIBLE,
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

/// The rectangles of the monitor that `hwnd` is on: all of it, and its work
/// area.
fn monitor_of(hwnd: HWND) -> Result<MONITORINFO> {
    let monitor = unsafe { MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST) };
    let mut info = MONITORINFO {
        cbSize: size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    unsafe { GetMonitorInfoW(monitor, &mut info).ok()? };
    Ok(info)
}

/// Switches between the window and the full screen (the F key), in the same
/// window, so that the road goes on: from the `/window` mode to a full-screen
/// window on the monitor it is on, and back to a window of the default size
/// on that monitor. The caller closes the windows of the other monitors.
pub fn toggle_fullscreen(hwnd: HWND) -> Result<()> {
    let info = monitor_of(hwnd)?;
    match BEHAVIOR.get() {
        Behavior::Window => {
            let rect = info.rcMonitor;
            // A maximized window cannot take the new size.
            let _ = unsafe { ShowWindow(hwnd, SW_RESTORE) };
            set_styles(hwnd, WS_POPUP | WS_VISIBLE, WS_EX_TOPMOST);
            BEHAVIOR.set(Behavior::Play);
            LAST_MOVE.set(None);
            place(
                hwnd,
                HWND_TOPMOST,
                Placement {
                    left: rect.left,
                    top: rect.top,
                    right: rect.right,
                    bottom: rect.bottom,
                },
            )?;
            if unsafe { SetTimer(Some(hwnd), CURSOR_TIMER, CURSOR_TIMER_MILLISECONDS, None) } == 0 {
                return Err(Error::from_thread());
            }
            // The cursor of the window was visible a moment ago.
            unsafe { SetCursor(None) };
        }
        Behavior::Play => {
            let rect = normal_window_rect(info.rcWork, unsafe { GetDpiForWindow(hwnd) })?;
            BEHAVIOR.set(Behavior::Window);
            let _ = unsafe { KillTimer(Some(hwnd), CURSOR_TIMER) };
            set_styles(hwnd, NORMAL_STYLE | WS_VISIBLE, WINDOW_EX_STYLE(0));
            place(hwnd, HWND_NOTOPMOST, rect)?;
            if let Ok(arrow) = unsafe { LoadCursorW(None, IDC_ARROW) } {
                unsafe { SetCursor(Some(arrow)) };
            }
        }
        Behavior::Saver | Behavior::Preview => {}
    }
    Ok(())
}

fn set_styles(
    hwnd: HWND,
    style: windows::Win32::UI::WindowsAndMessaging::WINDOW_STYLE,
    ex_style: WINDOW_EX_STYLE,
) {
    unsafe {
        SetWindowLongPtrW(hwnd, GWL_STYLE, style.0 as isize);
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex_style.0 as isize);
    }
}

/// Moves and sizes `hwnd` and applies the styles that were just set.
fn place(hwnd: HWND, after: HWND, rect: Placement) -> Result<()> {
    unsafe {
        SetWindowPos(
            hwnd,
            Some(after),
            rect.left,
            rect.top,
            rect.width(),
            rect.height(),
            SWP_FRAMECHANGED | SWP_SHOWWINDOW,
        )
    }
}

/// Gives the keyboard to `hwnd`.
pub fn focus(hwnd: HWND) {
    let _ = unsafe { SetForegroundWindow(hwnd) };
}

/// Destroys a window without ending the program, when the run goes on in
/// another window.
pub fn close(hwnd: HWND) {
    QUIT_ON_DESTROY.set(false);
    let _ = unsafe { DestroyWindow(hwnd) };
    QUIT_ON_DESTROY.set(true);
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

/// Creates an invisible window at `position`, for rendering tests of windows
/// that lie side by side.
#[cfg(test)]
pub fn create_hidden_window_at(position: (i32, i32), size: (i32, i32)) -> Result<HWND> {
    let instance = register_class()?;
    unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            CLASS_NAME,
            TITLE,
            WS_POPUP,
            position.0,
            position.1,
            size.0,
            size.1,
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

/// Every monitor with its rectangle on the virtual desktop, in physical pixels.
fn monitors() -> Result<Vec<(HMONITOR, RECT)>> {
    unsafe extern "system" fn collect(
        monitor: HMONITOR,
        _: HDC,
        _: *mut RECT,
        data: LPARAM,
    ) -> BOOL {
        // SAFETY: `data` is the address of the vector below, which outlives
        // the synchronous EnumDisplayMonitors call.
        let monitors = unsafe { &mut *(data.0 as *mut Vec<(HMONITOR, RECT)>) };
        let mut info = MONITORINFO {
            cbSize: size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if unsafe { GetMonitorInfoW(monitor, &mut info) }.as_bool() {
            monitors.push((monitor, info.rcMonitor));
        }
        true.into()
    }

    let mut monitors: Vec<(HMONITOR, RECT)> = Vec::new();
    unsafe {
        EnumDisplayMonitors(
            None,
            None,
            Some(collect),
            LPARAM(&mut monitors as *mut Vec<(HMONITOR, RECT)> as isize),
        )
    }
    .ok()?;
    Ok(monitors)
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
        WM_KEYDOWN if keeps_running => {
            // Bit 30 is set where the key was already down: it repeats.
            let repeat = (lparam.0 >> 30) & 1 == 1;
            let command =
                CONTROLS.with_borrow_mut(|controls| controls.key_down(key_of(wparam.0), repeat));
            if let Some(command) = command {
                COMMANDS.with_borrow_mut(|commands| commands.push((command, hwnd)));
            }
            ACTIVITY.set(true);
            return LRESULT(0);
        }
        WM_KEYUP if keeps_running => {
            CONTROLS.with_borrow_mut(|controls| controls.key_up(key_of(wparam.0)));
            return LRESULT(0);
        }
        // The release of a held key is not seen after the window lost the keyboard.
        WM_KILLFOCUS if keeps_running => {
            CONTROLS.with_borrow_mut(Controls::release_all);
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
        WM_MOUSEMOVE if behavior == Behavior::Window => {
            ACTIVITY.set(true);
            return LRESULT(0);
        }
        WM_MOUSEMOVE if behavior == Behavior::Play => {
            ACTIVITY.set(true);
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
            if QUIT_ON_DESTROY.get() {
                unsafe { PostQuitMessage(0) };
            }
            return LRESULT(0);
        }
        _ => {}
    }
    unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
}

#[cfg(test)]
mod tests {
    use windows::Win32::UI::WindowsAndMessaging::{
        MSG, PM_REMOVE, PeekMessageW, SendMessageW, WM_QUIT,
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

    #[test]
    fn the_keys_of_a_run_are_told_apart() {
        assert_eq!(key_of(usize::from(VK_UP.0)), Key::Up);
        assert_eq!(key_of(usize::from(VK_DOWN.0)), Key::Down);
        assert_eq!(key_of(usize::from(VK_LEFT.0)), Key::Left);
        assert_eq!(key_of(usize::from(VK_RIGHT.0)), Key::Right);
        assert_eq!(key_of(usize::from(VK_SPACE.0)), Key::Space);
        assert_eq!(key_of(0x44), Key::D);
        assert_eq!(key_of(0x46), Key::F);
        assert_eq!(key_of(0x4D), Key::M);
        assert_eq!(key_of(0x41), Key::A);
        assert_eq!(key_of(0x42), Key::B);
        assert_eq!(key_of(0x43), Key::Other);
        assert_eq!(key_of(usize::from(VK_ESCAPE.0)), Key::Other);
    }

    /// Sends `message` to `hwnd` and returns the result.
    fn send(hwnd: HWND, message: u32, wparam: usize, lparam: isize) {
        unsafe { SendMessageW(hwnd, message, Some(WPARAM(wparam)), Some(LPARAM(lparam))) };
    }

    #[test]
    fn the_keys_of_a_run_become_input_in_the_window_and_play_modes() {
        for behavior in [Behavior::Window, Behavior::Play] {
            BEHAVIOR.set(behavior);
            let hwnd = create_hidden_window(320, 180).expect("a hidden window");
            let _ = take_input();
            send(hwnd, WM_KEYDOWN, usize::from(VK_SPACE.0), 0);
            send(hwnd, WM_KEYDOWN, 0x44, 0);
            send(hwnd, WM_KEYDOWN, 0x46, 0);
            // Bit 30 of the second word: the system repeats a held key.
            send(hwnd, WM_KEYDOWN, 0x44, 1 << 30);
            send(hwnd, WM_KEYDOWN, usize::from(VK_UP.0), 0);
            let input = take_input();
            assert_eq!(
                input.commands,
                vec![
                    (Command::TogglePause, hwnd),
                    (Command::ToggleSide, hwnd),
                    (Command::ToggleFullscreen, hwnd),
                ],
                "{behavior:?}"
            );
            assert!(input.activity);
            assert_eq!(input.speed_direction, 1.0);
            // The input is taken once; the held key stays held.
            let again = take_input();
            assert!(again.commands.is_empty() && !again.activity);
            assert_eq!(again.speed_direction, 1.0);
            send(hwnd, WM_KEYUP, usize::from(VK_UP.0), 0);
            assert_eq!(take_input().speed_direction, 0.0);
            // A window that loses the keyboard lets go of the keys.
            send(hwnd, WM_KEYDOWN, usize::from(VK_DOWN.0), 0);
            assert_eq!(take_input().speed_direction, -1.0);
            send(hwnd, WM_KILLFOCUS, 0, 0);
            assert_eq!(take_input().speed_direction, 0.0);
            unsafe { DestroyWindow(hwnd).expect("the window is destroyed") };
        }
    }

    #[test]
    fn the_mouse_is_activity_in_the_window_and_play_modes() {
        for behavior in [Behavior::Window, Behavior::Play] {
            BEHAVIOR.set(behavior);
            let hwnd = create_hidden_window(320, 180).expect("a hidden window");
            let _ = take_input();
            send(hwnd, WM_MOUSEMOVE, 0, 0);
            assert!(take_input().activity, "{behavior:?}");
            unsafe { DestroyWindow(hwnd).expect("the window is destroyed") };
        }
    }

    #[test]
    fn the_preview_ignores_the_keys_of_a_run() {
        BEHAVIOR.set(Behavior::Preview);
        let hwnd = create_hidden_window(320, 180).expect("a hidden window");
        let _ = take_input();
        for key in [
            usize::from(VK_SPACE.0),
            usize::from(VK_UP.0),
            0x44,
            0x46,
            0x4D,
        ] {
            send(hwnd, WM_KEYDOWN, key, 0);
        }
        send(hwnd, WM_MOUSEMOVE, 0, 0);
        let input = take_input();
        assert!(input.commands.is_empty() && !input.activity);
        assert_eq!(input.speed_direction, 0.0);
        unsafe { DestroyWindow(hwnd).expect("the window is destroyed") };
    }

    #[test]
    fn a_window_closed_on_purpose_does_not_end_the_program() {
        BEHAVIOR.set(Behavior::Play);
        let hwnd = create_hidden_window(320, 180).expect("a hidden window");
        let mut queued = MSG::default();
        close(hwnd);
        assert!(!unsafe { IsWindow(Some(hwnd)) }.as_bool());
        assert!(!unsafe { PeekMessageW(&mut queued, None, WM_QUIT, WM_QUIT, PM_REMOVE) }.as_bool());
        // The next destruction ends it again.
        let other = create_hidden_window(320, 180).expect("a hidden window");
        unsafe { DestroyWindow(other).expect("the window is destroyed") };
        assert!(unsafe { PeekMessageW(&mut queued, None, WM_QUIT, WM_QUIT, PM_REMOVE) }.as_bool());
    }

    #[test]
    fn the_f_key_switches_between_a_window_and_the_full_screen_of_its_monitor() {
        let hwnd = create_normal_window().expect("the window of the window mode");
        assert!(!is_fullscreen());
        let rect_of = |hwnd| {
            let mut rect = RECT::default();
            unsafe {
                windows::Win32::UI::WindowsAndMessaging::GetWindowRect(hwnd, &mut rect)
                    .expect("the rectangle of the window");
            }
            rect
        };
        let monitor = monitor_of(hwnd).expect("the monitor");
        toggle_fullscreen(hwnd).expect("the full screen");
        assert!(is_fullscreen());
        let full = rect_of(hwnd);
        assert_eq!(
            (full.left, full.top, full.right, full.bottom),
            (
                monitor.rcMonitor.left,
                monitor.rcMonitor.top,
                monitor.rcMonitor.right,
                monitor.rcMonitor.bottom
            )
        );
        // Back to a window that is inside the work area.
        toggle_fullscreen(hwnd).expect("the window");
        assert!(!is_fullscreen());
        let back = rect_of(hwnd);
        // The window has its title bar again: it does not cover the monitor.
        assert!(back.bottom - back.top < full.bottom - full.top);
        assert!(back.left >= monitor.rcWork.left && back.top >= monitor.rcWork.top);
        assert!(back.right <= monitor.rcWork.right && back.bottom <= monitor.rcWork.bottom);
        // And once more, to see that it can be done again.
        toggle_fullscreen(hwnd).expect("the full screen again");
        assert!(is_fullscreen());
        unsafe { DestroyWindow(hwnd).expect("the window is destroyed") };
    }
}
