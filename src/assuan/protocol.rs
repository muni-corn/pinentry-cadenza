//! Assuan protocol primitives: command parsing, percent coding, and response
//! writers.

use std::io::{self, Write};

// gpg-error codes used in Assuan responses: the libgpg-error code with
// GPG_ERR_SOURCE_PINENTRY (5) in the top byte. GPG_ERR_CANCELED is 99,
// GPG_ERR_NOT_CONFIRMED is 114, and GPG_ERR_NO_PIN_ENTRY is 85.
pub const ERR_CANCELLED: u32 = 83886179;
pub const ERR_NOT_CONFIRMED: u32 = 83886194;
pub const ERR_UNKNOWN_IPC_COMMAND: u32 = 275;
pub const ERR_NO_PIN_ENTRY: u32 = 83886165;

/// Parsed representation of an Assuan command.
#[derive(Debug)]
pub enum Command {
    SetDesc(String),
    SetPrompt(String),
    SetTitle(String),
    SetOk(String),
    SetCancel(String),
    SetNotOk(String),
    SetError(String),
    SetTimeout(u32),
    SetKeyInfo(String),
    SetRepeat(String),
    SetRepeatError(String),
    /// `SETQUALITYBAR` with an optional custom label; `None` means use the
    /// default.
    SetQualityBar(Option<String>),
    SetQualityBarTooltip(String),
    GetPin,
    Confirm {
        #[allow(dead_code)]
        one_button: bool,
    },
    Message,
    GetInfo(String),
    /// `OPTION key [value]` — a client configuration key-value pair.
    // note: named Option to match the Assuan keyword; does not shadow std::option::Option
    Option(String, Option<String>),
    Reset,
    Bye,
    Nop,
    Unknown(String),
}

impl Command {
    /// Returns `true` for commands that configure a dialog and can be replayed
    /// to another pinentry to reproduce the same state.
    pub fn is_replayable(&self) -> bool {
        !matches!(
            self,
            Self::GetPin
                | Self::Confirm { .. }
                | Self::Message
                | Self::GetInfo(_)
                | Self::Bye
                | Self::Nop
                | Self::Unknown(_)
        )
    }
}

/// Parses a single Assuan command line into a `Command`.
pub fn parse_command(line: &str) -> Command {
    let (keyword, rest) = split_first_word(line);
    match keyword.to_ascii_uppercase().as_str() {
        "SETDESC" => Command::SetDesc(percent_decode(rest)),
        "SETPROMPT" => Command::SetPrompt(percent_decode(rest)),
        "SETTITLE" => Command::SetTitle(percent_decode(rest)),
        "SETOK" => Command::SetOk(percent_decode(rest)),
        "SETCANCEL" => Command::SetCancel(percent_decode(rest)),
        "SETNOTOK" => Command::SetNotOk(percent_decode(rest)),
        "SETERROR" => Command::SetError(percent_decode(rest)),
        "SETTIMEOUT" => Command::SetTimeout(rest.parse().unwrap_or(0)),
        "SETKEYINFO" => Command::SetKeyInfo(percent_decode(rest)),
        "SETREPEAT" => Command::SetRepeat(percent_decode(rest)),
        "SETREPEATERROR" => Command::SetRepeatError(percent_decode(rest)),
        "SETQUALITYBAR" => {
            if rest.is_empty() {
                Command::SetQualityBar(None)
            } else {
                Command::SetQualityBar(Some(percent_decode(rest)))
            }
        }
        "SETQUALITYBAR_TT" => Command::SetQualityBarTooltip(percent_decode(rest)),
        "GETPIN" => Command::GetPin,
        "CONFIRM" => Command::Confirm {
            one_button: rest.contains("--one-button"),
        },
        "MESSAGE" => Command::Message,
        "GETINFO" => Command::GetInfo(rest.to_string()),
        "OPTION" => parse_option(rest),
        "RESET" => Command::Reset,
        "BYE" => Command::Bye,
        "NOP" => Command::Nop,
        _ => Command::Unknown(keyword.to_string()),
    }
}

/// Decodes a percent-encoded Assuan string (`%XX` sequences) to a `String`.
pub fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut result = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hi = char::from(bytes[i + 1]).to_digit(16);
            let lo = char::from(bytes[i + 2]).to_digit(16);
            if let (Some(hi), Some(lo)) = (hi, lo) {
                result.push((hi * 16 + lo) as u8);
                i += 3;
                continue;
            }
        }
        result.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&result).into_owned()
}

/// Percent-encodes a string for use in an Assuan response.
///
/// Encodes `%`, newlines, carriage returns, and other ASCII control characters.
#[allow(dead_code)]
pub fn percent_encode(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    for byte in s.bytes() {
        match byte {
            b'%' => result.push_str("%25"),
            b'\n' => result.push_str("%0A"),
            b'\r' => result.push_str("%0D"),
            b if b < 0x20 || b == 0x7f => result.push_str(&format!("%{b:02X}")),
            _ => result.push(byte as char),
        }
    }
    result
}

/// Writes `OK [msg]` to `w` and flushes.
pub fn write_ok(w: &mut impl Write, msg: &str) -> io::Result<()> {
    if msg.is_empty() {
        writeln!(w, "OK")?;
    } else {
        writeln!(w, "OK {msg}")?;
    }
    w.flush()
}

