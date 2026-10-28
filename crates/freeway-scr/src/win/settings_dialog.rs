//! The settings dialog, shown for `/c` and when the `.scr` file is opened.
//!
//! The layout is the dialog template in `resources/freeway.rc`; the control
//! IDs below must match it.

use freeway_core::settings::{SPEED_CHOICES, position_to_speed, speed_to_position};
use freeway_core::{
    CarColor, CarShapes, DisplayLanguage, LaneChanges, Language, SceneMode, Settings,
    SpeedometerStyle, Text, TruckStyle, TunnelLighting,
};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Controls::{
    BST_CHECKED, BST_UNCHECKED, CheckDlgButton, CheckRadioButton, EM_LIMITTEXT, ICC_BAR_CLASSES,
    INITCOMMONCONTROLSEX, InitCommonControlsEx, IsDlgButtonChecked, TBM_SETPOS, TBM_SETRANGE,
};
use windows::Win32::UI::Input::KeyboardAndMouse::EnableWindow;
use windows::Win32::UI::WindowsAndMessaging::{
    CB_ADDSTRING, CB_GETCURSEL, CB_RESETCONTENT, CB_SETCURSEL, CBN_SELCHANGE, DialogBoxParamW,
    EndDialog, GWLP_USERDATA, GetDlgItem, GetDlgItemInt, GetWindowLongPtrW, IDCANCEL, IDOK,
    SendDlgItemMessageW, SetDlgItemTextW, SetWindowLongPtrW, SetWindowTextW, WM_COMMAND,
    WM_HSCROLL, WM_INITDIALOG,
};
use windows::core::{BOOL, Error, HSTRING, PCWSTR, Result};

use crate::args::{Mode, limit_minutes};

const DIALOG: usize = 100;
const SPEED_SLIDER: i32 = 101;
const SPEED_VALUE: i32 = 102;
const LEFT_HAND: i32 = 103;
const RIGHT_HAND: i32 = 104;
const SPEEDOMETER: i32 = 105;
const UFO: i32 = 106;
const LANGUAGE: i32 = 107;
const SPEED_LABEL: i32 = 108;
const SIDE_LABEL: i32 = 109;
const LANGUAGE_LABEL: i32 = 110;
const MOUNTAINS: i32 = 111;
const TRUCKS_LABEL: i32 = 112;
const TRUCKS: i32 = 113;
const TUNNELS_LABEL: i32 = 114;
const TUNNELS: i32 = 115;
const VARY_SPEED: i32 = 116;
const TRAFFIC_BY_TIME: i32 = 117;
const SCENERY_ZONES: i32 = 118;
const CAR_SHAPES_LABEL: i32 = 119;
const CAR_SHAPES: i32 = 120;
const SPEEDOMETER_LABEL: i32 = 121;
const CAR_COLOR_LABEL: i32 = 122;
const CAR_COLOR: i32 = 123;
/// The button that opens the dialog of the run, and the controls of that dialog
/// (`RUN_DIALOG` in the resource).
const RUN_OPEN: i32 = 124;
const RUN_DIALOG: usize = 200;
const RUN_WINDOW: i32 = 125;
const RUN_FULLSCREEN: i32 = 126;
const RUN_MINUTES_LABEL: i32 = 127;
const RUN_MINUTES: i32 = 128;
const RUN_START: i32 = 129;
/// The line that says how a run ends.
const RUN_END_NOTE: i32 = 130;
const LANE_CHANGES_LABEL: i32 = 131;
const LANE_CHANGES: i32 = 132;
const SPAN_MONITORS: i32 = 133;
const MODE_LABEL: i32 = 134;
const MODE: i32 = 135;

/// How many characters the minutes may have: `MINUTES_RANGE` ends at 1440.
const MINUTES_DIGITS: usize = 4;

