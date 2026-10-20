//! Full LSP session against the real `zedsnap-ls` binary:
//! initialize → didOpen → code actions → executeCommand → image written.

use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, ChildStdout, Command, Stdio};
use std::sync::mpsc::{self, Receiver, Sender, RecvTimeoutError};
use std::thread;
use std::time::Duration;

use lsp_types::Url;
use serde_json::{json, Value};

const SAMPLE: &str = "\
fn main() {
    let name = \"world\";
    println!(\"hello {name}\");
}
";

struct Client {
    child: Child,
    stdin: std::process::ChildStdin,
    messages: Receiver<Value>,
    next_id: i32,
}

impl Client {
    fn start() -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_zedsnap-ls"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("could not spawn zedsnap-ls");
        let stdout = child.stdout.take().expect("stdout captured");
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || read_messages(stdout, tx));

        Self {
            stdin: child.stdin.take().expect("stdin captured"),
            child,
            messages: rx,
            next_id: 0,
        }
    }

    fn send(&mut self, message: Value) {
        let body = message.to_string();
        write!(self.stdin, "Content-Length: {}\r\n\r\n{body}", body.len()).unwrap();
        self.stdin.flush().unwrap();
    }

    fn recv(&self) -> Value {
        match self.messages.recv_timeout(Duration::from_secs(60)) {
            Ok(message) => message,
            Err(RecvTimeoutError::Timeout) => panic!("timed out waiting for server messages"),
            Err(RecvTimeoutError::Disconnected) => panic!("the server exited unexpectedly"),
        }
    }

    fn notify(&mut self, method: &str, params: Value) {
        self.send(json!({ "jsonrpc": "2.0", "method": method, "params": params }));
    }

    /// Sends a request and returns (result, other messages seen while we
    /// waited for the response).
    fn request(&mut self, method: &str, params: Value) -> (Value, Vec<Value>) {
        self.next_id += 1;
        let id = self.next_id;
        self.send(json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }));

        let mut others = Vec::new();
        loop {
            let message = self.recv();
            let is_ours = message.get("id").and_then(Value::as_i64) == Some(id as i64)
                && message.get("method").is_none();
            if is_ours {
                if let Some(error) = message.get("error") {
                    panic!("the server returned an error for {method}: {error}");
                }
                return (message["result"].clone(), others);
            }
            others.push(message);
        }
    }

    fn shutdown(mut self) {
        self.request("shutdown", Value::Null);
        self.notify("exit", Value::Null);
        // Close stdin as well so the server's reader thread sees EOF even if
        // the exit notification somehow does not arrive first.
        let Client {
            mut child, stdin, ..
        } = self;
        drop(stdin);
        child.wait().expect("the server did not exit");
    }
}

fn read_messages(stdout: ChildStdout, tx: Sender<Value>) {
    let mut reader = BufReader::new(stdout);
    loop {
        match read_message(&mut reader) {
            Ok(Some(message)) => {
                if tx.send(message).is_err() {
                    return;
                }
            }
            Ok(None) | Err(_) => return,
        }
    }
}

fn read_message(reader: &mut BufReader<ChildStdout>) -> std::io::Result<Option<Value>> {
    let mut content_length: Option<usize> = None;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 {
            return Ok(None);
        }
        let line = line.trim_end();
        if line.is_empty() {
            break;
        }
        if let Some((key, value)) = line.split_once(':') {
            if key.eq_ignore_ascii_case("content-length") {
                content_length = value.trim().parse().ok();
            }
        }
    }

    let len = content_length
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidData, "missing Content-Length"))?;
    let mut buffer = vec![0_u8; len];
    reader.read_exact(&mut buffer)?;
    Ok(Some(serde_json::from_slice(&buffer)?))
}

fn titles(actions: &Value) -> Vec<String> {
    actions
        .as_array()
        .expect("code actions must be a list")
        .iter()
        .filter_map(|action| action["title"].as_str().map(str::to_string))
        .collect()
}

