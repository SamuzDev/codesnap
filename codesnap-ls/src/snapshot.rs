//! Orchestration: slices the requested lines, highlights and builds the SVG.
//! Rasterizing (for the clipboard image) lives here too.

use anyhow::{bail, Context, Result};
use resvg::{tiny_skia, usvg};

use crate::config::Config;
use crate::highlight;
use crate::render::{self, Snapshot};

/// JetBrains Mono is embedded so the image looks identical on every machine,
/// even with no monospace fonts installed. (OFL, see `assets/fonts/OFL.txt`.)
const FONT_REGULAR: &[u8] = include_bytes!("../assets/fonts/JetBrainsMono-Regular.ttf");
const FONT_BOLD: &[u8] = include_bytes!("../assets/fonts/JetBrainsMono-Bold.ttf");
const FONT_ITALIC: &[u8] = include_bytes!("../assets/fonts/JetBrainsMono-Italic.ttf");

pub struct SnapRequest<'a> {
    /// Full document text.
    pub text: &'a str,
    /// Language id sent by Zed (e.g. `rust`).
    pub language_id: Option<&'a str>,
    /// File extension, used to detect the syntax.
    pub extension: Option<&'a str>,
    /// File name, shown in the window title.
    pub file_name: Option<String>,
    /// Line span (0-based, inclusive). `None` = whole file.
    pub lines: Option<(usize, usize)>,
}

/// Builds the snapshot SVG (nothing touches the clipboard or the disk yet).
pub fn render_svg(request: &SnapRequest, config: &Config) -> Result<String> {
    let code = match request.lines {
        Some((start, end)) => slice_lines(request.text, start, end),
        None => request.text.to_string(),
    };
    let code = normalize(&code, config.tab_width);
    if code.trim().is_empty() {
        bail!("the selection is empty: select some code first");
    }

    let theme = highlight::theme(&config.theme);
    let lines = highlight::highlight(
        &code,
        request.language_id,
        request.extension,
        theme,
    );

    let title = config
        .window_title
        .clone()
        .or_else(|| request.file_name.clone());
    Ok(render::build_svg(&Snapshot {
        lines: &lines,
        theme,
        title: title.as_deref(),
        config,
    }))
}

/// Rasterizes the SVG into a pixmap with `resvg`, using only the embedded
/// fonts. The result is RGBA (premultiplied), ready for the clipboard.
pub fn rasterize(svg: &str, scale: f32) -> Result<tiny_skia::Pixmap> {
    let scale = if scale.is_finite() && scale > 0.0 {
        scale.clamp(0.25, 8.0)
    } else {
        2.0
    };

    let mut options = usvg::Options::default();
    {
        let fontdb = options.fontdb_mut();
        fontdb.load_font_data(FONT_REGULAR.to_vec());
        fontdb.load_font_data(FONT_BOLD.to_vec());
        fontdb.load_font_data(FONT_ITALIC.to_vec());
        fontdb.set_monospace_family("JetBrains Mono");
    }
    options.font_family = "JetBrains Mono".to_string();
    // High-quality anti-aliased shapes and precise glyph placement.
    options.shape_rendering = usvg::ShapeRendering::GeometricPrecision;
    options.text_rendering = usvg::TextRendering::GeometricPrecision;

    let tree = usvg::Tree::from_str(svg, &options).context("could not parse the SVG")?;
    let size = tree.size();
    let width = ((size.width() * scale).ceil() as u32).max(1);
    let height = ((size.height() * scale).ceil() as u32).max(1);

    let mut pixmap =
        tiny_skia::Pixmap::new(width, height).context("could not allocate the image")?;
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    Ok(pixmap)
}

/// Converts a premultiplied pixmap into straight-alpha RGBA for the clipboard.
pub fn unpremultiply(pixmap: &tiny_skia::Pixmap) -> Vec<u8> {
    let mut data = pixmap.data().to_vec();
    for pixel in data.as_chunks_mut::<4>().0 {
        let alpha = pixel[3];
        if alpha == 255 || alpha == 0 {
            if alpha == 0 {
                pixel[0] = 0;
                pixel[1] = 0;
                pixel[2] = 0;
            }
            continue;
        }
        let scale = 255.0 / f32::from(alpha);
        for channel in &mut pixel[..3] {
            *channel = (f32::from(*channel) * scale).round().clamp(0.0, 255.0) as u8;
        }
    }
    data
}

