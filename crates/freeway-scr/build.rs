//! Embeds the resources (settings dialog and manifest) into the Windows executable.

fn main() {
    println!("cargo:rerun-if-changed=freeway.rc");
    println!("cargo:rerun-if-changed=resources");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
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
