use std::{
    cell::Cell,
    rc::Rc,
    sync::{OnceLock, mpsc},
    time::Duration,
};

use anyhow::Result;
use gtk4::{gdk, glib, prelude::*};
use gtk4_layer_shell::LayerShell;

use crate::state::{DialogRequest, DialogResult, PinentryState};

// duration of the enter and exit CSS opacity transitions
const ANIM_MS: u64 = 220;

// loaded once per process lifetime into the default display's style context
static CSS_LOADED: OnceLock<()> = OnceLock::new();

const CSS: &str = "
.pinentry-scrim {
    background-color: rgba(0, 0, 0, 0.5);
    transition: opacity 220ms ease-out;
    opacity: 0;
}
.pinentry-scrim.visible {
    opacity: 1;
}
.pinentry-card {
    transition: opacity 220ms ease-out;
    opacity: 0;
}
.pinentry-card.visible {
    opacity: 1;
}
";

/// Shared per-dialog state accessed from multiple GTK signal handlers.
struct Ctx {
    tx: mpsc::Sender<DialogResult>,
    main_loop: glib::MainLoop,
    scrim: gtk4::Box,
    card: gtk4::Box,
    /// Guards against double-close (e.g. button click + keyboard shortcut).
    closing: Cell<bool>,
}

impl Ctx {
    /// Sends `result`, starts the exit animation, and schedules main loop quit.
    ///
    /// Ignores subsequent calls so that double-clicks cannot change the result.
    fn close(&self, result: DialogResult) {
        if self.closing.get() {
            return;
        }
        self.closing.set(true);

        let _ = self.tx.send(result);

        // start exit animation by removing the visible class
        self.scrim.remove_css_class("visible");
        self.card.remove_css_class("visible");

        // quit after the transition finishes (slight buffer over ANIM_MS)
        let ml = self.main_loop.clone();
        glib::timeout_add_local_once(Duration::from_millis(ANIM_MS + 20), move || {
            ml.quit();
        });
    }
}

/// Runs the GTK4 layer-shell dialog and blocks until the user responds.
pub fn run(pinentry_state: &PinentryState, request: DialogRequest) -> Result<DialogResult> {
    let (tx, rx) = mpsc::channel::<DialogResult>();

    // load animation CSS once — additive providers on the same display are fine
    CSS_LOADED.get_or_init(|| {
        let provider = gtk4::CssProvider::new();
        provider.load_from_data(CSS);
        if let Some(display) = gdk::Display::default() {
            gtk4::style_context_add_provider_for_display(
                &display,
                &provider,
                gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
            );
        }
    });

    let main_loop = glib::MainLoop::new(None, false);

    // fullscreen scrim (dark overlay that covers the entire output)
    let scrim = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Vertical)
        .vexpand(true)
        .hexpand(true)
        .build();
    scrim.add_css_class("pinentry-scrim");

    // dialog card — centered within the scrim, max 460 px wide
    let card = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Vertical)
        .halign(gtk4::Align::Center)
        .spacing(12)
        .margin_top(24)
        .margin_bottom(24)
        .margin_start(24)
        .margin_end(24)
        .width_request(460)
        .build();
    card.add_css_class("card");
    card.add_css_class("pinentry-card");

    let ctx = Rc::new(Ctx {
        tx,
        main_loop: main_loop.clone(),
        scrim: scrim.clone(),
        card: card.clone(),
        closing: Cell::new(false),
    });

    build_content(&card, pinentry_state, request, Rc::clone(&ctx));

    // vertical spacers to center the card
    let top_space = gtk4::Box::builder().vexpand(true).build();
    let bot_space = gtk4::Box::builder().vexpand(true).build();
    scrim.append(&top_space);
    scrim.append(&card);
    scrim.append(&bot_space);

    let window = gtk4::Window::builder()
        .title("pinentry-cadenza")
        .decorated(false)
        .child(&scrim)
        .build();

    // configure as a Wayland layer-shell surface anchored to all four edges
    window.init_layer_shell();
    window.set_layer(gtk4_layer_shell::Layer::Overlay);
    window.set_anchor(gtk4_layer_shell::Edge::Top, true);
    window.set_anchor(gtk4_layer_shell::Edge::Bottom, true);
    window.set_anchor(gtk4_layer_shell::Edge::Left, true);
    window.set_anchor(gtk4_layer_shell::Edge::Right, true);
    window.set_keyboard_mode(gtk4_layer_shell::KeyboardMode::Exclusive);

    // Escape cancels the dialog
    let key_ctrl = gtk4::EventControllerKey::new();
    {
        let ctx = Rc::clone(&ctx);
        key_ctrl.connect_key_pressed(move |_, key, _, _| {
            if key == gdk::Key::Escape {
                ctx.close(DialogResult::Cancelled);
                glib::Propagation::Stop
            } else {
                glib::Propagation::Proceed
            }
        });
    }
    window.add_controller(key_ctrl);

    window.present();

    // trigger enter animation on the next main-loop iteration (after the
    // window has been mapped and painted at least once)
    {
        let scrim = scrim.clone();
        let card = card.clone();
        glib::idle_add_local_once(move || {
            scrim.add_css_class("visible");
            card.add_css_class("visible");
        });
    }

    main_loop.run();
    window.destroy();

    Ok(rx.try_recv().unwrap_or(DialogResult::Cancelled))
}

// -- content builders ---------------------------------------------------------

fn build_content(card: &gtk4::Box, state: &PinentryState, request: DialogRequest, ctx: Rc<Ctx>) {
    match request {
        DialogRequest::GetPin => build_getpin(card, state, ctx),
        DialogRequest::Confirm { one_button: false } => build_confirm(card, state, ctx),
        DialogRequest::Confirm { one_button: true } | DialogRequest::Message => {
            build_one_button(card, state, ctx);
        }
    }
}

