//! Charmtone color palette and style helpers, matching crush's visual identity.
//! Many constants/functions used in later phases.
#![allow(dead_code)]

use ruse::prelude::*;
use ruse::style::Border;

/// Custom border with ▌ on the left, nothing else
pub const FOCUS_BORDER: Border = Border {
    top: "",
    bottom: "",
    left: "\u{258C}", // ▌
    right: "",
    top_left: "",
    top_right: "",
    bottom_left: "",
    bottom_right: "",
    middle_left: "",
    middle_right: "",
    middle: "",
    middle_top: "",
    middle_bottom: "",
};

// Charmtone palette — matches crush's color scheme exactly

// Backgrounds
pub const BG_BASE: &str = "#2e2c26"; // Pepper - main background
pub const BG_LIGHTER: &str = "#43322b"; // BBQ - thinking blocks, panels
pub const BG_SUBTLE: &str = "#373431"; // Charcoal - subtle backgrounds
pub const BG_OVERLAY: &str = "#555452"; // Iron - overlays

// Foregrounds
pub const FG_BASE: &str = "#a8a19a"; // Ash - primary text
pub const FG_MUTED: &str = "#4a4843"; // Squid - muted text
pub const FG_HALF_MUTED: &str = "#706b65"; // Smoke - half-muted
pub const FG_SUBTLE: &str = "#c1bfb9"; // Oyster - subtle (lighter)
pub const FG_WHITE: &str = "#ffffee"; // Butter - white text

// Accents
pub const PRIMARY: &str = "#9d84b7"; // Charple - user borders, primary
pub const SECONDARY: &str = "#96d34a"; // Bok - cursor, secondary
pub const GREEN_DARK: &str = "#84be42"; // Guac - assistant focus, success
pub const GREEN: &str = "#6ec47f"; // Julep - success indicators
pub const BLUE: &str = "#4db8ff"; // Malibu - links, headings
pub const RED: &str = "#ff6b57"; // Coral - errors
pub const RED_DARK: &str = "#d24545"; // Sriracha - dark errors

// Icons
pub const MODEL_ICON: &str = "\u{25C7}"; // ◇
pub const TOOL_PENDING: &str = "\u{25CF}"; // ●
pub const TOOL_SUCCESS: &str = "\u{2713}"; // ✓
pub const TOOL_ERROR: &str = "\u{00D7}"; // ×
pub const SECTION_SEP: &str = "\u{2500}"; // ─

// Style helpers

pub fn base_style() -> Style {
    Style::new().foreground(Color::parse(FG_BASE))
}

pub fn muted_style() -> Style {
    Style::new().foreground(Color::parse(FG_MUTED))
}

pub fn half_muted_style() -> Style {
    Style::new().foreground(Color::parse(FG_HALF_MUTED))
}

pub fn subtle_style() -> Style {
    Style::new().foreground(Color::parse(FG_SUBTLE))
}

pub fn primary_style() -> Style {
    Style::new().foreground(Color::parse(PRIMARY))
}

pub fn green_dark_style() -> Style {
    Style::new().foreground(Color::parse(GREEN_DARK))
}

pub fn green_style() -> Style {
    Style::new().foreground(Color::parse(GREEN))
}

pub fn red_style() -> Style {
    Style::new().foreground(Color::parse(RED))
}

pub fn blue_style() -> Style {
    Style::new().foreground(Color::parse(BLUE))
}

pub fn separator(width: usize) -> String {
    let sep = SECTION_SEP.repeat(width);
    muted_style().render(&[&sep])
}

/// User message style (blurred): thin left border in Charple
pub fn user_blurred(w: usize) -> Style {
    Style::new()
        .foreground(Color::parse(FG_BASE))
        .padding_left(1)
        .border(NORMAL_BORDER, &[false, false, false, true]) // left only
        .border_foreground(Color::parse(PRIMARY))
        .width(w as u16)
}

/// User message style (focused): thick left border ▌ in Charple
pub fn user_focused(w: usize) -> Style {
    Style::new()
        .foreground(Color::parse(FG_BASE))
        .padding_left(1)
        .border(FOCUS_BORDER, &[false, false, false, true]) // left only
        .border_foreground(Color::parse(PRIMARY))
        .width(w as u16)
}

/// Assistant message style (blurred): just left padding
pub fn assistant_blurred() -> Style {
    Style::new()
        .foreground(Color::parse(FG_BASE))
        .padding_left(2)
}

/// Assistant message style (focused): thick left border ▌ in Guac
pub fn assistant_focused(w: usize) -> Style {
    Style::new()
        .foreground(Color::parse(FG_BASE))
        .padding_left(1)
        .border(FOCUS_BORDER, &[false, false, false, true])
        .border_foreground(Color::parse(GREEN_DARK))
        .width(w as u16)
}

/// Tool call style (blurred): muted, left padding
pub fn tool_blurred() -> Style {
    Style::new()
        .foreground(Color::parse(FG_MUTED))
        .padding_left(2)
}

/// Tool call style (focused): thick left border ▌ in Guac
pub fn tool_focused(w: usize) -> Style {
    Style::new()
        .foreground(Color::parse(FG_MUTED))
        .padding_left(1)
        .border(FOCUS_BORDER, &[false, false, false, true])
        .border_foreground(Color::parse(GREEN_DARK))
        .width(w as u16)
}
