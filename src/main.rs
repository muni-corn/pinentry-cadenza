mod assuan;
mod fallback;
mod gui;
mod sound;
mod state;

use state::PinentryState;

fn main() {
    // skip argv[0] (the program name itself)
    let args: Vec<String> = std::env::args().skip(1).collect();

    if fallback::should_use_curses() {
        fallback::exec_pinentry_curses(&args);
    }

    let mut state = PinentryState::default();
    if let Err(e) = assuan::server_loop(&mut state) {
        eprintln!("server loop failed: {e}")
    };
}
