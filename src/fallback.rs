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
//!    `SSH_CLIENT`, `SSH_TTY`) — only present if the agent itself was started
//!    in an SSH session. gpg-agent never forwards these from its clients.
//! 2. **CLI arguments** (`--display`, `--ttyname`, `--ttytype`). gpg-agent 2.4
//!    only passes `--display`, and only when the caller has `DISPLAY` set; the
//!    tty options are honored for manual invocation.
//! 3. **Process environment** (`WAYLAND_DISPLAY`, `DISPLAY`, `TERM`, `GPG_TTY`,
//!    `XDG_SESSION_TYPE`) — inherited from gpg-agent itself, with caller values
//!    layered on top. Caller variables that are unset are *not* removed, so an
//!    SSH caller still sees the agent's graphical environment.
//!    `XDG_SESSION_TYPE=tty` is the exception: it is forwarded by the caller,
//!    so it reliably marks a terminal caller.
//!
//! See `docs/notes/gpg-agent-caller-context.md` for the full details.
//!
//! The check must be done **before** the assuan greeting is emitted. If we
//! wrote the greeting first, the connected gpg-agent would be confused by a
//! second greeting from the exec'd fallback binary.
//!
//! # Terminal selection
//!
//! When falling back, [`exec_fallback_pinentry`] selects between
//! `pinentry-curses` and `pinentry-tty` based on `prefers_curses`: if
//! `ttytype` indicates a capable terminal (anything other than `dumb`,
//! `unknown`, or absent), `pinentry-curses` is tried first; otherwise
//! `pinentry-tty` leads. The other binary is tried as a fallback if the
//! preferred one is not installed. A `USE_TTY=1` or `USE_CURSES=1` hint in
//! `PINENTRY_USER_DATA` overrides the terminal type check.

mod caller_env;
mod context;
mod exec;
mod handoff;
mod user_data;

pub use caller_env::CallerEnv;
pub use context::CallerContext;
pub use exec::{exec_fallback_pinentry, terminal_candidates};
pub use handoff::hand_off_to_terminal;

/// Returns `true` if the pinentry should fall back to a terminal-based program.
///
/// The decision is based on the provided [`CallerContext`] rather than reading
/// the process environment directly, so it can be called after CLI args or
/// assuan OPTION lines have been applied to refine the picture of the caller.
pub fn should_use_curses(ctx: &CallerContext) -> bool {
    // the caller explicitly asked for a terminal pinentry
    if ctx.terminal_hint.is_some() {
        return true;
    }

    // explicit ssh session marker in the inherited environment
    if ctx.ssh_session {
        return true;
    }

    // a tty session type can only have been supplied by a terminal caller
    if ctx.is_tty_session() {
        return true;
    }

    // caller has no graphical display — must use terminal
    if !ctx.has_display() {
        return true;
    }

    false
}

/// Returns `true` if a session that is already in progress should be handed
/// to a terminal pinentry, judging by the caller's real environment.
///
/// This is the counterpart of [`should_use_curses`] for after the assuan
/// greeting, when `exec()` is no longer possible.
pub fn should_hand_off(caller: &CallerEnv) -> bool {
    caller.ssh_session
}

#[cfg(test)]
mod tests {
    use super::{user_data::TerminalHint, *};

    // helper to build a minimal context for testing
    fn ctx(display: Option<&str>, ttyname: Option<&str>, ssh_session: bool) -> CallerContext {
        CallerContext {
            display: display.map(str::to_owned),
            ttyname: ttyname.map(str::to_owned),
            ttytype: None,
            ssh_session,
            session_type: None,
            terminal_hint: None,
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
    fn terminal_hint_overrides_a_graphical_context() {
        let c = CallerContext {
            terminal_hint: Some(TerminalHint::Tty),
            ..ctx(Some("wayland-1"), Some("/dev/pts/1"), false)
        };
        assert!(should_use_curses(&c));
    }

    #[test]
    fn tty_session_type_uses_curses_despite_inherited_display() {
        let c = CallerContext {
            session_type: Some("tty".into()),
            ..ctx(Some("wayland-1"), None, false)
        };
        assert!(should_use_curses(&c));
    }

    #[test]
    fn graphical_session_type_prefers_gui() {
        let c = CallerContext {
            session_type: Some("wayland".into()),
            ..ctx(Some("wayland-1"), None, false)
        };
        assert!(!should_use_curses(&c));
    }

    #[test]
    fn empty_display_treated_as_absent() {
        let c = ctx(Some(""), Some("/dev/pts/1"), false);
        assert!(should_use_curses(&c));
    }

    #[test]
    fn ssh_callers_are_handed_off() {
        assert!(should_hand_off(&CallerEnv { ssh_session: true }));
    }

    #[test]
    fn local_callers_keep_the_gui() {
        assert!(!should_hand_off(&CallerEnv::default()));
    }
}
