//! Handlers for `GETINFO` queries and `OPTION` settings.

use std::io::{self, Write};

use super::protocol::{ERR_UNKNOWN_IPC_COMMAND, write_data, write_err, write_ok};
use crate::state::PinentryState;

/// Handles a `GETINFO` command, writing the response to `out`.
pub(super) fn dispatch_getinfo(
    out: &mut impl Write,
    state: &PinentryState,
    info: &str,
) -> io::Result<()> {
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
pub(super) fn apply_option(state: &mut PinentryState, key: &str, value: Option<String>) {
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
