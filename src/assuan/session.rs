//! A pinentry-side Assuan session over an arbitrary reader and writer.

use std::io::{self, BufRead, BufReader, Stdin, Stdout, Write};

use anyhow::Result;
use secrecy::ExposeSecret;

use super::{
    handlers::{apply_option, dispatch_getinfo},
    protocol::{
        Command, ERR_CANCELLED, ERR_NOT_CONFIRMED, ERR_UNKNOWN_IPC_COMMAND, parse_command,
        percent_encode, write_data, write_err, write_ok,
    },
};
use crate::state::{DialogRequest, DialogResult, PinentryState};

/// What the client asked for next.
#[derive(Debug, Clone, Copy)]
pub enum Step {
    /// The client requested a dialog. Answer it with [`Session::respond`].
    Dialog(DialogRequest),
    /// The client said `BYE` or closed the connection.
    Closed,
}

/// Reads Assuan commands, accumulates dialog state, and writes responses.
///
/// The session handles every command that does not need user interaction
/// itself. When the client asks for a dialog, [`Session::next_dialog`] returns
/// control to the caller, who shows the dialog and then calls
/// [`Session::respond`] with the outcome.
pub struct Session<R, W> {
    reader: R,
    writer: W,
    state: PinentryState,
    pending: Option<DialogRequest>,
    /// The raw line of the pending dialog request.
    pending_line: String,
    /// Raw lines of the commands that configure a dialog, in arrival order.
    transcript: Vec<String>,
}

/// The pieces of a session needed to continue it in another program.
///
/// See [`Session::into_handoff`].
pub struct Handoff<R, W> {
    /// The client's input, including any data already buffered.
    pub reader: R,
    /// The client's output.
    pub writer: W,
    /// Raw `OPTION`, `SET*` and `RESET` lines received so far.
    pub transcript: Vec<String>,
    /// The raw line of the dialog request that has not been answered yet.
    pub pending_line: String,
}

impl Session<BufReader<Stdin>, Stdout> {
    /// Creates a session that talks to gpg-agent over stdin and stdout.
    pub fn stdio() -> Self {
        Self::new(BufReader::new(io::stdin()), io::stdout())
    }
}

impl<R: BufRead, W: Write> Session<R, W> {
    /// Creates a session that reads commands from `reader` and writes
    /// responses to `writer`.
    pub fn new(reader: R, writer: W) -> Self {
        Self {
            reader,
            writer,
            state: PinentryState::default(),
            pending: None,
            pending_line: String::new(),
            transcript: Vec::new(),
        }
    }

    /// Gives up the session so another program can take over the connection.
    ///
    /// Call this right after [`Session::next_dialog`] returned a dialog. The
    /// other program must be brought up to date by replaying
    /// [`Handoff::transcript`] and then sending [`Handoff::pending_line`].
    pub fn into_handoff(self) -> Handoff<R, W> {
        Handoff {
            reader: self.reader,
            writer: self.writer,
            transcript: self.transcript,
            pending_line: self.pending_line,
        }
    }

    /// The state accumulated from `SET*` and `OPTION` commands so far.
    pub fn state(&self) -> &PinentryState {
        &self.state
    }

    /// Writes the Assuan greeting.
    pub fn greet(&mut self) -> io::Result<()> {
        writeln!(
            self.writer,
            "OK Pleased to meet you, process {}",
            std::process::id()
        )?;
        self.writer.flush()
    }

    /// Processes commands until the client requests a dialog or disconnects.
    ///
    /// # Errors
    ///
    /// Returns an error if writing a response fails.
    pub fn next_dialog(&mut self) -> Result<Step> {
        let mut buf = String::new();
        loop {
            buf.clear();
            // a read error is treated like a closed connection
            if !matches!(self.reader.read_line(&mut buf), Ok(n) if n > 0) {
                return Ok(Step::Closed);
            }

            let line = buf.trim().to_owned();

            // skip blank lines and comment lines
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            let command = parse_command(&line);
            if command.is_replayable() {
                self.transcript.push(line.clone());
            }
            if let Some(step) = self.dispatch(command, line)? {
                return Ok(step);
            }
        }
    }

