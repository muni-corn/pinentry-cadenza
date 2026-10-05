//! The client process that gpg-agent is serving, from `OPTION owner`.

/// Identifies the process that triggered a pinentry request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Owner {
    /// Process id of the client (for example `gpg`).
    pub pid: u32,
    /// User id of the client.
    pub uid: u32,
    /// Node name of the machine gpg-agent runs on.
    pub host: Option<String>,
}

impl Owner {
    /// Parses the value of `OPTION owner`, which looks like
    /// `<pid>/<uid> <host>`.
    ///
    /// Returns `None` if the pid or uid is missing or not a number.
    ///
    /// # Examples
    ///
    /// ```
    /// let owner = Owner::parse("1234/1000 cherri").unwrap();
    /// assert_eq!(owner.pid, 1234);
    /// ```
    pub fn parse(value: &str) -> Option<Self> {
        let (ids, host) = match value.trim().split_once(char::is_whitespace) {
            Some((ids, host)) => (ids, Some(host.trim())),
            None => (value.trim(), None),
        };
        let (pid, uid) = ids.split_once('/')?;
        Some(Self {
            pid: pid.parse().ok()?,
            uid: uid.parse().ok()?,
            host: host.filter(|h| !h.is_empty()).map(str::to_owned),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_pid_uid_and_host() {
        assert_eq!(
            Owner::parse("1234/1000 cherri"),
            Some(Owner {
                pid: 1234,
                uid: 1000,
                host: Some("cherri".into())
            })
        );
    }

    #[test]
    fn host_is_optional() {
        let owner = Owner::parse("42/0").unwrap();
        assert_eq!((owner.pid, owner.uid, owner.host), (42, 0, None));
    }

    #[test]
    fn rejects_malformed_values() {
        assert_eq!(Owner::parse(""), None);
        assert_eq!(Owner::parse("1234"), None);
        assert_eq!(Owner::parse("abc/1000 host"), None);
        assert_eq!(Owner::parse("1234/-1 host"), None);
    }
}
