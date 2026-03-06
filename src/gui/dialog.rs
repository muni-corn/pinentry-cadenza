use std::sync::mpsc;

use iced::{
    Color, Element, Event, Length, Task, event,
    widget::{button, column, container, row, text, text_input},
};
use iced_layershell::{
    reexport::{Anchor, KeyboardInteractivity, Layer},
    settings::{LayerShellSettings, StartMode},
    to_layer_message,
};

use super::theme;
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
    // tracks whether the text input has been focused yet
    input_focused: bool,
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
    FocusInput,
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
        move || AppState {
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
            input_focused: false,
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
        Message::FocusInput => {
            state.input_focused = true;
            iced::widget::operation::focus(INPUT_ID.clone())
        }
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
    let title = text(state.title.as_deref().unwrap_or("Authentication required"))
        .size(17)
        .color(theme::TEXT);
    let desc = text(
        state
            .desc
            .as_deref()
            .unwrap_or("An application is asking for authentication."),
    )
    .color(theme::TEXT);
    let input = text_input(state.prompt.as_deref().unwrap_or(""), &state.input)
        .id(INPUT_ID.clone())
        .on_input(Message::InputChanged)
        .on_submit(Message::Submit)
        .secure(true)
        .width(Length::Fill)
        .style(theme::text_input_style);
    let buttons = button_row(vec![
        ButtonSpec {
            default_prompt: "Cancel",
            prompt_override: state.cancel_label.as_deref(),
            message: Message::Cancel,
        },
        ButtonSpec {
            default_prompt: "Submit",
            prompt_override: state.ok_label.as_deref(),
            message: Message::Submit,
        },
    ]);

    let mut content = column![title, desc].spacing(12);
    if let Some(banner) = error_banner(&state.error_text) {
        content = content.push(banner);
    }
    content
        .push(input)
        .push(buttons)
        .spacing(12)
        .padding(28)
        .into()
}

/// Layout for `CONFIRM`: description + error + OK / [Not OK] / Cancel.
///
/// Shows a three-button row when `SETNOTOK` was called, two-button otherwise.
fn confirm_content(state: &AppState) -> Element<'_, Message> {
    let desc = text(
        state
            .desc
            .as_deref()
            .unwrap_or("An application is asking for confirmation."),
    )
    .color(theme::TEXT);

    let mut btn_specs = vec![ButtonSpec {
        default_prompt: "Cancel",
        prompt_override: state.cancel_label.as_deref(),
        message: Message::Cancel,
    }];

    if state.notok_label.is_some() {
        btn_specs.push(ButtonSpec {
            default_prompt: "Refuse",
            prompt_override: state.notok_label.as_deref(),
            message: Message::NotConfirmed,
        });
    }
    btn_specs.push(ButtonSpec {
        default_prompt: "Confirm",
        prompt_override: state.ok_label.as_deref(),
        message: Message::Submit,
    });

    let buttons = button_row(btn_specs);

    let mut content = column![desc].spacing(12);
    if let Some(banner) = error_banner(&state.error_text) {
        content = content.push(banner);
    }
    content.push(buttons).spacing(12).padding(28).into()
}

/// Layout for `CONFIRM --one-button` and `MESSAGE`: description + single OK.
fn one_button_content(state: &AppState) -> Element<'_, Message> {
    let desc = text(state.desc.as_deref().unwrap_or("")).color(theme::TEXT);
    let buttons = button_row(vec![ButtonSpec {
        default_prompt: "OK",
        prompt_override: state.ok_label.as_deref(),
        message: Message::Submit,
    }]);
    column![desc, buttons].spacing(12).padding(28).into()
}

// -- shared helpers --

/// Wraps `content` in a centered, max-480-px themed card on the scrim.
fn make_card(content: Element<'_, Message>) -> Element<'_, Message> {
    let card = container(content).style(theme::card).max_width(480);

    iced::widget::center(card)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

/// Renders a themed error banner, or `None` if no error is set.
fn error_banner(error_text: &Option<String>) -> Option<impl Into<Element<'_, Message>>> {
    error_text.as_ref().map(|err| {
        container(text(err).color(theme::ERROR))
            .padding([6, 10])
            .width(Length::Fill)
            .style(theme::error_banner)
    })
}

struct ButtonSpec<'a> {
    default_prompt: &'a str,
    prompt_override: Option<&'a str>,
    message: Message,
}

/// Builds a right-aligned button row.
///
/// The first entry receives the primary style; all others are styled as
/// secondary buttons.
fn button_row<'a>(specs: Vec<ButtonSpec<'a>>) -> Element<'a, Message> {
    let specs_count = specs.len();
    let mut iter = specs.into_iter().enumerate();
    let space = iced::widget::space().width(Length::Fill);
    let mut r = if let Some((
        _,
        ButtonSpec {
            default_prompt,
            prompt_override,
            message,
        },
    )) = iter.next()
    {
        let label = prompt_override.unwrap_or(default_prompt);
        let btn = button(label)
            .on_press(message)
            .style(theme::secondary_button);
        row![btn, space]
    } else {
        row![space]
    }
    .spacing(8);

    for (
        i,
        ButtonSpec {
            default_prompt,
            prompt_override,
            message,
        },
    ) in iter
    {
        let label = prompt_override.unwrap_or(default_prompt);
        let btn = if i < specs_count - 1 {
            button(label)
                .on_press(message)
                .style(theme::secondary_button)
        } else {
            button(label).on_press(message).style(theme::primary_button)
        };
        r = r.push(btn);
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

fn subscription(state: &AppState) -> iced::Subscription<Message> {
    use std::time::Duration;

    let key_sub = event::listen_with(on_key_event);

    // for GetPin, schedule a one-shot focus 750 ms after the window opens;
    // once input_focused flips true the timer subscription is dropped
    if let DialogRequest::GetPin = state.request
        && !state.input_focused
    {
        iced::Subscription::batch([
            key_sub,
            iced::time::every(Duration::from_millis(750)).map(|_| Message::FocusInput),
        ])
    } else {
        key_sub
    }
}

fn style(_state: &AppState, _theme: &iced::Theme) -> iced::theme::Style {
    iced::theme::Style {
        background_color: Color::from_rgba(0.0, 0.0, 0.0, 0.65),
        text_color: theme::TEXT,
    }
}
