//! The LSP side: Zed asks for code actions on the current selection, ZedSnap
//! offers "📸 snapshot as PNG/SVG" and runs them with
//! `workspace/executeCommand`.
//!
//! This is the only way a Zed extension can learn the editor selection:
//! extensions cannot read the buffer, but a language server receives the
//! range of the code-action request.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::Result;
use lsp_server::{Connection, ErrorCode, Message, Notification, Request, RequestId, Response};
use lsp_types::{
    CodeActionParams, DidChangeTextDocumentParams, DidCloseTextDocumentParams,
    DidOpenTextDocumentParams, ExecuteCommandParams, InitializeParams, MessageType,
    Range as LspRange, ShowDocumentParams, ShowMessageParams, Url,
};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::config::Config;
use crate::snapshot::{self, OutputFormat, SnapRequest};

pub const COMMAND_PNG: &str = "zedsnap.png";
pub const COMMAND_SVG: &str = "zedsnap.svg";

struct Document {
    text: String,
    language_id: Option<String>,
}

#[derive(Deserialize)]
struct SnapArguments {
    uri: Url,
    range: LspRange,
}

pub fn run() -> Result<()> {
    let (connection, io_threads) = Connection::stdio();

    let capabilities = json!({
        "textDocumentSync": 1,
        "codeActionProvider": true,
        "executeCommandProvider": { "commands": [COMMAND_PNG, COMMAND_SVG] },
    });

    let initialize_params = connection.initialize(capabilities)?;
    let initialize_params: InitializeParams = serde_json::from_value(initialize_params)?;

    let config = Config::from_value(initialize_params.initialization_options.clone());
    let root = workspace_root(&initialize_params);

    let result = {
        let mut server = Server {
            connection: &connection,
            documents: HashMap::new(),
            config,
            root,
            next_request_id: 1_000_000,
        };
        server.main_loop()
    };

    // Drop the connection so the writer thread is not left waiting.
    drop(connection);
    io_threads.join()?;
    result
}

#[allow(deprecated)] // `root_uri` is deprecated LSP-wise but Zed still sends it.
fn workspace_root(params: &InitializeParams) -> Option<PathBuf> {
    params
        .workspace_folders
        .as_ref()
        .and_then(|folders| folders.first())
        .and_then(|folder| folder.uri.to_file_path().ok())
        .or_else(|| {
            params
                .root_uri
                .as_ref()
                .and_then(|uri| uri.to_file_path().ok())
        })
}

struct Server<'a> {
    connection: &'a Connection,
    documents: HashMap<Url, Document>,
    config: Config,
    root: Option<PathBuf>,
    next_request_id: i32,
}

