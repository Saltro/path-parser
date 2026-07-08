//! Light / dark theme detection and color palettes.

use ratatui::style::Color;
use std::process::Command;

// ----------------------------------------------------------------------
// Theme
// ----------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Theme {
    Dark,
    Light,
}

// ----------------------------------------------------------------------
// Colors
// ----------------------------------------------------------------------

/// All theme-sensitive colors used by the TUI.
#[derive(Debug, Clone, Copy)]
pub struct Colors {
    pub header: Color,
    pub path: Color,
    pub child: Color,
    pub overwritten: Color,
    pub duplicate: Color,
    pub highlight: Color,
    pub status: Color,
    pub marker: Color,
    pub number: Color,
    pub modal_bg: Color,
    pub modal_text: Color,
    pub search_text: Color,
    pub missing: Color,
    pub checkmark: Color,
    pub search_hl_bg: Color,
}

impl Colors {
    pub fn for_theme(theme: Theme) -> Self {
        match theme {
            Theme::Dark => Self {
                header: Color::Cyan,
                path: Color::White,
                child: Color::White,
                overwritten: Color::Yellow,
                duplicate: Color::Red,
                highlight: Color::LightBlue,
                status: Color::Gray,
                marker: Color::LightMagenta,
                number: Color::LightBlue,
                modal_bg: Color::Black,
                modal_text: Color::White,
                search_text: Color::White,
                missing: Color::Red,
                checkmark: Color::Green,
                search_hl_bg: Color::Yellow,
            },
            Theme::Light => Self {
                header: Color::Cyan,
                path: Color::Black,
                child: Color::DarkGray,
                overwritten: Color::Yellow,
                duplicate: Color::Red,
                highlight: Color::Blue,
                status: Color::DarkGray,
                marker: Color::Magenta,
                number: Color::Blue,
                modal_bg: Color::White,
                modal_text: Color::Black,
                search_text: Color::Black,
                missing: Color::Red,
                checkmark: Color::Green,
                search_hl_bg: Color::Yellow,
            },
        }
    }
}

// ----------------------------------------------------------------------
// Detection
// ----------------------------------------------------------------------

/// Detect whether the terminal / OS is using a light or dark theme.
///
/// Strategy (first match wins):
///   1. `COLORFGBG` env var — common in xterm, iTerm2, tmux, etc.
///   2. macOS: `defaults read -g AppleInterfaceStyle`
///   3. Fall back to **Dark** (the most common terminal default).
pub fn detect_theme() -> Theme {
    // 1. COLORFGBG
    if let Ok(val) = std::env::var("COLORFGBG") {
        if is_light_colorfgbg(&val) {
            return Theme::Light;
        }
        // If the var exists but doesn't look light, assume dark.
        return Theme::Dark;
    }

    // 2. macOS system-wide appearance
    if cfg!(target_os = "macos") {
        if let Ok(output) = Command::new("defaults")
            .args(["read", "-g", "AppleInterfaceStyle"])
            .output()
        {
            let style = String::from_utf8_lossy(&output.stdout).trim().to_lowercase();
            if style.contains("dark") {
                return Theme::Dark;
            } else if !style.is_empty() {
                // "Light" or anything else
                return Theme::Light;
            }
        }
        // On macOS, if AppleInterfaceStyle is not set, the default is Light.
        return Theme::Light;
    }

    // 3. Default
    Theme::Dark
}

/// Parse `COLORFGBG` to decide if the background is light.
///
/// Format examples: `"15;0"`, `"0;15"`, `"15"`, `"default;0"`.
/// The background is typically the **last** number.
/// Bright colors (8–15) with value 7 or 15 suggest a light background.
fn is_light_colorfgbg(val: &str) -> bool {
    // Take the last semicolon-separated segment.
    let last = val.rsplit(';').next().unwrap_or(val);
    match last.trim() {
        // 7 = white (dark-on-light), 15 = bright white
        "7" | "15" => true,
        // RGB-style (rare but possible in some terminals)
        s if s.contains(',') => {
            // e.g. "rgb:ffff/ffff/ffff"
            let s = s.trim_start_matches("rgb:");
            let parts: Vec<&str> = s.split('/').collect();
            if parts.len() == 3 {
                let avg: u32 = parts
                    .iter()
                    .filter_map(|p| u32::from_str_radix(p, 16).ok())
                    .sum::<u32>()
                    / 3;
                return avg > 0x8000;
            }
            false
        }
        _ => false,
    }
}

// ----------------------------------------------------------------------
// Tests
// ----------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colorfgbg_light() {
        assert!(is_light_colorfgbg("0;15"));
        assert!(is_light_colorfgbg("0;7"));
        assert!(is_light_colorfgbg("15"));
        assert!(is_light_colorfgbg("default;7"));
    }

    #[test]
    fn colorfgbg_dark() {
        assert!(!is_light_colorfgbg("15;0"));
        assert!(!is_light_colorfgbg("7;0"));
        assert!(!is_light_colorfgbg("0"));
    }
}
