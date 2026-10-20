//! Syntax highlighting with syntect (the syntax definitions and themes ship
//! inside the crate itself, so it works offline and without system deps).

use std::io::Cursor;
use std::sync::OnceLock;

use syntect::easy::HighlightLines;
use syntect::highlighting::{Color, FontStyle, Style, Theme, ThemeSet};
use syntect::parsing::{SyntaxReference, SyntaxSet};
use syntect::util::LinesWithEndings;

use crate::config::DEFAULT_THEME;

/// One highlighted line: pairs of (style, text).
pub type StyledLine = Vec<(Style, String)>;

/// Bundled extra themes (beyond syntect's defaults).
const DRACULA_TMTHEME: &[u8] = include_bytes!("../assets/themes/Dracula.tmTheme");

pub fn syntax_set() -> &'static SyntaxSet {
    static SET: OnceLock<SyntaxSet> = OnceLock::new();
    SET.get_or_init(SyntaxSet::load_defaults_newlines)
}

/// syntect defaults plus the themes bundled in `assets/themes/`.
fn theme_set() -> &'static ThemeSet {
    static SET: OnceLock<ThemeSet> = OnceLock::new();
    SET.get_or_init(|| {
        let mut set = ThemeSet::load_defaults();
        if let Ok(theme) = ThemeSet::load_from_reader(&mut Cursor::new(DRACULA_TMTHEME)) {
            set.themes.insert("dracula".to_string(), theme);
        }
        set
    })
}

pub fn available_themes() -> Vec<String> {
    let mut names: Vec<String> = theme_set().themes.keys().cloned().collect();
    names.sort();
    names
}

pub fn theme(name: &str) -> &'static Theme {
    let themes = theme_set();
    themes
        .themes
        .get(name)
        .or_else(|| themes.themes.get(DEFAULT_THEME))
        .unwrap_or_else(|| themes.themes.values().next().expect("syntect loaded no themes"))
}

/// Highlights `code` line by line.
///
/// Detection order: file extension, language id sent by Zed, first line
/// (shebangs) and, if nothing matches, plain text.
pub fn highlight(
    code: &str,
    language_id: Option<&str>,
    extension: Option<&str>,
    theme: &Theme,
) -> Vec<StyledLine> {
    let set = syntax_set();
    let first_line = code.lines().next().unwrap_or("");
    let syntax = find_syntax(language_id, extension, first_line);
    let mut highlighter = HighlightLines::new(syntax, theme);
    let plain_foreground = theme
        .settings
        .foreground
        .unwrap_or(Color { r: 200, g: 200, b: 200, a: 255 });

    let mut lines = Vec::new();
    for line in LinesWithEndings::from(code) {
        let spans = match highlighter.highlight_line(line, set) {
            Ok(ranges) => ranges
                .into_iter()
                .map(|(style, text)| (style, strip_newline(text)))
                .collect(),
            // If the grammar fails (patterns too complex) we do not want to
            // abort: paint the line with the plain foreground color.
            Err(_) => vec![(
                Style {
                    foreground: plain_foreground,
                    background: theme
                        .settings
                        .background
                        .unwrap_or(Color { r: 0, g: 0, b: 0, a: 255 }),
                    font_style: FontStyle::empty(),
                },
                strip_newline(line),
            )],
        };
        lines.push(spans);
    }
    lines
}

fn find_syntax(
    language_id: Option<&str>,
    extension: Option<&str>,
    first_line: &str,
) -> &'static SyntaxReference {
    let set = syntax_set();

    if let Some(extension) = extension {
        if let Some(syntax) = set.find_syntax_by_extension(extension) {
            return syntax;
        }
    }

    if let Some(language_id) = language_id {
        for token in tokens_for_language_id(language_id) {
            if let Some(syntax) = set.find_syntax_by_extension(token) {
                return syntax;
            }
            if let Some(syntax) = set.find_syntax_by_name(token) {
                return syntax;
            }
        }
        if let Some(syntax) = set.find_syntax_by_token(language_id) {
            return syntax;
        }
    }

    if let Some(syntax) = set.find_syntax_by_first_line(first_line) {
        return syntax;
    }

    set.find_syntax_plain_text()
}

/// Candidate syntax extensions/names for each known LSP language id.
fn tokens_for_language_id(language_id: &str) -> &'static [&'static str] {
    match language_id {
        "rust" => &["rs"],
        "python" => &["py", "python"],
        "javascript" | "javascriptreact" => &["js", "javascript"],
        "typescript" => &["ts", "typescript"],
        "typescriptreact" => &["tsx"],
        "shellscript" | "bash" | "sh" | "zsh" => &["sh", "bash", "shell"],
        "c" => &["c"],
        "cpp" | "c++" => &["cpp", "c++"],
        "csharp" => &["cs"],
        "go" => &["go"],
        "java" => &["java"],
        "ruby" => &["rb"],
        "php" => &["php"],
        "html" => &["html"],
        "css" => &["css"],
        "scss" => &["scss"],
        "json" => &["json"],
        "jsonc" => &["json"],
        "yaml" => &["yaml", "yml"],
        "toml" => &["toml"],
        "markdown" => &["md", "markdown"],
        "lua" => &["lua"],
        "haskell" => &["hs"],
        "scala" => &["scala"],
        "kotlin" => &["kt"],
        "swift" => &["swift"],
        "xml" => &["xml"],
        "sql" => &["sql"],
        "make" | "makefile" => &["make", "makefile"],
        "dockerfile" => &["dockerfile"],
        "graphql" => &["graphql"],
        "elixir" => &["ex", "elixir"],
        "erlang" => &["erl"],
        "clojure" => &["clj"],
        "zig" => &["zig"],
        "dart" => &["dart"],
        "r" => &["r"],
        "prisma" => &["prisma"],
        "vue" => &["vue"],
        "svelte" => &["svelte"],
        "astro" => &["astro"],
        "nix" => &["nix"],
        "ocaml" => &["ml", "ocaml"],
        "elm" => &["elm"],
        "gleam" => &["gleam"],
        "terraform" | "tf" => &["tf"],
        "diff" => &["diff"],
        "julia" => &["jl"],
        _ => &[],
    }
}

fn strip_newline(text: &str) -> String {
    text.strip_suffix("\r\n")
        .or_else(|| text.strip_suffix('\n'))
        .unwrap_or(text)
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_rust_syntax() {
        let theme = theme(DEFAULT_THEME);
        let lines = highlight(
            "fn main() { println!(\"hola\"); }\n",
            Some("rust"),
            Some("rs"),
            theme,
        );
        assert_eq!(lines.len(), 1);
        assert!(lines[0].len() > 1, "should produce multiple spans");
        let joined: String = lines[0].iter().map(|(_, text)| text.as_str()).collect();
        assert_eq!(joined, "fn main() { println!(\"hola\"); }");
    }

    #[test]
    fn falls_back_to_plain_text_for_unknown_languages() {
        let theme = theme(DEFAULT_THEME);
        let lines = highlight("sin resaltar\n", Some("language-inexistente"), None, theme);
        assert_eq!(lines.len(), 1);
    }

    #[test]
    fn lists_builtin_themes() {
        let themes = available_themes();
        assert!(themes.iter().any(|name| name == DEFAULT_THEME));
    }
}