/// The texts of the controls that are set from the string table, by ID.
const CONTROL_TEXTS: [(i32, Text); 21] = [
    (MODE_LABEL, Text::ModeLabel),
    (SPEED_LABEL, Text::SpeedLabel),
    (SIDE_LABEL, Text::DrivingSideLabel),
    (LEFT_HAND, Text::LeftHandTraffic),
    (RIGHT_HAND, Text::RightHandTraffic),
    (SPEEDOMETER_LABEL, Text::SpeedometerLabel),
    (UFO, Text::Ufo),
    (MOUNTAINS, Text::Mountains),
    (VARY_SPEED, Text::VarySpeed),
    (TRAFFIC_BY_TIME, Text::TrafficByTime),
    (SCENERY_ZONES, Text::SceneryZones),
    (SPAN_MONITORS, Text::SpanMonitors),
    (TRUCKS_LABEL, Text::TrucksLabel),
    (TUNNELS_LABEL, Text::TunnelsLabel),
    (CAR_SHAPES_LABEL, Text::CarShapesLabel),
    (CAR_COLOR_LABEL, Text::CarColorLabel),
    (LANE_CHANGES_LABEL, Text::LaneChangesLabel),
    (LANGUAGE_LABEL, Text::LanguageLabel),
    (RUN_OPEN, Text::RunOpen),
    // OK and Cancel use the IDs of the standard buttons.
    (IDOK.0, Text::Ok),
    (IDCANCEL.0, Text::Cancel),
];

/// The items of the mode drop-down, in the order of `SceneMode::ALL`.
const MODE_TEXTS: [Text; 2] = [Text::ModeKitchenSink, Text::ModeOriginal];

/// The controls that the Original mode does not use, by ID: what the settings
/// of the added features are for. Their values are kept and saved.
const ADDED_FEATURE_CONTROLS: [i32; 15] = [
    SPEEDOMETER_LABEL,
    SPEEDOMETER,
    UFO,
    MOUNTAINS,
    VARY_SPEED,
    TRAFFIC_BY_TIME,
    SCENERY_ZONES,
    TRUCKS_LABEL,
    TRUCKS,
    TUNNELS_LABEL,
    TUNNELS,
    CAR_SHAPES_LABEL,
    CAR_SHAPES,
    LANE_CHANGES_LABEL,
    LANE_CHANGES,
];

/// The items of the language drop-down, in the order of `Language::ALL`.
const LANGUAGE_TEXTS: [Text; 3] = [
    Text::LanguageAuto,
    Text::LanguageEnglish,
    Text::LanguageJapanese,
];

/// The items of the trucks drop-down, in the order of `TruckStyle::ALL`.
const TRUCK_TEXTS: [Text; 3] = [
    Text::TrucksNone,
    Text::TrucksJapanese,
    Text::TrucksNorthAmerican,
];

/// The items of the tunnels drop-down, in the order of `TunnelLighting::ALL`.
const TUNNEL_TEXTS: [Text; 4] = [
    Text::TunnelsNone,
    Text::TunnelsMixed,
    Text::TunnelsSodium,
    Text::TunnelsLed,
];

/// The items of the speedometer drop-down, in the order of `SpeedometerStyle::ALL`.
const SPEEDOMETER_TEXTS: [Text; 4] = [
    Text::SpeedometerNone,
    Text::SpeedometerDigits,
    Text::SpeedometerSimple,
    Text::SpeedometerDetailed,
];

/// The items of the car shapes drop-down, in the order of `CarShapes::ALL`.
const CAR_SHAPE_TEXTS: [Text; 4] = [
    Text::CarShapesNone,
    Text::CarShapesBoxes,
    Text::CarShapesBlocks,
    Text::CarShapesDetailed,
];

/// The items of the car color drop-down, in the order of `CarColor::ALL`.
const CAR_COLOR_TEXTS: [Text; 2] = [Text::CarColorTranslucent, Text::CarColorBlack];

/// The items of the lane changes drop-down, in the order of `LaneChanges::ALL`.
const LANE_CHANGE_TEXTS: [Text; 3] = [
    Text::LaneChangesNone,
    Text::LaneChangesOwnCar,
    Text::LaneChangesAll,
];

/// The texts of the controls of the dialog of the run, by ID.
const RUN_CONTROL_TEXTS: [(i32, Text); 6] = [
    (RUN_WINDOW, Text::RunInWindow),
    (RUN_FULLSCREEN, Text::RunFullscreen),
    (RUN_MINUTES_LABEL, Text::RunMinutesLabel),
    (RUN_END_NOTE, Text::RunEndNote),
    (RUN_START, Text::RunStart),
    (IDCANCEL.0, Text::Cancel),
];

/// `WM_USER`; the windows crate does not define this message.
const TBM_GETPOS: u32 = 0x0400;