impl Server<'_> {
    fn main_loop(&mut self) -> Result<()> {
        for message in &self.connection.receiver {
            match message {
                Message::Request(request) => {
                    // Per LSP the client follows a successful `shutdown` with an
                    // `exit` notification; keep serving until then (or until the
                    // client closes stdin).
                    if self.connection.handle_shutdown(&request)? {
                        continue;
                    }
                    self.handle_request(request)?;
                }
                Message::Notification(notification) => {
                    if notification.method.as_str() == "exit" {
                        // Nothing else will be read; exit promptly.
                        std::process::exit(0);
                    }
                    self.handle_notification(notification);
                }
                // Responses to our own requests (showDocument).
                Message::Response(_) => {}
            }
        }
        Ok(())
    }

    fn handle_request(&mut self, request: Request) -> Result<()> {
        let response = match request.method.as_str() {
            "textDocument/codeAction" => match serde_json::from_value(request.params) {
                Ok(params) => Response::new_ok(request.id, self.code_actions(params)),
                Err(err) => invalid_params(request.id, err),
            },
            "workspace/executeCommand" => match serde_json::from_value(request.params) {
                Ok(params) => match self.execute_command(&params) {
                    Ok(path) => {
                        self.show_message(
                            MessageType::INFO,
                            format!("📸 ZedSnap: image saved to {}", path.display()),
                        );
                        self.show_document(&path);
                        Response::new_ok(request.id, Value::Null)
                    }
                    Err(err) => {
                        self.show_message(
                            MessageType::ERROR,
                            format!("ZedSnap failed: {err:#}"),
                        );
                        Response::new_err(
                            request.id,
                            ErrorCode::InternalError as i32,
                            format!("{err:#}"),
                        )
                    }
                },
                Err(err) => invalid_params(request.id, err),
            },
            other => Response::new_err(
                request.id,
                ErrorCode::MethodNotFound as i32,
                format!("unsupported method: {other}"),
            ),
        };
        self.connection.sender.send(Message::Response(response))?;
        Ok(())
    }

    fn handle_notification(&mut self, notification: Notification) {
        match notification.method.as_str() {
            "textDocument/didOpen" => {
                if let Ok(params) =
                    serde_json::from_value::<DidOpenTextDocumentParams>(notification.params)
                {
                    let document = params.text_document;
                    self.documents.insert(
                        document.uri,
                        Document {
                            text: document.text,
                            language_id: Some(document.language_id),
                        },
                    );
                }
            }
            "textDocument/didChange" => {
                if let Ok(params) =
                    serde_json::from_value::<DidChangeTextDocumentParams>(notification.params)
                {
                    // Full sync: the last change carries the whole text.
                    if let Some(change) = params.content_changes.into_iter().last() {
                        let uri = params.text_document.uri;
                        match self.documents.get_mut(&uri) {
                            Some(document) => document.text = change.text,
                            None => {
                                self.documents.insert(
                                    uri,
                                    Document {
                                        text: change.text,
                                        language_id: None,
                                    },
                                );
                            }
                        }
                    }
                }
            }
            "textDocument/didClose" => {
                if let Ok(params) =
                    serde_json::from_value::<DidCloseTextDocumentParams>(notification.params)
                {
                    self.documents.remove(&params.text_document.uri);
                }
            }
            _ => {}
        }
    }

    fn code_actions(&self, params: CodeActionParams) -> Value {
        let target = if params.range.start == params.range.end {
            "file"
        } else {
            "selection"
        };
        let arguments = json!([{ "uri": params.text_document.uri, "range": params.range }]);
        json!([
            {
                "title": format!("📸 ZedSnap: snapshot of {target} as PNG"),
                "command": {
                    "title": "Snapshot as PNG",
                    "command": COMMAND_PNG,
                    "arguments": arguments,
                },
            },
            {
                "title": format!("📸 ZedSnap: snapshot of {target} as SVG"),
                "command": {
                    "title": "Snapshot as SVG",
                    "command": COMMAND_SVG,
                    "arguments": arguments,
                },
            },
        ])
    }

    fn execute_command(&mut self, params: &ExecuteCommandParams) -> Result<PathBuf> {
        let format = match params.command.as_str() {
            COMMAND_PNG => OutputFormat::Png,
            COMMAND_SVG => OutputFormat::Svg,
            other => anyhow::bail!("unknown command: {other}"),
        };
        let argument = params
            .arguments
            .first()
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("missing command arguments"))?;
        let SnapArguments { uri, range } = serde_json::from_value(argument)?;

        let (text, language_id) = self.document_text(&uri)?;
        let file_path = uri.to_file_path().ok();
        let file_name = file_path
            .as_ref()
            .and_then(|path| path.file_name())
            .map(|name| name.to_string_lossy().into_owned());
        let extension = file_path
            .as_ref()
            .and_then(|path| path.extension())
            .map(|ext| ext.to_string_lossy().into_owned());
        let stem = snapshot::output_stem(file_name.as_deref());

        let default_dir = self.root.clone().or_else(|| {
            file_path
                .as_ref()
                .and_then(|path| path.parent().map(Path::to_path_buf))
        });

        let request = SnapRequest {
            text: &text,
            language_id: language_id.as_deref(),
            extension: extension.as_deref(),
            file_name,
            lines: line_span(&range),
            format,
        };
        let bytes = snapshot::render(&request, &self.config)?;
        let dir = snapshot::output_dir(&self.config, default_dir.as_deref());
        snapshot::save(&bytes, &dir, &stem, format)
    }

    fn document_text(&self, uri: &Url) -> Result<(String, Option<String>)> {
        if let Some(document) = self.documents.get(uri) {
            return Ok((document.text.clone(), document.language_id.clone()));
        }
        let path = uri
            .to_file_path()
            .map_err(|()| anyhow::anyhow!("could not convert {uri} into a local path"))?;
        let text = std::fs::read_to_string(&path)?;
        Ok((text, None))
    }

    fn show_message(&self, typ: MessageType, message: String) {
        let notification = Notification::new(
            "window/showMessage".to_string(),
            ShowMessageParams { typ, message },
        );
        self.connection
            .sender
            .send(Message::Notification(notification))
            .ok();
    }

    /// Asks the client to open the snapshot (Zed shows it in its image
    /// viewer). Harmless if the client ignores it.
    fn show_document(&mut self, path: &Path) {
        let Ok(uri) = Url::from_file_path(path) else {
            return;
        };
        self.next_request_id += 1;
        let request = Request::new(
            RequestId::from(self.next_request_id),
            "window/showDocument".to_string(),
            ShowDocumentParams {
                uri,
                external: Some(false),
                take_focus: Some(true),
                selection: None,
            },
        );
        self.connection.sender.send(Message::Request(request)).ok();
    }
}

/// Converts an LSP range into an inclusive line span (0-based) expanded to
/// whole lines. An empty selection means "whole file".
fn line_span(range: &LspRange) -> Option<(usize, usize)> {
    if range.start == range.end {
        return None;
    }
    let start = range.start.line as usize;
    let mut end = range.end.line as usize;
    // A selection ending at column 0 does not include that line.
    if range.end.character == 0 && end > start {
        end -= 1;
    }
    Some((start, end))
}

fn invalid_params(id: RequestId, err: serde_json::Error) -> Response {
    Response::new_err(id, ErrorCode::InvalidParams as i32, err.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use lsp_types::{Position, Range};

    #[test]
    fn empty_selection_means_whole_file() {
        let range = Range::new(Position::new(3, 1), Position::new(3, 1));
        assert_eq!(line_span(&range), None);
    }

    #[test]
    fn selection_is_expanded_to_whole_lines() {
        let range = Range::new(Position::new(3, 4), Position::new(5, 2));
        assert_eq!(line_span(&range), Some((3, 5)));
    }

    #[test]
    fn selection_ending_at_column_zero_excludes_last_line() {
        let range = Range::new(Position::new(3, 4), Position::new(5, 0));
        assert_eq!(line_span(&range), Some((3, 4)));
    }
}
