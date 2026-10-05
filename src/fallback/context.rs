//! Caller context used to decide between the GUI and a terminal pinentry.

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
    /// Session type (e.g. `wayland` or `tty`) from `XDG_SESSION_TYPE`.
    ///
    /// gpg-agent forwards this variable from its client, so `tty` can only
    /// come from a terminal caller when the agent itself runs in a graphical
    /// session.
    pub session_type: Option<String>,
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
            session_type: env::var("XDG_SESSION_TYPE").ok().filter(|s| !s.is_empty()),
        }
    }

    /// Overrides fields with any non-`None` values supplied via CLI arguments.
    ///
    /// gpg-agent 2.4 passes only `--display` on the command line (and only
    /// when the caller has `DISPLAY` set). `--ttyname` and `--ttytype` are
    /// never passed by gpg-agent; they are honored here for manual use.
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
    /// context from the inherited environment (catching SSH_* vars if the
    /// agent itself runs in an SSH session) and then refines it with whatever
    /// was passed on argv.
    pub fn from_args_and_env(
        display: Option<String>,
        ttyname: Option<String>,
        ttytype: Option<String>,
    ) -> Self {
        let mut ctx = Self::from_env();
        ctx.apply_args(display, ttyname, ttytype);
        ctx
    }

    /// Returns `true` if the session type is `tty`.
    pub fn is_tty_session(&self) -> bool {
        self.session_type.as_deref() == Some("tty")
    }

    /// Returns `true` if a non-empty display is present.
    pub fn has_display(&self) -> bool {
        self.display.as_deref().is_some_and(|d| !d.is_empty())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
            ..Default::default()
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
}