/// How the dialog was closed.
pub enum Outcome {
    /// Cancel, or the close button: nothing is saved.
    Cancel,
    /// OK: the new settings are to be saved.
    Save(Settings),
    /// Start: the new settings are to be saved, and then the program runs in
    /// the mode that was chosen.
    Start(Settings, Mode),
}

/// Shows the dialog with `current` as the initial values.
pub fn show(owner: Option<HWND>, current: Settings) -> Result<Outcome> {
    // The slider is a common control, which needs its class registered.
    let controls = INITCOMMONCONTROLSEX {
        dwSize: size_of::<INITCOMMONCONTROLSEX>() as u32,
        dwICC: ICC_BAR_CLASSES,
    };
    unsafe { InitCommonControlsEx(&controls).ok()? };

    let mut state = State {
        settings: current,
        outcome: None,
    };
    let result = unsafe {
        DialogBoxParamW(
            Some(GetModuleHandleW(None)?.into()),
            // MAKEINTRESOURCE: a resource ID is passed as a pointer-sized number.
            PCWSTR(DIALOG as *const u16),
            owner,
            Some(dialog_proc),
            LPARAM((&raw mut state) as isize),
        )
    };
    if result == -1 {
        return Err(Error::from_thread());
    }
    Ok(match state.outcome {
        None => Outcome::Cancel,
        Some(None) => Outcome::Save(state.settings),
        Some(Some(mode)) => Outcome::Start(state.settings, mode),
    })
}

/// What the dialog procedure reads and writes; it lives on the stack of
/// `show`, which does not return before the dialog is closed.
struct State {
    settings: Settings,
    /// `None` until the dialog is closed with OK (`Some(None)`) or Start
    /// (`Some(Some(mode))`).
    outcome: Option<Option<Mode>>,
}

unsafe extern "system" fn dialog_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> isize {
    match message {
        WM_INITDIALOG => {
            // Keep the state for the later messages, and fill in the controls.
            unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, lparam.0) };
            if let Some(state) = unsafe { state_of(hwnd) } {
                init_controls(hwnd, &state.settings);
                apply_language(hwnd, state.settings.language);
                // The drop-down is empty until the language is applied.
                select_trucks(hwnd, state.settings.trucks);
                select_tunnels(hwnd, state.settings.tunnels);
                select_car_shapes(hwnd, state.settings.car_shapes);
                select_car_color(hwnd, state.settings.car_color);
                select_lane_changes(hwnd, state.settings.lane_changes);
                select_speedometer(hwnd, state.settings.speedometer);
                select_mode(hwnd, state.settings.mode);
                update_enabled(hwnd);
            }
            // Let the dialog manager give the focus to the first control.
            1
        }
        WM_HSCROLL => {
            update_speed_label(hwnd);
            0
        }
        WM_COMMAND => {
            // The low word of wparam is the ID of the control or menu item.
            match (wparam.0 & 0xFFFF) as i32 {
                // The high word of wparam is the notification code.
                LANGUAGE if (wparam.0 >> 16) as u32 == CBN_SELCHANGE => {
                    // Show the chosen language at once; it is saved only by OK.
                    apply_language(hwnd, selected_language(hwnd));
                    1
                }
                MODE | CAR_SHAPES if (wparam.0 >> 16) as u32 == CBN_SELCHANGE => {
                    // The Original mode uses none of the added features, and
                    // the color means nothing while the cars have no bodies.
                    update_enabled(hwnd);
                    1
                }
                id if id == IDOK.0 => {
                    if let Some(state) = unsafe { state_of(hwnd) } {
                        state.settings = read_controls(hwnd);
                        state.outcome = Some(None);
                    }
                    end(hwnd, 1);
                    1
                }
                RUN_OPEN => {
                    let language = super::resolve_language(selected_language(hwnd));
                    if let Some(state) = unsafe { state_of(hwnd) }
                        && let Some(mode) = run_dialog(hwnd, language)
                    {
                        // Both dialogs close: the settings are saved and the run starts.
                        state.settings = read_controls(hwnd);
                        state.outcome = Some(Some(mode));
                        end(hwnd, 1);
                    }
                    1
                }
                id if id == IDCANCEL.0 => {
                    end(hwnd, 0);
                    1
                }
                _ => 0,
            }
        }
        _ => 0,
    }
}

/// The state stored by `WM_INITDIALOG`.
///
/// # Safety
///
/// Only valid while `show` is running, that is, inside the dialog procedure.
unsafe fn state_of<'a>(hwnd: HWND) -> Option<&'a mut State> {
    let pointer = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *mut State;
    unsafe { pointer.as_mut() }
}

