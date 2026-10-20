//! CodeSnap — 📸 polaroid shots of your code in Zed.
//!
//! This is the WebAssembly side of the extension. Zed compiles it to
//! `wasm32-wasip2` and asks it how to launch the `codesnap-ls` language
//! server, which is what actually generates the images (Zed extensions cannot
//! read the editor selection, but a language server receives the range of
//! every code-action request).
//!
//! Binary resolution order:
//!   1. `lsp.codesnap-ls.binary.path` in `settings.json`
//!   2. `codesnap-ls` on the `$PATH` (e.g. `cargo install --path codesnap-ls`)
//!   3. A previously downloaded binary (cached per version)
//!   4. A pre-built binary from the GitHub releases

use std::fs;
use zed_extension_api::settings::LspSettings;
use zed_extension_api::{self as zed, LanguageServerId, Result};

/// GitHub repository that hosts the `codesnap-ls` binaries.
const GITHUB_REPO: &str = "SamuzDev/codesnap";
const SERVER_BINARY: &str = "codesnap-ls";

struct CodeSnapExtension {
    downloaded_binary_path: Option<String>,
}

impl CodeSnapExtension {
    fn language_server_binary_path(
        &mut self,
        language_server_id: &LanguageServerId,
        worktree: &zed::Worktree,
    ) -> Result<String> {
        // 1. Explicit path in the settings.
        if let Ok(lsp_settings) = LspSettings::for_worktree(SERVER_BINARY, worktree) {
            if let Some(path) = lsp_settings.binary.and_then(|binary| binary.path) {
                return Ok(path);
            }
        }

        // 2. Binary on the $PATH.
        if let Some(path) = worktree.which(SERVER_BINARY) {
            return Ok(path);
        }

        // 3. Binary downloaded in a previous session.
        if let Some(path) = &self.downloaded_binary_path {
            if fs::metadata(path).is_ok_and(|stat| stat.is_file()) {
                return Ok(path.clone());
            }
        }

        // 4. Download a pre-built binary from the GitHub releases.
        zed::set_language_server_installation_status(
            language_server_id,
            &zed::LanguageServerInstallationStatus::CheckingForUpdate,
        );
        let release = zed::latest_github_release(
            GITHUB_REPO,
            zed::GithubReleaseOptions {
                require_assets: true,
                pre_release: false,
            },
        )
        .map_err(|err| {
            format!(
                "no release of {SERVER_BINARY} was found: {err}. \
                 Install it manually with \
                 `cargo install --path codesnap/codesnap-ls` so it is on your $PATH."
            )
        })?;

        let (platform, arch) = zed::current_platform();
        let asset_name = format!(
            "{SERVER_BINARY}-{arch}-{os}.{ext}",
            arch = match arch {
                zed::Architecture::Aarch64 => "aarch64",
                zed::Architecture::X86 => "x86",
                zed::Architecture::X8664 => "x86_64",
            },
            os = match platform {
                zed::Os::Mac => "macos",
                zed::Os::Linux => "linux",
                zed::Os::Windows => "windows",
            },
            ext = match platform {
                zed::Os::Windows => "zip",
                _ => "tar.gz",
            },
        );

        let asset = release
            .assets
            .iter()
            .find(|asset| asset.name == asset_name)
            .ok_or_else(|| format!("no release asset named {asset_name:?} was found"))?;

        let version_dir = format!("{SERVER_BINARY}-{}", release.version);
        let binary_path = format!(
            "{version_dir}/{SERVER_BINARY}{suffix}",
            suffix = match platform {
                zed::Os::Windows => ".exe",
                _ => "",
            },
        );

        if !fs::metadata(&binary_path).is_ok_and(|stat| stat.is_file()) {
            zed::set_language_server_installation_status(
                language_server_id,
                &zed::LanguageServerInstallationStatus::Downloading,
            );

            zed::download_file(
                &asset.download_url,
                &version_dir,
                match platform {
                    zed::Os::Windows => zed::DownloadedFileType::Zip,
                    _ => zed::DownloadedFileType::GzipTar,
                },
            )
            .map_err(|err| format!("could not download {asset_name}: {err}"))?;

            zed::make_file_executable(&binary_path)?;

            zed::set_language_server_installation_status(
                language_server_id,
                &zed::LanguageServerInstallationStatus::None,
            );

            // Clean up binaries from previous versions.
            if let Ok(entries) = fs::read_dir(".") {
                for entry in entries.flatten() {
                    let name = entry.file_name();
                    let name = name.to_string_lossy();
                    if name.starts_with(SERVER_BINARY) && name != version_dir.as_str() {
                        fs::remove_dir_all(entry.path()).ok();
                    }
                }
            }
        }

        self.downloaded_binary_path = Some(binary_path.clone());
        Ok(binary_path)
    }
}

impl zed::Extension for CodeSnapExtension {
    fn new() -> Self {
        Self {
            downloaded_binary_path: None,
        }
    }

    fn language_server_command(
        &mut self,
        language_server_id: &LanguageServerId,
        worktree: &zed::Worktree,
    ) -> Result<zed::Command> {
        let args = LspSettings::for_worktree(SERVER_BINARY, worktree)
            .ok()
            .and_then(|settings| settings.binary)
            .and_then(|binary| binary.arguments)
            .unwrap_or_default();

        Ok(zed::Command {
            command: self.language_server_binary_path(language_server_id, worktree)?,
            args,
            env: Default::default(),
        })
    }

    fn language_server_initialization_options(
        &mut self,
        _language_server_id: &LanguageServerId,
        worktree: &zed::Worktree,
    ) -> Result<Option<zed::serde_json::Value>> {
        Ok(LspSettings::for_worktree(SERVER_BINARY, worktree)
            .ok()
            .and_then(|settings| settings.initialization_options))
    }
}

zed::register_extension!(CodeSnapExtension);
