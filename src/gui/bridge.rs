//! Connects the Assuan session thread to the GTK event loop.

use std::thread;

use super::dialog::{ActiveDialog, PinentryAppMsg};
use crate::{
    assuan::{StdioSession, Step},
    state::{DialogRequest, DialogResult},
};

/// An Assuan session that has already received its first dialog request.
pub struct Startup {
    /// The session, positioned just after the first dialog request.
    pub session: StdioSession,
    /// The dialog the client asked for.
    pub first: DialogRequest,
}

/// Continues serving the session on a background thread.
///
/// The thread blocks on stdin, so it must not run on the GTK thread. It sends
/// `ServerExited` once the session ends.
pub(super) fn spawn(sender: relm4::Sender<PinentryAppMsg>, startup: Startup) {
    thread::spawn(move || {
        if let Err(e) = run_session(&sender, startup) {
            eprintln!("server loop failed: {e}");
        }

        sender.emit(PinentryAppMsg::ServerExited);
    });
}

/// Shows each requested dialog on the GTK thread and blocks until it is
/// answered, starting with the dialog in `startup`.
fn run_session(sender: &relm4::Sender<PinentryAppMsg>, startup: Startup) -> anyhow::Result<()> {
    let Startup {
        mut session,
        first: mut request,
    } = startup;

    loop {
        let (dialog, result_rx) = ActiveDialog::new(session.state().clone(), request);
        sender.emit(PinentryAppMsg::ShowDialog(Box::new(dialog)));

        // block until the GTK thread delivers the user's response
        let result = result_rx.recv().unwrap_or(DialogResult::Cancelled);
        session.respond(result)?;

        match session.next_dialog()? {
            Step::Dialog(next) => request = next,
            Step::Closed => return Ok(()),
        }
    }
}