fn temp_root(name: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("zedsnap-test-{}-{name}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    root
}

fn initialize(client: &mut Client, root: &std::path::Path, config: Value) {
    let (result, _) = client.request(
        "initialize",
        json!({
            "processId": null,
            "rootUri": Url::from_file_path(root).unwrap(),
            "capabilities": {},
            "initializationOptions": config,
        }),
    );
    assert_eq!(
        result["capabilities"]["codeActionProvider"],
        json!(true),
        "the server must advertise code actions: {result}"
    );
    assert!(
        result["capabilities"]["executeCommandProvider"]["commands"]
            .as_array()
            .is_some_and(|commands| commands.len() == 3),
        "the server must advertise all three snapshot commands"
    );
    client.notify("initialized", json!({}));
}

#[test]
fn snapshots_a_selection_to_png() {
    let root = temp_root("selection");
    let uri = Url::from_file_path(root.join("main.rs")).unwrap();
    let mut client = Client::start();

    initialize(
        &mut client,
        &root,
        json!({ "lineNumbers": true, "outputDir": root.to_str().unwrap() }),
    );
    client.notify(
        "textDocument/didOpen",
        json!({
            "textDocument": {
                "uri": uri,
                "languageId": "rust",
                "version": 1,
                "text": SAMPLE,
            }
        }),
    );

    // With a selection → the action targets the selection.
    let selection = json!({
        "start": { "line": 1, "character": 4 },
        "end": { "line": 2, "character": 0 },
    });
    let (actions, _) = client.request(
        "textDocument/codeAction",
        json!({
            "textDocument": { "uri": uri },
            "range": selection,
            "context": { "diagnostics": [] },
        }),
    );
    let titles = titles(&actions);
    assert_eq!(titles.len(), 3, "{titles:?}");
    assert!(
        titles.iter().any(|t| t.contains("selection") && t.contains("PNG")),
        "missing the selection action: {titles:?}"
    );
    assert!(
        titles.iter().any(|t| t.contains("JPG")),
        "missing the JPG action: {titles:?}"
    );

    // Run the snapshot.
    let (result, others) = client.request(
        "workspace/executeCommand",
        json!({
            "command": "zedsnap.png",
            "arguments": [{ "uri": uri, "range": selection }],
        }),
    );
    assert!(result.is_null());

    let toast = others
        .iter()
        .find(|m| m["method"] == "window/showMessage")
        .expect("should notify with a toast");
    assert!(
        toast["params"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("saved")),
        "{toast}"
    );

    let shown = others
        .iter()
        .find(|m| m["method"] == "window/showDocument")
        .expect("should ask to open the image");
    let shown_path = Url::parse(shown["params"]["uri"].as_str().unwrap())
        .unwrap()
        .to_file_path()
        .unwrap();
    assert!(shown_path.starts_with(&root), "{shown_path:?}");

    let bytes = fs::read(&shown_path).unwrap();
    assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
    assert!(bytes.len() > 1_000, "the PNG looks too small");

    client.shutdown();
}

#[test]
fn snapshots_the_whole_file_to_svg() {
    let root = temp_root("file");
    let uri = Url::from_file_path(root.join("main.rs")).unwrap();
    let mut client = Client::start();

    initialize(&mut client, &root, json!({ "outputDir": root.to_str().unwrap() }));
    client.notify(
        "textDocument/didOpen",
        json!({
            "textDocument": {
                "uri": uri,
                "languageId": "rust",
                "version": 1,
                "text": SAMPLE,
            }
        }),
    );

    // Cursor only (no selection) → the action targets the whole file.
    let cursor = json!({
        "start": { "line": 0, "character": 0 },
        "end": { "line": 0, "character": 0 },
    });
    let (actions, _) = client.request(
        "textDocument/codeAction",
        json!({
            "textDocument": { "uri": uri },
            "range": cursor,
            "context": { "diagnostics": [] },
        }),
    );
    let titles = titles(&actions);
    assert!(
        titles.iter().any(|t| t.contains("file") && t.contains("SVG")),
        "missing the whole-file action: {titles:?}"
    );

    let (result, others) = client.request(
        "workspace/executeCommand",
        json!({
            "command": "zedsnap.svg",
            "arguments": [{ "uri": uri, "range": cursor }],
        }),
    );
    assert!(result.is_null());

    let shown = others
        .iter()
        .find(|m| m["method"] == "window/showDocument")
        .expect("should ask to open the image");
    let shown_path = Url::parse(shown["params"]["uri"].as_str().unwrap())
        .unwrap()
        .to_file_path()
        .unwrap();
    let svg = fs::read_to_string(&shown_path).unwrap();
    assert!(svg.starts_with("<svg"), "not an SVG: {}", &svg[..svg.len().min(40)]);
    // Highlighted tokens are separate text nodes, so check the pieces.
    assert!(svg.contains(">fn<"));
    assert!(svg.contains(">main<"));

    client.shutdown();
}
