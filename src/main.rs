mod assuan;
mod cli;
mod fallback;
mod gui;
mod sound;
mod state;

use gui::PinentryApp;
use relm4::RelmApp;

fn main() {
    // skip argv[0] (the program name itself)
    let args: Vec<String> = std::env::args().skip(1).collect();

    // handle --version and --help before any initialization
    if args.iter().any(|a| a == "--version" || a == "-V") {
        println!("pinentry-cadenza {}", env!("CARGO_PKG_VERSION"));
        return;
    }
    if args.iter().any(|a| a == "--help" || a == "-h") {
        print_help();
        return;
    }

    if fallback::should_use_curses() {
        // forward the original args verbatim — curses/tty understands them
        fallback::exec_pinentry_curses(&args);
    }

    // strip pinentry-specific options so GTK4 does not see unknown flags
    // (e.g. --display, --ttyname) and refuse to start
    let gtk_args = cli::filter_gtk_args(&args);

    RelmApp::new("com.musicaloft.pinentry-cadenza")
        .visible_on_activate(false)
        .with_args(gtk_args)
        .run::<PinentryApp>(());
}

fn print_help() {
    println!("Usage: pinentry-cadenza [OPTION...]");
    println!("Wayland pinentry dialog for GnuPG.");
    println!();
    println!("Options:");
    println!("  -V, --version                        print version and exit");
    println!("  -h, --help                           print this help and exit");
    println!("  -d, --display DISPLAY                set the X display");
    println!("  -T, --ttyname TTY                    set the terminal device");
    println!("  -t, --ttytype TYPE                   set the terminal type");
    println!("  -C, --lc-ctype STRING                set LC_CTYPE locale");
    println!("  -M, --lc-messages STRING             set LC_MESSAGES locale");
    println!("  -W, --parent-wid WID                 set the parent window ID");
    println!("      --touch-file FILENAME            touch file on user activity");
    println!("      --timeout SECONDS                set dialog timeout");
    println!("  -g, --no-global-grab                 do not grab keyboard/mouse");
    println!("      --allow-external-password-cache  allow external password cache");
    println!("      --debug                          enable debug output");
    println!("      --purge-keys                     purge outdated cached keys");
}
