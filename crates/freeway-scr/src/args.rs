//! Parsing of the command line that Windows passes to a screensaver.
//!
//! Windows starts a `.scr` file with `/s` (run full screen), `/p <hwnd>`
//! (draw a preview inside the Screen Saver Settings dialog) or
//! `/c[:<hwnd>]` (show the settings dialog). Launching the file without
//! arguments, for example by double-clicking it, also shows the settings.

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
}

/// Reasons why the command line could not be understood.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArgsError {
    UnknownOption(String),
    MissingWindowHandle,
    InvalidWindowHandle(String),
}

impl fmt::Display for ArgsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownOption(arg) => write!(f, "Unknown option: {arg}"),
            Self::MissingWindowHandle => write!(f, "The /p option requires a window handle"),
            Self::InvalidWindowHandle(value) => write!(f, "Invalid window handle: {value}"),
        }
    }
}

impl std::error::Error for ArgsError {}

/// Parses the arguments that follow the program name.
///
/// Options are case-insensitive, may start with `/` or `-`, and accept the
/// window handle either after a colon (`/p:1234`) or as the next argument
/// (`/p 1234`). Arguments after the recognized option are ignored.
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
}
