//! Connects the Assuan session thread to the GTK event loop.

use std::{io, thread};

use super::dialog::{ActiveDialog, PinentryAppMsg};
use crate::{
    assuan::{Session, Step},
    state::DialogResult,
};

/// Serves the Assuan session on a background thread.
///
/// The thread blocks on stdin, so it must not run on the GTK thread. It sends
/// `ServerExited` once the session ends.
pub(super) fn spawn(sender: relm4::Sender<PinentryAppMsg>) {
    thread::spawn(move || {
        if let Err(e) = run_session(&sender) {
            eprintln!("server loop failed: {e}");
        }

        sender.emit(PinentryAppMsg::ServerExited);
    });
}

/// Serves the session on stdin and stdout, showing each requested dialog on
/// the GTK thread and blocking until it is answered.
fn run_session(sender: &relm4::Sender<PinentryAppMsg>) -> anyhow::Result<()> {
    let mut session = Session::new(io::BufReader::new(io::stdin()), io::stdout());
    session.greet()?;

    while let Step::Dialog(request) = session.next_dialog()? {
        let (dialog, result_rx) = ActiveDialog::new(session.state().clone(), request);
        sender.emit(PinentryAppMsg::ShowDialog(Box::new(dialog)));

        // block until the GTK thread delivers the user's response
        let result = result_rx.recv().unwrap_or(DialogResult::Cancelled);
        session.respond(result)?;
    }
    Ok(())
}
