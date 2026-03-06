use std::{sync::mpsc, time::Duration};

use iced::{
    Animation, Color, Element, Event, Length, Padding, Task,
    animation::Easing,
    event,
    time::Instant,
    widget::{Row, button, column, container, text, text_input},
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

// base sizes at scale 1.0
const TITLE_SIZE: f32 = 17.0;
const DESC_SIZE: f32 = 15.0;
const INPUT_SIZE: f32 = 15.0;
const BTN_SIZE: f32 = 14.0;
const BTN_PADDING: (f32, f32) = (12.0, 24.0);
const CARD_MAX_WIDTH: f32 = 480.0;
const CARD_PADDING: f32 = 28.0;
const SPACING: f32 = 12.0;

// The scale for UI elements when the dialog is "away"; i.e. about to enter or just exited.
const SCALE_GONE: f32 = 0.75;

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

    /// Passphrase input buffer (GetPin only)
    input: String,

    /// Tracks whether the text input has been focused yet
    input_focused: bool,

    /// Enter/exit animation: false = hidden/small, true = visible/full
    anim: Animation<bool>,

    /// True until the entry animation has fully settled
    is_opening: bool,

    /// True until the exit animation has fully settled
    is_closing: bool,
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

    /// Fired on every display frame while an animation is in progress.
    Tick(Instant),
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
            let mut anim = Animation::new(false)
                .duration(Duration::from_millis(250))
                .easing(Easing::EaseOutCubic);

            // begin the enter animation immediately
            anim.go_mut(true, Instant::now() + Duration::from_millis(200));

            AppState {
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
                anim,
                is_opening: true,
                is_closing: false,
            }
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
            begin_exit(state, result)
        }
        Message::NotConfirmed => begin_exit(state, DialogResult::NotConfirmed),
        Message::Cancel | Message::EscapePressed => begin_exit(state, DialogResult::Cancelled),
        Message::InputChanged(s) => {
            state.input = s;
            Task::none()
        }
        Message::TabPressed => iced::widget::operation::focus_next(),
        Message::FocusInput => {
            state.input_focused = true;
            iced::widget::operation::focus(INPUT_ID.clone())
        }
        Message::Tick(now) => {
            if !state.anim.is_animating(now) {
                // clear the entry flag once the enter animation settles
                if state.is_opening {
                    state.is_opening = false;
                }
                // once the exit animation settles, deliver the result and quit
                if state.is_closing {
                    return iced::exit();
                }
            }
            Task::none()
        }
        // layer shell runtime messages — handled transparently by the framework
        _ => Task::none(),
    }
}

/// Starts the exit animation and stores `result` for delivery when it finishes.
///
/// Ignores subsequent calls so that double-clicks cannot change the stored
/// result.
fn begin_exit(state: &mut AppState, result: DialogResult) -> Task<Message> {
    let _ = state.tx.send(result);
    if !state.is_closing {
        state.anim.go_mut(false, Instant::now());
        state.is_closing = true;
    }
    Task::none()
}

fn view(state: &AppState) -> Element<'_, Message> {
    let s = state.anim.interpolate(SCALE_GONE, 1.0, Instant::now());
    let alpha = state.anim.interpolate(0.0_f32, 1.0_f32, Instant::now());
    let inner = match state.request {
        DialogRequest::GetPin => getpin_content(state, s, alpha),
        DialogRequest::Confirm { one_button: false } => confirm_content(state, s, alpha),
        DialogRequest::Confirm { one_button: true } | DialogRequest::Message => {
            one_button_content(state, s, alpha)
        }
    };
    make_card(s, alpha, inner)
}

// -- dialog layout builders --

/// Layout for `GETPIN`: title + description + error + prompt + input +
/// Submit/Cancel.
fn getpin_content(state: &AppState, s: f32, alpha: f32) -> Element<'_, Message> {
    let title = text(state.title.as_deref().unwrap_or("Authentication required"))
        .size(TITLE_SIZE * s)
        .color(theme::TEXT.scale_alpha(alpha));

    let desc = text(
        state
            .desc
            .as_deref()
            .unwrap_or("An application is asking for authentication."),
    )
    .size(DESC_SIZE * s)
    .color(theme::TEXT.scale_alpha(alpha));

    let input = text_input(state.prompt.as_deref().unwrap_or(""), &state.input)
        .id(INPUT_ID.clone())
        .on_input(Message::InputChanged)
        .on_submit(Message::Submit)
        .secure(true)
        .size(INPUT_SIZE * s)
        .width(Length::Fill)
        .style(move |_, status| theme::text_input_style(status, s, alpha))
        .padding(button_padding(s));

    let buttons = button_row(
        vec![
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
        ],
        s,
        alpha,
    );

    let mut content = column![title, desc].spacing(SPACING * s);
    if let Some(banner) = error_banner(&state.error_text, s) {
        content = content.push(banner);
    }
    content
        .push(input)
        .push(buttons)
        .spacing(SPACING * s)
        .padding(CARD_PADDING * s)
        .into()
}

