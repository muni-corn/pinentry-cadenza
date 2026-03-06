use std::sync::mpsc;

use iced::{
    Color, Element, Event, Task,
    widget::{button, column, container, row, text},
};
use iced_layershell::{
    reexport::{Anchor, KeyboardInteractivity, Layer},
    settings::{LayerShellSettings, StartMode},
    to_layer_message,
};

use crate::state::{DialogRequest, DialogResult, PinentryState};

/// Internal iced application state for a single dialog invocation.
#[allow(dead_code)]
struct AppState {
    tx: mpsc::Sender<DialogResult>,
    request: DialogRequest,
    desc: Option<String>,
    ok_label: Option<String>,
    cancel_label: Option<String>,
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
    Cancel,
    InputChanged(String),
    IcedEvent(Event),
}

/// Runs the iced layer shell dialog, blocking until the user responds.
pub fn run(pinentry_state: &PinentryState, request: DialogRequest) -> DialogResult {
    let (tx, rx) = mpsc::channel::<DialogResult>();

    let desc = pinentry_state.desc.clone();
    let ok_label = pinentry_state.ok_label.clone();
    let cancel_label = pinentry_state.cancel_label.clone();

    let _ = iced_layershell::application(
        // boot must be Fn (not FnOnce) — clone captured values on each call
        move || AppState {
            tx: tx.clone(),
            request,
            desc: desc.clone(),
            ok_label: ok_label.clone(),
            cancel_label: cancel_label.clone(),
            input: String::new(),
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
                DialogRequest::Confirm { one_button: _ } | DialogRequest::Message => {
                    DialogResult::Confirmed
                }
            };
            let _ = state.tx.send(result);
            iced::exit()
        }
        Message::Cancel => {
            let _ = state.tx.send(DialogResult::Cancelled);
            iced::exit()
        }
        Message::InputChanged(s) => {
            state.input = s;
            Task::none()
        }
        Message::IcedEvent(Event::Keyboard(iced::keyboard::Event::KeyPressed {
            key: iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape),
            ..
        })) => {
            let _ = state.tx.send(DialogResult::Cancelled);
            iced::exit()
        }
        Message::IcedEvent(_) => Task::none(),
        // layer shell runtime messages — handled transparently by the framework
        _ => Task::none(),
    }
}

fn view(state: &AppState) -> Element<'_, Message> {
    use iced::Length;

    let card = container(
        column![
            text("Passphrase required"),
            row![
                button(state.ok_label.as_deref().unwrap_or("Submit")).on_press(Message::Submit),
                button(state.cancel_label.as_deref().unwrap_or("Cancel")).on_press(Message::Cancel),
            ]
            .spacing(8),
        ]
        .spacing(12)
        .padding(24),
    )
    .style(container::rounded_box);

    iced::widget::center(card)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

fn subscription(_state: &AppState) -> iced::Subscription<Message> {
    iced::event::listen().map(Message::IcedEvent)
}

fn style(_state: &AppState, _theme: &iced::Theme) -> iced::theme::Style {
    iced::theme::Style {
        background_color: Color::from_rgba(0.0, 0.0, 0.0, 0.65),
        text_color: Color::WHITE,
    }
}
