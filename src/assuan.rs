use std::io::{self, BufRead, Write};

use crate::state::PinentryState;

// gpg-error codes used in Assuan responses
const ERR_NOT_IMPLEMENTED: u32 = 83886179;
const ERR_UNKNOWN_IPC_COMMAND: u32 = 275;

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
pub fn write_ok(w: &mut impl Write, msg: &str) {
    if msg.is_empty() {
        writeln!(w, "OK").expect("write error");
    } else {
        writeln!(w, "OK {msg}").expect("write error");
    }
    w.flush().expect("flush error");
}

/// Writes `ERR <code> <msg>` to `w` and flushes.
pub fn write_err(w: &mut impl Write, code: u32, msg: &str) {
    writeln!(w, "ERR {code} {msg}").expect("write error");
    w.flush().expect("flush error");
}

/// Writes `D <data>` to `w` and flushes.
pub fn write_data(w: &mut impl Write, data: &str) {
    writeln!(w, "D {data}").expect("write error");
    w.flush().expect("flush error");
}

/// Runs the Assuan server loop.
///
/// Emits the Assuan greeting, then reads commands from stdin and writes
/// responses to stdout until `BYE` is received or stdin is closed.
pub fn server_loop(state: &mut PinentryState) {
    let stdout = io::stdout();
    let mut out = stdout.lock();

    writeln!(out, "OK Pleased to meet you").expect("write error");
    out.flush().expect("flush error");

    let stdin = io::stdin();
    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };

        let line = line.trim();

        // skip blank lines and comment lines
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        match parse_command(line) {
            Command::SetDesc(s) => {
                state.desc = Some(s);
                write_ok(&mut out, "");
            }
            Command::SetPrompt(s) => {
                state.prompt = Some(s);
                write_ok(&mut out, "");
            }
            Command::SetTitle(s) => {
                state.title = Some(s);
                write_ok(&mut out, "");
            }
            Command::SetOk(s) => {
                state.ok_label = Some(s);
                write_ok(&mut out, "");
            }
            Command::SetCancel(s) => {
                state.cancel_label = Some(s);
                write_ok(&mut out, "");
            }
            Command::SetNotOk(s) => {
                state.notok_label = Some(s);
                write_ok(&mut out, "");
            }
            Command::SetError(s) => {
                state.error = Some(s);
                write_ok(&mut out, "");
            }
            Command::SetTimeout(secs) => {
                state.timeout = Some(secs);
                write_ok(&mut out, "");
            }
            Command::SetKeyInfo(s) => {
                state.keyinfo = Some(s);
                write_ok(&mut out, "");
            }
            Command::SetRepeat(s) => {
                state.repeat_prompt = Some(s);
                write_ok(&mut out, "");
            }
            Command::SetRepeatError(s) => {
                state.repeat_error = Some(s);
                write_ok(&mut out, "");
            }
            Command::SetQualityBar(label) => {
                // empty string signals "show with default label"
                state.quality_bar = Some(label.unwrap_or_default());
                write_ok(&mut out, "");
            }
            Command::SetQualityBarTooltip(s) => {
                state.quality_bar_tt = Some(s);
                write_ok(&mut out, "");
            }
            Command::GetPin => {
                // placeholder until the GUI is implemented (commit 4)
                write_err(&mut out, ERR_NOT_IMPLEMENTED, "Not yet implemented");
                state.reset_per_request();
            }
            Command::Confirm { one_button: _ } => {
                write_err(&mut out, ERR_NOT_IMPLEMENTED, "Not yet implemented");
                state.reset_per_request();
            }
            Command::Message => {
                write_err(&mut out, ERR_NOT_IMPLEMENTED, "Not yet implemented");
                state.reset_per_request();
            }
            Command::GetInfo(ref info) => {
                dispatch_getinfo(&mut out, state, info);
            }
            Command::Option(key, value) => {
                apply_option(state, &key, value);
                write_ok(&mut out, "");
            }
            Command::Reset => {
                state.reset_per_request();
                write_ok(&mut out, "");
            }
            Command::Bye => {
                write_ok(&mut out, "closing connection");
                break;
            }
            Command::Nop => {
                write_ok(&mut out, "");
            }
            Command::Unknown(ref cmd) => {
                write_err(
                    &mut out,
                    ERR_UNKNOWN_IPC_COMMAND,
                    &format!("unknown command '{cmd}'"),
                );
            }
        }
    }
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

/// Handles a `GETINFO` command, writing the response to `out`.
fn dispatch_getinfo(out: &mut impl Write, state: &PinentryState, info: &str) {
    match info.to_ascii_lowercase().as_str() {
        "version" => {
            write_data(out, env!("CARGO_PKG_VERSION"));
            write_ok(out, "");
        }
        "pid" => {
            write_data(out, &std::process::id().to_string());
            write_ok(out, "");
        }
        "flavor" => {
            write_data(out, "cadenza");
            write_ok(out, "");
        }
        "ttyinfo" => {
            let ttyname = state.ttyname.as_deref().unwrap_or("-");
            let ttytype = state.ttytype.as_deref().unwrap_or("-");
            let lc_ctype = state.lc_ctype.as_deref().unwrap_or("-");
            let lc_messages = state.lc_messages.as_deref().unwrap_or("-");
            write_data(
                out,
                &format!("{ttyname} {ttytype} {lc_ctype} {lc_messages}"),
            );
            write_ok(out, "");
        }
        _ => {
            write_err(out, ERR_UNKNOWN_IPC_COMMAND, "unknown getinfo query");
        }
    }
}

/// Applies a key-value option from an `OPTION` command to `state`.
///
/// Unknown option keys are silently ignored per the Assuan spec.
fn apply_option(state: &mut PinentryState, key: &str, value: Option<String>) {
    match key {
        "ttyname" => state.ttyname = value,
        "ttytype" => state.ttytype = value,
        "lc-ctype" => state.lc_ctype = value,
        "lc-messages" => state.lc_messages = value,
        "display" => state.display = value,
        "grab" => state.grab = true,
        "no-grab" => state.grab = false,
        "invisible-char" => state.invisible_char = value.and_then(|s| s.chars().next()),
        "formatted-passphrase" => state.formatted_passphrase = true,
        // all other options are accepted without effect
        _ => {}
    }
}
