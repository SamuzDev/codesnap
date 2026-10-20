//! Appearance options, received via `initializationOptions` (the
//! `lsp.zedsnap-ls.initialization_options` block in `settings.json`).

use serde::Deserialize;

pub const DEFAULT_THEME: &str = "dracula";

fn default_padding() -> f32 {
    48.0
}
fn default_font_size() -> f32 {
    14.0
}
fn default_corner_radius() -> f32 {
    12.0
}
fn default_scale() -> f32 {
    2.0
}
fn default_tab_width() -> usize {
    4
}
fn default_background() -> String {
    "#282a36,#1a1b26".to_string()
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Config {
    /// syntect theme used for highlighting and for the window background.
    pub theme: String,
    /// Image backdrop: a color, a comma-separated list (gradient) or
    /// `transparent`.
    pub background: String,
    /// Margin around the window, in pixels.
    pub padding: f32,
    /// Code font size, in pixels.
    pub font_size: f32,
    /// Show the line-number gutter.
    pub line_numbers: bool,
    /// Show the macOS-style traffic lights in the title bar.
    pub window_controls: bool,
    /// Window title (`null` = file name).
    pub window_title: Option<String>,
    /// Drop shadow under the window.
    pub shadow: bool,
    /// Window corner radius.
    pub corner_radius: f32,
    /// Image scale (2 = retina-sharp).
    pub scale: f32,
    /// Columns a tab character occupies.
    pub tab_width: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            theme: DEFAULT_THEME.to_string(),
            background: default_background(),
            padding: default_padding(),
            font_size: default_font_size(),
            line_numbers: false,
            window_controls: true,
            window_title: None,
            shadow: true,
            corner_radius: default_corner_radius(),
            scale: default_scale(),
            tab_width: default_tab_width(),
        }
    }
}

impl Config {
    pub fn from_value(value: Option<serde_json::Value>) -> Self {
        value
            .and_then(|value| serde_json::from_value(value).ok())
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partial_options_fall_back_to_defaults() {
        let config = Config::from_value(Some(serde_json::json!({ "lineNumbers": true })));
        assert!(config.line_numbers);
        assert_eq!(config.font_size, 14.0);
        assert_eq!(config.theme, DEFAULT_THEME);
    }

    #[test]
    fn invalid_options_fall_back_to_defaults() {
        let config = Config::from_value(Some(serde_json::json!({ "font_size": "grande" })));
        assert_eq!(config.font_size, 14.0);
    }
}
