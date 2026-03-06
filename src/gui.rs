mod dialog;
pub mod theme;

use crate::state::{DialogRequest, DialogResult, PinentryState};

/// Launches a modal pinentry dialog and blocks until the user responds.
///
/// Configures a fullscreen Wayland overlay with exclusive keyboard grab, then
/// runs the iced event loop until the user submits or cancels.
pub fn run_dialog(state: &PinentryState, request: DialogRequest) -> DialogResult {
    dialog::run(state, request)
}
