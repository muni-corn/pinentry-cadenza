//! Fallback detection: decides whether to exec a terminal-based pinentry.
//!
//! # Decision logic
//!
//! pinentry-cadenza is a Wayland/GTK pinentry. When the gpg-agent that spawns
//! it is running under a graphical session but the *calling client* is an SSH
//! session (e.g. `git push` over SSH triggering agent forwarding), we must
//! hand off to a terminal pinentry so the password prompt appears in the
//! caller's terminal rather than on an unreachable display.
//!
//! The decision is made by [`should_use_curses`] using a [`CallerContext`]
//! that is built from three sources, in priority order:
//!
//! 1. **SSH env vars** inherited by the pinentry process (`SSH_CONNECTION`,
//!    `SSH_CLIENT`, `SSH_TTY`) — set when the agent itself was started in an
//!    SSH session.
//! 2. **CLI arguments** (`--display`, `--ttyname`, `--ttytype`) — gpg-agent
//!    passes these on argv when spawning pinentry, reflecting the calling
//!    client's context at the time of the request.
//! 3. **Process environment** (`WAYLAND_DISPLAY`, `DISPLAY`, `TERM`, `GPG_TTY`)
//!    — baseline from the graphical session that started the agent.
//!
//! The check must be done **before** the assuan greeting is emitted. If we
//! wrote the greeting first, the connected gpg-agent would be confused by a
//! second greeting from the exec'd fallback binary.
//!
//! # Terminal selection
//!
//! When falling back, [`exec_fallback_pinentry`] selects between
//! `pinentry-curses` and `pinentry-tty` based on [`prefers_curses`]: if
//! `ttytype` indicates a capable terminal (anything other than `dumb`,
//! `unknown`, or absent), `pinentry-curses` is tried first; otherwise
//! `pinentry-tty` leads. The other binary is tried as a fallback if the
//! preferred one is not installed.

use std::env;

/// Caller context used to decide whether to fall back to a terminal pinentry.
///
/// All fields are sourced from the pinentry's launch environment, CLI flags,
/// or assuan OPTION lines sent by gpg-agent on behalf of the calling client.
/// Building from the environment reflects the startup-time fallback check;
/// building from CLI args or assuan state gives a more accurate picture of
/// what the *caller* can display.
#[derive(Debug, Default, Clone)]
pub struct CallerContext {
    /// X11 or Wayland display string forwarded by the caller (e.g. `:0` or
    /// `wayland-1`). `None` or empty string means no graphical display.
    pub display: Option<String>,
    /// Terminal device path (e.g. `/dev/pts/3`). `None` means no tty.
    pub ttyname: Option<String>,
    /// Terminal type string (e.g. `xterm-256color`). Used to distinguish
    /// curses-capable terminals from dumb ones.
    pub ttytype: Option<String>,
    /// Whether an SSH session was detected in the launch environment.
    pub ssh_session: bool,
}

impl CallerContext {
    /// Builds a `CallerContext` from the current process's environment.
    ///
    /// This reflects what was set *at process startup* — suitable as a
    /// conservative baseline when no better caller info is available yet.
    pub fn from_env() -> Self {
        Self {
            display: env::var("WAYLAND_DISPLAY")
                .ok()
                .filter(|s| !s.is_empty())
                .or_else(|| env::var("DISPLAY").ok().filter(|s| !s.is_empty())),
            ttyname: env::var("GPG_TTY").ok().filter(|s| !s.is_empty()),
            ttytype: env::var("TERM").ok().filter(|s| !s.is_empty()),
            ssh_session: env::var_os("SSH_CONNECTION").is_some()
                || env::var_os("SSH_CLIENT").is_some()
                || env::var_os("SSH_TTY").is_some(),
        }
    }

    /// Overrides fields with any non-`None` values supplied via CLI arguments.
    ///
    /// gpg-agent passes `--display`, `--ttyname`, and `--ttytype` on the
    /// command line when it spawns pinentry, giving the caller's context
    /// before any assuan OPTION lines arrive. Applying these overrides makes
    /// the fallback decision more accurate for the SSH case.
    ///
    /// # Examples
    ///
    /// ```
    /// let mut ctx = CallerContext::from_env();
    /// ctx.apply_args(Some("wayland-1".into()), Some("/dev/pts/3".into()), None);
    /// ```
    pub fn apply_args(
        &mut self,
        display: Option<String>,
        ttyname: Option<String>,
        ttytype: Option<String>,
    ) {
        if let Some(d) = display {
            self.display = Some(d);
        }
        if let Some(t) = ttyname {
            self.ttyname = Some(t);
        }
        if let Some(tt) = ttytype {
            self.ttytype = Some(tt);
        }
    }

