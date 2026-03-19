use std::{fmt, sync::mpsc, time::Duration};

use gtk4::{gdk, glib, prelude::*};
use gtk4_layer_shell::LayerShell;
use relm4::prelude::*;
use secrecy::SecretString;

use crate::{
    assuan, sound,
    state::{DialogRequest, DialogResult, PinentryState},
};

// duration of the enter and exit CSS opacity transitions
const ANIM_MS: u64 = 220;

const CSS: &str = "
.pinentry-scrim {
    background-color: rgba(0, 0, 0, 0.5);
    transition: opacity 220ms ease-out;
    opacity: 0;
}
.pinentry-scrim.visible {
    opacity: 0.5;
}
.pinentry-card {
    transition: opacity 220ms ease-out;
    opacity: 0;
}
.pinentry-card.visible {
    opacity: 1;
}
";

// -- message types ------------------------------------------------------------

/// Wraps `mpsc::SyncSender` to provide `Debug` without exposing internals.
pub(crate) struct ResultSender(mpsc::SyncSender<DialogResult>);

impl fmt::Debug for ResultSender {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ResultSender").finish_non_exhaustive()
    }
}

#[derive(Debug)]
pub struct ShowDialogContext {
    state: PinentryState,
    request: DialogRequest,
    result_tx: ResultSender,
}

/// Messages handled by the `PinentryApp` component.
#[derive(Debug)]
pub enum PinentryAppMsg {
    /// The Assuan server is requesting a dialog be shown to the user.
    ShowDialog(Box<ShowDialogContext>),
    /// The user has made a choice (submit, confirm, cancel).
    Submit(DialogResult),
    /// The exit animation has finished — it is safe to hide the window.
    ExitAnimationDone,
    /// The Assuan server loop has finished — the app should quit.
    ServerExited,
}

// -- model --------------------------------------------------------------------

struct ActiveDialog {
    state: PinentryState,
    request: DialogRequest,
    result_tx: ResultSender,
}

/// Root Relm4 component for the pinentry application.
///
/// Owns a persistent (but initially hidden) fullscreen Wayland overlay window.
/// When a dialog is needed, the Assuan thread sends `ShowDialog`; the component
/// builds and shows the dialog, the user interacts, and the result is returned
/// to the Assuan thread via the per-dialog channel.
pub struct PinentryApp {
    /// Active dialog being shown, if any.
    active: Option<ActiveDialog>,
    /// Monotonically increasing counter; bumped on each new `ShowDialog` so
    /// `update_view` knows when to rebuild the card content.
    dialog_id: u64,
    /// True while the exit animation is in progress.
    closing: bool,
}

// -- widgets ------------------------------------------------------------------

pub struct PinentryAppWidgets {
    /// The root window (stored so `update_view` can show and hide it).
    window: gtk4::Window,
    /// Full-screen dark overlay — holds the `.pinentry-scrim` CSS class.
    scrim: gtk4::Box,
    /// Centered dialog card — holds the `.pinentry-card` CSS class.
    card: gtk4::Box,
    /// Inner content area inside the card; rebuilt for each new dialog.
    card_content: gtk4::Box,
    /// Tracks which `dialog_id` has been rendered into `card_content`.
    rendered_dialog_id: u64,
}

// -- component implementation -------------------------------------------------

impl SimpleComponent for PinentryApp {
    type Init = ();
    type Input = PinentryAppMsg;
    type Output = ();
    type Root = gtk4::Window;
    type Widgets = PinentryAppWidgets;

    fn init_root() -> Self::Root {
        gtk4::Window::builder()
            .title("pinentry-cadenza")
            .decorated(false)
            .build()
    }

    fn init(
        _init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        // load CSS once — safe to call unconditionally since it is only ever
        // called once by relm4 during init
        relm4::set_global_css(CSS);

        // configure the window as a fullscreen wayland layer-shell overlay
        root.init_layer_shell();
        root.set_layer(gtk4_layer_shell::Layer::Overlay);
        root.set_anchor(gtk4_layer_shell::Edge::Top, true);
        root.set_anchor(gtk4_layer_shell::Edge::Bottom, true);
        root.set_anchor(gtk4_layer_shell::Edge::Left, true);
        root.set_anchor(gtk4_layer_shell::Edge::Right, true);
        root.set_keyboard_mode(gtk4_layer_shell::KeyboardMode::Exclusive);

        // scrim: full-viewport dark overlay
        let scrim = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Vertical)
            .vexpand(true)
            .hexpand(true)
            .build();
        scrim.add_css_class("pinentry-scrim");

        // card: centered dialog panel (max 460 px wide)
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