fn end(hwnd: HWND, result: isize) {
    // Nothing can be done about a failure here; the dialog just stays open.
    let _ = unsafe { EndDialog(hwnd, result) };
}

/// The language selected in the drop-down.
fn selected_language(hwnd: HWND) -> Language {
    let index =
        unsafe { SendDlgItemMessageW(hwnd, LANGUAGE, CB_GETCURSEL, WPARAM(0), LPARAM(0)) }.0;
    // CB_ERR (-1) means nothing is selected; keep the default then.
    u32::try_from(index)
        .ok()
        .and_then(|index| Language::from_raw(Some(index)))
        .unwrap_or_default()
}

/// The truck style selected in the drop-down; the default while the
/// drop-down is still empty.
fn selected_trucks(hwnd: HWND) -> TruckStyle {
    let index = unsafe { SendDlgItemMessageW(hwnd, TRUCKS, CB_GETCURSEL, WPARAM(0), LPARAM(0)) }.0;
    // CB_ERR (-1) means nothing is selected.
    u32::try_from(index)
        .ok()
        .and_then(|index| TruckStyle::from_raw(Some(index)))
        .unwrap_or_default()
}

fn select_trucks(hwnd: HWND, trucks: TruckStyle) {
    unsafe {
        SendDlgItemMessageW(
            hwnd,
            TRUCKS,
            CB_SETCURSEL,
            WPARAM(trucks.to_raw() as usize),
            LPARAM(0),
        );
    }
}

/// The tunnel choice selected in the drop-down; the default while the
/// drop-down is still empty.
fn selected_tunnels(hwnd: HWND) -> TunnelLighting {
    let index = unsafe { SendDlgItemMessageW(hwnd, TUNNELS, CB_GETCURSEL, WPARAM(0), LPARAM(0)) }.0;
    // CB_ERR (-1) means nothing is selected.
    u32::try_from(index)
        .ok()
        .and_then(|index| TunnelLighting::from_raw(Some(index)))
        .unwrap_or_default()
}

fn select_tunnels(hwnd: HWND, tunnels: TunnelLighting) {
    unsafe {
        SendDlgItemMessageW(
            hwnd,
            TUNNELS,
            CB_SETCURSEL,
            WPARAM(tunnels.to_raw() as usize),
            LPARAM(0),
        );
    }
}

/// The car shape choice selected in the drop-down; the default while the
/// drop-down is still empty.
fn selected_car_shapes(hwnd: HWND) -> CarShapes {
    let index =
        unsafe { SendDlgItemMessageW(hwnd, CAR_SHAPES, CB_GETCURSEL, WPARAM(0), LPARAM(0)) }.0;
    // CB_ERR (-1) means nothing is selected.
    u32::try_from(index)
        .ok()
        .and_then(|index| CarShapes::from_raw(Some(index)))
        .unwrap_or_default()
}

fn select_car_shapes(hwnd: HWND, car_shapes: CarShapes) {
    unsafe {
        SendDlgItemMessageW(
            hwnd,
            CAR_SHAPES,
            CB_SETCURSEL,
            WPARAM(car_shapes.to_raw() as usize),
            LPARAM(0),
        );
    }
}

/// The car color choice selected in the drop-down; the default while the
/// drop-down is still empty.
fn selected_car_color(hwnd: HWND) -> CarColor {
    let index =
        unsafe { SendDlgItemMessageW(hwnd, CAR_COLOR, CB_GETCURSEL, WPARAM(0), LPARAM(0)) }.0;
    // CB_ERR (-1) means nothing is selected.
    u32::try_from(index)
        .ok()
        .and_then(|index| CarColor::from_raw(Some(index)))
        .unwrap_or_default()
}

fn select_car_color(hwnd: HWND, car_color: CarColor) {
    unsafe {
        SendDlgItemMessageW(
            hwnd,
            CAR_COLOR,
            CB_SETCURSEL,
            WPARAM(car_color.to_raw() as usize),
            LPARAM(0),
        );
    }
}