/// Layout for `CONFIRM`: description + error + OK / [Not OK] / Cancel.
///
/// Shows a three-button row when `SETNOTOK` was called, two-button otherwise.
fn confirm_content(state: &AppState, s: f32, alpha: f32) -> Element<'_, Message> {
    let desc = text(
        state
            .desc
            .as_deref()
            .unwrap_or("An application is asking for confirmation."),
    )
    .size(DESC_SIZE * s)
    .color(theme::TEXT.scale_alpha(alpha));

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

    let buttons = button_row(btn_specs, s, alpha);

    let mut content = column![desc].spacing(SPACING * s);
    if let Some(banner) = error_banner(&state.error_text, s) {
        content = content.push(banner);
    }
    content
        .push(buttons)
        .spacing(SPACING * s)
        .padding(CARD_PADDING * s)
        .into()
}

/// Layout for `CONFIRM --one-button` and `MESSAGE`: description + single OK.
fn one_button_content(state: &AppState, s: f32, alpha: f32) -> Element<'_, Message> {
    let desc = text(state.desc.as_deref().unwrap_or("")).color(theme::TEXT);
    let buttons = button_row(
        vec![ButtonSpec {
            default_prompt: "OK",
            prompt_override: state.ok_label.as_deref(),
            message: Message::Submit,
        }],
        s,
        alpha,
    );
    column![desc, buttons]
        .spacing(SPACING * s)
        .padding(CARD_PADDING * s)
        .into()
}

// -- shared helpers --

/// Wraps `content` in a centered, scaled card on the scrim.
///
/// `max_width` scales with `s` so the dialog grows from a smaller footprint
/// during the enter animation.
fn make_card(s: f32, alpha: f32, content: Element<'_, Message>) -> Element<'_, Message> {
    let card = container(content)
        .style(move |_| theme::card(s, alpha))
        .max_width(CARD_MAX_WIDTH * s);

    iced::widget::center(card)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

/// Renders a themed error banner, or `None` if no error is set.
fn error_banner(error_text: &Option<String>, s: f32) -> Option<impl Into<Element<'_, Message>>> {
    error_text.as_ref().map(|err| {
        container(text(err).size(DESC_SIZE * s).color(theme::ERROR))
            .padding([6.0 * s, 10.0 * s])
            .width(Length::Fill)
            .style(theme::error_banner)
    })
}

struct ButtonSpec<'a> {
    default_prompt: &'a str,
    prompt_override: Option<&'a str>,
    message: Message,
}

/// Builds a right-aligned button row with scaled text.
///
/// The last entry receives the primary style; all others are secondary.
fn button_row<'a>(specs: Vec<ButtonSpec<'a>>, s: f32, alpha: f32) -> Element<'a, Message> {
    let specs_count = specs.len();
    let mut iter = specs.into_iter().enumerate();
    let space = iced::widget::space().width(Length::Fill);
    let mut r = Row::new().spacing(SPACING * s);

    if let Some((
        _,
        ButtonSpec {
            default_prompt,
            prompt_override,
            message,
        },
    )) = iter.next()
    {
        let label = prompt_override.unwrap_or(default_prompt);
        let btn_text = text(label).size(BTN_SIZE * s);
        let btn = button(btn_text)
            .on_press(message)
            .style(move |_, status| theme::secondary_button(status, s, alpha))
            .padding(button_padding(s));
        r = r.push(btn);
    }

    r = r.push(space);

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
        let btn_text = text(label).size(BTN_SIZE * s);
        let btn = if i < specs_count - 1 {
            button(btn_text)
                .on_press(message)
                .style(move |_, status| theme::secondary_button(status, s, alpha))
                .padding(button_padding(s))
        } else {
            button(btn_text)
                .on_press(message)
                .style(move |_, status| theme::primary_button(status, s, alpha))
                .padding(button_padding(s))
        };
        r = r.push(btn);
    }
    r.into()
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
    let key_sub = event::listen_with(on_key_event);

    // drive redraws during the enter animation (animating_entry) and exit
    let frame_sub = if state.is_opening || state.is_closing {
        iced::window::frames().map(Message::Tick)
    } else {
        iced::Subscription::none()
    };

    // one-shot 500 ms timer to focus the passphrase input after the window opens
    let focus_sub = if matches!(state.request, DialogRequest::GetPin) && !state.input_focused {
        iced::time::every(Duration::from_millis(500)).map(|_| Message::FocusInput)
    } else {
        iced::Subscription::none()
    };

    iced::Subscription::batch([key_sub, frame_sub, focus_sub])
}

fn style(state: &AppState, _theme: &iced::Theme) -> iced::theme::Style {
    // fade the scrim in during enter and out during exit
    let alpha = state.anim.interpolate(0.0_f32, 0.5_f32, Instant::now());
    iced::theme::Style {
        background_color: Color::from_rgba(0.0, 0.0, 0.0, alpha),
        text_color: theme::TEXT,
    }
}

fn button_padding(s: f32) -> Padding {
    [BTN_PADDING.0 * s, BTN_PADDING.1 * s].into()
}
