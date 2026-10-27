//! Embeds the resources (display name, icon, settings dialog, manifest and
//! version information) into the Windows executable.

fn main() {
    println!("cargo:rerun-if-changed=freeway.rc");
    println!("cargo:rerun-if-changed=resources");
    // Why: the version information comes from Cargo.toml (through these
    // variables), so the build script has to run again when the version
    // changes, not only when freeway.rc does.
    println!("cargo:rerun-if-env-changed=CARGO_PKG_VERSION");
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
    // The version of the workspace is the only place where it is written; the
    // resource file takes it from these macros (docs/plans/0021, 3.1).
    let version = |name: &str| std::env::var(name).unwrap_or_else(|_| panic!("{name} is not set"));
    let macros = [
        format!("VERSION_MAJOR={}", version("CARGO_PKG_VERSION_MAJOR")),
        format!("VERSION_MINOR={}", version("CARGO_PKG_VERSION_MINOR")),
        format!("VERSION_PATCH={}", version("CARGO_PKG_VERSION_PATCH")),
        format!("VERSION_TEXT=\"{}\"", version("CARGO_PKG_VERSION")),
    ];
    let result = embed_resource::compile("freeway.rc", macros);
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