/// Writes `ERR <code> <msg>` to `w` and flushes.
pub fn write_err(w: &mut impl Write, code: u32, msg: &str) -> io::Result<()> {
    writeln!(w, "ERR {code} {msg}")?;
    w.flush()
}

/// Writes `D <data>` to `w` and flushes.
pub fn write_data(w: &mut impl Write, data: &str) -> io::Result<()> {
    writeln!(w, "D {data}")?;
    w.flush()
}

/// Splits a trimmed line into its first word and the remainder.
fn split_first_word(s: &str) -> (&str, &str) {
    if let Some(pos) = s.find(char::is_whitespace) {
        (&s[..pos], s[pos + 1..].trim_start())
    } else {
        (s, "")
    }
}

/// Parses the argument to an `OPTION` command into `Command::Option(key,
/// value)`.
///
/// Accepts `key=value`, `key value`, and bare `key` forms.
fn parse_option(arg: &str) -> Command {
    if let Some(eq) = arg.find('=') {
        let key = arg[..eq].trim().to_string();
        let value = arg[eq + 1..].to_string();
        let value = if value.is_empty() { None } else { Some(value) };
        Command::Option(key, value)
    } else if let Some(sp) = arg.find(char::is_whitespace) {
        let key = arg[..sp].trim().to_string();
        let value = arg[sp + 1..].trim().to_string();
        let value = if value.is_empty() { None } else { Some(value) };
        Command::Option(key, value)
    } else {
        Command::Option(arg.to_string(), None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_set_commands_with_percent_decoding() {
        let Command::SetDesc(s) = parse_command("SETDESC Enter%0Apassphrase") else {
            panic!("expected SetDesc");
        };
        assert_eq!(s, "Enter\npassphrase");
    }

    #[test]
    fn keyword_is_case_insensitive() {
        assert!(matches!(parse_command("getpin"), Command::GetPin));
        assert!(matches!(parse_command("Bye"), Command::Bye));
    }

    #[test]
    fn parses_timeout_and_falls_back_to_zero() {
        assert!(matches!(
            parse_command("SETTIMEOUT 30"),
            Command::SetTimeout(30)
        ));
        assert!(matches!(
            parse_command("SETTIMEOUT soon"),
            Command::SetTimeout(0)
        ));
    }

    #[test]
    fn quality_bar_label_is_optional() {
        assert!(matches!(
            parse_command("SETQUALITYBAR"),
            Command::SetQualityBar(None)
        ));
        let Command::SetQualityBar(Some(label)) = parse_command("SETQUALITYBAR Strength") else {
            panic!("expected a label");
        };
        assert_eq!(label, "Strength");
    }

    #[test]
    fn confirm_detects_one_button() {
        assert!(matches!(
            parse_command("CONFIRM --one-button"),
            Command::Confirm { one_button: true }
        ));
        assert!(matches!(parse_command("CONFIRM"), Command::Confirm {
            one_button: false
        }));
    }

    #[test]
    fn unknown_keyword_is_reported() {
        let Command::Unknown(k) = parse_command("FROBNICATE now") else {
            panic!("expected Unknown");
        };
        assert_eq!(k, "FROBNICATE");
    }

    #[test]
    fn option_accepts_equals_space_and_bare_forms() {
        let Command::Option(k, v) = parse_command("OPTION ttyname=/dev/pts/3") else {
            panic!("expected Option");
        };
        assert_eq!((k.as_str(), v.as_deref()), ("ttyname", Some("/dev/pts/3")));

        let Command::Option(k, v) = parse_command("OPTION owner 123/1000 host") else {
            panic!("expected Option");
        };
        assert_eq!((k.as_str(), v.as_deref()), ("owner", Some("123/1000 host")));

        let Command::Option(k, v) = parse_command("OPTION grab") else {
            panic!("expected Option");
        };
        assert_eq!((k.as_str(), v), ("grab", None));
    }

    #[test]
    fn option_with_empty_value_has_no_value() {
        let Command::Option(k, v) = parse_command("OPTION ttytype=") else {
            panic!("expected Option");
        };
        assert_eq!((k.as_str(), v), ("ttytype", None));
    }

    #[test]
    fn percent_decode_handles_escapes_and_malformed_input() {
        assert_eq!(percent_decode("a%41b"), "aAb");
        assert_eq!(percent_decode("100%"), "100%");
        assert_eq!(percent_decode("%zz"), "%zz");
        assert_eq!(percent_decode("%41"), "A");
    }

    #[test]
    fn percent_encode_escapes_control_characters() {
        assert_eq!(percent_encode("50%\r\n"), "50%25%0D%0A");
        assert_eq!(percent_encode("a\u{1}b"), "a%01b");
    }

    #[test]
    fn percent_round_trips() {
        let original = "pass%word\nwith\tstuff";
        assert_eq!(percent_decode(&percent_encode(original)), original);
    }

    #[test]
    fn writers_emit_assuan_lines() {
        let mut buf = Vec::new();
        write_ok(&mut buf, "").unwrap();
        write_ok(&mut buf, "closing connection").unwrap();
        write_data(&mut buf, "secret").unwrap();
        write_err(&mut buf, 99, "nope").unwrap();
        assert_eq!(
            String::from_utf8(buf).unwrap(),
            "OK\nOK closing connection\nD secret\nERR 99 nope\n"
        );
    }
}