    /// Builds a context from the environment and then overrides it with CLI
    /// argument values where present.
    ///
    /// This is the preferred constructor for use in `main` — it seeds the
    /// context from the inherited environment (catching SSH_* vars) and then
    /// refines it with whatever gpg-agent passed on argv.
    pub fn from_args_and_env(
        display: Option<String>,
        ttyname: Option<String>,
        ttytype: Option<String>,
    ) -> Self {
        let mut ctx = Self::from_env();
        ctx.apply_args(display, ttyname, ttytype);
        ctx
    }

    /// Returns `true` if a non-empty display is present.
    pub fn has_display(&self) -> bool {
        self.display.as_deref().is_some_and(|d| !d.is_empty())
    }
}

/// Returns `true` if the pinentry should fall back to a terminal-based program.
///
/// The decision is based on the provided [`CallerContext`] rather than reading
/// the process environment directly, so it can be called after CLI args or
/// assuan OPTION lines have been applied to refine the picture of the caller.
pub fn should_use_curses(ctx: &CallerContext) -> bool {
    // explicit ssh session marker in the inherited environment
    if ctx.ssh_session {
        return true;
    }

    // caller has no graphical display — must use terminal
    if !ctx.has_display() {
        return true;
    }

    false
}

/// Returns `true` when `ttytype` indicates a curses-capable terminal.
///
/// Terminals identified as `dumb`, `unknown`, or absent are treated as
/// incapable of curses rendering and should use `pinentry-tty` instead.
pub fn prefers_curses(ttytype: Option<&str>) -> bool {
    match ttytype {
        None | Some("") | Some("dumb") | Some("unknown") => false,
        Some(_) => true,
    }
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

    // helper to build a minimal context for testing
    fn ctx(display: Option<&str>, ttyname: Option<&str>, ssh_session: bool) -> CallerContext {
        CallerContext {
            display: display.map(str::to_owned),
            ttyname: ttyname.map(str::to_owned),
            ttytype: None,
            ssh_session,
        }
    }

    #[test]
    fn display_set_tty_set_prefers_gui() {
        let c = ctx(Some("wayland-1"), Some("/dev/pts/1"), false);
        assert!(!should_use_curses(&c));
    }

    #[test]
    fn display_unset_tty_set_uses_curses() {
        let c = ctx(None, Some("/dev/pts/1"), false);
        assert!(should_use_curses(&c));
    }

    #[test]
    fn display_unset_tty_unset_uses_curses() {
        let c = ctx(None, None, false);
        assert!(should_use_curses(&c));
    }

    #[test]
    fn display_set_tty_unset_prefers_gui() {
        let c = ctx(Some(":0"), None, false);
        assert!(!should_use_curses(&c));
    }

    #[test]
    fn ssh_session_always_uses_curses() {
        let c = ctx(Some("wayland-1"), Some("/dev/pts/1"), true);
        assert!(should_use_curses(&c));
    }

    #[test]
    fn empty_display_treated_as_absent() {
        let c = ctx(Some(""), Some("/dev/pts/1"), false);
        assert!(should_use_curses(&c));
    }

    #[test]
    fn apply_args_overrides_display() {
        let mut ctx = CallerContext::default();
        ctx.apply_args(Some("wayland-1".into()), None, None);
        assert_eq!(ctx.display.as_deref(), Some("wayland-1"));
        assert!(ctx.ttyname.is_none());
        assert!(ctx.ttytype.is_none());
    }

    #[test]
    fn apply_args_overrides_ttyname() {
        let mut ctx = CallerContext::default();
        ctx.apply_args(None, Some("/dev/pts/5".into()), None);
        assert_eq!(ctx.ttyname.as_deref(), Some("/dev/pts/5"));
        assert!(ctx.display.is_none());
    }

    #[test]
    fn apply_args_overrides_ttytype() {
        let mut ctx = CallerContext::default();
        ctx.apply_args(None, None, Some("xterm-256color".into()));
        assert_eq!(ctx.ttytype.as_deref(), Some("xterm-256color"));
    }

    #[test]
    fn apply_args_none_leaves_existing_values() {
        let mut ctx = CallerContext {
            display: Some("wayland-1".into()),
            ttyname: Some("/dev/pts/1".into()),
            ttytype: Some("xterm".into()),
            ssh_session: false,
        };
        ctx.apply_args(None, None, None);
        // existing values must be preserved when no override is provided
        assert_eq!(ctx.display.as_deref(), Some("wayland-1"));
        assert_eq!(ctx.ttyname.as_deref(), Some("/dev/pts/1"));
        assert_eq!(ctx.ttytype.as_deref(), Some("xterm"));
    }

    #[test]
    fn apply_args_display_overrides_env_display() {
        // simulate an env that has WAYLAND_DISPLAY but gpg-agent passes
        // --display "" (empty), meaning the caller has no display
        let mut ctx = CallerContext {
            display: Some("wayland-1".into()),
            ..Default::default()
        };
        ctx.apply_args(Some(String::new()), None, None);
        // an explicit empty display from argv should clear the display
        assert!(!ctx.has_display());
    }

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
