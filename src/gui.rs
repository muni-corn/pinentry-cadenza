mod dialog;
pub mod theme;

use anyhow::Result;

use crate::{
    sound,
    state::{DialogRequest, DialogResult, PinentryState},
};

/// Launches a modal pinentry dialog and blocks until the user responds.
///
/// Plays the dialog sound, then configures a fullscreen Wayland overlay with
/// exclusive keyboard grab and runs the iced event loop until the user
/// submits or cancels.
pub fn run_dialog(state: &PinentryState, request: DialogRequest) -> Result<DialogResult> {
    if state.error.is_some() {
        sound::play_error_sound();
    } else {
        sound::play_dialog_sound();
    }

    dialog::run(state, request)
}