        // card_content: swappable inner area rebuilt on each ShowDialog
        let card_content = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Vertical)
            .spacing(12)
            .build();
        card.append(&card_content);

        // vertical spacers so the card floats in the middle of the scrim
        let top_space = gtk4::Box::builder().vexpand(true).build();
        let bot_space = gtk4::Box::builder().vexpand(true).build();
        scrim.append(&top_space);
        scrim.append(&card);
        scrim.append(&bot_space);

        root.set_child(Some(&scrim));

        // Escape key cancels the active dialog
        let key_ctrl = gtk4::EventControllerKey::new();
        {
            let sender = sender.clone();
            key_ctrl.connect_key_pressed(move |_, key, _, _| {
                if key == gdk::Key::Escape {
                    sender.input(PinentryAppMsg::Submit(DialogResult::Cancelled));
                    glib::Propagation::Stop
                } else {
                    glib::Propagation::Proceed
                }
            });
        }
        root.add_controller(key_ctrl);

        // spawn the Assuan server on a background thread so it can block on
        // stdin without freezing the GTK event loop
        {
            let thread_sender = sender.input_sender().clone();
            std::thread::spawn(move || {
                let mut state = PinentryState::default();
                let result = assuan::server_loop(&mut state, |state, request| {
                    // create a one-shot channel for this dialog's result
                    let (result_tx, result_rx) = mpsc::sync_channel(1);

                    thread_sender.emit(PinentryAppMsg::ShowDialog(Box::new(ShowDialogContext {
                        state: state.clone(),
                        request,
                        result_tx: ResultSender(result_tx),
                    })));

                    // block until the GTK thread delivers the user's response
                    Ok(result_rx.recv().unwrap_or(DialogResult::Cancelled))
                });

                if let Err(e) = result {
                    eprintln!("server loop failed: {e}");
                }

                thread_sender.emit(PinentryAppMsg::ServerExited);
            });
        }

        let model = PinentryApp {
            active: None,
            dialog_id: 0,
            closing: false,
        };

        let widgets = PinentryAppWidgets {
            window: root,
            scrim,
            card,
            card_content,
            rendered_dialog_id: 0,
        };

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>) {
        match msg {
            PinentryAppMsg::ShowDialog(ctx) => {
                let ShowDialogContext {
                    state,
                    request,
                    result_tx,
                } = *ctx;

                // play the appropriate sound for this dialog
                if state.error.is_some() {
                    sound::play_error_sound();
                } else {
                    sound::play_dialog_sound();
                }

                self.dialog_id += 1;
                self.closing = false;
                self.active = Some(ActiveDialog {
                    state,
                    request,
                    result_tx,
                });
            }

            PinentryAppMsg::Submit(result) => {
                if self.active.is_none() {
                    return;
                }

                // deliver the result to the waiting Assuan thread before
                // starting the exit animation
                if let Some(active) = self.active.take() {
                    let _ = active.result_tx.0.send(result);
                }

                self.closing = true;

                // schedule the post-animation cleanup
                let sender = sender.clone();
                glib::timeout_add_local_once(Duration::from_millis(ANIM_MS + 20), move || {
                    sender.input(PinentryAppMsg::ExitAnimationDone);
                });
            }

            PinentryAppMsg::ExitAnimationDone => {
                // guard against a rapid ShowDialog that reset closing=false
                // before this timeout fired
                if self.closing {
                    self.active = None;
                    self.closing = false;
                }
            }

            PinentryAppMsg::ServerExited => {
                // clean up any orphaned dialog (shouldn't happen in normal
                // flow, but is safe to handle)
                if let Some(active) = self.active.take() {
                    let _ = active.result_tx.0.send(DialogResult::Cancelled);
                }
                self.closing = false;
                relm4::main_application().quit();
            }
        }
    }

    fn update_view(&self, widgets: &mut Self::Widgets, sender: ComponentSender<Self>) {
        // rebuild card content whenever a new dialog has arrived
        if self.dialog_id != widgets.rendered_dialog_id {
            widgets.rendered_dialog_id = self.dialog_id;

            // clear previous content
            while let Some(child) = widgets.card_content.first_child() {
                widgets.card_content.remove(&child);
            }

            if let Some(active) = &self.active {
                build_content(&widgets.card_content, active, &sender);

                // show the window and trigger the enter animation on the next
                // main-loop tick (after the window has mapped and painted)
                widgets.window.present();
                let scrim = widgets.scrim.clone();
                let card = widgets.card.clone();
                glib::idle_add_local_once(move || {
                    scrim.add_css_class("visible");
                    card.add_css_class("visible");
                });
            }
        }

        // start the exit animation by stripping the visible classes
        if self.closing {
            widgets.scrim.remove_css_class("visible");
            widgets.card.remove_css_class("visible");
        }

        // hide the window once the animation is done and no dialog is active
        if self.active.is_none() && !self.closing {
            widgets.window.set_visible(false);
        }
    }
}

// -- content builders ---------------------------------------------------------

