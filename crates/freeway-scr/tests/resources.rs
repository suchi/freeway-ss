//! Checks the resources that Windows reads from the built `.scr`: the display
//! name in the screensaver list, the icon in Explorer and the dialogs.
#![cfg(windows)]

use windows::Win32::Foundation::FreeLibrary;
use windows::Win32::System::LibraryLoader::{
    FindResourceW, LOAD_LIBRARY_AS_DATAFILE, LoadLibraryExW,
};
use windows::Win32::UI::WindowsAndMessaging::{LoadStringW, RT_DIALOG, RT_GROUP_ICON};
use windows::core::{HSTRING, PCWSTR, PWSTR};

#[test]
fn executable_has_display_name_and_icon() {
    // The binary that cargo builds for integration tests, with its resources.
    let path = HSTRING::from(env!("CARGO_BIN_EXE_freeway-scr"));
    let module = unsafe { LoadLibraryExW(&path, None, LOAD_LIBRARY_AS_DATAFILE) }
        .expect("the executable can be loaded as a data file");

    let mut buffer = [0u16; 64];
    let length = unsafe { LoadStringW(Some(module.into()), 1, PWSTR(buffer.as_mut_ptr()), 64) };
    let name = String::from_utf16_lossy(&buffer[..length.max(0) as usize]);

    // MAKEINTRESOURCE(1): the icon group with the lowest ID.
    let id = PCWSTR(std::ptr::without_provenance(1));
    let icon = unsafe { FindResourceW(Some(module), id, RT_GROUP_ICON) };
    let has_icon = !icon.is_invalid();

    // The settings dialog (100) and the dialog that its "Keep running..."
    // button opens (200).
    let dialogs = [100, 200].map(|id| {
        let id = PCWSTR(std::ptr::without_provenance(id));
        !unsafe { FindResourceW(Some(module), id, RT_DIALOG) }.is_invalid()
    });

    unsafe { FreeLibrary(module) }.expect("the module can be freed");
    assert_eq!(name, "Night Freeway SS");
    assert!(has_icon, "icon group 1 is missing");
    assert_eq!(dialogs, [true, true], "the dialogs 100 and 200 are missing");
}
