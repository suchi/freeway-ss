//! `cargo xtask dist`: builds the screensaver and packages it for release.
//!
//! The OS-independent parts (names, paths, packaging, checksums) are separate
//! from the build step, which needs Windows, so that they are tested anywhere.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::sha256::sha256_hex;
use crate::zip::{self, Entry};

pub const SCR_NAME: &str = "NightFreewaySS.scr";
const SUMS_NAME: &str = "SHA256SUMS.txt";
const BINARY_NAME: &str = "freeway-scr.exe";
const PACKAGE_NAME: &str = "freeway-scr";
/// The README for end users, packaged as `README.md`. The README at the
/// workspace root is for developers and is not packaged.
const DIST_README: &str = "packaging/README.md";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Arch {
    X64,
    Arm64,
}

impl Arch {
    pub const DEFAULT: Arch = Arch::X64;

    pub fn from_target(target: &str) -> Option<Arch> {
        match target {
            "x86_64-pc-windows-msvc" => Some(Arch::X64),
            "aarch64-pc-windows-msvc" => Some(Arch::Arm64),
            _ => None,
        }
    }

    pub fn target(self) -> &'static str {
        match self {
            Arch::X64 => "x86_64-pc-windows-msvc",
            Arch::Arm64 => "aarch64-pc-windows-msvc",
        }
    }

    /// The name used in directory and file names.
    pub fn label(self) -> &'static str {
        match self {
            Arch::X64 => "x64",
            Arch::Arm64 => "arm64",
        }
    }
}

/// Where the outputs of one `dist` run go.
#[derive(Debug, PartialEq, Eq)]
pub struct Layout {
    pub scr: PathBuf,
    pub zip: PathBuf,
    pub sums: PathBuf,
}

pub fn layout(target_dir: &Path, version: &str, arch: Arch) -> Layout {
    let dist = target_dir.join("dist");
    Layout {
        scr: dist.join(arch.label()).join(SCR_NAME),
        zip: dist.join(zip_name(version, arch)),
        sums: dist.join(SUMS_NAME),
    }
}

pub fn zip_name(version: &str, arch: Arch) -> String {
    format!("NightFreewaySS-v{version}-win-{}.zip", arch.label())
}

/// Reads `version` from the `[workspace.package]` table of the root manifest.
pub fn workspace_version(manifest: &str) -> Result<String, String> {
    let mut in_table = false;
    for line in manifest.lines().map(str::trim) {
        if line.starts_with('[') {
            in_table = line == "[workspace.package]";
        } else if in_table
            && let Some(rest) = line.strip_prefix("version")
            && let Some(value) = rest.trim_start().strip_prefix('=')
        {
            let version = value.trim().trim_matches('"');
            // The version becomes part of file names.
            let valid = !version.is_empty()
                && version
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-');
            return if valid {
                Ok(version.to_owned())
            } else {
                Err(format!("unexpected workspace version: {version:?}"))
            };
        }
    }
    Err("no version in [workspace.package] of Cargo.toml".to_owned())
}

/// Returns `existing` with the line for `zip_name` added or replaced.
/// Lines have the `sha256sum` format, so `sha256sum -c` can check them.
pub fn merge_checksum(existing: &str, zip_name: &str, hash: &str) -> String {
    let mut lines: Vec<String> = existing
        .lines()
        .filter(|line| {
            line.split_once("  ")
                .is_none_or(|(_, name)| name != zip_name)
        })
        .map(str::to_owned)
        .collect();
    lines.push(format!("{hash}  {zip_name}"));
    lines.sort_by(|a, b| {
        a.split_once("  ")
            .map(|p| p.1)
            .cmp(&b.split_once("  ").map(|p| p.1))
    });
    lines.iter().map(|line| format!("{line}\n")).collect()
}

/// Keeps the checksum lines whose file exists in `dir`.
fn existing_entries_only(sums: &str, dir: &Path) -> String {
    sums.lines()
        .filter(|line| {
            line.split_once("  ")
                .is_some_and(|(_, name)| dir.join(name).is_file())
        })
        .map(|line| format!("{line}\n"))
        .collect()
}

#[derive(Debug)]
pub struct Packaged {
    pub zip_name: String,
    pub hash: String,
}

