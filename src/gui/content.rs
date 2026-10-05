//! Dialog content builders: the widgets inside the card for each request type.

use gtk4::{glib, prelude::*};
use relm4::prelude::*;
use secrecy::SecretString;

use super::dialog::{PinentryApp, PinentryAppMsg};
use crate::state::{DialogRequest, DialogResult, PinentryState};

/// Populates `card_content` with dialog-type-specific widgets.
pub(super) fn build_content(
    card_content: &gtk4::Box,
    request: DialogRequest,
    state: &PinentryState,
    sender: &ComponentSender<PinentryApp>,
) {
    match request {
        DialogRequest::GetPin => build_getpin(card_content, state, sender),
        DialogRequest::Confirm { one_button: false } => build_confirm(card_content, state, sender),
        DialogRequest::Confirm { one_button: true } | DialogRequest::Message => {
            build_one_button(card_content, state, sender)
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
