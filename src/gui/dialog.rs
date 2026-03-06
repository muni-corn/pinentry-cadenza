use std::sync::mpsc;

use iced::{
    Background, Border, Color, Element, Event, Length, Task, event,
    widget::{button, column, container, row, text, text_input},
};
use iced_layershell::{
    reexport::{Anchor, KeyboardInteractivity, Layer},
    settings::{LayerShellSettings, StartMode},
    to_layer_message,
};

use crate::state::{DialogRequest, DialogResult, PinentryState};

// widget ID used to autofocus the passphrase input on startup
const INPUT_ID: iced::widget::Id = iced::widget::Id::new("passphrase-input");

/// Internal iced application state for a single dialog invocation.
struct AppState {
    tx: mpsc::Sender<DialogResult>,
    request: DialogRequest,
    // dialog text (snapshotted from PinentryState)
    title: Option<String>,
    desc: Option<String>,
    prompt: Option<String>,
    error_text: Option<String>,
    ok_label: Option<String>,
    cancel_label: Option<String>,
    notok_label: Option<String>,
    // passphrase input buffer (GetPin only)
    input: String,
}

/// Messages handled by the dialog application.
///
/// The `#[to_layer_message]` attribute injects additional layer-shell-specific
/// variants (e.g. `AnchorChange`, `SizeChange`) used internally by the runtime.
#[to_layer_message]
#[derive(Debug, Clone)]
enum Message {
    Submit,
    NotConfirmed,
    Cancel,
    InputChanged(String),
    EscapePressed,
    TabPressed,
}

/// Runs the iced layer shell dialog, blocking until the user responds.
pub fn run(pinentry_state: &PinentryState, request: DialogRequest) -> DialogResult {
    let (tx, rx) = mpsc::channel::<DialogResult>();

    let title = pinentry_state.title.clone();
    let desc = pinentry_state.desc.clone();
    let prompt = pinentry_state.prompt.clone();
    let error_text = pinentry_state.error.clone();
    let ok_label = pinentry_state.ok_label.clone();
    let cancel_label = pinentry_state.cancel_label.clone();
    let notok_label = pinentry_state.notok_label.clone();

    let _ = iced_layershell::application(
        // boot must be Fn (not FnOnce) — clone captured values on each call
        move || {
            let state = AppState {
                tx: tx.clone(),
                request,
                title: title.clone(),
                desc: desc.clone(),
                prompt: prompt.clone(),
                error_text: error_text.clone(),
                ok_label: ok_label.clone(),
                cancel_label: cancel_label.clone(),
                notok_label: notok_label.clone(),
                input: String::new(),
            };
            // autofocus the passphrase input; no-op for non-GetPin dialogs
            let focus: Task<Message> = match request {
                DialogRequest::GetPin => iced::widget::operation::focus(INPUT_ID.clone()),
                _ => Task::none(),
            };
            (state, focus)
        },
        || String::from("pinentry-cadenza"),
        update,
        view,
    )
    .style(style)
    .subscription(subscription)
    .layer_settings(LayerShellSettings {
        anchor: Anchor::Top | Anchor::Bottom | Anchor::Left | Anchor::Right,
        layer: Layer::Overlay,
        exclusive_zone: -1,
        keyboard_interactivity: KeyboardInteractivity::Exclusive,
        start_mode: StartMode::Active,
        ..Default::default()
    })
    .run();

    rx.try_recv().unwrap_or(DialogResult::Cancelled)
}

fn update(state: &mut AppState, message: Message) -> Task<Message> {
    match message {
        Message::Submit => {
            let result = match state.request {
                DialogRequest::GetPin => DialogResult::Pin(secrecy::SecretString::from(
                    std::mem::take(&mut state.input),
                )),
                DialogRequest::Confirm { .. } | DialogRequest::Message => DialogResult::Confirmed,
            };
            let _ = state.tx.send(result);
            iced::exit()
        }
        Message::NotConfirmed => {
            let _ = state.tx.send(DialogResult::NotConfirmed);
            iced::exit()
        }
        Message::Cancel | Message::EscapePressed => {
            let _ = state.tx.send(DialogResult::Cancelled);
            iced::exit()
        }
        Message::InputChanged(s) => {
            state.input = s;
            Task::none()
        }
        Message::TabPressed => iced::widget::operation::focus_next(),
        // layer shell runtime messages — handled transparently by the framework
        _ => Task::none(),
    }
}

fn view(state: &AppState) -> Element<'_, Message> {
    let inner = match state.request {
        DialogRequest::GetPin => getpin_content(state),
        DialogRequest::Confirm { one_button: false } => confirm_content(state),
        DialogRequest::Confirm { one_button: true } | DialogRequest::Message => {
            one_button_content(state)
        }
    };
    make_card(inner)
}

// -- dialog layout builders --

