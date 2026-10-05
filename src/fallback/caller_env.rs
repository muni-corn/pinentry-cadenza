//! The real environment of the process that asked gpg-agent for a passphrase.

use std::fs;

/// What the calling process's environment says about where it is running.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct CallerEnv {
    /// The caller was started from an SSH session.
    pub ssh_session: bool,
}

impl CallerEnv {
    /// Reads the environment of process `pid` from `/proc`.
    ///
    /// Returns `None` if it can't be read: the process is gone, belongs to
    /// another user, or the platform has no `/proc`.
    pub fn from_pid(pid: u32) -> Option<Self> {
        let raw = fs::read(format!("/proc/{pid}/environ")).ok()?;
        Some(Self::from_environ(&raw))
    }

    /// Parses the NUL-separated `NAME=VALUE` block found in
    /// `/proc/<pid>/environ`.
    pub fn from_environ(raw: &[u8]) -> Self {
        let mut env = Self::default();
        for entry in raw.split(|&b| b == 0) {
            let Some(eq) = entry.iter().position(|&b| b == b'=') else {
                continue;
            };
            if matches!(&entry[..eq], b"SSH_CONNECTION" | b"SSH_CLIENT" | b"SSH_TTY") {
                env.ssh_session = true;
            }
        }
        env
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_ssh_markers() {
        for marker in ["SSH_CONNECTION", "SSH_CLIENT", "SSH_TTY"] {
            let raw = format!("HOME=/home/muni\0{marker}=x\0TERM=xterm\0");
            assert!(CallerEnv::from_environ(raw.as_bytes()).ssh_session);
        }
    }

    #[test]
    fn local_environment_is_not_ssh() {
        let raw = b"HOME=/home/muni\0WAYLAND_DISPLAY=wayland-1\0";
        assert!(!CallerEnv::from_environ(raw).ssh_session);
    }

    #[test]
    fn tolerates_garbage() {
        assert_eq!(CallerEnv::from_environ(b""), CallerEnv::default());
        assert_eq!(
            CallerEnv::from_environ(b"\0\0noequals\0"),
            CallerEnv::default()
        );
    }

    #[test]
    fn missing_process_has_no_environment() {
        assert_eq!(CallerEnv::from_pid(u32::MAX), None);
    }

    #[test]
    fn can_read_our_own_environment() {
        assert!(CallerEnv::from_pid(std::process::id()).is_some());
    }
}
