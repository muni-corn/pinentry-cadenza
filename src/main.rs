mod assuan;
mod fallback;
mod gui;
mod sound;
mod state;

use gui::PinentryApp;
use relm4::RelmApp;

fn main() {
    // skip argv[0] (the program name itself)
    let args: Vec<String> = std::env::args().skip(1).collect();

    if fallback::should_use_curses() {
        fallback::exec_pinentry_curses(&args);
    }

    RelmApp::new("com.musicaloft.pinentry-cadenza")
        .visible_on_activate(false)
        .run::<PinentryApp>(());
}
