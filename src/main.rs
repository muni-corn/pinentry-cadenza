mod assuan;
mod state;

use state::PinentryState;

fn main() {
    let mut state = PinentryState::default();
    assuan::server_loop(&mut state);
}
