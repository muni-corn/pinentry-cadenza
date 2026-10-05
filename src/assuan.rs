mod protocol;

use std::io::{self, BufRead, Write};

use anyhow::Result;
use protocol::{
    Command, ERR_CANCELLED, ERR_NOT_CONFIRMED, ERR_UNKNOWN_IPC_COMMAND, parse_command,
    percent_encode, write_data, write_err, write_ok,
};
use secrecy::ExposeSecret;

use crate::state::{DialogRequest, DialogResult, PinentryState};

/// Runs the Assuan server loop.
///
/// Emits the Assuan greeting, then reads commands from stdin and writes
/// responses to stdout until `BYE` is received or stdin is closed.
///
/// `show_dialog` is called whenever a dialog must be presented to the user.
/// It receives a snapshot of the current `PinentryState` and the dialog type,
/// and returns the user's response. This callback runs on the calling thread
/// and blocks until the user dismisses the dialog.
pub fn server_loop(
    state: &mut PinentryState,
    show_dialog: impl Fn(&PinentryState, DialogRequest) -> Result<DialogResult>,
) -> Result<()> {
    let stdout = io::stdout();
    let mut out = stdout.lock();
    let pid = std::process::id();

    writeln!(out, "OK Pleased to meet you, process {pid}")?;
    out.flush()?;

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
                write_ok(&mut out, "")?;
            }
            Command::SetPrompt(s) => {
                state.prompt = Some(s);
                write_ok(&mut out, "")?;
            }
            Command::SetTitle(s) => {
                state.title = Some(s);
                write_ok(&mut out, "")?;
            }
            Command::SetOk(s) => {
                state.ok_label = Some(s);
                write_ok(&mut out, "")?;
            }
            Command::SetCancel(s) => {
                state.cancel_label = Some(s);
                write_ok(&mut out, "")?;
            }
            Command::SetNotOk(s) => {
                state.notok_label = Some(s);
                write_ok(&mut out, "")?;
            }
            Command::SetError(s) => {
                state.error = Some(s);
                write_ok(&mut out, "")?;
            }
            Command::SetTimeout(secs) => {
                state.timeout = Some(secs);
                write_ok(&mut out, "")?;
            }
            Command::SetKeyInfo(s) => {
                state.keyinfo = Some(s);
                write_ok(&mut out, "")?;
            }
            Command::SetRepeat(s) => {
                state.repeat_prompt = Some(s);
                write_ok(&mut out, "")?;
            }
            Command::SetRepeatError(s) => {
                state.repeat_error = Some(s);
                write_ok(&mut out, "")?;
            }
            Command::SetQualityBar(label) => {
                // empty string signals "show with default label"
                state.quality_bar = Some(label.unwrap_or_default());
                write_ok(&mut out, "")?;
            }
            Command::SetQualityBarTooltip(s) => {
                state.quality_bar_tt = Some(s);
                write_ok(&mut out, "")?;
            }
            Command::GetPin => {
                match show_dialog(state, DialogRequest::GetPin)? {
                    DialogResult::Pin(secret) => {
                        write_data(&mut out, &percent_encode(secret.expose_secret()))?;
                        write_ok(&mut out, "")?;
                    }
                    _ => write_err(&mut out, ERR_CANCELLED, "cancelled")?,
                }
                state.reset_per_request();
            }
            Command::Confirm { one_button } => {
                match show_dialog(state, DialogRequest::Confirm { one_button })? {
                    DialogResult::Confirmed => write_ok(&mut out, "")?,
                    DialogResult::NotConfirmed => {
                        write_err(&mut out, ERR_NOT_CONFIRMED, "not confirmed")?
                    }
                    _ => write_err(&mut out, ERR_CANCELLED, "cancelled")?,
                }
                state.reset_per_request();
            }
            Command::Message => {
                // message dialogs are informational — always respond OK
                show_dialog(state, DialogRequest::Message)?;
                write_ok(&mut out, "")?;
                state.reset_per_request();
            }
            Command::GetInfo(ref info) => {
                dispatch_getinfo(&mut out, state, info)?;
            }
            Command::Option(key, value) => {
                apply_option(state, &key, value);
                write_ok(&mut out, "")?;
            }
            Command::Reset => {
                state.reset_per_request();
                write_ok(&mut out, "")?;
            }
            Command::Bye => {
                write_ok(&mut out, "closing connection")?;
                break;
            }
            Command::Nop => {
                write_ok(&mut out, "")?;
            }
            Command::Unknown(ref cmd) => {
                write_err(
                    &mut out,
                    ERR_UNKNOWN_IPC_COMMAND,
                    &format!("unknown command '{cmd}'"),
                )?;
            }
        }
    }
    Ok(())
}

/// Handles a `GETINFO` command, writing the response to `out`.
fn dispatch_getinfo(out: &mut impl Write, state: &PinentryState, info: &str) -> io::Result<()> {
    match info.to_ascii_lowercase().as_str() {
        "version" => {
            write_data(out, env!("CARGO_PKG_VERSION"))?;
            write_ok(out, "")?;
        }
        "pid" => {
            write_data(out, &std::process::id().to_string())?;
            write_ok(out, "")?;
        }
        "flavor" => {
            write_data(out, "cadenza")?;
            write_ok(out, "")?;
        }
        "ttyinfo" => {
            let ttyname = state.ttyname.as_deref().unwrap_or("-");
            let ttytype = state.ttytype.as_deref().unwrap_or("-");
            let lc_ctype = state.lc_ctype.as_deref().unwrap_or("-");
            let lc_messages = state.lc_messages.as_deref().unwrap_or("-");
            write_data(
                out,
                &format!("{ttyname} {ttytype} {lc_ctype} {lc_messages}"),
            )?;
            write_ok(out, "")?;
        }
        _ => {
            write_err(out, ERR_UNKNOWN_IPC_COMMAND, "unknown getinfo query")?;
        }
    }
    Ok(())
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