/// Layout for `GETPIN`: title + description + error + prompt + input +
/// Submit/Cancel.
fn getpin_content(state: &AppState) -> Element<'_, Message> {
    let title = text(state.title.as_deref().unwrap_or("Authentication required")).size(16);
    let desc = text(
        state
            .desc
            .as_deref()
            .unwrap_or("An application is asking for authentication."),
    );
    let prompt = text(state.prompt.as_deref().unwrap_or("Passphrase:"));
    let input = text_input("", &state.input)
        .id(INPUT_ID.clone())
        .on_input(Message::InputChanged)
        .on_submit(Message::Submit)
        .secure(true)
        .width(Length::Fill);
    let buttons = button_row(vec![
        ("Submit", state.ok_label.as_deref(), Message::Submit),
        ("Cancel", state.cancel_label.as_deref(), Message::Cancel),
    ]);

    let mut content = column![title, desc].spacing(8);
    if let Some(banner) = error_banner(&state.error_text) {
        content = content.push(banner);
    }
    content
        .push(prompt)
        .push(input)
        .push(buttons)
        .spacing(8)
        .padding(24)
        .into()
}

/// Layout for `CONFIRM`: description + error + Okay / [Not okay] / Cancel.
///
/// Shows a three-button row when `SETNOTOK` was called, two-button otherwise.
fn confirm_content(state: &AppState) -> Element<'_, Message> {
    let desc = text(
        state
            .desc
            .as_deref()
            .unwrap_or("An application is asking for confirmation."),
    );

    let mut btn_specs: Vec<(&str, Option<&str>, Message)> =
        vec![("Confirm", state.ok_label.as_deref(), Message::Submit)];
    if state.notok_label.is_some() {
        btn_specs.push((
            "Refuse",
            state.notok_label.as_deref(),
            Message::NotConfirmed,
        ));
    }
    btn_specs.push(("Cancel", state.cancel_label.as_deref(), Message::Cancel));

    let buttons = button_row(btn_specs);

    let mut content = column![desc].spacing(8);
    if let Some(banner) = error_banner(&state.error_text) {
        content = content.push(banner);
    }
    content.push(buttons).spacing(8).padding(24).into()
}

/// Layout for `CONFIRM --one-button` and `MESSAGE`: description + single OK
/// button.
fn one_button_content(state: &AppState) -> Element<'_, Message> {
    let desc = text(state.desc.as_deref().unwrap_or(""));
    let buttons = button_row(vec![("OK", state.ok_label.as_deref(), Message::Submit)]);

    column![desc, buttons].spacing(8).padding(24).into()
}

// -- shared helpers --

/// Wraps `content` in a centered, max-480-px rounded card on the scrim.
fn make_card(content: Element<'_, Message>) -> Element<'_, Message> {
    let card = container(content)
        .style(container::rounded_box)
        .max_width(480);

    iced::widget::center(card)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

/// Renders a red-tinted error banner, or `None` if no error is set.
fn error_banner(error_text: &Option<String>) -> Option<impl Into<Element<'_, Message>>> {
    error_text.as_ref().map(|err| {
        container(text(err).color(Color::from_rgb(1.0, 0.35, 0.35)))
            .padding([6, 10])
            .width(Length::Fill)
            .style(|_theme| container::Style {
                background: Some(Background::Color(Color::from_rgba(1.0, 0.0, 0.0, 0.15))),
                border: Border::default().rounded(4),
                ..Default::default()
            })
    })
}

/// Builds a right-aligned button row from `(fallback_label, override_label,
/// message)` triples.
fn button_row<'a>(specs: Vec<(&'a str, Option<&'a str>, Message)>) -> Element<'a, Message> {
    let mut r = row![iced::widget::space().width(Length::Fill)].spacing(8);
    for (fallback, override_label, msg) in specs {
        let label = override_label.unwrap_or(fallback);
        r = r.push(button(label).on_press(msg));
    }
    r.width(Length::Fill).into()
}

/// Keyboard listener: maps Tab and Escape to explicit messages.
///
/// Must be a free function (not a closure) because `event::listen_with`
/// requires a function pointer.
fn on_key_event(
    event: iced::Event,
    _status: event::Status,
    _window: iced::window::Id,
) -> Option<Message> {
    use iced::keyboard::{Event as KeyEvent, Key, key::Named};
    match event {
        Event::Keyboard(KeyEvent::KeyPressed {
            key: Key::Named(Named::Escape),
            ..
        }) => Some(Message::EscapePressed),
        Event::Keyboard(KeyEvent::KeyPressed {
            key: Key::Named(Named::Tab),
            ..
        }) => Some(Message::TabPressed),
        _ => None,
    }
}

fn subscription(_state: &AppState) -> iced::Subscription<Message> {
    event::listen_with(on_key_event)
}

fn style(_state: &AppState, _theme: &iced::Theme) -> iced::theme::Style {
    iced::theme::Style {
        background_color: Color::from_rgba(0.0, 0.0, 0.0, 0.65),
        text_color: Color::WHITE,
    }
}