/// Populates `card_content` with dialog-type-specific widgets.
fn build_content(
    card_content: &gtk4::Box,
    active: &ActiveDialog,
    sender: &ComponentSender<PinentryApp>,
) {
    match active.request {
        DialogRequest::GetPin => build_getpin(card_content, &active.state, sender),
        DialogRequest::Confirm { one_button: false } => {
            build_confirm(card_content, &active.state, sender)
        }
        DialogRequest::Confirm { one_button: true } | DialogRequest::Message => {
            build_one_button(card_content, &active.state, sender)
        }
    }
}

/// Builds the GETPIN layout: title, description, optional error banner,
/// password entry, and Submit / Cancel buttons.
fn build_getpin(content: &gtk4::Box, state: &PinentryState, sender: &ComponentSender<PinentryApp>) {
    let title = gtk4::Label::builder()
        .label(state.title.as_deref().unwrap_or("Authentication required"))
        .halign(gtk4::Align::Start)
        .wrap(true)
        .build();
    title.add_css_class("title-4");
    content.append(&title);

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
    content.append(&desc);

    if let Some(err) = &state.error {
        content.append(&build_error_banner(err));
    }

    let entry = gtk4::PasswordEntry::builder()
        .placeholder_text(state.prompt.as_deref().unwrap_or("Passphrase"))
        .hexpand(true)
        .build();

    // Enter key inside the entry submits the passphrase
    {
        let sender = sender.clone();
        let entry_ref = entry.clone();
        entry.connect_activate(move |_| {
            let pin = SecretString::from(entry_ref.text().to_string());
            sender.input(PinentryAppMsg::Submit(DialogResult::Pin(pin)));
        });
    }
    content.append(&entry);

    let btn_row = build_btn_row();

    let cancel_btn = gtk4::Button::with_label(state.cancel_label.as_deref().unwrap_or("Cancel"));
    {
        let sender = sender.clone();
        cancel_btn.connect_clicked(move |_| {
            sender.input(PinentryAppMsg::Submit(DialogResult::Cancelled));
        });
    }
    btn_row.append(&cancel_btn);

    let submit_btn = gtk4::Button::with_label(state.ok_label.as_deref().unwrap_or("Submit"));
    submit_btn.add_css_class("suggested-action");
    {
        let sender = sender.clone();
        let entry = entry.clone();
        submit_btn.connect_clicked(move |_| {
            let pin = SecretString::from(entry.text().to_string());
            sender.input(PinentryAppMsg::Submit(DialogResult::Pin(pin)));
        });
    }
    btn_row.append(&submit_btn);

    content.append(&btn_row);

    // auto-focus the entry after the window maps
    glib::idle_add_local_once(move || {
        entry.grab_focus();
    });
}

/// Builds the CONFIRM layout: description, optional error banner, and
/// Cancel / [Not OK] / OK buttons.
fn build_confirm(
    content: &gtk4::Box,
    state: &PinentryState,
    sender: &ComponentSender<PinentryApp>,
) {
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
    content.append(&desc);

    if let Some(err) = &state.error {
        content.append(&build_error_banner(err));
    }

    let btn_row = build_btn_row();

    let cancel_btn = gtk4::Button::with_label(state.cancel_label.as_deref().unwrap_or("Cancel"));
    {
        let sender = sender.clone();
        cancel_btn.connect_clicked(move |_| {
            sender.input(PinentryAppMsg::Submit(DialogResult::Cancelled));
        });
    }
    btn_row.append(&cancel_btn);

    if let Some(notok_label) = &state.notok_label {
        let notok_btn = gtk4::Button::with_label(notok_label.as_str());
        {
            let sender = sender.clone();
            notok_btn.connect_clicked(move |_| {
                sender.input(PinentryAppMsg::Submit(DialogResult::NotConfirmed));
            });
        }
        btn_row.append(&notok_btn);
    }

    let ok_btn = gtk4::Button::with_label(state.ok_label.as_deref().unwrap_or("Confirm"));
    ok_btn.add_css_class("suggested-action");
    {
        let sender = sender.clone();
        ok_btn.connect_clicked(move |_| {
            sender.input(PinentryAppMsg::Submit(DialogResult::Confirmed));
        });
    }
    btn_row.append(&ok_btn);

    content.append(&btn_row);
}

/// Builds the single-button layout used for `CONFIRM --one-button` and
/// `MESSAGE` dialogs.
fn build_one_button(
    content: &gtk4::Box,
    state: &PinentryState,
    sender: &ComponentSender<PinentryApp>,
) {
    let desc = gtk4::Label::builder()
        .label(state.desc.as_deref().unwrap_or(""))
        .halign(gtk4::Align::Start)
        .wrap(true)
        .build();
    content.append(&desc);

    let ok_btn = gtk4::Button::with_label(state.ok_label.as_deref().unwrap_or("OK"));
    ok_btn.add_css_class("suggested-action");
    ok_btn.set_halign(gtk4::Align::End);
    {
        let sender = sender.clone();
        ok_btn.connect_clicked(move |_| {
            sender.input(PinentryAppMsg::Submit(DialogResult::Confirmed));
        });
    }
    content.append(&ok_btn);
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
