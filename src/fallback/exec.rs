//! Replaces the current process with a terminal pinentry.

/// Returns `true` when `ttytype` indicates a curses-capable terminal.
///
/// Terminals identified as `dumb`, `unknown`, or absent are treated as
/// incapable of curses rendering and should use `pinentry-tty` instead.
pub fn prefers_curses(ttytype: Option<&str>) -> bool {
    !matches!(ttytype, None | Some("") | Some("dumb") | Some("unknown"))
}

/// Replaces the current process with a terminal pinentry, forwarding `args`.
///
/// Selects `pinentry-curses` when `ttytype` indicates a curses-capable
/// terminal, otherwise `pinentry-tty`. Falls back to the other binary if the
/// preferred one is not found. Prints a diagnostic and exits with code 1 if
/// neither binary can be found.
pub fn exec_fallback_pinentry(args: &[String], ttytype: Option<&str>) -> ! {
    use std::os::unix::process::CommandExt;

    let candidates: &[&str] = if prefers_curses(ttytype) {
        &["pinentry-curses", "pinentry-tty"]
    } else {
        &["pinentry-tty", "pinentry-curses"]
    };

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
}