/// The lane changes choice selected in the drop-down; the default while the
/// drop-down is still empty.
fn selected_lane_changes(hwnd: HWND) -> LaneChanges {
    let index =
        unsafe { SendDlgItemMessageW(hwnd, LANE_CHANGES, CB_GETCURSEL, WPARAM(0), LPARAM(0)) }.0;
    // CB_ERR (-1) means nothing is selected.
    u32::try_from(index)
        .ok()
        .and_then(|index| LaneChanges::from_raw(Some(index)))
        .unwrap_or_default()
}

fn select_lane_changes(hwnd: HWND, lane_changes: LaneChanges) {
    unsafe {
        SendDlgItemMessageW(
            hwnd,
            LANE_CHANGES,
            CB_SETCURSEL,
            WPARAM(lane_changes.to_raw() as usize),
            LPARAM(0),
        );
    }
}

/// The mode selected in the drop-down; the default while the drop-down is
/// still empty.
fn selected_mode(hwnd: HWND) -> SceneMode {
    let index = unsafe { SendDlgItemMessageW(hwnd, MODE, CB_GETCURSEL, WPARAM(0), LPARAM(0)) }.0;
    // CB_ERR (-1) means nothing is selected.
    u32::try_from(index)
        .ok()
        .and_then(|index| SceneMode::from_raw(Some(index)))
        .unwrap_or_default()
}

fn select_mode(hwnd: HWND, mode: SceneMode) {
    unsafe {
        SendDlgItemMessageW(
            hwnd,
            MODE,
            CB_SETCURSEL,
            WPARAM(mode.to_raw() as usize),
            LPARAM(0),
        );
    }
}

/// Grays out what the choices made do not use: the controls of the added
/// features in the Original mode, and the color of the car bodies while the
/// cars have no bodies. The values stay as they are.
fn update_enabled(hwnd: HWND) {
    let kitchen_sink = selected_mode(hwnd) == SceneMode::KitchenSink;
    let bodies = kitchen_sink && selected_car_shapes(hwnd).has_bodies();
    let set = |id, enabled| {
        // A control that is missing is left as it is.
        if let Ok(control) = unsafe { GetDlgItem(Some(hwnd), id) } {
            let _ = unsafe { EnableWindow(control, enabled) };
        }
    };
    for id in ADDED_FEATURE_CONTROLS {
        set(id, kitchen_sink);
    }
    for id in [CAR_COLOR_LABEL, CAR_COLOR] {
        set(id, bodies);
    }
}

/// The speedometer choice selected in the drop-down; the default while the
/// drop-down is still empty.
fn selected_speedometer(hwnd: HWND) -> SpeedometerStyle {
    let index =
        unsafe { SendDlgItemMessageW(hwnd, SPEEDOMETER, CB_GETCURSEL, WPARAM(0), LPARAM(0)) }.0;
    // CB_ERR (-1) means nothing is selected.
    u32::try_from(index)
        .ok()
        .and_then(|index| SpeedometerStyle::from_raw(Some(index)))
        .unwrap_or_default()
}

fn select_speedometer(hwnd: HWND, speedometer: SpeedometerStyle) {
    unsafe {
        SendDlgItemMessageW(
            hwnd,
            SPEEDOMETER,
            CB_SETCURSEL,
            WPARAM(speedometer.to_raw() as usize),
            LPARAM(0),
        );
    }
}

/// Shows the dialog in the language that `selected` means, and selects it in
/// the drop-down.
fn apply_language(hwnd: HWND, selected: Language) {
    let language = super::resolve_language(selected);
    unsafe {
        let _ = SetWindowTextW(hwnd, &HSTRING::from(Text::DialogTitle.get(language)));
        for (id, text) in CONTROL_TEXTS {
            let _ = SetDlgItemTextW(hwnd, id, &HSTRING::from(text.get(language)));
        }
    }
    // The drop-downs are refilled, so the chosen items are kept across it.
    let trucks = selected_trucks(hwnd);
    let tunnels = selected_tunnels(hwnd);
    let car_shapes = selected_car_shapes(hwnd);
    let car_color = selected_car_color(hwnd);
    let lane_changes = selected_lane_changes(hwnd);
    let speedometer = selected_speedometer(hwnd);
    let mode = selected_mode(hwnd);
    fill_dropdown(hwnd, MODE, &MODE_TEXTS, language, mode.to_raw());
    fill_dropdown(hwnd, LANGUAGE, &LANGUAGE_TEXTS, language, selected.to_raw());
    fill_dropdown(hwnd, TRUCKS, &TRUCK_TEXTS, language, trucks.to_raw());
    fill_dropdown(hwnd, TUNNELS, &TUNNEL_TEXTS, language, tunnels.to_raw());
    fill_dropdown(
        hwnd,
        CAR_SHAPES,
        &CAR_SHAPE_TEXTS,
        language,
        car_shapes.to_raw(),
    );
    fill_dropdown(
        hwnd,
        CAR_COLOR,
        &CAR_COLOR_TEXTS,
        language,
        car_color.to_raw(),
    );
    fill_dropdown(
        hwnd,
        LANE_CHANGES,
        &LANE_CHANGE_TEXTS,
        language,
        lane_changes.to_raw(),
    );
    fill_dropdown(
        hwnd,
        SPEEDOMETER,
        &SPEEDOMETER_TEXTS,
        language,
        speedometer.to_raw(),
    );
}