    /// Answers the dialog most recently returned by [`Session::next_dialog`].
    ///
    /// Clears the per-request state afterwards. Does nothing if no dialog is
    /// pending.
    pub fn respond(&mut self, result: DialogResult) -> io::Result<()> {
        let Some(request) = self.pending.take() else {
            return Ok(());
        };

        let out = &mut self.writer;
        match request {
            DialogRequest::GetPin => match result {
                DialogResult::Pin(secret) => {
                    write_data(out, &percent_encode(secret.expose_secret()))?;
                    write_ok(out, "")?;
                }
                _ => write_err(out, ERR_CANCELLED, "cancelled")?,
            },
            DialogRequest::Confirm { .. } => match result {
                DialogResult::Confirmed => write_ok(out, "")?,
                DialogResult::NotConfirmed => write_err(out, ERR_NOT_CONFIRMED, "not confirmed")?,
                _ => write_err(out, ERR_CANCELLED, "cancelled")?,
            },
            // message dialogs are informational — always respond OK
            DialogRequest::Message => write_ok(out, "")?,
        }

        self.state.reset_per_request();
        Ok(())
    }

    /// Handles one command. Returns a step if the loop should yield.
    fn dispatch(&mut self, command: Command, line: String) -> Result<Option<Step>> {
        let out = &mut self.writer;
        let state = &mut self.state;
        match command {
            Command::SetDesc(s) => state.desc = Some(s),
            Command::SetPrompt(s) => state.prompt = Some(s),
            Command::SetTitle(s) => state.title = Some(s),
            Command::SetOk(s) => state.ok_label = Some(s),
            Command::SetCancel(s) => state.cancel_label = Some(s),
            Command::SetNotOk(s) => state.notok_label = Some(s),
            Command::SetError(s) => state.error = Some(s),
            Command::SetTimeout(secs) => state.timeout = Some(secs),
            Command::SetKeyInfo(s) => state.keyinfo = Some(s),
            Command::SetRepeat(s) => state.repeat_prompt = Some(s),
            Command::SetRepeatError(s) => state.repeat_error = Some(s),
            // empty string signals "show with default label"
            Command::SetQualityBar(label) => state.quality_bar = Some(label.unwrap_or_default()),
            Command::SetQualityBarTooltip(s) => state.quality_bar_tt = Some(s),
            Command::Option(key, value) => apply_option(state, &key, value),
            Command::Reset => state.reset_per_request(),
            Command::Nop => {}
            Command::GetInfo(info) => {
                dispatch_getinfo(out, state, &info)?;
                return Ok(None);
            }
            Command::Unknown(cmd) => {
                write_err(
                    out,
                    ERR_UNKNOWN_IPC_COMMAND,
                    &format!("unknown command '{cmd}'"),
                )?;
                return Ok(None);
            }
            Command::Bye => {
                write_ok(out, "closing connection")?;
                return Ok(Some(Step::Closed));
            }
            Command::GetPin => return Ok(Some(self.begin(DialogRequest::GetPin, line))),
            Command::Confirm { one_button } => {
                let request = DialogRequest::Confirm { one_button };
                return Ok(Some(self.begin(request, line)));
            }
            Command::Message => return Ok(Some(self.begin(DialogRequest::Message, line))),
        }
        write_ok(out, "")?;
        Ok(None)
    }

