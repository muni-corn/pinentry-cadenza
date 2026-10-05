//! Replaces the current process with a terminal pinentry.

use super::user_data::TerminalHint;

/// Returns `true` when `ttytype` indicates a curses-capable terminal.
///
/// Terminals identified as `dumb`, `unknown`, or absent are treated as
/// incapable of curses rendering and should use `pinentry-tty` instead.
pub fn prefers_curses(ttytype: Option<&str>) -> bool {
    !matches!(ttytype, None | Some("") | Some("dumb") | Some("unknown"))
}

/// Returns terminal pinentry binaries in the order they should be tried.
///
/// An explicit `hint` wins. Otherwise `pinentry-curses` leads when `ttytype`
/// indicates a curses-capable terminal, and `pinentry-tty` leads when it does
/// not.
pub fn terminal_candidates(ttytype: Option<&str>, hint: Option<TerminalHint>) -> [&'static str; 2] {
    let curses_first = match hint {
        Some(TerminalHint::Curses) => true,
        Some(TerminalHint::Tty) => false,
        None => prefers_curses(ttytype),
    };
    if curses_first {
        ["pinentry-curses", "pinentry-tty"]
    } else {
        ["pinentry-tty", "pinentry-curses"]
    }
}

/// Replaces the current process with a terminal pinentry, forwarding `args`.
///
/// Tries the binaries in the order given by [`terminal_candidates`]. Falls
/// back to the other binary if the preferred one is not found. Prints a
/// diagnostic and exits with code 1 if neither binary can be found.
pub fn exec_fallback_pinentry(
    args: &[String],
    ttytype: Option<&str>,
    hint: Option<TerminalHint>,
) -> ! {
    use std::os::unix::process::CommandExt;

    let candidates = terminal_candidates(ttytype, hint);

    for candidate in candidates {
        let err = std::process::Command::new(candidate).args(args).exec();

        // exec() returns only if the binary failed to start
        if err.kind() != std::io::ErrorKind::NotFound {
            eprintln!("pinentry-cadenza: failed to exec {candidate}: {err}");
            std::process::exit(1);
        }
    }

    eprintln!(
        "pinentry-cadenza: no fallback pinentry found (tried {})",
        candidates.join(", ")
    );
    std::process::exit(1);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefers_curses_with_capable_terminal() {
        assert!(prefers_curses(Some("xterm-256color")));
        assert!(prefers_curses(Some("screen-256color")));
        assert!(prefers_curses(Some("vt100")));
    }

    #[test]
    fn prefers_curses_rejects_dumb_terminals() {
        assert!(!prefers_curses(Some("dumb")));
        assert!(!prefers_curses(Some("unknown")));
        assert!(!prefers_curses(Some("")));
        assert!(!prefers_curses(None));
    }

    #[test]
    fn hint_overrides_terminal_type() {
        assert_eq!(
            terminal_candidates(Some("xterm"), Some(TerminalHint::Tty)),
            ["pinentry-tty", "pinentry-curses"]
        );
        assert_eq!(
            terminal_candidates(Some("dumb"), Some(TerminalHint::Curses)),
            ["pinentry-curses", "pinentry-tty"]
        );
    }

    #[test]
    fn without_a_hint_terminal_type_decides() {
        assert_eq!(terminal_candidates(Some("xterm-kitty"), None), [
            "pinentry-curses",
            "pinentry-tty"
        ]);
        assert_eq!(terminal_candidates(None, None), [
            "pinentry-tty",
            "pinentry-curses"
        ]);
    }
}
