mod assuan;
mod cli;
mod fallback;
mod gui;
mod sound;
mod state;

use clap::Parser as _;
use cli::Args;
use fallback::CallerContext;
use gui::PinentryApp;
use relm4::RelmApp;

fn main() {
    // collect raw args before clap consumes them — the fallback path needs to
    // forward them verbatim to pinentry-curses/pinentry-tty
    let raw_args: Vec<String> = std::env::args().skip(1).collect();

    // clap handles --help and --version and exits automatically
    let args = Args::parse();

    let ctx = CallerContext::from_env();
    if fallback::should_use_curses(&ctx) {
        fallback::exec_fallback_pinentry(&raw_args, ctx.ttytype.as_deref());
    }

    // prepend argv[0] as required by g_application_run, then append any
    // unrecognized options (e.g. GTK-specific flags) so GTK4 can handle them
    let program = std::env::args().next().unwrap_or_default();
    let mut gtk_args = vec![program];
    gtk_args.extend(args.gtk_options);

    RelmApp::new("com.musicaloft.pinentry-cadenza")
        .visible_on_activate(false)
        .with_args(gtk_args)
        .run::<PinentryApp>(());
}