    /// Records a pending dialog and returns the step that announces it.
    fn begin(&mut self, request: DialogRequest, line: String) -> Step {
        self.pending = Some(request);
        self.pending_line = line;
        Step::Dialog(request)
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use secrecy::SecretString;

    use super::*;

    fn session(input: &str) -> Session<Cursor<Vec<u8>>, Vec<u8>> {
        Session::new(Cursor::new(input.as_bytes().to_vec()), Vec::new())
    }

    fn output(s: &Session<Cursor<Vec<u8>>, Vec<u8>>) -> String {
        String::from_utf8(s.writer.clone()).unwrap()
    }

    #[test]
    fn greeting_names_the_process() {
        let mut s = session("");
        s.greet().unwrap();
        assert_eq!(
            output(&s),
            format!("OK Pleased to meet you, process {}\n", std::process::id())
        );
    }

    #[test]
    fn set_commands_accumulate_state_until_a_dialog() {
        let mut s = session("SETDESC hello%20there\nSETPROMPT pw:\nGETPIN\n");
        let step = s.next_dialog().unwrap();
        assert!(matches!(step, Step::Dialog(DialogRequest::GetPin)));
        assert_eq!(s.state().desc.as_deref(), Some("hello there"));
        assert_eq!(s.state().prompt.as_deref(), Some("pw:"));
        assert_eq!(output(&s), "OK\nOK\n");
    }

    #[test]
    fn pin_response_is_percent_encoded_data_then_ok() {
        let mut s = session("GETPIN\n");
        s.next_dialog().unwrap();
        s.respond(DialogResult::Pin(SecretString::from("a%b".to_string())))
            .unwrap();
        assert_eq!(output(&s), "D a%25b\nOK\n");
    }

    #[test]
    fn cancelled_getpin_reports_an_error() {
        let mut s = session("GETPIN\n");
        s.next_dialog().unwrap();
        s.respond(DialogResult::Cancelled).unwrap();
        // GPG_ERR_CANCELED from the pinentry source
        assert_eq!(output(&s), "ERR 83886179 cancelled\n");
    }

    #[test]
    fn confirm_maps_each_result() {
        let mut s = session("CONFIRM\nCONFIRM\nCONFIRM\n");
        s.next_dialog().unwrap();
        s.respond(DialogResult::Confirmed).unwrap();
        s.next_dialog().unwrap();
        s.respond(DialogResult::NotConfirmed).unwrap();
        s.next_dialog().unwrap();
        s.respond(DialogResult::Cancelled).unwrap();
        let out = output(&s);
        let lines: Vec<_> = out.lines().collect();
        assert_eq!(lines[0], "OK");
        // GPG_ERR_NOT_CONFIRMED and GPG_ERR_CANCELED from the pinentry source
        assert_eq!(lines[1], "ERR 83886194 not confirmed");
        assert_eq!(lines[2], "ERR 83886179 cancelled");
    }

    #[test]
    fn message_always_responds_ok() {
        let mut s = session("MESSAGE\n");
        let step = s.next_dialog().unwrap();
        assert!(matches!(step, Step::Dialog(DialogRequest::Message)));
        s.respond(DialogResult::Cancelled).unwrap();
        assert_eq!(output(&s), "OK\n");
    }

    #[test]
    fn respond_clears_per_request_state_but_keeps_options() {
        let mut s = session("OPTION ttyname=/dev/pts/9\nSETDESC x\nGETPIN\n");
        s.next_dialog().unwrap();
        s.respond(DialogResult::Cancelled).unwrap();
        assert!(s.state().desc.is_none());
        assert_eq!(s.state().ttyname.as_deref(), Some("/dev/pts/9"));
    }

    #[test]
    fn respond_without_a_pending_dialog_writes_nothing() {
        let mut s = session("");
        s.respond(DialogResult::Confirmed).unwrap();
        assert_eq!(output(&s), "");
    }

    #[test]
    fn bye_and_eof_both_close() {
        let mut s = session("NOP\nBYE\nGETPIN\n");
        assert!(matches!(s.next_dialog().unwrap(), Step::Closed));
        assert_eq!(output(&s), "OK\nOK closing connection\n");

        let mut s = session("");
        assert!(matches!(s.next_dialog().unwrap(), Step::Closed));
    }

    #[test]
    fn unknown_commands_and_getinfo_get_replies() {
        let mut s = session("FROB\nGETINFO flavor\n# comment\n\nBYE\n");
        s.next_dialog().unwrap();
        assert_eq!(
            output(&s),
            "ERR 275 unknown command 'FROB'\nD cadenza\nOK\nOK closing connection\n"
        );
    }

    #[test]
    fn handoff_carries_replayable_lines_and_the_pending_request() {
        let mut s = session(
            "OPTION ttyname=/dev/pts/2\nSETDESC hi\nGETINFO pid\nNOP\nSETPROMPT \
             pw\nRESET\nGETPIN\n",
        );
        s.next_dialog().unwrap();
        let handoff = s.into_handoff();
        assert_eq!(handoff.transcript, [
            "OPTION ttyname=/dev/pts/2",
            "SETDESC hi",
            "SETPROMPT pw",
            "RESET"
        ]);
        assert_eq!(handoff.pending_line, "GETPIN");
    }

    #[test]
    fn handoff_keeps_input_that_is_still_buffered() {
        let mut s = session("GETPIN\nBYE\n");
        s.next_dialog().unwrap();
        let mut rest = String::new();
        s.into_handoff().reader.read_line(&mut rest).unwrap();
        assert_eq!(rest, "BYE\n");
    }

    #[test]
    fn reset_clears_dialog_fields() {
        let mut s = session("SETDESC x\nRESET\nGETPIN\n");
        s.next_dialog().unwrap();
        assert!(s.state().desc.is_none());
    }
}
