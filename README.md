# ZedSnap

📸 Polaroid-style screenshots of your code for [Zed](https://zed.dev) — select
some code (or nothing for the whole file), hit `Ctrl+.` and pick
**ZedSnap: snapshot of selection as PNG**. ZedSnap writes a retina-sharp
image (PNG, SVG or JPG) and opens it in Zed's image viewer.

Inspired by Polacode and CodeSnap.

## How to use

1. Select the code you want to capture (leave the cursor alone for the whole
   file).
2. Open the code-action menu: `Ctrl+.` (`Cmd+.` on macOS).
3. Choose `📸 ZedSnap: snapshot of selection as PNG` (or `... as SVG` /
   `... as JPG`).

The image is saved as `zedsnap-<file>-<timestamp>.<ext>` in the project root
and opened automatically.

**Formats:** PNG (lossless, best quality), SVG (vector, scales forever) and
JPG (lossy, smallest files for sharing).

## Install

ZedSnap has two parts:

- The **extension** (WebAssembly) that registers the language server.
- The **`zedsnap-ls` binary** that actually renders the images.

### Prebuilt binaries (Linux & Windows)

The extension downloads the right `zedsnap-ls` binary from the
[GitHub releases](https://github.com/SamuzDev/zedsnap/releases)
automatically:

| Asset | Platform |
| --- | --- |
| `zedsnap-ls-x86_64-linux.tar.gz` | Linux x86_64 (static, any distro) |
| `zedsnap-ls-aarch64-linux.tar.gz` | Linux aarch64 |
| `zedsnap-ls-x86_64-windows.zip` | Windows x86_64 |

macOS: build from source (`cargo install --path zedsnap-ls`) until macOS
binaries are published.

### From source (development)

```bash
# 1. Install the language server so it is on your $PATH
cargo install --path zedsnap-ls

# 2. In Zed: command palette → "dev: install dev extension"
#    and pick this folder. Zed compiles the WASM for you.
```

Reload the window afterwards. If Zed cannot find the binary on its own, point
at it explicitly in `settings.json`:

```json
{
  "lsp": {
    "zedsnap-ls": {
      "binary": {
        "path": "/home/you/.cargo/bin/zedsnap-ls"
      }
    }
  }
}
```

## Settings

All options go under `lsp.zedsnap-ls.initialization_options` in
`settings.json`:

```json
{
  "lsp": {
    "zedsnap-ls": {
      "initialization_options": {
        "theme": "base16-ocean.dark",
        "background": "#667eea,#764ba2",
        "padding": 48,
        "fontSize": 14,
        "lineNumbers": false,
        "windowControls": true,
        "windowTitle": null,
        "shadow": true,
        "cornerRadius": 12,
        "scale": 2,
        "jpegQuality": 90,
        "tabWidth": 4,
        "outputDir": null
      }
    }
  }
}
```

| Option | Default | Description |
| --- | --- | --- |
| `theme` | `base16-ocean.dark` | syntect theme used for highlighting (run `zedsnap-ls --themes` to list them). |
| `background` | `#667eea,#764ba2` | Backdrop: one color, a comma-separated list (diagonal gradient) or `transparent`. |
| `padding` | `48` | Margin around the window, in pixels. |
| `fontSize` | `14` | Code font size, in pixels. |
| `lineNumbers` | `false` | Show the line-number gutter. |
| `windowControls` | `true` | Show the macOS-style traffic lights. |
| `windowTitle` | file name | Title shown in the window's title bar. |
| `shadow` | `true` | Drop shadow under the window. |
| `cornerRadius` | `12` | Window corner radius. |
| `scale` | `2` | PNG scale factor (2 = retina-sharp). |
| `jpegQuality` | `90` | JPEG quality (50–100). |
| `tabWidth` | `4` | Columns a tab character occupies. |
| `outputDir` | project root | Output folder (`~/` is expanded). |

## Supported languages

Rust, Python, JavaScript/TypeScript (+ React/TSX), Go, C/C++/C#, Java, Ruby,
PHP, HTML, CSS/SCSS, JSON/JSONC, YAML, TOML, Markdown, SQL, Shell, Lua,
Haskell, Scala, Kotlin, Swift, XML, GraphQL, Elixir, Erlang, Clojure, Zig,
Dart, R, Prisma, Vue, Svelte, Astro, Nix, OCaml, Elm, Gleam, Terraform, Diff,
Julia and Dockerfile.

## Architecture

Zed extensions cannot read the buffer or the editor selection. A language
server, however, receives the range of every `textDocument/codeAction`
request. So:

1. The WASM extension registers `zedsnap-ls` for every supported language and
   tells Zed how to launch the binary.
2. When you ask for code actions, Zed sends your selection to `zedsnap-ls`.
3. The server answers with the two ZedSnap actions; running one renders the
   SVG (highlighted with [syntect]), rasterizes it with [resvg] at 2x using
   the embedded JetBrains Mono, saves it and asks Zed to open it via
   `window/showDocument`.

[syntect]: https://crates.io/crates/syntect
[resvg]: https://crates.io/crates/resvg

## Development

```bash
cargo test                          # unit + integration tests
cargo test --test preview           # writes target/zedsnap-preview.png
cargo install --path zedsnap-ls     # rebuild + reinstall the server
```

The preview test renders a sample file so you can inspect the output without
opening Zed.

## License

[MIT](LICENSE). The embedded JetBrains Mono fonts are licensed under the SIL
Open Font License 1.1 (see [`zedsnap-ls/assets/fonts/OFL.txt`](zedsnap-ls/assets/fonts/OFL.txt)).
