//! Parsing of the command line that Windows passes to a screensaver.
//!
//! Windows starts a `.scr` file with `/s` (run full screen), `/p <hwnd>`
//! (draw a preview inside the Screen Saver Settings dialog) or
//! `/c[:<hwnd>]` (show the settings dialog). Launching the file without
//! arguments, for example by double-clicking it, also shows the settings.
//!
//! Two more options run the road for as long as it is wanted, not as a
//! screensaver: `/window` (a window) and `/play` (full screen), which input
//! does not end, and `/minutes:<n>` with either of them, which closes it
//! after `n` minutes.

use std::fmt;

/// How the screensaver was asked to run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Run full screen on every monitor.
    Fullscreen,
    /// Draw inside the preview area of the Screen Saver Settings dialog.
    Preview { parent: usize },
    /// Show the settings dialog, owned by `parent` when it is given.
    Configure { parent: Option<usize> },
    /// Run in an ordinary window that input does not close. The window
    /// closes after `minutes` minutes, if they are given.
    Window { minutes: Option<u32> },
    /// Run full screen on every monitor, not ended by input. The screen
    /// closes after `minutes` minutes, if they are given.
    Play { minutes: Option<u32> },
}

/// The fewest and the most minutes that `/minutes` accepts: a day.
pub const MINUTES_RANGE: std::ops::RangeInclusive<u32> = 1..=1440;

/// The minutes for a run from what was typed into a box: 0 means that the run
/// does not end by itself, and more than a day is a day.
pub fn limit_minutes(typed: u32) -> Option<u32> {
    (typed > 0).then(|| typed.min(*MINUTES_RANGE.end()))
}

impl Mode {
    /// The command line that starts this mode, without the program name;
    /// `parse` reads it back. The modes that Windows starts have none.
    pub fn arguments(self) -> Vec<String> {
        let (option, minutes) = match self {
            Mode::Window { minutes } => ("/window", minutes),
            Mode::Play { minutes } => ("/play", minutes),
            Mode::Fullscreen => return vec!["/s".to_owned()],
            Mode::Preview { parent } => return vec!["/p".to_owned(), parent.to_string()],
            Mode::Configure { parent } => {
                return vec![match parent {
                    Some(parent) => format!("/c:{parent}"),
                    None => "/c".to_owned(),
                }];
            }
        };
        let mut arguments = vec![option.to_owned()];
        arguments.extend(minutes.map(|minutes| format!("/minutes:{minutes}")));
        arguments
    }
}

/// Reasons why the command line could not be understood.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArgsError {
    UnknownOption(String),
    MissingWindowHandle,
    InvalidWindowHandle(String),
    /// `/minutes` without a number of minutes in range.
    InvalidMinutes(String),
    /// `/minutes` without `/window` or `/play`.
    MinutesWithoutRun,
    /// Both `/window` and `/play`.
    ConflictingRuns,
}

impl fmt::Display for ArgsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownOption(arg) => write!(f, "Unknown option: {arg}"),
            Self::MissingWindowHandle => write!(f, "The /p option requires a window handle"),
            Self::InvalidWindowHandle(value) => write!(f, "Invalid window handle: {value}"),
            Self::InvalidMinutes(value) => write!(
                f,
                "Invalid number of minutes: \"{value}\" (the /minutes option takes {} to {}, as in /minutes:30)",
                MINUTES_RANGE.start(),
                MINUTES_RANGE.end()
            ),
            Self::MinutesWithoutRun => {
                write!(f, "The /minutes option goes with /window or /play")
            }
            Self::ConflictingRuns => write!(f, "The /window and /play options cannot be combined"),
        }
    }
}

impl std::error::Error for ArgsError {}

/// Parses the arguments that follow the program name.
///
/// Options are case-insensitive, may start with `/` or `-`, and accept the
/// window handle either after a colon (`/p:1234`) or as the next argument
/// (`/p 1234`). Arguments after the option of Windows are ignored. `/window`,
/// `/play` and `/minutes:<n>` may come in any order, and nothing else may
/// come with them.
pub fn parse<I, S>(args: I) -> Result<Mode, ArgsError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut args = args.into_iter();
    let Some(first) = args.next() else {
        return Ok(Mode::Configure { parent: None });
    };
    let first = first.as_ref().trim();
    let unknown = || ArgsError::UnknownOption(first.to_owned());
    let option = first.strip_prefix(['/', '-']).ok_or_else(unknown)?;
    let (name, inline_value) = match option.split_once(':') {
        Some((name, value)) => (name, Some(value)),
        None => (option, None),
    };

    match name.to_ascii_lowercase().as_str() {
        "s" => Ok(Mode::Fullscreen),
        "window" | "play" | "minutes" => parse_run(first, args),
        "c" => Ok(Mode::Configure {
            parent: inline_value.map(parse_window_handle).transpose()?,
        }),
        "p" => {
            let next = args.next();
            let value = inline_value
                .or_else(|| next.as_ref().map(AsRef::as_ref))
                .ok_or(ArgsError::MissingWindowHandle)?;
            Ok(Mode::Preview {
                parent: parse_window_handle(value)?,
            })
        }
        _ => Err(unknown()),
    }
}

