//! Windows screensaver host for Night Freeway SS.

// A screensaver must not open a console window.
#![cfg_attr(windows, windows_subsystem = "windows")]

// Why: on other platforms nothing runs the screensaver, but these modules
// are still compiled and unit-tested there, so unused code is expected.
#[cfg_attr(not(windows), allow(dead_code))]
mod args;
#[cfg_attr(not(windows), allow(dead_code))]
mod input;
#[cfg_attr(not(windows), allow(dead_code))]
mod speedometer;
#[cfg(windows)]
mod win;

use std::process::ExitCode;

fn main() -> ExitCode {
    let args = std::env::args_os()
        .skip(1)
        .map(|arg| arg.to_string_lossy().into_owned());
    match args::parse(args) {
        Ok(mode) => run(mode),
        Err(error) => {
            report_error(&error.to_string());
            ExitCode::FAILURE
        }
    }
}

#[cfg(windows)]
fn run(mode: args::Mode) -> ExitCode {
    match win::run(mode) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            report_error(&format!("Night Freeway SS stopped: {error}"));
            ExitCode::FAILURE
        }
    }
}

#[cfg(not(windows))]
fn run(_mode: args::Mode) -> ExitCode {
    report_error("Night Freeway SS runs only on Windows.");
    ExitCode::FAILURE
}

#[cfg(windows)]
fn report_error(message: &str) {
    win::show_error(message);
}

#[cfg(not(windows))]
fn report_error(message: &str) {
    eprintln!("{message}");
}
