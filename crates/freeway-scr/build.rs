//! Embeds the resources (display name, icon, settings dialog and manifest) into
//! the Windows executable.

fn main() {
    println!("cargo:rerun-if-changed=freeway.rc");
    println!("cargo:rerun-if-changed=resources");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    // Why: the .scr is often run from the folder it was extracted to, such as
    // Downloads. Resolving its DLL imports (d2d1, d3d11, dwrite, ...) only in
    // System32 (LOAD_LIBRARY_SEARCH_SYSTEM32) keeps a DLL planted in that
    // folder from being loaded.
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        println!("cargo:rustc-link-arg-bins=/DEPENDENTLOADFLAG:0x800");
    }
    let result = embed_resource::compile("freeway.rc", embed_resource::NONE);
    // Why: building on Windows must never produce an executable without the
    // dialog, so a missing resource compiler is an error there. Anywhere else
    // (checking the Windows target on Linux, see AGENTS.md) nothing is linked,
    // so the resources are skipped with a warning.
    if cfg!(windows) {
        result.manifest_required().unwrap();
    } else {
        result.manifest_optional().unwrap();
    }
}
