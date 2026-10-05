//! Parses terminal hints from `PINENTRY_USER_DATA`.
//!
//! GnuPG passes the caller's `PINENTRY_USER_DATA` to the pinentry unchanged.
//! The format is not specified by GnuPG; by community convention (pinentry-mac
//! and many wrapper scripts) it is a comma-separated list of `NAME=VALUE`
//! pairs. A bare `NAME` is accepted as shorthand for `NAME=1`.

/// A terminal pinentry the caller explicitly asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminalHint {
    /// `USE_CURSES=1`: use `pinentry-curses`.
    Curses,
    /// `USE_TTY=1`: use `pinentry-tty`.
    Tty,
}

/// Extracts a terminal hint from a `PINENTRY_USER_DATA` value.
///
/// If several hints are present, the first one wins. A value of `0` disables
/// a hint.
///
/// # Examples
///
/// ```
/// assert_eq!(parse_terminal_hint("USE_TTY=1"), Some(TerminalHint::Tty));
/// assert_eq!(
///     parse_terminal_hint("FOO=1,USE_CURSES"),
///     Some(TerminalHint::Curses)
/// );
/// ```
pub fn parse_terminal_hint(value: &str) -> Option<TerminalHint> {
    value.split(',').find_map(|pair| {
        let (name, setting) = match pair.split_once('=') {
            Some((name, setting)) => (name.trim(), setting.trim()),
            None => (pair.trim(), "1"),
        };
        if setting == "0" || setting.is_empty() {
            return None;
        }
        match name {
            "USE_CURSES" => Some(TerminalHint::Curses),
            "USE_TTY" => Some(TerminalHint::Tty),
            _ => None,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_both_hints() {
        assert_eq!(parse_terminal_hint("USE_TTY=1"), Some(TerminalHint::Tty));
        assert_eq!(
            parse_terminal_hint("USE_CURSES=1"),
            Some(TerminalHint::Curses)
        );
    }

    #[test]
    fn bare_names_count_as_enabled() {
        assert_eq!(parse_terminal_hint("USE_TTY"), Some(TerminalHint::Tty));
    }

    #[test]
    fn finds_hint_among_other_pairs() {
        assert_eq!(
            parse_terminal_hint("FOO=bar, USE_CURSES=1 ,BAZ"),
            Some(TerminalHint::Curses)
        );
    }

    #[test]
    fn first_hint_wins() {
        assert_eq!(
            parse_terminal_hint("USE_TTY=1,USE_CURSES=1"),
            Some(TerminalHint::Tty)
        );
    }

    #[test]
    fn zero_and_empty_disable_a_hint() {
        assert_eq!(parse_terminal_hint("USE_TTY=0"), None);
        assert_eq!(parse_terminal_hint("USE_TTY="), None);
        assert_eq!(
            parse_terminal_hint("USE_TTY=0,USE_CURSES=1"),
            Some(TerminalHint::Curses)
        );
    }

    #[test]
    fn unrelated_data_has_no_hint() {
        assert_eq!(parse_terminal_hint(""), None);
        assert_eq!(parse_terminal_hint("USE_QT=1"), None);
        assert_eq!(parse_terminal_hint("terminal"), None);
    }
}