/// Replaces the items of a drop-down by `texts` in `language`, and selects
/// the item at `selected`.
fn fill_dropdown(hwnd: HWND, id: i32, texts: &[Text], language: DisplayLanguage, selected: u32) {
    unsafe {
        SendDlgItemMessageW(hwnd, id, CB_RESETCONTENT, WPARAM(0), LPARAM(0));
        for text in texts {
            let item = HSTRING::from(text.get(language));
            SendDlgItemMessageW(
                hwnd,
                id,
                CB_ADDSTRING,
                WPARAM(0),
                LPARAM(item.as_ptr() as isize),
            );
        }
        SendDlgItemMessageW(hwnd, id, CB_SETCURSEL, WPARAM(selected as usize), LPARAM(0));
    }
}

fn init_controls(hwnd: HWND, settings: &Settings) {
    unsafe {
        SendDlgItemMessageW(
            hwnd,
            SPEED_SLIDER,
            TBM_SETRANGE,
            WPARAM(0),
            // MAKELONG(min, max)
            LPARAM(((SPEED_CHOICES - 1) << 16) as isize),
        );
        SendDlgItemMessageW(
            hwnd,
            SPEED_SLIDER,
            TBM_SETPOS,
            WPARAM(1),
            LPARAM(speed_to_position(settings.speed_kmh) as isize),
        );
        let side = if settings.left_hand_traffic {
            LEFT_HAND
        } else {
            RIGHT_HAND
        };
        let _ = CheckRadioButton(hwnd, LEFT_HAND, RIGHT_HAND, side);
        for (id, checked) in [
            (UFO, settings.show_ufo),
            (MOUNTAINS, settings.show_mountains),
            (VARY_SPEED, settings.vary_speed),
            (TRAFFIC_BY_TIME, settings.traffic_by_time),
            (SCENERY_ZONES, settings.scenery_zones),
            (SPAN_MONITORS, settings.span_monitors),
        ] {
            let _ = CheckDlgButton(hwnd, id, if checked { BST_CHECKED } else { BST_UNCHECKED });
        }
    }
    update_speed_label(hwnd);
}

/// Shows the dialog of the run, over `owner`, with the texts in `language`.
/// The dialog begins with the window and no time limit. Returns the mode to
/// run, or `None` for Cancel. The choice is not saved: Start runs, it does not
/// set.
fn run_dialog(owner: HWND, language: DisplayLanguage) -> Option<Mode> {
    let mut state = RunState {
        language,
        mode: None,
    };
    let instance = unsafe { GetModuleHandleW(None) }.ok()?;
    let result = unsafe {
        DialogBoxParamW(
            Some(instance.into()),
            PCWSTR(RUN_DIALOG as *const u16),
            Some(owner),
            Some(run_dialog_proc),
            LPARAM((&raw mut state) as isize),
        )
    };
    // -1 is a failure to show the dialog: nothing starts.
    if result == -1 {
        return None;
    }
    state.mode
}

/// What the procedure of the dialog of the run reads and writes; it lives on
/// the stack of `run_dialog`.
struct RunState {
    language: DisplayLanguage,
    mode: Option<Mode>,
}

unsafe extern "system" fn run_dialog_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> isize {
    match message {
        WM_INITDIALOG => {
            unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, lparam.0) };
            if let Some(state) = unsafe { run_state_of(hwnd) } {
                init_run_controls(hwnd, state.language);
            }
            1
        }
        WM_COMMAND => match (wparam.0 & 0xFFFF) as i32 {
            RUN_START => {
                if let Some(state) = unsafe { run_state_of(hwnd) } {
                    state.mode = Some(read_run_mode(hwnd));
                }
                end(hwnd, 1);
                1
            }
            id if id == IDCANCEL.0 => {
                end(hwnd, 0);
                1
            }
            _ => 0,
        },
        _ => 0,
    }
}

