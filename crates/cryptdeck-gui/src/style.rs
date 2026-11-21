//! Cryptdeck's look: a light and a dark palette, and the widget styles
//! built from them. Views pick a style from here instead of using iced's
//! defaults, so both modes stay consistent.

use std::sync::LazyLock;

use iced::font::Weight;
use iced::theme::{Mode, Palette};
use iced::widget::{button, checkbox, container, text, text_input};
use iced::{border, Background, Border, Color, Font, Shadow, Theme, Vector};
use serde::{Deserialize, Serialize};

/// Headings and names.
pub const SEMIBOLD: Font = Font {
    weight: Weight::Semibold,
    ..Font::DEFAULT
};

/// Corner radius for buttons and inputs.
const RADIUS: f32 = 8.0;
/// Corner radius for cards and banners.
const CARD_RADIUS: f32 = 12.0;

/// What the user picked in the appearance switch.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum ThemePref {
    /// Follow the operating system's light or dark mode.
    #[default]
    System,
    Light,
    Dark,
}

impl ThemePref {
    pub const ALL: [ThemePref; 3] = [ThemePref::System, ThemePref::Light, ThemePref::Dark];

    /// The theme to draw with, given the system's current mode.
    pub fn resolve(self, system: Mode) -> Theme {
        let dark = match self {
            ThemePref::Light => false,
            ThemePref::Dark => true,
            ThemePref::System => system == Mode::Dark,
        };
        if dark {
            DARK_THEME.clone()
        } else {
            LIGHT_THEME.clone()
        }
    }
}

/// The colours every style is built from.
pub struct Tokens {
    /// Window background.
    pub bg: Color,
    /// Cards, inputs and secondary buttons.
    pub surface: Color,
    pub surface_hover: Color,
    /// Track behind the segmented switch.
    pub track: Color,
    pub border: Color,
    pub border_strong: Color,
    pub text: Color,
    pub muted: Color,
    pub primary: Color,
    pub primary_hover: Color,
    pub primary_soft: Color,
    pub on_primary: Color,
    pub success: Color,
    pub success_soft: Color,
    pub warning: Color,
    pub warning_soft: Color,
    pub danger: Color,
    pub danger_soft: Color,
    pub shadow: Color,
}