/// Copies `exe` to the `.scr` path, zips it with `LICENSE` and the end-user
/// README (`DIST_README`) from `root`, and records the checksum of the zip.
pub fn package(
    root: &Path,
    exe: &Path,
    layout: &Layout,
    version: &str,
    arch: Arch,
) -> Result<Packaged, String> {
    let exe_data = fs::read(exe).map_err(|e| format!("cannot read {}: {e}", exe.display()))?;
    let license = read_text(&root.join("LICENSE"))?;
    let readme = read_text(&root.join(DIST_README))?;

    write(&layout.scr, &exe_data)?;
    let archive = zip::write_stored(&[
        Entry {
            name: SCR_NAME,
            data: &exe_data,
        },
        Entry {
            name: "LICENSE",
            data: &license,
        },
        Entry {
            name: "README.md",
            data: &readme,
        },
    ])?;
    write(&layout.zip, &archive)?;

    let name = zip_name(version, arch);
    let hash = sha256_hex(&archive);
    // Why: target/dist outlives a release, so lines for zips that are no
    // longer there (an older version) are dropped; `sha256sum -c` would fail.
    let dist_dir = layout.zip.parent().unwrap_or(Path::new("."));
    let existing = fs::read_to_string(&layout.sums).unwrap_or_default();
    let existing = existing_entries_only(&existing, dist_dir);
    write(
        &layout.sums,
        merge_checksum(&existing, &name, &hash).as_bytes(),
    )?;
    Ok(Packaged {
        zip_name: name,
        hash,
    })
}

fn read_text(path: &Path) -> Result<Vec<u8>, String> {
    fs::read(path).map_err(|e| format!("cannot read {}: {e}", path.display()))
}

fn write(path: &Path, data: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("cannot create {}: {e}", parent.display()))?;
    }
    fs::write(path, data).map_err(|e| format!("cannot write {}: {e}", path.display()))
}

pub fn run(root: &Path, arch: Arch) -> Result<(), String> {
    // Why: the resources (settings dialog, icon, display name) are compiled
    // with the Windows resource compiler, and the release is linked by the
    // MSVC toolchain, so the .scr is built on Windows. Other systems can only
    // type-check the Windows target (AGENTS.md).
    if !cfg!(windows) {
        return Err("`cargo xtask dist` builds the .scr and runs only on Windows".to_owned());
    }
    let manifest = fs::read_to_string(root.join("Cargo.toml"))
        .map_err(|e| format!("cannot read Cargo.toml: {e}"))?;
    let version = workspace_version(&manifest)?;
    let target_dir = target_dir(root);

    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let status = Command::new(cargo)
        .current_dir(root)
        .args([
            "build",
            "--release",
            "--locked",
            "-p",
            PACKAGE_NAME,
            "--target",
        ])
        .arg(arch.target())
        .status()
        .map_err(|e| format!("cannot run cargo: {e}"))?;
    if !status.success() {
        return Err(format!("cargo build failed for {}", arch.target()));
    }

    let exe = target_dir
        .join(arch.target())
        .join("release")
        .join(BINARY_NAME);
    let layout = layout(&target_dir, &version, arch);
    let packaged = package(root, &exe, &layout, &version, arch)?;
    println!("scr: {}", layout.scr.display());
    println!("zip: {}", layout.zip.display());
    println!("sha256: {}  {}", packaged.hash, packaged.zip_name);
    println!("sums: {}", layout.sums.display());
    Ok(())
}

