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
//!
//! `/span` and `/nospan` with `/s` or `/play` choose, for that run only,
//! whether the monitors are drawn as one screen, whatever the saved setting is.
//!
//! `/original` and `/kitchensink` with `/s`, `/window` or `/play` choose, for
//! that run only, the mode the road is drawn in, whatever the saved setting is.

use std::fmt;

use freeway_core::SceneMode;

/// How the screensaver was asked to run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Run full screen on every monitor. `span` is `Some` where the command
    /// line chose whether to draw them as one screen; `None` follows the settings.
    Fullscreen { span: Option<bool> },
    /// Draw inside the preview area of the Screen Saver Settings dialog.
    Preview { parent: usize },
    /// Show the settings dialog, owned by `parent` when it is given.
    Configure { parent: Option<usize> },
    /// Run in an ordinary window that input does not close. The window
    /// closes after `minutes` minutes, if they are given.
    Window { minutes: Option<u32> },
    /// Run full screen on every monitor, not ended by input. The screen
    /// closes after `minutes` minutes, if they are given. `span` is as for
    /// `Fullscreen`.
    Play {
        minutes: Option<u32>,
        span: Option<bool>,
    },
}

/// What the command line asks for: how to run, and the mode to draw in if the
/// command line chose one (`None` follows the settings).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommandLine {
    pub mode: Mode,
    pub scene_mode: Option<SceneMode>,
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
        let (option, minutes, span) = match self {
            Mode::Window { minutes } => ("/window", minutes, None),
            Mode::Play { minutes, span } => ("/play", minutes, span),
            Mode::Fullscreen { span } => {
                let mut arguments = vec!["/s".to_owned()];
                arguments.extend(span_argument(span));
                return arguments;
            }
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
        arguments.extend(span_argument(span));
        arguments
    }
}

/// The option that makes the run span the monitors or not, if it was chosen.
fn span_argument(span: Option<bool>) -> Option<String> {
    span.map(|span| if span { "/span" } else { "/nospan" }.to_owned())
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
    /// Both `/span` and `/nospan`.
    ConflictingSpan,
    /// `/span` or `/nospan` without `/s` or `/play`.
    SpanWithoutRun,
    /// Both `/original` and `/kitchensink`.
    ConflictingSceneMode,
    /// `/original` or `/kitchensink` without `/s`, `/window` or `/play`.
    SceneModeWithoutRun,
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
            Self::ConflictingSpan => write!(f, "The /span and /nospan options cannot be combined"),
            Self::SpanWithoutRun => {
                write!(f, "The /span and /nospan options go with /s or /play")
            }
            Self::ConflictingSceneMode => write!(
                f,
                "The /original and /kitchensink options cannot be combined"
            ),
            Self::SceneModeWithoutRun => write!(
                f,
                "The /original and /kitchensink options go with /s, /window or /play"
            ),
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
/// come with them. `/span` and `/nospan` may come anywhere, and only with `/s`
/// or `/play`. `/original` and `/kitchensink` may come anywhere too, and only
/// with `/s`, `/window` or `/play`.
pub fn parse<I, S>(args: I) -> Result<CommandLine, ArgsError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    // Why: the options for the mode and for the monitors are taken out first,
    // so that every mode reads the rest as it always did, wherever they stood.
    let (scene_mode, rest) = take_scene_mode(args)?;
    let (span, rest) = take_span(rest)?;
    let mode = match (parse_mode(rest)?, span) {
        (mode, None) => mode,
        (Mode::Fullscreen { .. }, span) => Mode::Fullscreen { span },
        (Mode::Play { minutes, .. }, span) => Mode::Play { minutes, span },
        _ => return Err(ArgsError::SpanWithoutRun),
    };
    if scene_mode.is_some()
        && !matches!(
            mode,
            Mode::Fullscreen { .. } | Mode::Window { .. } | Mode::Play { .. }
        )
    {
        return Err(ArgsError::SceneModeWithoutRun);
    }
    Ok(CommandLine { mode, scene_mode })
}