/// The arguments of the run modes: `first` and `rest` together, in any order.
fn parse_run<I, S>(first: &str, rest: I) -> Result<Mode, ArgsError>
where
    I: Iterator<Item = S>,
    S: AsRef<str>,
{
    let rest: Vec<S> = rest.collect();
    let (mut window, mut play, mut minutes) = (false, false, None);
    for arg in std::iter::once(first).chain(rest.iter().map(|arg| arg.as_ref().trim())) {
        let unknown = || ArgsError::UnknownOption(arg.to_owned());
        let option = arg.strip_prefix(['/', '-']).ok_or_else(unknown)?;
        let (name, value) = match option.split_once(':') {
            Some((name, value)) => (name, Some(value)),
            None => (option, None),
        };
        match (name.to_ascii_lowercase().as_str(), value) {
            ("window", None) => window = true,
            ("play", None) => play = true,
            ("minutes", value) => minutes = Some(parse_minutes(value.unwrap_or_default())?),
            _ => return Err(unknown()),
        }
    }
    match (window, play) {
        (true, true) => Err(ArgsError::ConflictingRuns),
        (true, false) => Ok(Mode::Window { minutes }),
        (false, true) => Ok(Mode::Play { minutes }),
        (false, false) => Err(ArgsError::MinutesWithoutRun),
    }
}

/// Minutes are passed as unsigned decimal numbers within `MINUTES_RANGE`.
fn parse_minutes(value: &str) -> Result<u32, ArgsError> {
    let value = value.trim();
    match value.parse::<u32>() {
        Ok(minutes)
            if MINUTES_RANGE.contains(&minutes) && value.bytes().all(|b| b.is_ascii_digit()) =>
        {
            Ok(minutes)
        }
        _ => Err(ArgsError::InvalidMinutes(value.to_owned())),
    }
}

