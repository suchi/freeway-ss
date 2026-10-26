//! Screensaver windows and the input handling that ends the screensaver.

use std::cell::{Cell, RefCell};
use std::ffi::c_void;

use windows::Win32::Foundation::{
    E_FAIL, ERROR_CLASS_ALREADY_EXISTS, HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM,
};
use windows::Win32::Graphics::Gdi::{
    BLACK_BRUSH, EnumDisplayMonitors, GetMonitorInfoW, GetStockObject, HBRUSH, HDC, HMONITOR,
    MONITORINFO, SC_SCREENSAVE,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CS_HREDRAW, CS_VREDRAW, CreateWindowExW, DefWindowProcW, GetClientRect, GetCursorPos,
    IDC_ARROW, IsWindow, LoadCursorW, PostQuitMessage, RegisterClassExW, SetCursor,
    SetForegroundWindow, WINDOW_EX_STYLE, WM_ACTIVATEAPP, WM_DESTROY, WM_ERASEBKGND, WM_KEYDOWN,
    WM_LBUTTONDOWN, WM_MBUTTONDOWN, WM_MOUSEHWHEEL, WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_RBUTTONDOWN,
    WM_SETCURSOR, WM_SYSCOMMAND, WM_SYSKEYDOWN, WM_XBUTTONDOWN, WNDCLASSEXW, WS_CHILD,
    WS_EX_TOPMOST, WS_POPUP, WS_VISIBLE,
};
use windows::core::{BOOL, Error, PCWSTR, Result, w};

use crate::input::MouseExitTracker;

const CLASS_NAME: PCWSTR = w!("NightFreewaySSWindow");
const TITLE: PCWSTR = w!("Night Freeway SS");
/// Pixels the cursor may move on either axis before the screensaver ends.
const MOUSE_EXIT_THRESHOLD: u32 = 10;

thread_local! {
    /// Whether keyboard and mouse input end the program. Only the full-screen
    /// mode exits on input; the preview lives inside the settings dialog.
    static EXIT_ON_INPUT: Cell<bool> = const { Cell::new(false) };
    static MOUSE: RefCell<MouseExitTracker> =
        const { RefCell::new(MouseExitTracker::new(MOUSE_EXIT_THRESHOLD)) };
}

/// Converts a window handle received on the command line.
pub fn hwnd_from_handle(handle: usize) -> HWND {
    HWND(handle as *mut c_void)
}

/// Creates a topmost borderless window covering each monitor
/// (docs/adr/0003-render-independent-scene-per-monitor.md).
pub fn create_fullscreen_windows() -> Result<Vec<HWND>> {
    EXIT_ON_INPUT.set(true);
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

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let exit_on_input = EXIT_ON_INPUT.get();
    match message {
        WM_SETCURSOR if exit_on_input => {
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
