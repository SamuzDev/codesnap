//! Orchestration: slices the requested lines, highlights, renders and saves.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{bail, Context, Result};
use resvg::{tiny_skia, usvg};

use crate::config::Config;
use crate::highlight;
use crate::render::{self, Snapshot};

/// JetBrains Mono is embedded so the PNG looks identical on every machine,
/// even with no monospace fonts installed. (OFL, see `assets/fonts/OFL.txt`.)
const FONT_REGULAR: &[u8] = include_bytes!("../assets/fonts/JetBrainsMono-Regular.ttf");
const FONT_BOLD: &[u8] = include_bytes!("../assets/fonts/JetBrainsMono-Bold.ttf");
const FONT_ITALIC: &[u8] = include_bytes!("../assets/fonts/JetBrainsMono-Italic.ttf");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    Png,
    Svg,
    Jpg,
}

impl OutputFormat {
    pub fn extension(self) -> &'static str {
        match self {
            OutputFormat::Png => "png",
            OutputFormat::Svg => "svg",
            OutputFormat::Jpg => "jpg",
        }
    }
}

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
    pub format: OutputFormat,
}

/// Renders the snapshot and returns its bytes (nothing touches the disk yet).
pub fn render(request: &SnapRequest, config: &Config) -> Result<Vec<u8>> {
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
    let svg = render::build_svg(&Snapshot {
        lines: &lines,
        theme,
        title: title.as_deref(),
        config,
    });

    match request.format {
        OutputFormat::Svg => Ok(svg.into_bytes()),
        OutputFormat::Png => encode_png(&rasterize(&svg, config.scale)?),
        OutputFormat::Jpg => encode_jpeg(&rasterize(&svg, config.scale)?, config.jpeg_quality),
    }
}

/// Output folder: the user's, the project root, or the system temp dir.
pub fn output_dir(config: &Config, default_dir: Option<&Path>) -> PathBuf {
    if let Some(dir) = &config.output_dir {
        return expand_home(dir);
    }
    default_dir
        .map(Path::to_path_buf)
        .unwrap_or_else(std::env::temp_dir)
}

/// File-name stem: the file's stem or `snippet`.
pub fn output_stem(file_name: Option<&str>) -> String {
    file_name
        .and_then(|name| Path::new(name).file_stem())
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_else(|| "snippet".to_string())
}

/// Writes the bytes into `dir` under a unique name and returns the path.
pub fn save(
    bytes: &[u8],
    dir: &Path,
    stem: &str,
    format: OutputFormat,
) -> Result<PathBuf> {
    fs::create_dir_all(dir)
        .with_context(|| format!("could not create the folder {}", dir.display()))?;
    let path = dir.join(format!(
        "zedsnap-{stem}-{}.{}",
        timestamp(),
        format.extension()
    ));
    fs::write(&path, bytes).with_context(|| format!("could not write {}", path.display()))?;
    Ok(path)
}

/// Rasterizes the SVG into a pixmap with `resvg`, using only the embedded
/// fonts. The result is encoded afterwards as PNG or JPEG.
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

fn encode_png(pixmap: &tiny_skia::Pixmap) -> Result<Vec<u8>> {
    pixmap.encode_png().context("could not encode the PNG")
}

/// JPEG is lossy but much smaller — handy for sharing. JPEG has no alpha
/// channel, so the image is composited over white.
fn encode_jpeg(pixmap: &tiny_skia::Pixmap, quality: u8) -> Result<Vec<u8>> {
    let quality = quality.clamp(50, 100);
    let (width, height) = (pixmap.width(), pixmap.height());
    if width > u16::MAX as u32 || height > u16::MAX as u32 {
        bail!("image too large for JPEG ({width}x{height}): lower the scale");
    }

    let data = pixmap.data();
    let mut rgb = Vec::with_capacity(data.len() / 4 * 3);
    for pixel in data.chunks_exact(4) {
        // Premultiplied channels over white: out = premultiplied + (255 - a).
        let keep = 255 - pixel[3];
        rgb.push(pixel[0].saturating_add(keep));
        rgb.push(pixel[1].saturating_add(keep));
        rgb.push(pixel[2].saturating_add(keep));
    }

    let mut out = Vec::new();
    jpeg_encoder::Encoder::new(&mut out, quality)
        .encode(
            &rgb,
            width as u16,
            height as u16,
            jpeg_encoder::ColorType::Rgb,
        )
        .map_err(|err| anyhow::anyhow!("could not encode the JPEG: {err}"))?;
    Ok(out)
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

fn expand_home(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/") {
        let home = std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .ok();
        if let Some(home) = home {
            return PathBuf::from(home).join(rest);
        }
    }
    PathBuf::from(path)
}

/// `YYYYMMDD-HHMMSS` in UTC, without dragging in a date dependency.
fn timestamp() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    let (year, month, day) = civil_from_days((seconds / 86_400) as i64);
    let rem = seconds % 86_400;
    format!(
        "{year:04}{month:02}{day:02}-{:02}{:02}{:02}",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

/// Howard Hinnant's `civil_from_days` algorithm.
fn civil_from_days(days_since_epoch: i64) -> (i64, u32, u32) {
    let z = days_since_epoch + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let day_of_era = (z - era * 146_097) as u64;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era as i64 + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let mp = (5 * day_of_year + 2) / 153;
    let day = (day_of_year - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if month <= 2 { year + 1 } else { year }, month, day)
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
    fn renders_a_png() {
        let config = Config::default();
        let request = SnapRequest {
            text: "fn main() {\n    println!(\"hello\");\n}\n",
            language_id: Some("rust"),
            extension: Some("rs"),
            file_name: Some("main.rs".to_string()),
            lines: None,
            format: OutputFormat::Png,
        };
        let png = render(&request, &config).unwrap();
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
    }

    #[test]
    fn renders_an_svg() {
        let config = Config::default();
        let request = SnapRequest {
            text: "print('hello')\n",
            language_id: Some("python"),
            extension: Some("py"),
            file_name: Some("hello.py".to_string()),
            lines: Some((0, 0)),
            format: OutputFormat::Svg,
        };
        let svg = render(&request, &config).unwrap();
        let svg = String::from_utf8(svg).unwrap();
        assert!(svg.starts_with("<svg"));
    }

    #[test]
    fn renders_a_jpg() {
        let config = Config::default();
        let request = SnapRequest {
            text: "fn main() {}\n",
            language_id: Some("rust"),
            extension: Some("rs"),
            file_name: Some("main.rs".to_string()),
            lines: None,
            format: OutputFormat::Jpg,
        };
        let jpg = render(&request, &config).unwrap();
        assert_eq!(&jpg[..2], &[0xFF, 0xD8], "JPEG must start with the SOI marker");
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
            format: OutputFormat::Png,
        };
        assert!(render(&request, &config).is_err());
    }
}