/// Window handles are passed as unsigned decimal numbers; zero is not a window.
fn parse_window_handle(value: &str) -> Result<usize, ArgsError> {
    let value = value.trim();
    match value.parse::<usize>() {
        Ok(handle) if handle != 0 => Ok(handle),
        _ => Err(ArgsError::InvalidWindowHandle(value.to_owned())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_args(args: &[&str]) -> Result<Mode, ArgsError> {
        parse(args.iter().copied())
    }

    #[test]
    fn no_arguments_opens_settings() {
        assert_eq!(parse_args(&[]), Ok(Mode::Configure { parent: None }));
    }

    #[test]
    fn fullscreen_option_is_case_insensitive() {
        for arg in ["/s", "/S", "-s", " /s "] {
            assert_eq!(parse_args(&[arg]), Ok(Mode::Fullscreen), "{arg}");
        }
    }

    #[test]
    fn configure_accepts_optional_owner_window() {
        assert_eq!(parse_args(&["/c"]), Ok(Mode::Configure { parent: None }));
        assert_eq!(
            parse_args(&["/C:65874"]),
            Ok(Mode::Configure {
                parent: Some(65874)
            })
        );
    }

    #[test]
    fn preview_accepts_handle_after_colon_or_space() {
        let expected = Ok(Mode::Preview { parent: 1312 });
        assert_eq!(parse_args(&["/p", "1312"]), expected);
        assert_eq!(parse_args(&["/P:1312"]), expected);
    }

    #[test]
    fn preview_requires_a_valid_handle() {
        assert_eq!(parse_args(&["/p"]), Err(ArgsError::MissingWindowHandle));
        assert_eq!(
            parse_args(&["/p", "0"]),
            Err(ArgsError::InvalidWindowHandle("0".to_owned()))
        );
        assert_eq!(
            parse_args(&["/p", "abc"]),
            Err(ArgsError::InvalidWindowHandle("abc".to_owned()))
        );
        assert_eq!(
            parse_args(&["/c:"]),
            Err(ArgsError::InvalidWindowHandle(String::new()))
        );
    }

    #[test]
    fn unknown_options_are_rejected() {
        for arg in ["/a", "/x", "s", "/"] {
            assert_eq!(
                parse_args(&[arg]),
                Err(ArgsError::UnknownOption(arg.to_owned())),
                "{arg}"
            );
        }
    }

    #[test]
    fn extra_arguments_are_ignored() {
        assert_eq!(parse_args(&["/s", "extra"]), Ok(Mode::Fullscreen));
    }

    #[test]
    fn the_run_options_are_read_in_any_order_and_case() {
        let window = |minutes| Ok(Mode::Window { minutes });
        let play = |minutes| Ok(Mode::Play { minutes });
        assert_eq!(parse_args(&["/window"]), window(None));
        assert_eq!(parse_args(&["/play"]), play(None));
        assert_eq!(parse_args(&["/window", "/minutes:30"]), window(Some(30)));
        assert_eq!(parse_args(&["/minutes:30", "/window"]), window(Some(30)));
        assert_eq!(parse_args(&["-PLAY", "-Minutes:1440"]), play(Some(1440)));
        assert_eq!(parse_args(&[" /Window ", "/MINUTES:1"]), window(Some(1)));
    }

    #[test]
    fn the_minutes_must_be_a_number_in_range() {
        for value in ["0", "1441", "-5", "+5", "abc", "", "1.5", "99999999999"] {
            let arg = format!("/minutes:{value}");
            assert_eq!(
                parse_args(&["/window", &arg]),
                Err(ArgsError::InvalidMinutes(value.to_owned())),
                "{value}"
            );
        }
        // No colon is no number either, whichever way the options come.
        assert_eq!(
            parse_args(&["/play", "/minutes"]),
            Err(ArgsError::InvalidMinutes(String::new()))
        );
        assert_eq!(
            parse_args(&["/minutes"]),
            Err(ArgsError::InvalidMinutes(String::new()))
        );
    }

    #[test]
    fn the_minutes_need_a_run_mode_and_the_run_modes_do_not_mix() {
        assert_eq!(
            parse_args(&["/minutes:30"]),
            Err(ArgsError::MinutesWithoutRun)
        );
        assert_eq!(
            parse_args(&["/window", "/play"]),
            Err(ArgsError::ConflictingRuns)
        );
        assert_eq!(
            parse_args(&["/play", "/minutes:5", "/window"]),
            Err(ArgsError::ConflictingRuns)
        );
    }

    #[test]
    fn nothing_else_goes_with_the_run_modes() {
        for args in [
            &["/window", "extra"][..],
            &["/window", "/s"],
            &["/play", "/x"],
            &["/minutes:5", "/p", "1"],
            &["/window:1"],
        ] {
            assert!(
                matches!(parse_args(args), Err(ArgsError::UnknownOption(_))),
                "{args:?}"
            );
        }
    }

    #[test]
    fn the_modes_of_windows_do_not_change() {
        assert_eq!(parse_args(&["/s", "/window"]), Ok(Mode::Fullscreen));
        assert_eq!(
            parse_args(&["/c:7", "/minutes:5"]),
            Ok(Mode::Configure { parent: Some(7) })
        );
        assert_eq!(
            parse_args(&["/p", "9", "/play"]),
            Ok(Mode::Preview { parent: 9 })
        );
    }

    #[test]
    fn a_mode_is_read_back_from_its_arguments() {
        let modes = [
            Mode::Fullscreen,
            Mode::Preview { parent: 4711 },
            Mode::Configure { parent: None },
            Mode::Configure { parent: Some(12) },
            Mode::Window { minutes: None },
            Mode::Window { minutes: Some(30) },
            Mode::Play { minutes: None },
            Mode::Play {
                minutes: Some(1440),
            },
        ];
        for mode in modes {
            assert_eq!(parse(mode.arguments()), Ok(mode), "{mode:?}");
        }
        assert_eq!(
            Mode::Window { minutes: Some(30) }.arguments(),
            ["/window", "/minutes:30"]
        );
    }

    #[test]
    fn the_typed_minutes_are_limited_to_a_day_and_zero_is_no_limit() {
        assert_eq!(limit_minutes(0), None);
        assert_eq!(limit_minutes(1), Some(1));
        assert_eq!(limit_minutes(1440), Some(1440));
        assert_eq!(limit_minutes(1441), Some(1440));
        assert_eq!(limit_minutes(u32::MAX), Some(1440));
        // What it gives is what `/minutes` accepts.
        let arguments = Mode::Window {
            minutes: limit_minutes(5000),
        }
        .arguments();
        assert_eq!(
            parse(arguments),
            Ok(Mode::Window {
                minutes: Some(1440)
            })
        );
    }

    #[test]
    fn the_messages_of_the_run_options_name_the_range() {
        let message = ArgsError::InvalidMinutes("0".to_owned()).to_string();
        assert!(message.contains("1 to 1440"), "{message}");
    }
}