/// Owned straight-alpha RGBA data + dimensions, for `arboard::set_image`.
pub fn clipboard_image_owned(pixmap: &tiny_skia::Pixmap) -> (usize, usize, Vec<u8>) {
    (
        pixmap.width() as usize,
        pixmap.height() as usize,
        unpremultiply(pixmap),
    )
}

/// Returns the requested line span (0-based, inclusive).
fn slice_lines(text: &str, start: usize, end: usize) -> String {
    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    if lines.is_empty() {
        return String::new();
    }
    let start = start.min(lines.len() - 1);
    let end = end.clamp(start, lines.len() - 1);
    lines[start..=end].concat()
}

/// Expands tabs, strips the common indentation (so nested selections sit
/// flush) and drops blank lines at the edges.
fn normalize(code: &str, tab_width: usize) -> String {
    let expanded: Vec<String> = code
        .lines()
        .map(|line| expand_tabs(line, tab_width))
        .collect();

    let min_indent = expanded
        .iter()
        .filter(|line| !line.trim().is_empty())
        .map(|line| line.chars().take_while(|ch| *ch == ' ').count())
        .min()
        .unwrap_or(0);

    let mut lines: Vec<String> = expanded
        .iter()
        .map(|line| {
            let indent = line.chars().take_while(|ch| *ch == ' ').count();
            line[min_indent.min(indent)..].trim_end().to_string()
        })
        .collect();

    while lines.first().is_some_and(|line| line.is_empty()) {
        lines.remove(0);
    }
    while lines.last().is_some_and(|line| line.is_empty()) {
        lines.pop();
    }
    lines.join("\n")
}

fn expand_tabs(line: &str, tab_width: usize) -> String {
    let tab_width = tab_width.max(1);
    let mut out = String::with_capacity(line.len());
    let mut column = 0usize;
    for ch in line.chars() {
        if ch == '\t' {
            let spaces = tab_width - column % tab_width;
            out.extend(std::iter::repeat_n(' ', spaces));
            column += spaces;
        } else {
            out.push(ch);
            column += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slices_inclusive_line_spans() {
        let text = "one\ntwo\nthree\n";
        assert_eq!(slice_lines(text, 1, 2), "two\nthree\n");
        assert_eq!(slice_lines(text, 0, 0), "one\n");
        assert_eq!(slice_lines(text, 0, 99), text);
    }

    #[test]
    fn normalizes_indentation_and_tabs() {
        let code = "\t\tfn main() {\n\t\t\tprintln!();\n\t\t}\n";
        assert_eq!(normalize(code, 4), "fn main() {\n    println!();\n}");
    }

    #[test]
    fn renders_an_svg() {
        let config = Config::default();
        let request = SnapRequest {
            text: "fn main() {\n    println!(\"hello\");\n}\n",
            language_id: Some("rust"),
            extension: Some("rs"),
            file_name: Some("main.rs".to_string()),
            lines: None,
        };
        let svg = render_svg(&request, &config).unwrap();
        assert!(svg.starts_with("<svg"));
        assert!(svg.contains(">fn<"));
        assert!(svg.contains(">main<"));
    }

    #[test]
    fn rasterizes_to_a_pixmap() {
        let config = Config::default();
        let request = SnapRequest {
            text: "print('hello')\n",
            language_id: Some("python"),
            extension: Some("py"),
            file_name: Some("hello.py".to_string()),
            lines: Some((0, 0)),
        };
        let svg = render_svg(&request, &config).unwrap();
        let pixmap = rasterize(&svg, config.scale).unwrap();
        assert!(pixmap.width() > 100);
        assert!(pixmap.height() > 50);
    }

    #[test]
    fn rejects_empty_selections() {
        let config = Config::default();
        let request = SnapRequest {
            text: "   \n\n",
            language_id: None,
            extension: None,
            file_name: None,
            lines: Some((0, 1)),
        };
        assert!(render_svg(&request, &config).is_err());
    }
}
