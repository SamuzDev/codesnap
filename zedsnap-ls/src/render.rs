//! Builds the "polaroid" SVG: gradient backdrop, floating window with macOS
//! traffic lights, drop shadow and highlighted monospace code.
//!
//! Polacode gets that look in VS Code with a webview + CSS + dom-to-image.
//! Zed extensions have no DOM, so everything here is vector graphics that
//! `resvg` rasterizes into a PNG.

use syntect::highlighting::{Color, FontStyle, Style, Theme};

use crate::config::Config;
use crate::highlight::StyledLine;

/// Monospace font stack. The first entry (embedded in the binary) is the one
/// `resvg` uses, so the output looks the same on every machine.
const FONT_STACK: &str = "'JetBrains Mono', 'Fira Code', 'Cascadia Code', 'SF Mono', Menlo, 'Liberation Mono', 'DejaVu Sans Mono', Consolas, monospace";

/// Advance width of JetBrains Mono, as a fraction of the font size.
const CHAR_ADVANCE: f32 = 0.6;

/// Vertical metrics of JetBrains Mono (em units), used to center each glyph
/// inside its line box instead of eyeballing the baseline.
const ASCENT_EM: f32 = 1.02;
const DESCENT_EM: f32 = 0.30;

const TRAFFIC_LIGHTS: [&str; 3] = ["#ff5f57", "#febc2e", "#28c840"];

pub struct Snapshot<'a> {
    pub lines: &'a [StyledLine],
    pub theme: &'a Theme,
    pub title: Option<&'a str>,
    pub config: &'a Config,
}

enum Background {
    Transparent,
    Solid(String),
    Gradient(Vec<String>),
}

fn parse_background(spec: &str) -> Background {
    let spec = spec.trim();
    if spec.is_empty() || spec.eq_ignore_ascii_case("transparent") {
        return Background::Transparent;
    }
    let stops: Vec<String> = spec
        .split(',')
        .map(|stop| stop.trim().to_string())
        .filter(|stop| !stop.is_empty())
        .collect();
    match stops.len() {
        0 => Background::Transparent,
        1 => Background::Solid(stops.into_iter().next().unwrap()),
        _ => Background::Gradient(stops),
    }
}

fn hex(color: Color) -> String {
    format!("#{:02x}{:02x}{:02x}", color.r, color.g, color.b)
}

fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(ch),
        }
    }
    out
}

/// Merges consecutive spans with the same color and style: fewer nodes in the
/// SVG and seamless columns.
fn merge_spans(line: &StyledLine) -> Vec<(Style, String)> {
    let mut merged: Vec<(Style, String)> = Vec::new();
    for (style, text) in line {
        if text.is_empty() {
            continue;
        }
        if let Some((last_style, last_text)) = merged.last_mut() {
            if last_style.foreground == style.foreground
                && last_style.font_style == style.font_style
            {
                last_text.push_str(text);
                continue;
            }
        }
        merged.push((*style, text.clone()));
    }
    merged
}