const fn rgb(hex: u32) -> Color {
    Color::from_rgb8((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

const fn rgba(hex: u32, a: f32) -> Color {
    Color::from_rgba8((hex >> 16) as u8, (hex >> 8) as u8, hex as u8, a)
}

static LIGHT: Tokens = Tokens {
    bg: rgb(0xF5F6F8),
    surface: rgb(0xFFFFFF),
    surface_hover: rgb(0xF0F1F4),
    track: rgb(0xE8EAEE),
    border: rgb(0xE3E5EA),
    border_strong: rgb(0xCDD1D9),
    text: rgb(0x14171C),
    muted: rgb(0x687080),
    primary: rgb(0x4F46E5),
    primary_hover: rgb(0x4338CA),
    primary_soft: rgb(0xEEF0FF),
    on_primary: rgb(0xFFFFFF),
    success: rgb(0x15803D),
    success_soft: rgb(0xDCFCE7),
    warning: rgb(0xB45309),
    warning_soft: rgb(0xFEF3C7),
    danger: rgb(0xB91C1C),
    danger_soft: rgb(0xFEE2E2),
    shadow: rgba(0x0F172A, 0.06),
};

static DARK: Tokens = Tokens {
    bg: rgb(0x0E1014),
    surface: rgb(0x171A20),
    surface_hover: rgb(0x1F232B),
    track: rgb(0x1B1E25),
    border: rgb(0x262A33),
    border_strong: rgb(0x363B47),
    text: rgb(0xE7E9ED),
    muted: rgb(0x9198A5),
    primary: rgb(0x6366F1),
    primary_hover: rgb(0x7C7FF5),
    primary_soft: rgba(0x6366F1, 0.18),
    on_primary: rgb(0xFFFFFF),
    success: rgb(0x4ADE80),
    success_soft: rgba(0x22C55E, 0.15),
    warning: rgb(0xFBBF24),
    warning_soft: rgba(0xF59E0B, 0.14),
    danger: rgb(0xF87171),
    danger_soft: rgba(0xEF4444, 0.15),
    shadow: rgba(0x000000, 0.0),
};

fn palette(t: &Tokens) -> Palette {
    Palette {
        background: t.bg,
        text: t.text,
        primary: t.primary,
        success: t.success,
        warning: t.warning,
        danger: t.danger,
    }
}

static LIGHT_THEME: LazyLock<Theme> =
    LazyLock::new(|| Theme::custom("Cryptdeck Light", palette(&LIGHT)));
static DARK_THEME: LazyLock<Theme> =
    LazyLock::new(|| Theme::custom("Cryptdeck Dark", palette(&DARK)));

/// The colours for the theme being drawn.
pub fn tokens(theme: &Theme) -> &'static Tokens {
    if theme.extended_palette().is_dark {
        &DARK
    } else {
        &LIGHT
    }
}

fn faded(style: button::Style) -> button::Style {
    button::Style {
        background: style.background.map(|b| b.scale_alpha(0.5)),
        text_color: style.text_color.scale_alpha(0.5),
        border: Border {
            color: style.border.color.scale_alpha(0.5),
            ..style.border
        },
        ..style
    }
}

// ----- Buttons -----

/// The main action: filled with the accent colour.
pub fn btn_primary(theme: &Theme, status: button::Status) -> button::Style {
    let t = tokens(theme);
    let base = button::Style {
        background: Some(t.primary.into()),
        text_color: t.on_primary,
        border: border::rounded(RADIUS),
        shadow: Shadow {
            color: t.shadow,
            offset: Vector::new(0.0, 1.0),
            blur_radius: 2.0,
        },
        snap: true,
    };
    match status {
        button::Status::Hovered | button::Status::Pressed => button::Style {
            background: Some(t.primary_hover.into()),
            ..base
        },
        button::Status::Active => base,
        button::Status::Disabled => faded(base),
    }
}

/// A complementary action: a plain surface with an outline.
pub fn btn_secondary(theme: &Theme, status: button::Status) -> button::Style {
    let t = tokens(theme);
    let base = button::Style {
        background: Some(t.surface.into()),
        text_color: t.text,
        border: border::rounded(RADIUS).color(t.border_strong).width(1),
        shadow: Shadow {
            color: t.shadow,
            offset: Vector::new(0.0, 1.0),
            blur_radius: 2.0,
        },
        snap: true,
    };
    match status {
        button::Status::Hovered | button::Status::Pressed => button::Style {
            background: Some(t.surface_hover.into()),
            ..base
        },
        button::Status::Active => base,
        button::Status::Disabled => faded(base),
    }
}

/// A quiet action: just text, with a background on hover.
pub fn btn_ghost(theme: &Theme, status: button::Status) -> button::Style {
    let t = tokens(theme);
    let base = button::Style {
        background: None,
        text_color: t.muted,
        border: border::rounded(RADIUS),
        shadow: Shadow::default(),
        snap: true,
    };
    match status {
        button::Status::Hovered | button::Status::Pressed => button::Style {
            background: Some(t.surface_hover.into()),
            text_color: t.text,
            ..base
        },
        button::Status::Active => base,
        button::Status::Disabled => faded(base),
    }
}

/// A quiet action that removes or locks something.
pub fn btn_danger_ghost(theme: &Theme, status: button::Status) -> button::Style {
    let t = tokens(theme);
    let base = button::Style {
        background: None,
        text_color: t.danger,
        border: border::rounded(RADIUS),
        shadow: Shadow::default(),
        snap: true,
    };
    match status {
        button::Status::Hovered | button::Status::Pressed => button::Style {
            background: Some(t.danger_soft.into()),
            ..base
        },
        button::Status::Active => base,
        button::Status::Disabled => faded(base),
    }
}

/// One option of a segmented switch.
pub fn btn_segment(selected: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |theme, status| {
        let t = tokens(theme);
        if selected {
            button::Style {
                background: Some(t.surface.into()),
                text_color: t.text,
                border: border::rounded(RADIUS - 2.0).color(t.border).width(1),
                shadow: Shadow {
                    color: t.shadow,
                    offset: Vector::new(0.0, 1.0),
                    blur_radius: 2.0,
                },
                snap: true,
            }
        } else {
            button::Style {
                background: None,
                text_color: match status {
                    button::Status::Hovered | button::Status::Pressed => t.text,
                    _ => t.muted,
                },
                border: border::rounded(RADIUS - 2.0),
                shadow: Shadow::default(),
                snap: true,
            }
        }
    }
}

// ----- Inputs -----

pub fn input(theme: &Theme, status: text_input::Status) -> text_input::Style {
    let t = tokens(theme);
    let border_color = match status {
        text_input::Status::Focused { .. } => t.primary,
        text_input::Status::Hovered => t.muted,
        _ => t.border_strong,
    };
    text_input::Style {
        background: Background::Color(match status {
            text_input::Status::Disabled => t.surface_hover,
            _ => t.surface,
        }),
        border: border::rounded(RADIUS).color(border_color).width(1),
        icon: t.muted,
        placeholder: t.muted,
        value: t.text,
        selection: t
            .primary_soft
            .scale_alpha(if theme.extended_palette().is_dark {
                2.0
            } else {
                1.0
            }),
    }
}

pub fn check(theme: &Theme, status: checkbox::Status) -> checkbox::Style {
    let t = tokens(theme);
    let (checked, disabled, hovered) = match status {
        checkbox::Status::Active { is_checked } => (is_checked, false, false),
        checkbox::Status::Hovered { is_checked } => (is_checked, false, true),
        checkbox::Status::Disabled { is_checked } => (is_checked, true, false),
    };
    let alpha = if disabled { 0.5 } else { 1.0 };
    checkbox::Style {
        background: Background::Color(
            if checked { t.primary } else { t.surface }.scale_alpha(alpha),
        ),
        icon_color: t.on_primary,
        border: border::rounded(5)
            .color(
                if checked {
                    t.primary
                } else if hovered {
                    t.muted
                } else {
                    t.border_strong
                }
                .scale_alpha(alpha),
            )
            .width(1),
        text_color: Some(t.text.scale_alpha(alpha)),
    }
}

// ----- Text -----

pub fn muted(theme: &Theme) -> text::Style {
    text::Style {
        color: Some(tokens(theme).muted),
    }
}

pub fn danger_text(theme: &Theme) -> text::Style {
    text::Style {
        color: Some(tokens(theme).danger),
    }
}

// ----- Containers -----

/// A raised surface for each volume and for the settings.
pub fn card(theme: &Theme) -> container::Style {
    let t = tokens(theme);
    container::Style {
        background: Some(t.surface.into()),
        text_color: Some(t.text),
        border: border::rounded(CARD_RADIUS).color(t.border).width(1),
        shadow: Shadow {
            color: t.shadow,
            offset: Vector::new(0.0, 1.0),
            blur_radius: 3.0,
        },
        ..container::Style::default()
    }
}

/// The track behind the segmented switch.
pub fn track(theme: &Theme) -> container::Style {
    let t = tokens(theme);
    container::Style {
        background: Some(t.track.into()),
        border: border::rounded(RADIUS + 1.0),
        ..container::Style::default()
    }
}

/// A thin line between sections of a card.
pub fn divider(theme: &Theme) -> container::Style {
    container::Style {
        background: Some(tokens(theme).border.into()),
        ..container::Style::default()
    }
}

#[derive(Clone, Copy)]
pub enum Tone {
    Success,
    Warning,
    Danger,
}

/// A soft, tinted box for banners and notices.
pub fn banner(tone: Tone) -> impl Fn(&Theme) -> container::Style {
    move |theme| {
        let t = tokens(theme);
        let (soft, strong) = match tone {
            Tone::Success => (t.success_soft, t.success),
            Tone::Warning => (t.warning_soft, t.warning),
            Tone::Danger => (t.danger_soft, t.danger),
        };
        container::Style {
            background: Some(soft.into()),
            text_color: Some(t.text),
            border: border::rounded(CARD_RADIUS)
                .color(strong.scale_alpha(0.35))
                .width(1),
            ..container::Style::default()
        }
    }
}

/// The small "Locked" / "Unlocked" label on each volume.
pub fn pill(unlocked: bool) -> impl Fn(&Theme) -> container::Style {
    move |theme| {
        let t = tokens(theme);
        let (bg, fg) = if unlocked {
            (t.success_soft, t.success)
        } else {
            (t.surface_hover, t.muted)
        };
        container::Style {
            background: Some(bg.into()),
            text_color: Some(fg),
            border: border::rounded(999),
            ..container::Style::default()
        }
    }
}

/// The dot inside the status pill.
pub fn dot(unlocked: bool) -> impl Fn(&Theme) -> container::Style {
    move |theme| {
        let t = tokens(theme);
        container::Style {
            background: Some(if unlocked { t.success } else { t.muted }.into()),
            border: border::rounded(999),
            ..container::Style::default()
        }
    }
}

/// Shown over the whole window while a file is dragged onto it.
pub fn drop_zone(theme: &Theme) -> container::Style {
    let t = tokens(theme);
    container::Style {
        background: Some(t.primary_soft.into()),
        text_color: Some(t.text),
        border: border::rounded(16).color(t.primary).width(2),
        ..container::Style::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_follows_the_os_and_overrides_win() {
        let is_dark = |p: ThemePref, m| p.resolve(m).extended_palette().is_dark;
        assert!(is_dark(ThemePref::System, Mode::Dark));
        assert!(!is_dark(ThemePref::System, Mode::Light));
        assert!(!is_dark(ThemePref::System, Mode::None));
        assert!(is_dark(ThemePref::Dark, Mode::Light));
        assert!(!is_dark(ThemePref::Light, Mode::Dark));
    }

    #[test]
    fn theme_pref_is_stored_in_lowercase() {
        assert_eq!(
            serde_json::to_string(&ThemePref::Dark).unwrap(),
            r#""dark""#
        );
    }
}