/// Takes `/original` and `/kitchensink` out of `args`: the mode the run was
/// told to draw in, and the other arguments.
fn take_scene_mode<I, S>(args: I) -> Result<(Option<SceneMode>, Vec<String>), ArgsError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let (mut original, mut kitchen_sink) = (false, false);
    let mut rest = Vec::new();
    for arg in args {
        let arg = arg.as_ref();
        let name = arg
            .trim()
            .strip_prefix(['/', '-'])
            .map(str::to_ascii_lowercase);
        match name.as_deref() {
            Some("original") => original = true,
            Some("kitchensink") => kitchen_sink = true,
            _ => rest.push(arg.to_owned()),
        }
    }
    match (original, kitchen_sink) {
        (true, true) => Err(ArgsError::ConflictingSceneMode),
        (false, false) => Ok((None, rest)),
        (original, _) => Ok((
            Some(if original {
                SceneMode::Original
            } else {
                SceneMode::KitchenSink
            }),
            rest,
        )),
    }
}

/// Takes `/span` and `/nospan` out of `args`: whether the run was told to span
/// the monitors, and the other arguments.
fn take_span<I, S>(args: I) -> Result<(Option<bool>, Vec<String>), ArgsError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let (mut span, mut nospan) = (false, false);
    let mut rest = Vec::new();
    for arg in args {
        let arg = arg.as_ref();
        let name = arg
            .trim()
            .strip_prefix(['/', '-'])
            .map(str::to_ascii_lowercase);
        match name.as_deref() {
            Some("span") => span = true,
            Some("nospan") => nospan = true,
            _ => rest.push(arg.to_owned()),
        }
    }
    match (span, nospan) {
        (true, true) => Err(ArgsError::ConflictingSpan),
        (false, false) => Ok((None, rest)),
        (span, _) => Ok((Some(span), rest)),
    }
}