pub fn build_svg(snapshot: &Snapshot) -> String {
    let config = snapshot.config;
    let font_size = config.font_size.clamp(6.0, 72.0);
    let char_width = font_size * CHAR_ADVANCE;
    let line_height = (font_size * 1.6).round();

    let line_count = snapshot.lines.len().max(1);
    let max_columns = snapshot
        .lines
        .iter()
        .map(|line| line.iter().map(|(_, text)| text.chars().count()).sum::<usize>())
        .max()
        .unwrap_or(0)
        .max(16);

    // Interior layout of the window.
    let padding = config.padding.clamp(0.0, 512.0);
    let pad_x = (font_size * 2.0).round();
    let pad_top = (font_size * 1.2).round();
    let pad_bottom = (font_size * 1.8).round();
    let has_chrome = config.window_controls || snapshot.title.is_some();
    let titlebar_height = if has_chrome { 44.0 } else { 12.0 };

    let digits = line_count.to_string().len();
    let gutter = if config.line_numbers {
        digits as f32 * char_width + 28.0
    } else {
        0.0
    };

    let mut window_width = gutter + max_columns as f32 * char_width + pad_x * 2.0;
    if let Some(title) = snapshot.title {
        let title_width = title.chars().count() as f32 * 13.0 * CHAR_ADVANCE;
        window_width = window_width.max(title_width + 176.0);
    }
    window_width = window_width.max(320.0);
    let window_height = titlebar_height + pad_top + line_count as f32 * line_height + pad_bottom;

    let (window_x, window_y) = (padding, padding);
    let width = (window_width + padding * 2.0).ceil();
    let height = (window_height + padding * 2.0).ceil();

    // Window colors taken from the theme.
    let window_bg = snapshot
        .theme
        .settings
        .background
        .map(hex)
        .unwrap_or_else(|| "#282c34".to_string());
    let foreground = snapshot
        .theme
        .settings
        .foreground
        .map(hex)
        .unwrap_or_else(|| "#c8ccd4".to_string());

    let mut svg = String::with_capacity(16 * 1024);
    svg.push_str(&format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{width:.0}" height="{height:.0}" viewBox="0 0 {width:.0} {height:.0}" font-family="{FONT_STACK}" xml:space="preserve" style="white-space: pre" shape-rendering="geometricPrecision" text-rendering="geometricPrecision">"#,
    ));

    // Defs: backdrop gradient + window shadow.
    let background = parse_background(&config.background);
    svg.push_str("<defs>");
    if let Background::Gradient(stops) = &background {
        svg.push_str(r#"<linearGradient id="bg" x1="0" y1="0" x2="1" y2="1">"#);
        let last = (stops.len() - 1).max(1);
        for (index, stop) in stops.iter().enumerate() {
            svg.push_str(&format!(
                r#"<stop offset="{:.4}" stop-color="{}"/>"#,
                index as f32 / last as f32,
                escape(stop)
            ));
        }
        svg.push_str("</linearGradient>");
    }
    if config.shadow {
        svg.push_str(r##"<filter id="shadow" x="-40%" y="-40%" width="180%" height="200%"><feDropShadow dx="0" dy="12" stdDeviation="14" flood-color="#000000" flood-opacity="0.38"/></filter>"##);
    }
    svg.push_str("</defs>");

    // Backdrop.
    match &background {
        Background::Transparent => {}
        Background::Solid(color) => {
            svg.push_str(&format!(
                r#"<rect width="100%" height="100%" fill="{}"/>"#,
                escape(color)
            ));
        }
        Background::Gradient(_) => {
            svg.push_str(r#"<rect width="100%" height="100%" fill="url(#bg)"/>"#);
        }
    }

    // Window.
    let filter = if config.shadow {
        r#" filter="url(#shadow)""#
    } else {
        ""
    };
    svg.push_str(&format!(
        r##"<rect x="{window_x:.1}" y="{window_y:.1}" width="{window_width:.1}" height="{window_height:.1}" rx="{radius:.1}" fill="{window_bg}"{filter} stroke="#ffffff" stroke-opacity="0.08"/>"##,
        radius = config.corner_radius.clamp(0.0, 32.0),
    ));

    // Traffic lights.
    if config.window_controls {
        for (index, color) in TRAFFIC_LIGHTS.iter().enumerate() {
            svg.push_str(&format!(
                r#"<circle cx="{:.1}" cy="{:.1}" r="6.5" fill="{color}"/>"#,
                window_x + 26.0 + index as f32 * 22.0,
                window_y + titlebar_height / 2.0,
            ));
        }
    }

    // Title.
    if let Some(title) = snapshot.title {
        svg.push_str(&format!(
            r#"<text x="{:.1}" y="{:.1}" font-size="13" fill="{foreground}" fill-opacity="0.55" text-anchor="middle">{}</text>"#,
            window_x + window_width / 2.0,
            window_y + titlebar_height / 2.0 + 4.5,
            escape(title),
        ));
    }

    // Code: the baseline sits at half-leading + ascent, so glyphs are optically
    // centered in their line box with the real JetBrains Mono metrics.
    let code_x = window_x + pad_x + gutter;
    let half_leading = ((line_height - font_size * (ASCENT_EM + DESCENT_EM)) / 2.0).max(0.0);
    let baseline = font_size * ASCENT_EM + half_leading;
    for (row, line) in snapshot.lines.iter().enumerate() {
        let y = window_y + titlebar_height + pad_top + row as f32 * line_height + baseline;

        if config.line_numbers {
            svg.push_str(&format!(
                r#"<text x="{:.1}" y="{y:.1}" font-size="{font_size:.1}" fill="{foreground}" fill-opacity="0.4" text-anchor="end">{}</text>"#,
                window_x + pad_x + gutter - 16.0,
                row + 1,
            ));
        }

        let mut column = 0usize;
        for (style, text) in merge_spans(line) {
            let columns = text.chars().count();
            // Whitespace paints nothing: the horizontal offset already covers it.
            if text.trim().is_empty() {
                column += columns;
                continue;
            }
            let x = code_x + column as f32 * char_width;
            let mut attrs = String::new();
            if style.font_style.contains(FontStyle::BOLD) {
                attrs.push_str(r#" font-weight="bold""#);
            }
            if style.font_style.contains(FontStyle::ITALIC) {
                attrs.push_str(r#" font-style="italic""#);
            }
            if style.font_style.contains(FontStyle::UNDERLINE) {
                attrs.push_str(r#" text-decoration="underline""#);
            }
            svg.push_str(&format!(
                r#"<text x="{x:.1}" y="{y:.1}" font-size="{font_size:.1}" fill="{}"{attrs}>{}</text>"#,
                hex(style.foreground),
                escape(&text),
            ));
            column += columns;
        }
    }

    svg.push_str("</svg>");
    svg
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{config::Config, highlight};

    #[test]
    fn builds_a_svg_with_window_chrome() {
        let config = Config::default();
        let theme = highlight::theme(&config.theme);
        let lines = highlight::highlight("fn main() {}\n", Some("rust"), Some("rs"), theme);
        let svg = build_svg(&Snapshot {
            lines: &lines,
            theme,
            title: Some("main.rs"),
            config: &config,
        });
        assert!(svg.starts_with("<svg"));
        assert!(svg.contains("main.rs"));
        assert!(svg.contains("#ff5f57"));
        assert!(svg.contains(">fn<"));
        assert!(svg.contains(">main<"));
        assert!(svg.contains("() {}"));
    }

    #[test]
    fn escapes_markup_in_code() {
        let config = Config::default();
        let theme = highlight::theme(&config.theme);
        let lines = highlight::highlight("if a < b && c > d {}\n", Some("rust"), Some("rs"), theme);
        let svg = build_svg(&Snapshot {
            lines: &lines,
            theme,
            title: None,
            config: &config,
        });
        assert!(svg.contains("&lt;"));
        assert!(!svg.contains("a < b"));
    }
}
