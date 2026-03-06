//! Visual theme for the pinentry dialog.
//!
//! Palette based on Catppuccin Mocha. All style functions follow the iced
//! `Fn(&Theme, Status) -> Style` or `Fn(&Theme) -> Style` signatures so they
//! can be passed directly to widget `.style()` calls.

use iced::{
    Background, Border, Color, Shadow, Vector,
    widget::{button, container, text_input},
};

// -- color palette --

/// Main card surface background.
pub const BASE: Color = Color {
    r: 0.118,
    g: 0.118,
    b: 0.180,
    a: 1.0,
};

/// Deeper inset background (text input fill).
pub const MANTLE: Color = Color {
    r: 0.094,
    g: 0.094,
    b: 0.145,
    a: 1.0,
};

/// Raised surface color used for borders and subtle hover fills.
pub const SURFACE: Color = Color {
    r: 0.192,
    g: 0.196,
    b: 0.267,
    a: 1.0,
};

/// Primary text color.
pub const TEXT: Color = Color {
    r: 0.804,
    g: 0.839,
    b: 0.957,
    a: 1.0,
};

/// Muted text color for secondary labels (prompt, placeholders).
pub const SUBTEXT: Color = Color {
    r: 0.729,
    g: 0.761,
    b: 0.871,
    a: 1.0,
};

/// Accent color for the primary action button and focus rings.
pub const ACCENT: Color = Color {
    r: 0.537,
    g: 0.706,
    b: 0.980,
    a: 1.0,
};

/// Error highlight color.
pub const ERROR: Color = Color {
    r: 0.953,
    g: 0.545,
    b: 0.659,
    a: 1.0,
};

// -- style functions --

/// Style for the dialog card container.
pub fn card(_theme: &iced::Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(BASE)),
        border: Border {
            color: SURFACE,
            width: 1.0,
            radius: 12.0.into(),
        },
        shadow: Shadow {
            color: Color {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: 0.45,
            },
            offset: Vector { x: 0.0, y: 6.0 },
            blur_radius: 24.0,
        },
        ..Default::default()
    }
}

/// Style for the primary action button (Okay / Submit) with hover and press
/// states.
pub fn primary_button(_theme: &iced::Theme, status: button::Status) -> button::Style {
    let bg = match status {
        button::Status::Hovered => lighten(ACCENT, 0.10),
        button::Status::Pressed => darken(ACCENT, 0.20),
        _ => ACCENT,
    };
    button::Style {
        background: Some(Background::Color(bg)),
        // dark text gives better contrast on the light accent color
        text_color: MANTLE,
        border: Border {
            radius: 12.0.into(),
            ..Default::default()
        },
        ..Default::default()
    }
}

/// Style for secondary buttons (Cancel, Not Okay) with subtle outline.
pub fn secondary_button(_theme: &iced::Theme, status: button::Status) -> button::Style {
    let bg = match status {
        button::Status::Hovered => Color {
            r: 1.0,
            g: 1.0,
            b: 1.0,
            a: 0.12,
        },
        button::Status::Pressed => Color {
            r: 1.0,
            g: 1.0,
            b: 1.0,
            a: 0.06,
        },
        _ => Color::TRANSPARENT,
    };
    button::Style {
        background: Some(Background::Color(bg)),
        text_color: SUBTEXT,
        border: Border {
            color: SURFACE,
            width: 2.0,
            radius: 12.0.into(),
        },
        ..Default::default()
    }
}

/// Style for the passphrase text input with a visible focus ring.
pub fn text_input_style(_theme: &iced::Theme, status: text_input::Status) -> text_input::Style {
    let (border_color, border_width) = match status {
        text_input::Status::Focused { .. } => (ACCENT, 2.0),
        text_input::Status::Hovered => (SUBTEXT, 1.0),
        _ => (SURFACE, 1.0),
    };
    text_input::Style {
        background: Background::Color(MANTLE),
        border: Border {
            color: border_color,
            width: border_width,
            radius: 12.0.into(),
        },
        icon: SUBTEXT,
        placeholder: SUBTEXT,
        value: TEXT,
        selection: Color { a: 0.35, ..ACCENT },
    }
}

/// Style for the error banner container.
pub fn error_banner(_theme: &iced::Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(Color {
            r: ERROR.r * 0.15,
            g: ERROR.g * 0.08,
            b: ERROR.b * 0.12,
            a: 1.0,
        })),
        border: Border {
            color: Color { a: 0.35, ..ERROR },
            width: 1.0,
            radius: 6.0.into(),
        },
        ..Default::default()
    }
}

// -- internal helpers --

/// Darkens a color by subtracting `amount` from each RGB channel.
fn darken(c: Color, amount: f32) -> Color {
    Color {
        r: (c.r - amount).max(0.0),
        g: (c.g - amount).max(0.0),
        b: (c.b - amount).max(0.0),
        a: c.a,
    }
}

/// Darkens a color by subtracting `amount` from each RGB channel.
fn lighten(c: Color, amount: f32) -> Color {
    Color {
        r: (c.r + amount).min(1.0),
        g: (c.g + amount).min(1.0),
        b: (c.b + amount).min(1.0),
        a: c.a,
    }
}
