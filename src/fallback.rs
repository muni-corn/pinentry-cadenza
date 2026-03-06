use std::env;

/// Returns `true` if the pinentry should fall back to a terminal-based program.
///
/// This is the case when an SSH session is detected, which means a graphical
/// overlay would be unreachable, or when no display server is available at all.
pub fn should_use_curses() -> bool {
    // ssh session detected — a graphical dialog would be unreachable
    if env::var_os("SSH_CONNECTION").is_some()
        || env::var_os("SSH_CLIENT").is_some()
        || env::var_os("SSH_TTY").is_some()
    {
        return true;
    }

    // no display server available (treat unset and empty string the same way)
    !env_var_is_set("WAYLAND_DISPLAY") && !env_var_is_set("DISPLAY")
}

/// Returns true if an env var is set to a non-empty value.
fn env_var_is_set(var: &str) -> bool {
    env::var_os(var).is_some_and(|v| !v.is_empty())
}

/// Replaces the current process with `pinentry-curses`, forwarding `args`.
///
/// Falls back to `pinentry-tty` if `pinentry-curses` is not found. Prints a
/// diagnostic to stderr and exits with code 1 if neither binary can be found.
pub fn exec_pinentry_curses(args: &[String]) -> ! {
    use std::os::unix::process::CommandExt;

    for candidate in ["pinentry-curses", "pinentry-tty"] {
        let err = std::process::Command::new(candidate).args(args).exec();

        // exec() returns only if the binary failed to start
        if err.kind() != std::io::ErrorKind::NotFound {
            eprintln!("pinentry-cadenza: failed to exec {candidate}: {err}");
            std::process::exit(1);
        }
    }

    eprintln!("pinentry-cadenza: no fallback pinentry found (tried pinentry-curses, pinentry-tty)");
    std::process::exit(1);
}
