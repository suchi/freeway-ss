//! The settings dialog, shown for `/c` and when the `.scr` file is opened.
//!
//! The layout is the dialog template in `resources/freeway.rc`; the control
//! IDs below must match it.

use freeway_core::settings::{SPEED_CHOICES, position_to_speed, speed_to_position};
use freeway_core::{
    CarBodies, DisplayLanguage, Language, Settings, Text, TruckStyle, TunnelLighting,
};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Controls::{
    BST_CHECKED, BST_UNCHECKED, CheckDlgButton, CheckRadioButton, ICC_BAR_CLASSES,
    INITCOMMONCONTROLSEX, InitCommonControlsEx, IsDlgButtonChecked, TBM_SETPOS, TBM_SETRANGE,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CB_ADDSTRING, CB_GETCURSEL, CB_RESETCONTENT, CB_SETCURSEL, CBN_SELCHANGE, DialogBoxParamW,
    EndDialog, GWLP_USERDATA, GetWindowLongPtrW, IDCANCEL, IDOK, SendDlgItemMessageW,
    SetDlgItemTextW, SetWindowLongPtrW, SetWindowTextW, WM_COMMAND, WM_HSCROLL, WM_INITDIALOG,
};
use windows::core::{Error, HSTRING, PCWSTR, Result};

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
const CAR_BODIES_LABEL: i32 = 119;
const CAR_BODIES: i32 = 120;

/// The texts of the controls that are set from the string table, by ID.
const CONTROL_TEXTS: [(i32, Text); 16] = [
    (SPEED_LABEL, Text::SpeedLabel),
    (SIDE_LABEL, Text::DrivingSideLabel),
    (LEFT_HAND, Text::LeftHandTraffic),
    (RIGHT_HAND, Text::RightHandTraffic),
    (SPEEDOMETER, Text::Speedometer),
    (UFO, Text::Ufo),
    (MOUNTAINS, Text::Mountains),
    (VARY_SPEED, Text::VarySpeed),
    (TRAFFIC_BY_TIME, Text::TrafficByTime),
    (SCENERY_ZONES, Text::SceneryZones),
    (TRUCKS_LABEL, Text::TrucksLabel),
    (TUNNELS_LABEL, Text::TunnelsLabel),
    (CAR_BODIES_LABEL, Text::CarBodiesLabel),
    (LANGUAGE_LABEL, Text::LanguageLabel),
    // OK and Cancel use the IDs of the standard buttons.
    (IDOK.0, Text::Ok),
    (IDCANCEL.0, Text::Cancel),
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

/// The items of the car bodies drop-down, in the order of `CarBodies::ALL`.
const CAR_BODY_TEXTS: [Text; 3] = [
    Text::CarBodiesTransparent,
    Text::CarBodiesTranslucent,
    Text::CarBodiesBlack,
];

/// `WM_USER`; the windows crate does not define this message.
const TBM_GETPOS: u32 = 0x0400;

/// Shows the dialog with `current` as the initial values.
///
/// Returns the new settings when the user chose OK, or `None` for Cancel.
pub fn show(owner: Option<HWND>, current: Settings) -> Result<Option<Settings>> {
    // The slider is a common control, which needs its class registered.
    let controls = INITCOMMONCONTROLSEX {
        dwSize: size_of::<INITCOMMONCONTROLSEX>() as u32,
        dwICC: ICC_BAR_CLASSES,
    };
    unsafe { InitCommonControlsEx(&controls).ok()? };

    let mut state = State {
        settings: current,
        accepted: false,
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
    Ok(state.accepted.then_some(state.settings))
}

/// What the dialog procedure reads and writes; it lives on the stack of
/// `show`, which does not return before the dialog is closed.
struct State {
    settings: Settings,
    accepted: bool,
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
                select_car_bodies(hwnd, state.settings.car_bodies);
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
                id if id == IDOK.0 => {
                    if let Some(state) = unsafe { state_of(hwnd) } {
                        state.settings = read_controls(hwnd);
                        state.accepted = true;
                    }
                    end(hwnd, 1);
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

/// The car body choice selected in the drop-down; the default while the
/// drop-down is still empty.
fn selected_car_bodies(hwnd: HWND) -> CarBodies {
    let index =
        unsafe { SendDlgItemMessageW(hwnd, CAR_BODIES, CB_GETCURSEL, WPARAM(0), LPARAM(0)) }.0;
    // CB_ERR (-1) means nothing is selected.
    u32::try_from(index)
        .ok()
        .and_then(|index| CarBodies::from_raw(Some(index)))
        .unwrap_or_default()
}

fn select_car_bodies(hwnd: HWND, car_bodies: CarBodies) {
    unsafe {
        SendDlgItemMessageW(
            hwnd,
            CAR_BODIES,
            CB_SETCURSEL,
            WPARAM(car_bodies.to_raw() as usize),
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
    let car_bodies = selected_car_bodies(hwnd);
    fill_dropdown(hwnd, LANGUAGE, &LANGUAGE_TEXTS, language, selected.to_raw());
    fill_dropdown(hwnd, TRUCKS, &TRUCK_TEXTS, language, trucks.to_raw());
    fill_dropdown(hwnd, TUNNELS, &TUNNEL_TEXTS, language, tunnels.to_raw());
    fill_dropdown(
        hwnd,
        CAR_BODIES,
        &CAR_BODY_TEXTS,
        language,
        car_bodies.to_raw(),
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
            (SPEEDOMETER, settings.show_speedometer),
            (UFO, settings.show_ufo),
            (MOUNTAINS, settings.show_mountains),
            (VARY_SPEED, settings.vary_speed),
            (TRAFFIC_BY_TIME, settings.traffic_by_time),
            (SCENERY_ZONES, settings.scenery_zones),
        ] {
            let _ = CheckDlgButton(hwnd, id, if checked { BST_CHECKED } else { BST_UNCHECKED });
        }
    }
    update_speed_label(hwnd);
}

fn read_controls(hwnd: HWND) -> Settings {
    Settings {
        speed_kmh: position_to_speed(slider_position(hwnd)),
        left_hand_traffic: unsafe { IsDlgButtonChecked(hwnd, LEFT_HAND) } == BST_CHECKED.0,
        show_speedometer: unsafe { IsDlgButtonChecked(hwnd, SPEEDOMETER) } == BST_CHECKED.0,
        show_ufo: unsafe { IsDlgButtonChecked(hwnd, UFO) } == BST_CHECKED.0,
        show_mountains: unsafe { IsDlgButtonChecked(hwnd, MOUNTAINS) } == BST_CHECKED.0,
        trucks: selected_trucks(hwnd),
        tunnels: selected_tunnels(hwnd),
        car_bodies: selected_car_bodies(hwnd),
        language: selected_language(hwnd),
        vary_speed: unsafe { IsDlgButtonChecked(hwnd, VARY_SPEED) } == BST_CHECKED.0,
        traffic_by_time: unsafe { IsDlgButtonChecked(hwnd, TRAFFIC_BY_TIME) } == BST_CHECKED.0,
        scenery_zones: unsafe { IsDlgButtonChecked(hwnd, SCENERY_ZONES) } == BST_CHECKED.0,
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