/// Builds the GETPIN layout: title + description + optional error + password
/// entry + Submit/Cancel buttons.
fn build_getpin(card: &gtk4::Box, state: &PinentryState, ctx: Rc<Ctx>) {
    let title = gtk4::Label::builder()
        .label(state.title.as_deref().unwrap_or("Authentication required"))
        .halign(gtk4::Align::Start)
        .wrap(true)
        .build();
    title.add_css_class("title-4");
    card.append(&title);

    let desc = gtk4::Label::builder()
        .label(
            state
                .desc
                .as_deref()
                .unwrap_or("An application is asking for authentication."),
        )
        .halign(gtk4::Align::Start)
        .wrap(true)
        .build();
    card.append(&desc);

    if let Some(err) = &state.error {
        card.append(&build_error_banner(err));
    }

    let entry = gtk4::PasswordEntry::builder()
        .placeholder_text(state.prompt.as_deref().unwrap_or("Passphrase"))
        .hexpand(true)
        .build();

    // Enter key inside the entry submits the passphrase
    {
        let ctx = Rc::clone(&ctx);
        let entry_ref = entry.clone();
        entry.connect_activate(move |_| {
            let passphrase = entry_ref.text().to_string();
            ctx.close(DialogResult::Pin(secrecy::SecretString::from(passphrase)));
        });
    }
    card.append(&entry);

    let btn_row = build_btn_row();

    let cancel_btn = gtk4::Button::with_label(state.cancel_label.as_deref().unwrap_or("Cancel"));
    {
        let ctx = Rc::clone(&ctx);
        cancel_btn.connect_clicked(move |_| ctx.close(DialogResult::Cancelled));
    }
    btn_row.append(&cancel_btn);

    let submit_btn = gtk4::Button::with_label(state.ok_label.as_deref().unwrap_or("Submit"));
    submit_btn.add_css_class("suggested-action");
    {
        let ctx = Rc::clone(&ctx);
        let entry_ref = entry.clone();
        submit_btn.connect_clicked(move |_| {
            let passphrase = entry_ref.text().to_string();
            ctx.close(DialogResult::Pin(secrecy::SecretString::from(passphrase)));
        });
    }
    btn_row.append(&submit_btn);

    card.append(&btn_row);

    // auto-focus the entry after the window maps
    glib::idle_add_local_once(move || {
        entry.grab_focus();
    });
}

/// Builds the CONFIRM layout: description + optional error + OK / [Not OK] /
/// Cancel buttons.
fn build_confirm(card: &gtk4::Box, state: &PinentryState, ctx: Rc<Ctx>) {
    let desc = gtk4::Label::builder()
        .label(
            state
                .desc
                .as_deref()
                .unwrap_or("An application is asking for confirmation."),
        )
        .halign(gtk4::Align::Start)
        .wrap(true)
        .build();
    card.append(&desc);

    if let Some(err) = &state.error {
        card.append(&build_error_banner(err));
    }

    let btn_row = build_btn_row();

    let cancel_btn = gtk4::Button::with_label(state.cancel_label.as_deref().unwrap_or("Cancel"));
    {
        let ctx = Rc::clone(&ctx);
        cancel_btn.connect_clicked(move |_| ctx.close(DialogResult::Cancelled));
    }
    btn_row.append(&cancel_btn);

    if let Some(notok_label) = &state.notok_label {
        let notok_btn = gtk4::Button::with_label(notok_label.as_str());
        {
            let ctx = Rc::clone(&ctx);
            notok_btn.connect_clicked(move |_| ctx.close(DialogResult::NotConfirmed));
        }
        btn_row.append(&notok_btn);
    }

    let ok_btn = gtk4::Button::with_label(state.ok_label.as_deref().unwrap_or("Confirm"));
    ok_btn.add_css_class("suggested-action");
    {
        let ctx = Rc::clone(&ctx);
        ok_btn.connect_clicked(move |_| ctx.close(DialogResult::Confirmed));
    }
    btn_row.append(&ok_btn);

    card.append(&btn_row);
}

/// Builds the one-button layout used for `CONFIRM --one-button` and `MESSAGE`.
fn build_one_button(card: &gtk4::Box, state: &PinentryState, ctx: Rc<Ctx>) {
    let desc = gtk4::Label::builder()
        .label(state.desc.as_deref().unwrap_or(""))
        .halign(gtk4::Align::Start)
        .wrap(true)
        .build();
    card.append(&desc);

    let ok_btn = gtk4::Button::with_label(state.ok_label.as_deref().unwrap_or("OK"));
    ok_btn.add_css_class("suggested-action");
    ok_btn.set_halign(gtk4::Align::End);
    {
        let ctx = Rc::clone(&ctx);
        ok_btn.connect_clicked(move |_| ctx.close(DialogResult::Confirmed));
    }
    card.append(&ok_btn);
}

// -- shared helpers -----------------------------------------------------------

/// Creates a right-aligned horizontal button row.
fn build_btn_row() -> gtk4::Box {
    gtk4::Box::builder()
        .orientation(gtk4::Orientation::Horizontal)
        .spacing(8)
        .halign(gtk4::Align::End)
        .build()
}

/// Builds a compact error banner with the `error` CSS class.
fn build_error_banner(error: &str) -> gtk4::Box {
    let banner = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Horizontal)
        .margin_top(4)
        .margin_bottom(4)
        .build();
    banner.add_css_class("error");

    let label = gtk4::Label::builder()
        .label(error)
        .halign(gtk4::Align::Start)
        .wrap(true)
        .hexpand(true)
        .build();
    banner.append(&label);
    banner
}
