//! Design tokens, after Sanity's design language: near-black canvas, pure neutral greys with no
//! tint, and one coral-red accent kept for the highest-priority action — the play button.

use rune::tok::{Family, TextStyle};
use rune::{Color, oklch, oklcha};

use crate::game::Source;
use crate::settings::{Settings, ThemeMode};

/// Play-button colours: (name, dark-mode fill, light-mode fill, ink on it).
pub const ACCENTS: [(&str, (f32, f32, f32), (f32, f32, f32), bool); 7] = [
    ("Arach", (0.68, 0.17, 28.0), (0.57, 0.23, 29.0), true),
    ("Craie", (0.96, 0.012, 85.0), (0.24, 0.012, 260.0), true),
    ("Ambre", (0.81, 0.15, 75.0), (0.70, 0.15, 65.0), true),
    ("Corail", (0.71, 0.17, 30.0), (0.62, 0.19, 30.0), false),
    ("Menthe", (0.82, 0.13, 165.0), (0.62, 0.13, 165.0), true),
    ("Azur", (0.72, 0.14, 245.0), (0.58, 0.17, 255.0), false),
    ("Lilas", (0.74, 0.13, 295.0), (0.56, 0.18, 295.0), false),
];

#[derive(Clone, Copy)]
pub struct Palette {
    pub dark: bool,
    pub bg: Color,
    pub side: Color,
    pub surface: Color,
    pub raised: Color,
    pub line: Color,
    pub text: Color,
    pub muted: Color,
    pub faint: Color,
    pub hover: Color,
    /// A text field's or select's resting fill.
    pub field: Color,
    /// Menus, listboxes and dialogs: a step above `raised`.
    pub popover: Color,
    /// The rule inside a popover.
    pub popover_line: Color,
    pub shadow: Color,
    pub accent: Color,
    /// Text on the accent.
    pub on_accent: Color,
    pub danger: Color,
}

pub fn palette(settings: &Settings, system_dark: bool) -> Palette {
    let dark = match settings.theme {
        ThemeMode::System => system_dark,
        ThemeMode::Dark => true,
        ThemeMode::Light => false,
    };
    let (_, d, l, ink_on) = ACCENTS[settings.accent.min(ACCENTS.len() - 1)];
    let a = if dark { d } else { l };
    let accent = oklch(a.0, a.1, a.2);
    // Chalk turns into ink on a light page, so its text flips too.
    // Coral takes ink in the dark (as Sanity's brand button does); its deep light-mode red takes white.
    let on_dark_accent = ink_on && !(matches!(settings.accent, 0 | 1) && !dark);
    let on_accent = if on_dark_accent { oklch(0.145, 0.0, 0.0) } else { Color::WHITE };
    if dark {
        Palette {
            dark,
            bg: oklch(0.145, 0.0, 0.0),          // canvas #0b0b0b
            side: oklch(0.17, 0.0, 0.0),
            surface: oklch(0.245, 0.0, 0.0),     // canvas-soft #212121
            raised: oklch(0.32, 0.0, 0.0),       // graphite #353535
            line: oklcha(1.0, 0.0, 0.0, 0.09),
            text: oklch(1.0, 0.0, 0.0),
            muted: oklch(0.79, 0.0, 0.0),        // ash #b9b9b9
            faint: oklch(0.59, 0.0, 0.0),        // mute #797979
            hover: oklcha(1.0, 0.0, 0.0, 0.05),
            field: oklch(0.145, 0.0, 0.0),
            popover: oklch(0.245, 0.0, 0.0),
            popover_line: oklcha(1.0, 0.0, 0.0, 0.1),
            shadow: Color::BLACK.alpha(0.6),
            accent,
            on_accent,
            danger: oklch(0.57, 0.23, 29.0),     // error #dd0000
        }
    } else {
        Palette {
            dark,
            bg: oklch(1.0, 0.0, 0.0),            // canvas-light #ffffff
            side: oklch(0.948, 0.0, 0.0),        // paper #ededed
            surface: oklch(1.0, 0.0, 0.0),
            raised: oklch(0.948, 0.0, 0.0),
            line: oklch(0.948, 0.0, 0.0),        // hairline #ededed
            text: oklch(0.145, 0.0, 0.0),        // ink #0b0b0b
            muted: oklch(0.32, 0.0, 0.0),
            faint: oklch(0.59, 0.0, 0.0),
            hover: oklcha(0.0, 0.0, 0.0, 0.04),
            field: oklch(1.0, 0.0, 0.0),
            popover: oklch(1.0, 0.0, 0.0),
            popover_line: oklch(0.948, 0.0, 0.0),
            shadow: oklcha(0.0, 0.0, 0.0, 0.12),
            accent,
            on_accent,
            danger: oklch(0.57, 0.23, 29.0),
        }
    }
}