fn target_dir(root: &Path) -> PathBuf {
    match std::env::var_os("CARGO_TARGET_DIR") {
        Some(dir) => root.join(dir),
        None => root.join("target"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HASH_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const HASH_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

    #[test]
    fn targets_map_to_architectures() {
        assert_eq!(Arch::from_target("x86_64-pc-windows-msvc"), Some(Arch::X64));
        assert_eq!(
            Arch::from_target("aarch64-pc-windows-msvc"),
            Some(Arch::Arm64)
        );
        assert_eq!(Arch::from_target("x86_64-unknown-linux-gnu"), None);
        for arch in [Arch::X64, Arch::Arm64] {
            assert_eq!(Arch::from_target(arch.target()), Some(arch));
        }
    }

    #[test]
    fn layout_has_version_and_architecture_in_names() {
        let x64 = layout(Path::new("t"), "1.0.0", Arch::X64);
        assert_eq!(x64.scr, Path::new("t/dist/x64/NightFreewaySS.scr"));
        assert_eq!(
            x64.zip,
            Path::new("t/dist/NightFreewaySS-v1.0.0-win-x64.zip")
        );
        assert_eq!(x64.sums, Path::new("t/dist/SHA256SUMS.txt"));
        let arm = layout(Path::new("t"), "1.0.0", Arch::Arm64);
        assert_eq!(arm.scr, Path::new("t/dist/arm64/NightFreewaySS.scr"));
        assert_eq!(
            arm.zip,
            Path::new("t/dist/NightFreewaySS-v1.0.0-win-arm64.zip")
        );
    }

    #[test]
    fn reads_the_workspace_version() {
        let manifest = "[workspace]\nmembers = []\n\n[workspace.package]\nversion = \"1.2.3\"\nedition = \"2024\"\n";
        assert_eq!(workspace_version(manifest).unwrap(), "1.2.3");
        // The real manifest of this repository.
        let real = include_str!("../../Cargo.toml");
        assert!(workspace_version(real).is_ok());
    }

    #[test]
    fn ignores_versions_outside_the_workspace_package_table() {
        let manifest = "[package]\nversion = \"9.9.9\"\n[workspace.package]\nedition = \"2024\"\n";
        assert!(workspace_version(manifest).is_err());
    }

    #[test]
    fn rejects_versions_unsafe_for_file_names() {
        let manifest = "[workspace.package]\nversion = \"1.0/../x\"\n";
        assert!(workspace_version(manifest).is_err());
    }

    #[test]
    fn checksum_lines_are_added_replaced_and_sorted() {
        let one = merge_checksum("", "b.zip", HASH_A);
        assert_eq!(one, format!("{HASH_A}  b.zip\n"));
        let two = merge_checksum(&one, "a.zip", HASH_B);
        assert_eq!(two, format!("{HASH_B}  a.zip\n{HASH_A}  b.zip\n"));
        let replaced = merge_checksum(&two, "b.zip", HASH_B);
        assert_eq!(replaced, format!("{HASH_B}  a.zip\n{HASH_B}  b.zip\n"));
    }

    #[test]
    fn packages_the_scr_license_and_readme() {
        let dir = std::env::temp_dir().join(format!("xtask-dist-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("LICENSE"), "license text").unwrap();
        fs::write(dir.join("README.md"), "developer readme").unwrap();
        fs::create_dir_all(dir.join("packaging")).unwrap();
        fs::write(dir.join(DIST_README), "readme text").unwrap();
        let exe = dir.join("freeway-scr.exe");
        fs::write(&exe, b"MZ fake executable").unwrap();

        let target = dir.join("target");
        let paths = layout(&target, "1.0.0", Arch::X64);
        let packaged = package(&dir, &exe, &paths, "1.0.0", Arch::X64).unwrap();

        assert_eq!(fs::read(&paths.scr).unwrap(), b"MZ fake executable");
        let archive = fs::read(&paths.zip).unwrap();
        let files = zip::read_stored(&archive).unwrap();
        assert_eq!(
            files,
            vec![
                (SCR_NAME.to_owned(), b"MZ fake executable".to_vec()),
                ("LICENSE".to_owned(), b"license text".to_vec()),
                ("README.md".to_owned(), b"readme text".to_vec()),
            ]
        );

        assert_eq!(packaged.zip_name, "NightFreewaySS-v1.0.0-win-x64.zip");
        assert_eq!(packaged.hash, sha256_hex(&archive));
        assert_eq!(packaged.hash.len(), 64);
        let sums = fs::read_to_string(&paths.sums).unwrap();
        assert_eq!(sums, format!("{}  {}\n", packaged.hash, packaged.zip_name));

        // A line for a zip that no longer exists (an older version) is dropped.
        fs::write(
            &paths.sums,
            format!(
                "{HASH_A}  NightFreewaySS-v0.9.0-win-x64.zip\n{}",
                fs::read_to_string(&paths.sums).unwrap()
            ),
        )
        .unwrap();
        package(&dir, &exe, &paths, "1.0.0", Arch::X64).unwrap();
        assert_eq!(fs::read_to_string(&paths.sums).unwrap().lines().count(), 1);

        // A second architecture adds to the same checksum file.
        let arm = layout(&target, "1.0.0", Arch::Arm64);
        package(&dir, &exe, &arm, "1.0.0", Arch::Arm64).unwrap();
        assert_eq!(fs::read_to_string(&paths.sums).unwrap().lines().count(), 2);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_input_is_an_error() {
        let dir = std::env::temp_dir().join(format!("xtask-dist-missing-{}", std::process::id()));
        let paths = layout(&dir.join("target"), "1.0.0", Arch::X64);
        let error = package(&dir, &dir.join("none.exe"), &paths, "1.0.0", Arch::X64).unwrap_err();
        assert!(error.contains("none.exe"));
    }
}
