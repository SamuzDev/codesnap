# CodeSnap

📸 Polaroid-style screenshots of your code for [Zed](https://zed.dev) — select
some code (or nothing for the whole file), hit `Ctrl+.` and pick
**CodeSnap: copy selection as PNG** (or **save selection as PNG…**). CodeSnap
copies a retina-sharp image (PNG) or the SVG source to your clipboard, or opens
a save dialog so you choose where to store it.

Inspired by Polacode and the original CodeSnap.

## How to use

1. Select the code you want to capture (leave the cursor alone for the whole
   file).
2. Open the code-action menu: `Ctrl+.` (`Cmd+.` on macOS).
3. Choose one of the four actions:

   | Action | What it does |
   | --- | --- |
   | `copy selection as PNG` | Raster image → clipboard |
   | `copy selection as SVG` | Vector source → clipboard |
   | `save selection as PNG…` | Native save dialog → `.png` file |
   | `save selection as SVG…` | Native save dialog → `.svg` file |

**Clipboard on Linux:** copying uses the system clipboard with X11 **and**
Wayland (data-control) support. If it still fails on your setup, use the
**save** actions instead.

## Install

CodeSnap has two parts:

- The **extension** (WebAssembly) that registers the language server.
- The **`codesnap-ls` binary** that actually renders the images.

### Prebuilt binaries (Linux & Windows)

The extension downloads the right `codesnap-ls` binary from the
[GitHub releases](https://github.com/SamuzDev/codesnap/releases)
automatically:

| Asset | Platform |
| --- | --- |
| `codesnap-ls-x86_64-linux.tar.gz` | Linux x86_64 (static, any distro) |
| `codesnap-ls-aarch64-linux.tar.gz` | Linux aarch64 |
| `codesnap-ls-x86_64-windows.zip` | Windows x86_64 |

macOS: build from source (`cargo install --path codesnap-ls`) until macOS
binaries are published.

### From source (development)

```bash
# 1. Install the language server so it is on your $PATH
cargo install --path codesnap-ls

# 2. In Zed: command palette → "dev: install dev extension"
#    and pick this folder. Zed compiles the WASM for you.
```

Reload the window afterwards. If Zed cannot find the binary on its own, point
at it explicitly in `settings.json`:

```json
{
  "lsp": {
    "codesnap-ls": {
      "binary": {
        "path": "/home/you/.cargo/bin/codesnap-ls"
      }
    }
  }
}
```

## Settings

All options go under `lsp.codesnap-ls.initialization_options` in
`settings.json`:

```json
{
  "lsp": {
    "codesnap-ls": {
      "initialization_options": {
        "theme": "dracula",
        "background": "#282a36,#1a1b26",
        "padding": 48,
        "fontSize": 14,
        "lineNumbers": false,
        "windowControls": true,
        "windowTitle": null,
        "shadow": true,
        "cornerRadius": 12,
        "scale": 2,
        "tabWidth": 4
      }
    }
  }
}
```

| Option | Default | Description |
| --- | --- | --- |
| `theme` | `dracula` | syntect theme used for highlighting (run `codesnap-ls --themes` to list them). |
| `background` | `#282a36,#1a1b26` | Backdrop: one color, a comma-separated list (diagonal gradient) or `transparent`. |
| `padding` | `48` | Margin around the window, in pixels. |
| `fontSize` | `14` | Code font size, in pixels. |
| `lineNumbers` | `false` | Show the line-number gutter. |
| `windowControls` | `true` | Show the macOS-style traffic lights. |
| `windowTitle` | file name | Title shown in the window's title bar. |
| `shadow` | `true` | Drop shadow under the window. |
| `cornerRadius` | `12` | Window corner radius. |
| `scale` | `2` | Image scale factor (2 = retina-sharp). |
| `tabWidth` | `4` | Columns a tab character occupies. |

## Supported languages

Rust, Python, JavaScript/TypeScript (+ React/TSX), Go, C/C++/C#, Java, Ruby,
PHP, HTML, CSS/SCSS, JSON/JSONC, YAML, TOML, Markdown, SQL, Shell, Lua,
Haskell, Scala, Kotlin, Swift, GraphQL, Elixir, Erlang, Clojure, Zig,
Dart, R, Prisma, Vue, Svelte, Astro, Nix, OCaml, Elm, Gleam, Terraform, Diff,
Julia and Dockerfile.

## Architecture

Zed extensions cannot read the buffer or the editor selection. A language
server, however, receives the range of every `textDocument/codeAction`
request. So:

1. The WASM extension registers `codesnap-ls` for every supported language and
   tells Zed how to launch the binary.
2. When you ask for code actions, Zed sends your selection to `codesnap-ls`.
3. The server answers with the four CodeSnap actions; running one renders the
   SVG (highlighted with [syntect]), rasterizes it with [resvg] at 2x using
   the embedded JetBrains Mono, and either copies it to the system clipboard
   or opens a native save dialog ([rfd]).

[syntect]: https://crates.io/crates/syntect
[resvg]: https://crates.io/crates/resvg
[rfd]: https://crates.io/crates/rfd

## Development

```bash
cargo test                          # unit + integration tests
cargo test --test preview           # writes target/codesnap-preview.png
cargo install --path codesnap-ls     # rebuild + reinstall the server
```

The preview test renders a sample file so you can inspect the output without
opening Zed.

## License

[MIT](LICENSE). The embedded JetBrains Mono fonts are licensed under the SIL
Open Font License 1.1 (see [`codesnap-ls/assets/fonts/OFL.txt`](codesnap-ls/assets/fonts/OFL.txt)).
