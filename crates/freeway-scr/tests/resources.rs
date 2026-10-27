//! Checks the resources that Windows reads from the built `.scr`: the display
//! name in the screensaver list, the icon in Explorer, the dialogs and the
//! version information.
#![cfg(windows)]

use windows::Win32::Foundation::FreeLibrary;
use windows::Win32::System::LibraryLoader::{
    FindResourceW, LOAD_LIBRARY_AS_DATAFILE, LoadLibraryExW, LoadResource, LockResource,
    SizeofResource,
};
use windows::Win32::UI::WindowsAndMessaging::{LoadStringW, RT_DIALOG, RT_GROUP_ICON, RT_VERSION};
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

/// The text as UTF-16LE with its terminating NUL, the way the version
/// resource stores its strings.
fn utf16_nul(text: &str) -> Vec<u8> {
    text.encode_utf16()
        .chain(std::iter::once(0))
        .flat_map(u16::to_le_bytes)
        .collect()
}

/// Whether the string `key` of the version resource has the value `value`: the
/// key, up to two bytes of padding, and the value.
fn has_string(resource: &[u8], key: &str, value: &str) -> bool {
    let (key, value) = (utf16_nul(key), utf16_nul(value));
    resource
        .windows(key.len())
        .enumerate()
        .filter(|(_, window)| *window == key.as_slice())
        .any(|(at, _)| {
            let rest = &resource[at + key.len()..];
            rest.starts_with(&value) || (rest.starts_with(&[0, 0]) && rest[2..].starts_with(&value))
        })
}

#[test]
fn executable_has_the_version_information_of_the_workspace() {
    let path = HSTRING::from(env!("CARGO_BIN_EXE_freeway-scr"));
    let module = unsafe { LoadLibraryExW(&path, None, LOAD_LIBRARY_AS_DATAFILE) }
        .expect("the executable can be loaded as a data file");
    // MAKEINTRESOURCE(1): the version resource.
    let id = PCWSTR(std::ptr::without_provenance(1));
    let found = unsafe { FindResourceW(Some(module), id, RT_VERSION) };
    assert!(!found.is_invalid(), "the version resource is missing");
    let size = unsafe { SizeofResource(Some(module), found) } as usize;
    let loaded = unsafe { LoadResource(Some(module), found) }.expect("the resource loads");
    let data = unsafe { LockResource(loaded) }.cast::<u8>();
    // SAFETY: the resource is `size` bytes long and stays mapped until the
    // module is freed below; the bytes are copied before that.
    let resource = unsafe { std::slice::from_raw_parts(data, size) }.to_vec();
    unsafe { FreeLibrary(module) }.expect("the module can be freed");

    let version = env!("CARGO_PKG_VERSION");
    for (key, value) in [
        ("FileVersion", version),
        ("ProductVersion", version),
        ("ProductName", "Night Freeway SS"),
        ("FileDescription", "Night Freeway SS"),
        ("InternalName", "NightFreewaySS"),
        ("OriginalFilename", "NightFreewaySS.scr"),
        ("LegalCopyright", "Copyright (c) 2026 suchi"),
    ] {
        assert!(has_string(&resource, key, value), "{key}: {value}");
    }
}