/// Each store's colour, for its dot in the sidebar and its share of a drive bar.
pub fn store_color(s: Source) -> Color {
    match s {
        Source::Steam => oklch(0.72, 0.11, 235.0),
        Source::Epic => oklch(0.86, 0.0, 0.0),
        Source::Gog => oklch(0.63, 0.17, 305.0),
        Source::Ubisoft => oklch(0.66, 0.15, 262.0),
        Source::BattleNet => oklch(0.76, 0.12, 215.0),
        Source::Riot => oklch(0.64, 0.21, 22.0),
        Source::Ea => oklch(0.74, 0.16, 52.0),
        Source::Xbox => oklch(0.68, 0.18, 142.0),
        Source::Amazon => oklch(0.78, 0.15, 75.0),
        Source::Rockstar => oklch(0.82, 0.16, 95.0),
        Source::Itch => oklch(0.66, 0.2, 18.0),
        Source::Heroic => oklch(0.7, 0.13, 200.0),
        Source::Legendary => oklch(0.72, 0.06, 280.0),
        Source::Ds => oklch(0.8, 0.03, 250.0),
        Source::N3ds => oklch(0.66, 0.2, 5.0),
        Source::Gba => oklch(0.6, 0.18, 285.0),
        Source::Gc => oklch(0.58, 0.16, 290.0),
        Source::Wii => oklch(0.9, 0.01, 240.0),
        Source::Ps1 => oklch(0.7, 0.02, 260.0),
        Source::Ps2 => oklch(0.5, 0.15, 265.0),
        Source::Ps3 => oklch(0.42, 0.05, 260.0),
        Source::Switch => oklch(0.64, 0.22, 25.0),
        Source::Retro => oklch(0.72, 0.14, 330.0),
        Source::Folder => oklch(0.76, 0.12, 145.0),
        Source::Custom => oklch(0.62, 0.02, 260.0),
    }
}

// ---------------------------------------------------------------- type

/// Page titles and game names: the same sans as the rest, regular weight, tracked tight —
/// size and tracking carry the hierarchy, never a contrasting heavy face.
pub fn display(size: f32) -> TextStyle {
    let mut s = TextStyle::new(size, 450.0);
    s.line_height = size * 1.05;
    s.tracking = -size * 0.035;
    s
}

pub fn ui(size: f32, weight: f32) -> TextStyle {
    TextStyle::new(size, weight)
}

/// A quiet label above a group or a column, in mono like Sanity's technical eyebrows.
pub fn caption() -> TextStyle {
    let mut s = TextStyle::new(11.5, 400.0);
    s.family = Family::SystemMono;
    s
}

/// Figures that line up: sizes, durations, counts.
pub fn num(size: f32, weight: f32) -> TextStyle {
    TextStyle::new(size, weight).features("'tnum' 1")
}

/// Paths and launch arguments, where every character matters.
pub fn mono(size: f32) -> TextStyle {
    let mut s = TextStyle::new(size, 400.0);
    s.family = Family::SystemMono;
    s
}
