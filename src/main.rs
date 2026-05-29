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

    // build caller context from env (catches SSH_* vars) then override with
    // any display/ttyname/ttytype passed on argv by gpg-agent — this decision
    // must happen before we emit the assuan greeting so that we can exec() the
    // fallback binary without confusing the already-connected gpg-agent client
    let ctx = CallerContext::from_args_and_env(
        args.display.clone(),
        args.ttyname.clone(),
        args.ttytype.clone(),
    );
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