fn parse_mode<I, S>(args: I) -> Result<Mode, ArgsError>
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
        "s" => Ok(Mode::Fullscreen { span: None }),
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
        (false, true) => Ok(Mode::Play {
            minutes,
            span: None,
        }),
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
        parse(args.iter().copied()).map(|line| line.mode)
    }

    fn parse_scene_mode(args: &[&str]) -> Result<Option<SceneMode>, ArgsError> {
        parse(args.iter().copied()).map(|line| line.scene_mode)
    }

    #[test]
    fn no_arguments_opens_settings() {
        assert_eq!(parse_args(&[]), Ok(Mode::Configure { parent: None }));
    }

    #[test]
    fn fullscreen_option_is_case_insensitive() {
        for arg in ["/s", "/S", "-s", " /s "] {
            assert_eq!(
                parse_args(&[arg]),
                Ok(Mode::Fullscreen { span: None }),
                "{arg}"
            );
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
        assert_eq!(
            parse_args(&["/s", "extra"]),
            Ok(Mode::Fullscreen { span: None })
        );
    }

    #[test]
    fn the_run_options_are_read_in_any_order_and_case() {
        let window = |minutes| Ok(Mode::Window { minutes });
        let play = |minutes| {
            Ok(Mode::Play {
                minutes,
                span: None,
            })
        };
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
        assert_eq!(
            parse_args(&["/s", "/window"]),
            Ok(Mode::Fullscreen { span: None })
        );
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
            Mode::Fullscreen { span: None },
            Mode::Fullscreen { span: Some(true) },
            Mode::Fullscreen { span: Some(false) },
            Mode::Preview { parent: 4711 },
            Mode::Configure { parent: None },
            Mode::Configure { parent: Some(12) },
            Mode::Window { minutes: None },
            Mode::Window { minutes: Some(30) },
            Mode::Play {
                minutes: None,
                span: None,
            },
            Mode::Play {
                minutes: Some(1440),
                span: None,
            },
            Mode::Play {
                minutes: Some(5),
                span: Some(true),
            },
            Mode::Play {
                minutes: None,
                span: Some(false),
            },
        ];
        for mode in modes {
            assert_eq!(
                parse(mode.arguments()),
                Ok(CommandLine {
                    mode,
                    scene_mode: None
                }),
                "{mode:?}"
            );
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
            parse(arguments).map(|line| line.mode),
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

    #[test]
    fn the_monitors_are_spanned_or_not_for_one_run() {
        let saver = |span| Ok(Mode::Fullscreen { span });
        let play = |span| {
            Ok(Mode::Play {
                minutes: None,
                span,
            })
        };
        assert_eq!(parse_args(&["/s", "/span"]), saver(Some(true)));
        assert_eq!(parse_args(&["/s", "/nospan"]), saver(Some(false)));
        assert_eq!(parse_args(&["/play", "/span"]), play(Some(true)));
        assert_eq!(parse_args(&["/play", "/nospan"]), play(Some(false)));
        // Any order, either prefix, any case.
        assert_eq!(parse_args(&["/span", "/s"]), saver(Some(true)));
        assert_eq!(parse_args(&["-NoSpan", "-S"]), saver(Some(false)));
        assert_eq!(parse_args(&["/SPAN", "/Play"]), play(Some(true)));
        assert_eq!(parse_args(&[" /nospan ", "/play"]), play(Some(false)));
        assert_eq!(
            parse_args(&["/play", "/span", "/minutes:30"]),
            Ok(Mode::Play {
                minutes: Some(30),
                span: Some(true)
            })
        );
        assert_eq!(
            parse_args(&["/minutes:7", "/nospan", "/play"]),
            Ok(Mode::Play {
                minutes: Some(7),
                span: Some(false)
            })
        );
    }

    #[test]
    fn without_the_span_options_the_settings_decide() {
        assert_eq!(parse_args(&["/s"]), Ok(Mode::Fullscreen { span: None }));
        assert_eq!(
            parse_args(&["/play"]),
            Ok(Mode::Play {
                minutes: None,
                span: None
            })
        );
    }

    #[test]
    fn the_span_options_do_not_mix_and_need_a_screen_run() {
        for args in [
            &["/s", "/span", "/nospan"][..],
            &["/nospan", "/play", "/span"],
            &["/span", "/span", "/nospan"],
        ] {
            assert_eq!(
                parse_args(args),
                Err(ArgsError::ConflictingSpan),
                "{args:?}"
            );
        }
        for args in [
            &["/span"][..],
            &["/nospan"],
            &["/window", "/span"],
            &["/nospan", "/window", "/minutes:5"],
            &["/p", "9", "/span"],
            &["/c", "/nospan"],
            &["/c:7", "/span"],
        ] {
            assert_eq!(parse_args(args), Err(ArgsError::SpanWithoutRun), "{args:?}");
        }
    }

    #[test]
    fn the_span_options_take_no_value_and_leave_other_errors_alone() {
        // Not the option, so it is an unknown option like any other.
        for arg in ["/span:1", "/nospan:0", "/spans", "span"] {
            assert!(
                matches!(
                    parse_args(&["/play", arg]),
                    Err(ArgsError::UnknownOption(_))
                ),
                "{arg}"
            );
        }
        assert_eq!(
            parse_args(&["/play", "/span", "/minutes:0"]),
            Err(ArgsError::InvalidMinutes("0".to_owned()))
        );
        assert_eq!(
            parse_args(&["/span", "/minutes:5"]),
            Err(ArgsError::MinutesWithoutRun)
        );
        assert_eq!(
            parse_args(&["/window", "/play", "/span"]),
            Err(ArgsError::ConflictingRuns)
        );
    }

    #[test]
    fn extra_arguments_after_the_options_of_windows_are_still_ignored() {
        assert_eq!(
            parse_args(&["/s", "extra", "/span"]),
            Ok(Mode::Fullscreen { span: Some(true) })
        );
    }

    #[test]
    fn the_messages_of_the_span_options_name_them() {
        let both = ArgsError::ConflictingSpan.to_string();
        assert!(both.contains("/span") && both.contains("/nospan"), "{both}");
        let alone = ArgsError::SpanWithoutRun.to_string();
        assert!(alone.contains("/s") && alone.contains("/play"), "{alone}");
    }

    #[test]
    fn the_mode_is_chosen_for_one_run() {
        let kitchen_sink = Ok(Some(SceneMode::KitchenSink));
        let original = Ok(Some(SceneMode::Original));
        assert_eq!(parse_scene_mode(&["/s", "/original"]), original);
        assert_eq!(parse_scene_mode(&["/s", "/kitchensink"]), kitchen_sink);
        assert_eq!(parse_scene_mode(&["/window", "/original"]), original);
        assert_eq!(parse_scene_mode(&["/play", "/kitchensink"]), kitchen_sink);
        // Any order, either prefix, any case.
        assert_eq!(parse_scene_mode(&["/original", "/s"]), original);
        assert_eq!(parse_scene_mode(&["-KitchenSink", "-Play"]), kitchen_sink);
        assert_eq!(parse_scene_mode(&[" /ORIGINAL ", "/window"]), original);
        assert_eq!(
            parse_scene_mode(&["/minutes:5", "/kitchensink", "/window"]),
            kitchen_sink
        );
        // Without them the settings decide.
        assert_eq!(parse_scene_mode(&["/s"]), Ok(None));
        assert_eq!(parse_scene_mode(&["/window", "/minutes:3"]), Ok(None));
        assert_eq!(parse_scene_mode(&[]), Ok(None));
    }

    #[test]
    fn the_mode_goes_with_the_other_options_of_a_run() {
        assert_eq!(
            parse(["/play", "/original", "/nospan", "/minutes:30"]),
            Ok(CommandLine {
                mode: Mode::Play {
                    minutes: Some(30),
                    span: Some(false)
                },
                scene_mode: Some(SceneMode::Original)
            })
        );
        assert_eq!(
            parse(["/kitchensink", "/span", "/s"]),
            Ok(CommandLine {
                mode: Mode::Fullscreen { span: Some(true) },
                scene_mode: Some(SceneMode::KitchenSink)
            })
        );
        // Arguments after the option of Windows are still ignored.
        assert_eq!(
            parse_args(&["/s", "extra", "/original"]),
            Ok(Mode::Fullscreen { span: None })
        );
    }

    #[test]
    fn the_mode_options_do_not_mix_and_need_a_run() {
        for args in [
            &["/s", "/original", "/kitchensink"][..],
            &["/kitchensink", "/play", "/original"],
            &["/original", "/original", "/kitchensink"],
        ] {
            assert_eq!(
                parse_args(args),
                Err(ArgsError::ConflictingSceneMode),
                "{args:?}"
            );
        }
        for args in [
            &["/original"][..],
            &["/kitchensink"],
            &["/c", "/original"],
            &["/c:7", "/kitchensink"],
            &["/p", "9", "/original"],
            &["/minutes:5", "/original"],
        ] {
            assert!(
                matches!(
                    parse_args(args),
                    Err(ArgsError::SceneModeWithoutRun | ArgsError::MinutesWithoutRun)
                ),
                "{args:?}"
            );
        }
        assert_eq!(
            parse_args(&["/original"]),
            Err(ArgsError::SceneModeWithoutRun)
        );
        assert_eq!(
            parse_args(&["/p", "9", "/kitchensink"]),
            Err(ArgsError::SceneModeWithoutRun)
        );
    }

    #[test]
    fn the_mode_options_take_no_value_and_leave_other_errors_alone() {
        for arg in ["/original:1", "/kitchensink:0", "/originals", "original"] {
            assert!(
                matches!(
                    parse_args(&["/play", arg]),
                    Err(ArgsError::UnknownOption(_))
                ),
                "{arg}"
            );
        }
        assert_eq!(
            parse_args(&["/play", "/original", "/minutes:0"]),
            Err(ArgsError::InvalidMinutes("0".to_owned()))
        );
        assert_eq!(
            parse_args(&["/window", "/play", "/original"]),
            Err(ArgsError::ConflictingRuns)
        );
        assert_eq!(
            parse_args(&["/window", "/original", "/span"]),
            Err(ArgsError::SpanWithoutRun)
        );
    }

    #[test]
    fn the_messages_of_the_mode_options_name_them() {
        let both = ArgsError::ConflictingSceneMode.to_string();
        assert!(
            both.contains("/original") && both.contains("/kitchensink"),
            "{both}"
        );
        let alone = ArgsError::SceneModeWithoutRun.to_string();
        assert!(
            alone.contains("/s") && alone.contains("/window") && alone.contains("/play"),
            "{alone}"
        );
    }
}
