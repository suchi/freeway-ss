//! Development commands for Night Freeway SS, run as `cargo xtask <command>`.
//!
//! This crate uses only the standard library on purpose: it runs in the
//! release workflow, so every dependency would be part of the supply chain.

mod crc32;
mod dist;
mod icon;
mod sha256;
mod zip;

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use dist::Arch;

const USAGE: &str = "\
Usage: cargo xtask <command>

Commands:
  dist [--target <triple>]  Build the screensaver and package it (Windows only).
                            <triple> is x86_64-pc-windows-msvc (default) or
                            aarch64-pc-windows-msvc.
                            Outputs: target/dist/<arch>/NightFreewaySS.scr,
                            target/dist/NightFreewaySS-v<version>-win-<arch>.zip
                            and target/dist/SHA256SUMS.txt.
  icon                      Regenerate crates/freeway-scr/resources/freeway.ico.
";

#[derive(Debug, PartialEq, Eq)]
enum Command {
    Dist(Arch),
    Icon,
    Help,
}

fn parse_args(args: &[String]) -> Result<Command, String> {
    let mut args = args.iter().map(String::as_str);
    match args.next() {
        Some("dist") => {
            let mut arch = Arch::DEFAULT;
            while let Some(arg) = args.next() {
                let target = match arg {
                    "--target" => args.next().ok_or("--target needs a value")?,
                    _ => match arg.strip_prefix("--target=") {
                        Some(target) => target,
                        None => return Err(format!("unknown argument: {arg}")),
                    },
                };
                arch = Arch::from_target(target)
                    .ok_or_else(|| format!("unsupported target: {target}"))?;
            }
            Ok(Command::Dist(arch))
        }
        Some("icon") => match args.next() {
            None => Ok(Command::Icon),
            Some(arg) => Err(format!("unknown argument: {arg}")),
        },
        Some("help" | "--help" | "-h") | None => Ok(Command::Help),
        Some(other) => Err(format!("unknown command: {other}")),
    }
}

fn workspace_root() -> PathBuf {
    // xtask/ sits directly under the workspace root.
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask has a parent directory")
        .to_owned()
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let command = match parse_args(&args) {
        Ok(command) => command,
        Err(message) => {
            eprintln!("error: {message}\n\n{USAGE}");
            return ExitCode::FAILURE;
        }
    };
    let result = match command {
        Command::Dist(arch) => dist::run(&workspace_root(), arch),
        Command::Icon => icon::run(&workspace_root()),
        Command::Help => {
            print!("{USAGE}");
            Ok(())
        }
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("error: {message}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Command, String> {
        parse_args(&args.iter().map(|s| s.to_string()).collect::<Vec<_>>())
    }

    #[test]
    fn dist_defaults_to_x64() {
        assert_eq!(parse(&["dist"]), Ok(Command::Dist(Arch::X64)));
    }

    #[test]
    fn dist_accepts_both_targets() {
        assert_eq!(
            parse(&["dist", "--target", "aarch64-pc-windows-msvc"]),
            Ok(Command::Dist(Arch::Arm64))
        );
        assert_eq!(
            parse(&["dist", "--target=x86_64-pc-windows-msvc"]),
            Ok(Command::Dist(Arch::X64))
        );
    }

    #[test]
    fn dist_rejects_other_targets_and_arguments() {
        assert!(parse(&["dist", "--target", "x86_64-unknown-linux-gnu"]).is_err());
        assert!(parse(&["dist", "--target"]).is_err());
        assert!(parse(&["dist", "--release"]).is_err());
    }

    #[test]
    fn other_commands() {
        assert_eq!(parse(&["icon"]), Ok(Command::Icon));
        assert_eq!(parse(&[]), Ok(Command::Help));
        assert_eq!(parse(&["--help"]), Ok(Command::Help));
        assert!(parse(&["icon", "x"]).is_err());
        assert!(parse(&["build"]).is_err());
    }
}