/// The state stored by `WM_INITDIALOG` of the dialog of the run.
///
/// # Safety
///
/// Only valid while `run_dialog` is running, that is, inside its procedure.
unsafe fn run_state_of<'a>(hwnd: HWND) -> Option<&'a mut RunState> {
    let pointer = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *mut RunState;
    unsafe { pointer.as_mut() }
}

/// Sets the texts of the dialog of the run, and chooses the window and no time
/// limit to begin with.
fn init_run_controls(hwnd: HWND, language: DisplayLanguage) {
    unsafe {
        let _ = SetWindowTextW(hwnd, &HSTRING::from(Text::RunLabel.get(language)));
        for (id, text) in RUN_CONTROL_TEXTS {
            let _ = SetDlgItemTextW(hwnd, id, &HSTRING::from(text.get(language)));
        }
        let _ = CheckRadioButton(hwnd, RUN_WINDOW, RUN_FULLSCREEN, RUN_WINDOW);
        SendDlgItemMessageW(
            hwnd,
            RUN_MINUTES,
            EM_LIMITTEXT,
            WPARAM(MINUTES_DIGITS),
            LPARAM(0),
        );
        let _ = SetDlgItemTextW(hwnd, RUN_MINUTES, &HSTRING::from("0"));
    }
}

/// The mode that Start runs: the window or the full screen, which ends after
/// the minutes of the box (see `args::limit_minutes`).
fn read_run_mode(hwnd: HWND) -> Mode {
    let mut valid = BOOL(0);
    let value = unsafe { GetDlgItemInt(hwnd, RUN_MINUTES, Some(&mut valid), false) };
    let minutes = limit_minutes(if valid.as_bool() { value } else { 0 });
    if unsafe { IsDlgButtonChecked(hwnd, RUN_FULLSCREEN) } == BST_CHECKED.0 {
        // The dialog saves the settings first, so the saved choice applies.
        Mode::Play {
            minutes,
            span: None,
        }
    } else {
        Mode::Window { minutes }
    }
}

fn read_controls(hwnd: HWND) -> Settings {
    Settings {
        mode: selected_mode(hwnd),
        speed_kmh: position_to_speed(slider_position(hwnd)),
        left_hand_traffic: unsafe { IsDlgButtonChecked(hwnd, LEFT_HAND) } == BST_CHECKED.0,
        speedometer: selected_speedometer(hwnd),
        show_ufo: unsafe { IsDlgButtonChecked(hwnd, UFO) } == BST_CHECKED.0,
        show_mountains: unsafe { IsDlgButtonChecked(hwnd, MOUNTAINS) } == BST_CHECKED.0,
        trucks: selected_trucks(hwnd),
        tunnels: selected_tunnels(hwnd),
        car_shapes: selected_car_shapes(hwnd),
        car_color: selected_car_color(hwnd),
        lane_changes: selected_lane_changes(hwnd),
        language: selected_language(hwnd),
        vary_speed: unsafe { IsDlgButtonChecked(hwnd, VARY_SPEED) } == BST_CHECKED.0,
        traffic_by_time: unsafe { IsDlgButtonChecked(hwnd, TRAFFIC_BY_TIME) } == BST_CHECKED.0,
        scenery_zones: unsafe { IsDlgButtonChecked(hwnd, SCENERY_ZONES) } == BST_CHECKED.0,
        span_monitors: unsafe { IsDlgButtonChecked(hwnd, SPAN_MONITORS) } == BST_CHECKED.0,
    }
}

fn slider_position(hwnd: HWND) -> u32 {
    let position =
        unsafe { SendDlgItemMessageW(hwnd, SPEED_SLIDER, TBM_GETPOS, WPARAM(0), LPARAM(0)) };
    u32::try_from(position.0).unwrap_or(0)
}

fn update_speed_label(hwnd: HWND) {
    let text = format!("{} km/h", position_to_speed(slider_position(hwnd)));
    let _ = unsafe { SetDlgItemTextW(hwnd, SPEED_VALUE, &HSTRING::from(text)) };
}
