//! A language server client, for the script editor's completion, hover,
//! signature help, errors and go-to-definition -- from the servers VS Code
//! runs for its own: pyright or basedpyright for Python, rust-analyzer for
//! Rust.
//!
//! The Language Server Protocol is JSON-RPC over the server's stdin and
//! stdout, each message behind a `Content-Length` header. The server is a
//! process of its own, started at low priority so that a simulation never
//! waits on it. A thread reads what it writes and answers the questions it
//! puts to the client -- its settings, mostly, which pyright will not analyse
//! anything without -- and the UI takes the rest once a frame.
//!
//! Nothing on the UI thread waits on the server. A request goes out and its
//! reply is picked up in whichever frame it arrives; the editor matches
//! replies to its latest request of each kind and drops the others, so a
//! slow answer about text that has since changed is never shown.

use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::{Arc, Mutex};

/// A place in a document as the protocol counts it: a line, and a column in
/// the encoding agreed with the server.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct Position {
    pub line: u32,
    pub character: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Range {
    pub start: Position,
    pub end: Position,
}

/// What a column counts: UTF-16 code units, the protocol's default and all
/// pyright speaks, or bytes, which rust-analyzer offers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Encoding {
    Utf8,
    Utf16,
}

fn units(c: char, encoding: Encoding) -> u32 {
    match encoding {
        Encoding::Utf8 => c.len_utf8() as u32,
        Encoding::Utf16 => c.len_utf16() as u32,
    }
}

/// The character at `index` in `text` -- an index in `char`s, as egui's
/// cursors count -- as a protocol position.
pub fn position(text: &str, index: usize, encoding: Encoding) -> Position {
    let mut p = Position::default();
    for c in text.chars().take(index) {
        if c == '\n' {
            p.line += 1;
            p.character = 0;
        } else {
            p.character += units(c, encoding);
        }
    }
    p
}

/// A protocol position back to a `char` index in `text`. A column past the
/// end of its line is the line's end, and a line past the text's is the
/// text's end: a server answering about a version of the text a keystroke
/// old may point just beyond what is there now.
pub fn index(text: &str, position: Position, encoding: Encoding) -> usize {
    let mut index = 0;
    let mut chars = text.chars();
    let mut line = 0;
    while line < position.line {
        match chars.next() {
            Some('\n') => {
                line += 1;
                index += 1;
            }
            Some(_) => index += 1,
            None => return index,
        }
    }
    let mut column = 0;
    for c in chars {
        let width = units(c, encoding);
        if c == '\n' || column + width > position.character {
            break;
        }
        column += width;
        index += 1;
    }
    index
}

/// `path` as a `file:` URI, the way VS Code writes one: absolute, forward
/// slashes, and everything outside the unreserved characters escaped -- the
/// space in `Program Files`, say. The colon after a drive letter is kept.
pub fn uri(path: &Path) -> String {
    let path = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
    let text = path.to_string_lossy().replace('\\', "/");
    // `\\?\C:\...`, which `canonicalize` hands back on Windows.
    let text = text.strip_prefix("//?/").unwrap_or(&text);
    let mut out = String::from("file://");
    if !text.starts_with('/') {
        out.push('/');
    }
    for b in text.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' | b':' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// The path a `file:` URI names, or `None` for any other scheme. pyright
/// writes `file:///c%3A/...` -- the colon escaped, the drive lower case --
/// so both are undone here, and paths are compared with `same_path`.
pub fn path_of(uri: &str) -> Option<PathBuf> {
    let rest = uri.strip_prefix("file://")?;
    let mut bytes = Vec::with_capacity(rest.len());
    let raw = rest.as_bytes();
    let mut i = 0;
    while i < raw.len() {
        if raw[i] == b'%' && i + 2 < raw.len() {
            let hex = std::str::from_utf8(&raw[i + 1..i + 3]).unwrap_or("");
            if let Ok(b) = u8::from_str_radix(hex, 16) {
                bytes.push(b);
                i += 3;
                continue;
            }
        }
        bytes.push(raw[i]);
        i += 1;
    }
    let text = String::from_utf8(bytes).ok()?;
    // `/C:/x` is `C:/x` on Windows.
    let text = match text.as_bytes() {
        [b'/', _, b':', ..] if cfg!(windows) => text[1..].to_string(),
        _ => text,
    };
    Some(PathBuf::from(text))
}

/// Whether two paths name the same file, as far as their spelling goes: one
/// separator, and on Windows no case.
pub fn same_path(a: &Path, b: &Path) -> bool {
    let norm = |p: &Path| {
        let p = std::path::absolute(p).unwrap_or_else(|_| p.to_path_buf());
        let s = p.to_string_lossy().replace('\\', "/");
        let s = s.strip_prefix("//?/").map(str::to_string).unwrap_or(s);
        if cfg!(windows) { s.to_lowercase() } else { s }
    };
    norm(a) == norm(b)
}

/// One error, warning or hint in the open document.
#[derive(Debug, Clone, PartialEq)]
pub struct Diagnostic {
    pub range: Range,
    /// 1 error, 2 warning, 3 information, 4 hint.
    pub severity: u8,
    pub message: String,
    pub source: String,
    /// Tagged as unused or unreachable: drawn faded rather than underlined,
    /// as VS Code does.
    pub unnecessary: bool,
}

/// A completion as the list shows it and as accepting it applies it.
#[derive(Debug, Clone, Default)]
pub struct CompletionItem {
    pub label: String,
    /// `CompletionItemKind`: 2 method, 3 function, 6 variable, 7 class, 9
    /// module, 10 property, 14 keyword...
    pub kind: u8,
    pub detail: String,
    /// `labelDetails`: a function's parameters, a symbol's module.
    pub label_detail: String,
    pub documentation: Markup,
    pub sort_text: String,
    pub filter_text: String,
    /// What goes in: the edit's text, or `insertText`, or the label -- with
    /// a snippet's placeholders filled with their defaults.
    pub insert: String,
    /// Where the cursor lands inside `insert`, in `char`s, when a snippet
    /// said; its end otherwise.
    pub cursor: Option<usize>,
    /// The range the server said the text replaces. `None`: the word being
    /// typed, which the editor works out.
    pub range: Option<Range>,
    /// More edits to apply with it, elsewhere in the text -- the import an
    /// auto-import completion adds at the top.
    pub additional: Vec<(Range, String)>,
    pub deprecated: bool,
    /// The item as the server sent it, for `completionItem/resolve`.
    pub raw: Value,
}

#[derive(Debug, Clone, Default)]
pub struct CompletionList {
    /// The server did not send everything: ask again as the word grows,
    /// rather than filtering this list.
    pub incomplete: bool,
    pub items: Vec<CompletionItem>,
}

/// Documentation: Markdown, or plain text that must not be read as it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Markup {
    pub text: String,
    pub plain: bool,
}

impl Markup {
    pub fn is_empty(&self) -> bool {
        self.text.trim().is_empty()
    }
}

#[derive(Debug, Clone, Default)]
pub struct Hover {
    pub contents: Markup,
    pub range: Option<Range>,
}

/// A call's signatures, as the popup above a call shows them.
#[derive(Debug, Clone, Default)]
pub struct SignatureHelp {
    pub signatures: Vec<Signature>,
    pub active: usize,
}

#[derive(Debug, Clone, Default)]
pub struct Signature {
    pub label: String,
    pub documentation: Markup,
    /// Each parameter's span in `label`, in `char`s, and its documentation.
    pub parameters: Vec<((usize, usize), Markup)>,
    pub active_parameter: Option<usize>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Location {
    pub path: PathBuf,
    pub range: Range,
}

/// A reply to one of the editor's requests, by the id the request returned.
#[derive(Debug)]
pub enum Reply {
    Completion(i64, CompletionList),
    Resolved(i64, CompletionItem),
    Hover(i64, Option<Hover>),
    Signature(i64, Option<SignatureHelp>),
    Definition(i64, Vec<Location>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Initialize,
    Completion,
    Resolve,
    Hover,
    Signature,
    Definition,
    Shutdown,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Status {
    /// Started; `initialize` not answered yet. Requests wait for it.
    Starting,
    Ready,
    /// It exited, or never started. Said once in the log.
    Gone(String),
}

/// What the server said it can do, as far as the editor asks.
#[derive(Debug, Clone, Default)]
pub struct Capabilities {
    pub completion_triggers: Vec<String>,
    pub resolve: bool,
    pub hover: bool,
    pub signature_triggers: Vec<String>,
    pub signature_retriggers: Vec<String>,
    pub definition: bool,
}

/// The document the server has open: the script in the editor.
struct Document {
    uri: String,
    path: PathBuf,
    version: i64,
    hash: u64,
}

/// How to start a server, found by `find` or written in the app's settings.
#[derive(Debug, Clone, PartialEq)]
pub struct Spec {
    /// Its command's name, for the log and the status bar.
    pub name: String,
    pub program: PathBuf,
    pub args: Vec<String>,
}

/// One language server, running.
pub struct Server {
    pub spec: Spec,
    pub language_id: &'static str,
    child: Option<Child>,
    writer: Arc<Mutex<ChildStdin>>,
    inbox: Arc<Mutex<Vec<Value>>>,
    next_id: i64,
    pending: HashMap<i64, Kind>,
    pub status: Status,
    pub encoding: Encoding,
    pub capabilities: Capabilities,
    document: Option<Document>,
    /// The open document's diagnostics, as last published.
    pub diagnostics: Vec<Diagnostic>,
    /// Work the server has said it is doing -- rust-analyzer indexing, say --
    /// by its progress token: `(title, message, percentage)`.
    progress: HashMap<String, (String, String, Option<u32>)>,
    /// What the server asked to be shown the user: errors and warnings from
    /// `window/showMessage`. Drained into the log by the editor.
    pub messages: Vec<String>,
}

impl Server {
    /// Start `spec` for `language_id` documents under `root`. `settings` is
    /// what `workspace/configuration` is answered from and what goes out as
    /// `initializationOptions`; the reply to `initialize` arrives in a later
    /// frame, through `poll`.
    pub fn start(
        spec: Spec,
        language_id: &'static str,
        root: &Path,
        settings: Value,
        repaint: impl Fn() + Send + 'static,
    ) -> Result<Self, String> {
        let mut command = Command::new(&spec.program);
        command
            .args(&spec.args)
            .current_dir(root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        background(&mut command);
        let mut child = command
            .spawn()
            .map_err(|e| format!("cannot start {}: {e}", spec.program.display()))?;
        let stdin = child.stdin.take().ok_or("no stdin")?;
        let stdout = child.stdout.take().ok_or("no stdout")?;
        let writer = Arc::new(Mutex::new(stdin));
        let inbox = Arc::new(Mutex::new(Vec::new()));

        let root_uri = uri(root);
        {
            let writer = writer.clone();
            let inbox = inbox.clone();
            let settings = settings.clone();
            let root_uri = root_uri.clone();
            std::thread::Builder::new()
                .name(format!("lsp {}", spec.name))
                .spawn(move || {
                    let mut reader = BufReader::new(stdout);
                    while let Ok(Some(message)) = read_message(&mut reader) {
                        trace("<-", &message);
                        // A request from the server: answered here and now.
                        // pyright asks for its settings and analyses nothing
                        // until it has them, and the UI may not draw again
                        // for a while.
                        if let (Some(method), Some(id)) =
                            (message.get("method").and_then(Value::as_str), message.get("id"))
                        {
                            let reply = match answer(method, message.get("params"), &settings, &root_uri) {
                                Ok(result) => json!({"jsonrpc": "2.0", "id": id, "result": result}),
                                Err(e) => json!({"jsonrpc": "2.0", "id": id,
                                                 "error": {"code": -32601, "message": e}}),
                            };
                            let _ = send(&writer, &reply);
                            continue;
                        }
                        inbox.lock().unwrap().push(message);
                        repaint();
                    }
                    inbox.lock().unwrap().push(json!({"method": "kalast/exited"}));
                    repaint();
                })
                .map_err(|e| e.to_string())?;
        }

        let mut server = Self {
            spec,
            language_id,
            child: Some(child),
            writer,
            inbox,
            next_id: 1,
            pending: HashMap::new(),
            status: Status::Starting,
            encoding: Encoding::Utf16,
            capabilities: Capabilities::default(),
            document: None,
            diagnostics: Vec::new(),
            progress: HashMap::new(),
            messages: Vec::new(),
        };
        let name = root.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        server.request(
            Kind::Initialize,
            "initialize",
            json!({
                "processId": std::process::id(),
                "clientInfo": {"name": "kalast", "version": env!("CARGO_PKG_VERSION")},
                "rootUri": root_uri,
                "rootPath": root.to_string_lossy(),
                "workspaceFolders": [{"uri": root_uri, "name": name}],
                "initializationOptions": settings.get(server_section(&server.spec.name)).cloned().unwrap_or(json!({})),
                "capabilities": client_capabilities(),
            }),
        );
        Ok(server)
    }

    fn request(&mut self, kind: Kind, method: &str, params: Value) -> i64 {
        let id = self.next_id;
        self.next_id += 1;
        self.pending.insert(id, kind);
        let _ = send(&self.writer, &json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}));
        id
    }

    fn notify(&self, method: &str, params: Value) {
        let _ = send(&self.writer, &json!({"jsonrpc": "2.0", "method": method, "params": params}));
    }

    pub fn ready(&self) -> bool {
        self.status == Status::Ready
    }

    /// Keep the server's copy of the script the same as the editor's: open
    /// it, switch to another file, or send the new text -- the whole of it,
    /// which every server accepts and which a script is small enough for.
    /// Called every frame; costs a hash when nothing changed.
    pub fn sync(&mut self, path: &Path, text: &str) {
        if !self.ready() {
            return;
        }
        let hash = {
            use std::hash::{Hash, Hasher};
            let mut h = std::hash::DefaultHasher::new();
            text.hash(&mut h);
            h.finish()
        };
        match &mut self.document {
            Some(doc) if same_path(&doc.path, path) => {
                if doc.hash != hash {
                    doc.hash = hash;
                    doc.version += 1;
                    let params = json!({
                        "textDocument": {"uri": doc.uri, "version": doc.version},
                        "contentChanges": [{"text": text}],
                    });
                    self.notify("textDocument/didChange", params);
                }
            }
            _ => {
                if let Some(old) = self.document.take() {
                    self.notify("textDocument/didClose", json!({"textDocument": {"uri": old.uri}}));
                }
                self.diagnostics.clear();
                let doc = Document { uri: uri(path), path: path.to_path_buf(), version: 1, hash };
                self.notify(
                    "textDocument/didOpen",
                    json!({"textDocument": {
                        "uri": doc.uri, "languageId": self.language_id, "version": 1, "text": text,
                    }}),
                );
                self.document = Some(doc);
            }
        }
    }

    /// The script was written to disk: servers that check on save do it now.
    pub fn saved(&self) {
        if let Some(doc) = &self.document {
            self.notify("textDocument/didSave", json!({"textDocument": {"uri": doc.uri}}));
        }
    }

    fn at(&self, position: Position) -> Option<Value> {
        let doc = self.document.as_ref()?;
        Some(json!({
            "textDocument": {"uri": doc.uri},
            "position": {"line": position.line, "character": position.character},
        }))
    }

    /// Ask for completions at `position`; `trigger` is the character just
    /// typed when it is one of the server's triggers, `.` say.
    pub fn completion(&mut self, position: Position, trigger: Option<&str>) -> Option<i64> {
        let mut params = self.at(position)?;
        params["context"] = match trigger {
            Some(c) => json!({"triggerKind": 2, "triggerCharacter": c}),
            None => json!({"triggerKind": 1}),
        };
        Some(self.request(Kind::Completion, "textDocument/completion", params))
    }

    /// The documentation of one completion, which servers leave out of the
    /// list and send when it is selected.
    pub fn resolve(&mut self, item: &CompletionItem) -> Option<i64> {
        if !self.ready() || !self.capabilities.resolve || item.raw.is_null() {
            return None;
        }
        Some(self.request(Kind::Resolve, "completionItem/resolve", item.raw.clone()))
    }

    pub fn hover(&mut self, position: Position) -> Option<i64> {
        if !self.capabilities.hover {
            return None;
        }
        let params = self.at(position)?;
        Some(self.request(Kind::Hover, "textDocument/hover", params))
    }

    pub fn signature(&mut self, position: Position, trigger: Option<&str>, retrigger: bool) -> Option<i64> {
        let mut params = self.at(position)?;
        params["context"] = json!({
            "triggerKind": if trigger.is_some() { 2 } else if retrigger { 3 } else { 1 },
            "triggerCharacter": trigger,
            "isRetrigger": retrigger,
        });
        Some(self.request(Kind::Signature, "textDocument/signatureHelp", params))
    }

    pub fn definition(&mut self, position: Position) -> Option<i64> {
        if !self.capabilities.definition {
            return None;
        }
        let params = self.at(position)?;
        Some(self.request(Kind::Definition, "textDocument/definition", params))
    }

    /// What the server is busy with, for the status bar: `indexing 40%`.
    pub fn progress(&self) -> Option<String> {
        let (title, message, percentage) = self.progress.values().next()?;
        let mut s = title.clone();
        if !message.is_empty() {
            s = format!("{s} {message}");
        }
        if let Some(p) = percentage {
            s = format!("{s} {p}%");
        }
        Some(s)
    }

    /// Take what the server sent since the last frame: settle `initialize`,
    /// keep diagnostics and progress, and hand back the replies.
    pub fn poll(&mut self) -> Vec<Reply> {
        let messages = std::mem::take(&mut *self.inbox.lock().unwrap());
        let mut replies = Vec::new();
        for message in messages {
            if let Some(method) = message.get("method").and_then(Value::as_str) {
                self.notification(method, message.get("params").unwrap_or(&Value::Null));
                continue;
            }
            let Some(id) = message.get("id").and_then(Value::as_i64) else { continue };
            let Some(kind) = self.pending.remove(&id) else { continue };
            if let Some(error) = message.get("error") {
                if kind == Kind::Initialize {
                    let why = error.get("message").and_then(Value::as_str).unwrap_or("refused");
                    self.status = Status::Gone(format!("{} would not start: {why}", self.spec.name));
                }
                continue;
            }
            let result = message.get("result").unwrap_or(&Value::Null);
            match kind {
                Kind::Initialize => self.initialized(result),
                Kind::Completion => replies.push(Reply::Completion(id, completion_list(result))),
                Kind::Resolve => replies.push(Reply::Resolved(id, completion_item(result))),
                Kind::Hover => replies.push(Reply::Hover(id, hover(result))),
                Kind::Signature => replies.push(Reply::Signature(id, signature_help(result, self.encoding))),
                Kind::Definition => replies.push(Reply::Definition(id, locations(result))),
                Kind::Shutdown => {}
            }
        }
        replies
    }

    fn initialized(&mut self, result: &Value) {
        let caps = &result["capabilities"];
        self.encoding = match caps["positionEncoding"].as_str() {
            Some("utf-8") => Encoding::Utf8,
            _ => Encoding::Utf16,
        };
        let strings = |v: &Value| -> Vec<String> {
            v.as_array()
                .map(|a| a.iter().filter_map(|s| s.as_str().map(str::to_string)).collect())
                .unwrap_or_default()
        };
        let enabled = |v: &Value| !v.is_null() && v != &Value::Bool(false);
        self.capabilities = Capabilities {
            completion_triggers: strings(&caps["completionProvider"]["triggerCharacters"]),
            resolve: caps["completionProvider"]["resolveProvider"].as_bool().unwrap_or(false),
            hover: enabled(&caps["hoverProvider"]),
            signature_triggers: strings(&caps["signatureHelpProvider"]["triggerCharacters"]),
            signature_retriggers: strings(&caps["signatureHelpProvider"]["retriggerCharacters"]),
            definition: enabled(&caps["definitionProvider"]),
        };
        self.notify("initialized", json!({}));
        // pyright asks for its settings on this; others read them here.
        self.notify("workspace/didChangeConfiguration", json!({"settings": {}}));
        self.status = Status::Ready;
    }

    fn notification(&mut self, method: &str, params: &Value) {
        match method {
            "textDocument/publishDiagnostics" => {
                let ours = self.document.as_ref().is_some_and(|doc| {
                    params["uri"].as_str().and_then(path_of).is_some_and(|p| same_path(&p, &doc.path))
                });
                if ours {
                    self.diagnostics = params["diagnostics"]
                        .as_array()
                        .map(|a| a.iter().map(diagnostic).collect())
                        .unwrap_or_default();
                }
            }
            "$/progress" => {
                let token = params["token"].to_string();
                let value = &params["value"];
                match value["kind"].as_str() {
                    Some("begin") => {
                        self.progress.insert(
                            token,
                            (
                                value["title"].as_str().unwrap_or_default().to_string(),
                                value["message"].as_str().unwrap_or_default().to_string(),
                                value["percentage"].as_u64().map(|p| p as u32),
                            ),
                        );
                    }
                    Some("report") => {
                        if let Some(p) = self.progress.get_mut(&token) {
                            if let Some(m) = value["message"].as_str() {
                                p.1 = m.to_string();
                            }
                            p.2 = value["percentage"].as_u64().map(|p| p as u32).or(p.2);
                        }
                    }
                    Some("end") => {
                        self.progress.remove(&token);
                    }
                    _ => {}
                }
            }
            "window/showMessage" => {
                // Errors and warnings; the chatter below them stays out.
                if params["type"].as_u64().is_some_and(|t| t <= 2) {
                    if let Some(m) = params["message"].as_str() {
                        self.messages.push(format!("{}: {m}", self.spec.name));
                    }
                }
            }
            "kalast/exited" => {
                if !matches!(self.status, Status::Gone(_)) {
                    self.status = Status::Gone(format!("{} exited", self.spec.name));
                }
            }
            _ => {}
        }
    }
}

impl Drop for Server {
    /// Ask the server to leave, and make sure it has: a moment to exit on
    /// its own, on a thread of its own so closing the app waits for nothing,
    /// then killed. It would go anyway when the app does -- it watches the
    /// `processId` it was given, and its stdin closing -- but not before.
    fn drop(&mut self) {
        if self.ready() {
            self.request(Kind::Shutdown, "shutdown", Value::Null);
            self.notify("exit", Value::Null);
        }
        if let Some(mut child) = self.child.take() {
            std::thread::spawn(move || {
                for _ in 0..20 {
                    if matches!(child.try_wait(), Ok(Some(_))) {
                        return;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(100));
                }
                let _ = child.kill();
                let _ = child.wait();
            });
        }
    }
}

/// Where a server keeps its settings, by its command: rust-analyzer reads
/// `initializationOptions` as its whole configuration.
fn server_section(name: &str) -> &'static str {
    if name.contains("rust-analyzer") {
        "rust-analyzer"
    } else if name.contains("pylsp") {
        "pylsp"
    } else if name.contains("jedi") {
        "jedi"
    } else {
        "python"
    }
}

/// The settings kalast gives its servers. `python.pythonPath` is the one
/// that matters most: the interpreter kalast runs scripts with, so `import
/// kalast` and numpy resolve to what the script will actually import.
///
/// "standard" type checking rather than basedpyright's own "recommended",
/// which underlines half of any numpy script; a `pyrightconfig.json` or a
/// `[tool.pyright]` in the project still has the last word. rust-analyzer
/// gets a target directory of its own and no `cargo check` on save, so
/// opening an example never locks the build a user is running.
pub fn settings(python: Option<&Path>, search: &[PathBuf]) -> Value {
    // What might be `None` or unbound is a warning rather than an error: a
    // script reads `bodies[0].mesh.facets` knowing the mesh was loaded two
    // lines up, and a red line under each such use is noise.
    let analysis = json!({
        "typeCheckingMode": "standard",
        "diagnosticMode": "openFilesOnly",
        "autoImportCompletions": true,
        "autoSearchPaths": true,
        // Where imports are looked for besides the interpreter's own paths:
        // a release bundle's packages, whose interpreter cannot be run.
        "extraPaths": search.iter().map(|p| p.to_string_lossy()).collect::<Vec<_>>(),
        "useLibraryCodeForTypes": true,
        "diagnosticSeverityOverrides": {
            "reportOptionalMemberAccess": "warning",
            "reportOptionalSubscript": "warning",
            "reportOptionalIterable": "warning",
            "reportPossiblyUnbound": "warning",
        },
    });
    json!({
        "python": {"pythonPath": python.map(|p| p.to_string_lossy()), "analysis": analysis},
        "basedpyright": {"analysis": analysis},
        "rust-analyzer": {"checkOnSave": false, "cargo": {"targetDir": true}},
        "pylsp": {},
        "jedi": {},
    })
}

/// A request from the server, answered: its settings, the one folder, and
/// nothing for everything a script editor has no part in.
fn answer(method: &str, params: Option<&Value>, settings: &Value, root_uri: &str) -> Result<Value, String> {
    match method {
        "workspace/configuration" => {
            let items = params.and_then(|p| p["items"].as_array()).cloned().unwrap_or_default();
            Ok(Value::Array(
                items
                    .iter()
                    .map(|item| {
                        let mut v = settings;
                        if let Some(section) = item["section"].as_str() {
                            for key in section.split('.') {
                                v = &v[key];
                            }
                        }
                        v.clone()
                    })
                    .collect(),
            ))
        }
        "workspace/workspaceFolders" => Ok(json!([{"uri": root_uri, "name": "workspace"}])),
        "workspace/applyEdit" => Ok(json!({"applied": false})),
        "client/registerCapability"
        | "client/unregisterCapability"
        | "window/workDoneProgress/create"
        | "window/showMessageRequest"
        | "workspace/semanticTokens/refresh"
        | "workspace/inlayHint/refresh"
        | "workspace/codeLens/refresh"
        | "workspace/diagnostic/refresh" => Ok(Value::Null),
        _ => Err(format!("kalast does not handle {method}")),
    }
}

fn client_capabilities() -> Value {
    json!({
        "general": {"positionEncodings": ["utf-8", "utf-16"]},
        "workspace": {
            "configuration": true,
            "workspaceFolders": true,
            "didChangeConfiguration": {"dynamicRegistration": false},
        },
        "window": {"workDoneProgress": true},
        "textDocument": {
            "synchronization": {"didSave": true, "dynamicRegistration": false},
            "completion": {
                "contextSupport": true,
                "completionItem": {
                    // Plain text only: kalast has no tab stops to jump
                    // between. What arrives as a snippet anyway is flattened.
                    "snippetSupport": false,
                    "documentationFormat": ["markdown", "plaintext"],
                    "resolveSupport": {"properties": ["documentation", "detail"]},
                    "labelDetailsSupport": true,
                    "deprecatedSupport": true,
                    "tagSupport": {"valueSet": [1]},
                },
                "completionItemKind": {"valueSet": (1..=25).collect::<Vec<u8>>()},
            },
            "hover": {"contentFormat": ["markdown", "plaintext"]},
            "signatureHelp": {
                "contextSupport": true,
                "signatureInformation": {
                    "documentationFormat": ["markdown", "plaintext"],
                    "parameterInformation": {"labelOffsetSupport": true},
                    "activeParameterSupport": true,
                },
            },
            "definition": {"linkSupport": true},
            "publishDiagnostics": {"versionSupport": true, "tagSupport": {"valueSet": [1, 2]}},
        },
    })
}

/// Every message to and from the servers, when `KALAST_LSP_TRACE` names a
/// file: what VS Code's `trace.server` setting shows, for when completion
/// says nothing and the question is whether the server did.
fn trace(direction: &str, message: &Value) {
    static FILE: std::sync::OnceLock<Option<Mutex<std::fs::File>>> = std::sync::OnceLock::new();
    let file = FILE.get_or_init(|| {
        let path = std::env::var_os("KALAST_LSP_TRACE")?;
        std::fs::OpenOptions::new().create(true).append(true).open(path).ok().map(Mutex::new)
    });
    if let Some(file) = file {
        let mut text = message.to_string();
        if text.len() > 2000 {
            let mut cut = 2000;
            while !text.is_char_boundary(cut) {
                cut -= 1;
            }
            text.truncate(cut);
            text.push_str("...");
        }
        let elapsed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs_f64())
            .unwrap_or_default();
        let _ = writeln!(file.lock().unwrap(), "{elapsed:.3} {direction} {text}");
    }
}

fn send(writer: &Mutex<ChildStdin>, message: &Value) -> std::io::Result<()> {
    trace("->", message);
    let body = serde_json::to_vec(message)?;
    let mut w = writer.lock().unwrap();
    write!(w, "Content-Length: {}\r\n\r\n", body.len())?;
    w.write_all(&body)?;
    w.flush()
}

/// One message, or `None` at the end of the stream.
fn read_message(reader: &mut impl BufRead) -> std::io::Result<Option<Value>> {
    let mut length = None;
    let mut line = String::new();
    loop {
        line.clear();
        if reader.read_line(&mut line)? == 0 {
            return Ok(None);
        }
        let header = line.trim_end();
        if header.is_empty() {
            if length.is_some() {
                break;
            }
            continue;
        }
        if let Some((name, value)) = header.split_once(':') {
            if name.eq_ignore_ascii_case("content-length") {
                length = value.trim().parse::<usize>().ok();
            }
        }
    }
    let mut body = vec![0; length.unwrap_or(0)];
    reader.read_exact(&mut body)?;
    Ok(Some(serde_json::from_slice(&body).unwrap_or(Value::Null)))
}

/// Low priority, and on Windows no console window: a language server is a
/// console program, and started from a double-clicked app it would open one.
fn background(command: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        const BELOW_NORMAL_PRIORITY_CLASS: u32 = 0x0000_4000;
        command.creation_flags(CREATE_NO_WINDOW | BELOW_NORMAL_PRIORITY_CLASS);
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // SAFETY: `nice` is async-signal-safe, and nothing else runs
        // between the fork and the exec.
        unsafe {
            command.pre_exec(|| {
                libc::nice(10);
                Ok(())
            });
        }
    }
}

fn range(v: &Value) -> Range {
    let pos = |p: &Value| Position {
        line: p["line"].as_u64().unwrap_or(0) as u32,
        character: p["character"].as_u64().unwrap_or(0) as u32,
    };
    Range { start: pos(&v["start"]), end: pos(&v["end"]) }
}

fn markup(v: &Value) -> Markup {
    match v {
        Value::String(s) => Markup { text: s.clone(), plain: false },
        Value::Object(o) if o.contains_key("kind") => Markup {
            text: o.get("value").and_then(Value::as_str).unwrap_or_default().to_string(),
            plain: o.get("kind").and_then(Value::as_str) == Some("plaintext"),
        },
        // A `MarkedString` with a language: code.
        Value::Object(o) => Markup {
            text: format!(
                "```{}\n{}\n```",
                o.get("language").and_then(Value::as_str).unwrap_or_default(),
                o.get("value").and_then(Value::as_str).unwrap_or_default()
            ),
            plain: false,
        },
        Value::Array(a) => Markup {
            text: a.iter().map(|m| markup(m).text).collect::<Vec<_>>().join("\n\n"),
            plain: false,
        },
        _ => Markup::default(),
    }
}

fn diagnostic(v: &Value) -> Diagnostic {
    let tags: Vec<u64> = v["tags"].as_array().map(|a| a.iter().filter_map(Value::as_u64).collect()).unwrap_or_default();
    Diagnostic {
        range: range(&v["range"]),
        severity: v["severity"].as_u64().unwrap_or(1) as u8,
        message: v["message"].as_str().unwrap_or_default().to_string(),
        source: v["source"].as_str().unwrap_or_default().to_string(),
        unnecessary: tags.contains(&1),
    }
}

/// A snippet's text as plain text: each placeholder its default, and where
/// the first tab stop was, for the cursor.
pub fn flatten_snippet(snippet: &str) -> (String, Option<usize>) {
    let mut out = String::new();
    let mut cursor = None;
    let chars: Vec<char> = snippet.chars().collect();
    let mut i = 0;
    let mut count = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '\\' && i + 1 < chars.len() {
            out.push(chars[i + 1]);
            count += 1;
            i += 2;
            continue;
        }
        if c == '$' && i + 1 < chars.len() {
            // `$1`, `$0`
            if chars[i + 1].is_ascii_digit() {
                let mut j = i + 1;
                while j < chars.len() && chars[j].is_ascii_digit() {
                    j += 1;
                }
                cursor.get_or_insert(count);
                i = j;
                continue;
            }
            // `${1:default}`, `${1|a,b|}`, `${1}`
            if chars[i + 1] == '{' {
                let mut j = i + 2;
                while j < chars.len() && chars[j].is_ascii_digit() {
                    j += 1;
                }
                let mut depth = 1;
                let mut inner = String::new();
                if j < chars.len() && (chars[j] == ':' || chars[j] == '|') {
                    let choice = chars[j] == '|';
                    j += 1;
                    while j < chars.len() && depth > 0 {
                        match chars[j] {
                            '{' => depth += 1,
                            '}' => depth -= 1,
                            _ => {}
                        }
                        if depth > 0 {
                            inner.push(chars[j]);
                        }
                        j += 1;
                    }
                    if choice {
                        inner = inner.trim_end_matches('|').split(',').next().unwrap_or_default().to_string();
                    }
                } else {
                    while j < chars.len() && chars[j] != '}' {
                        j += 1;
                    }
                    j += 1;
                }
                cursor.get_or_insert(count);
                let (flat, _) = flatten_snippet(&inner);
                count += flat.chars().count();
                out.push_str(&flat);
                i = j;
                continue;
            }
        }
        out.push(c);
        count += 1;
        i += 1;
    }
    (out, cursor)
}

fn completion_item(v: &Value) -> CompletionItem {
    let snippet = v["insertTextFormat"].as_u64() == Some(2);
    let (text, edit_range) = match &v["textEdit"] {
        Value::Object(edit) => (
            edit.get("newText").and_then(Value::as_str).map(str::to_string),
            // An `InsertReplaceEdit` has two ranges: VS Code's default is to
            // insert, leaving the rest of a word the cursor is in.
            edit.get("range").or_else(|| edit.get("insert")).map(range),
        ),
        _ => (None, None),
    };
    let label = v["label"].as_str().unwrap_or_default().to_string();
    let raw_text = text
        .or_else(|| v["insertText"].as_str().map(str::to_string))
        .unwrap_or_else(|| label.clone());
    let (insert, cursor) = if snippet { flatten_snippet(&raw_text) } else { (raw_text, None) };
    let details = &v["labelDetails"];
    let label_detail = [details["detail"].as_str(), details["description"].as_str()]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" ");
    CompletionItem {
        kind: v["kind"].as_u64().unwrap_or(1) as u8,
        detail: v["detail"].as_str().unwrap_or_default().to_string(),
        label_detail,
        documentation: markup(&v["documentation"]),
        sort_text: v["sortText"].as_str().map(str::to_string).unwrap_or_else(|| label.clone()),
        filter_text: v["filterText"].as_str().map(str::to_string).unwrap_or_else(|| label.clone()),
        insert,
        cursor,
        range: edit_range,
        additional: v["additionalTextEdits"]
            .as_array()
            .map(|a| {
                a.iter()
                    .map(|e| (range(&e["range"]), e["newText"].as_str().unwrap_or_default().to_string()))
                    .collect()
            })
            .unwrap_or_default(),
        deprecated: v["deprecated"].as_bool().unwrap_or(false)
            || v["tags"].as_array().is_some_and(|t| t.iter().any(|t| t.as_u64() == Some(1))),
        raw: v.clone(),
        label,
    }
}

fn completion_list(v: &Value) -> CompletionList {
    match v {
        Value::Array(items) => CompletionList { incomplete: false, items: items.iter().map(completion_item).collect() },
        Value::Object(_) => CompletionList {
            incomplete: v["isIncomplete"].as_bool().unwrap_or(false),
            items: v["items"].as_array().map(|a| a.iter().map(completion_item).collect()).unwrap_or_default(),
        },
        _ => CompletionList::default(),
    }
}

fn hover(v: &Value) -> Option<Hover> {
    if v.is_null() {
        return None;
    }
    let contents = markup(&v["contents"]);
    if contents.is_empty() {
        return None;
    }
    Some(Hover { contents, range: v.get("range").filter(|r| !r.is_null()).map(range) })
}

fn signature_help(v: &Value, encoding: Encoding) -> Option<SignatureHelp> {
    let signatures = v["signatures"].as_array()?;
    if signatures.is_empty() {
        return None;
    }
    let top_parameter = v["activeParameter"].as_u64();
    let signatures = signatures
        .iter()
        .map(|s| {
            let label = s["label"].as_str().unwrap_or_default().to_string();
            let parameters = s["parameters"]
                .as_array()
                .map(|ps| {
                    let mut from = label.find('(').map(|i| label[..i].chars().count()).unwrap_or(0);
                    ps.iter()
                        .map(|p| {
                            let span = match &p["label"] {
                                // Offsets into the label, in the encoding.
                                Value::Array(a) if a.len() == 2 => {
                                    let offset = |n: u64| {
                                        let mut units_seen = 0;
                                        let mut chars = 0;
                                        for c in label.chars() {
                                            if units_seen >= n as u32 {
                                                break;
                                            }
                                            units_seen += units(c, encoding);
                                            chars += 1;
                                        }
                                        chars
                                    };
                                    (offset(a[0].as_u64().unwrap_or(0)), offset(a[1].as_u64().unwrap_or(0)))
                                }
                                // A substring: its first occurrence after the
                                // previous parameter's.
                                Value::String(name) => {
                                    let chars: Vec<char> = label.chars().collect();
                                    let needle: Vec<char> = name.chars().collect();
                                    let found = (from..chars.len().saturating_sub(needle.len()) + 1)
                                        .find(|&i| chars[i..].starts_with(&needle));
                                    match found {
                                        Some(i) => {
                                            from = i + needle.len();
                                            (i, i + needle.len())
                                        }
                                        None => (0, 0),
                                    }
                                }
                                _ => (0, 0),
                            };
                            (span, markup(&p["documentation"]))
                        })
                        .collect()
                })
                .unwrap_or_default();
            Signature {
                documentation: markup(&s["documentation"]),
                parameters,
                active_parameter: s["activeParameter"].as_u64().or(top_parameter).map(|p| p as usize),
                label,
            }
        })
        .collect::<Vec<_>>();
    let active = (v["activeSignature"].as_u64().unwrap_or(0) as usize).min(signatures.len() - 1);
    Some(SignatureHelp { signatures, active })
}

fn locations(v: &Value) -> Vec<Location> {
    let one = |l: &Value| -> Option<Location> {
        // A `LocationLink` or a `Location`.
        let target = l.get("targetUri").or_else(|| l.get("uri"))?.as_str()?;
        let r = l.get("targetSelectionRange").or_else(|| l.get("range"))?;
        Some(Location { path: path_of(target)?, range: range(r) })
    };
    match v {
        Value::Array(a) => a.iter().filter_map(one).collect(),
        Value::Object(_) => one(v).into_iter().collect(),
        _ => Vec::new(),
    }
}

/// The servers kalast looks for, best first, by language: the command and
/// its arguments. basedpyright and pyright are what VS Code's Pylance is
/// built on; pylsp and jedi-language-server are what is often installed
/// already. rust-analyzer is the one there is.
pub fn candidates(language_id: &str) -> &'static [(&'static str, &'static [&'static str])] {
    match language_id {
        "python" => &[
            ("basedpyright-langserver", &["--stdio"]),
            ("pyright-langserver", &["--stdio"]),
            ("pylsp", &[]),
            ("jedi-language-server", &[]),
        ],
        "rust" => &[("rust-analyzer", &[])],
        _ => &[],
    }
}

/// The server to run for `language_id`: the command set in the app's
/// settings, or else the first candidate installed -- beside the Python
/// interpreter (a venv's `Scripts` or `bin`), on the PATH, in uv's and
/// cargo's tool directories, or where Neovim's mason put it.
pub fn find(language_id: &str, configured: &str, python: Option<&Path>) -> Option<Spec> {
    let configured = configured.trim();
    if !configured.is_empty() {
        let mut words = split_command(configured).into_iter();
        let program = words.next()?;
        let name = Path::new(&program)
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| program.clone());
        let program = which(&program, &search_dirs(python)).unwrap_or_else(|| PathBuf::from(&program));
        return Some(Spec { name, program, args: words.collect() });
    }
    let dirs = search_dirs(python);
    candidates(language_id).iter().find_map(|(name, args)| {
        which(name, &dirs).map(|program| Spec {
            name: name.to_string(),
            program,
            args: args.iter().map(|s| s.to_string()).collect(),
        })
    })
}

fn search_dirs(python: Option<&Path>) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(dir) = python.and_then(Path::parent) {
        dirs.push(dir.to_path_buf());
        dirs.push(dir.join("Scripts"));
        dirs.push(dir.join("bin"));
    }
    if let Some(path) = std::env::var_os("PATH") {
        dirs.extend(std::env::split_paths(&path));
    }
    let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from);
    if let Some(home) = &home {
        dirs.push(home.join(".local").join("bin"));
        dirs.push(home.join(".cargo").join("bin"));
    }
    let mason = if cfg!(windows) {
        std::env::var_os("LOCALAPPDATA").map(|d| PathBuf::from(d).join("nvim-data"))
    } else {
        home.map(|h| h.join(".local").join("share").join("nvim"))
    };
    if let Some(data) = mason {
        dirs.push(data.join("mason").join("bin"));
    }
    dirs
}

/// `name` as an executable in one of `dirs`: on Windows with the extensions
/// a shell would try, npm's and mason's `.cmd` shims among them.
pub fn which(name: &str, dirs: &[PathBuf]) -> Option<PathBuf> {
    let path = Path::new(name);
    if path.components().count() > 1 {
        return path.is_file().then(|| path.to_path_buf());
    }
    let extensions: &[&str] = if cfg!(windows) { &["exe", "cmd", "bat", "com"] } else { &[""] };
    for dir in dirs {
        for ext in extensions {
            let candidate = if ext.is_empty() { dir.join(name) } else { dir.join(format!("{name}.{ext}")) };
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

/// A command line split into words, as a shell would for the simple cases:
/// spaces separate, double quotes group.
pub fn split_command(line: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut quoted = false;
    let mut any = false;
    for c in line.chars() {
        match c {
            '"' => {
                quoted = !quoted;
                any = true;
            }
            c if c.is_whitespace() && !quoted => {
                if any {
                    words.push(std::mem::take(&mut word));
                    any = false;
                }
            }
            c => {
                word.push(c);
                any = true;
            }
        }
    }
    if any {
        words.push(word);
    }
    words
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Columns count UTF-16 units by default: `é` is one, an emoji two.
    #[test]
    fn positions_count_the_encodings_units() {
        let text = "ab\nçé😀x\nz";
        // `x`, after one unit each for `ç` and `é` and two for the emoji.
        let p = position(text, 6, Encoding::Utf16);
        assert_eq!(p, Position { line: 1, character: 4 });
        assert_eq!(index(text, p, Encoding::Utf16), 6);
        let p = position(text, 6, Encoding::Utf8);
        assert_eq!(p, Position { line: 1, character: 8 });
        assert_eq!(index(text, p, Encoding::Utf8), 6);
        // Past a line's end is its end; past the text's is the text's.
        assert_eq!(index(text, Position { line: 0, character: 99 }, Encoding::Utf16), 2);
        assert_eq!(index(text, Position { line: 9, character: 0 }, Encoding::Utf16), text.chars().count());
    }

    #[test]
    fn uris_round_trip_through_pyrights_spelling() {
        let path = if cfg!(windows) {
            PathBuf::from(r"C:\Program Files\kalast\a b.py")
        } else {
            PathBuf::from("/opt/kalast/a b.py")
        };
        let u = uri(&path);
        assert!(u.starts_with("file:///"), "{u}");
        assert!(u.contains("a%20b.py"), "{u}");
        assert!(same_path(&path_of(&u).unwrap(), &path));
        if cfg!(windows) {
            let pyright = "file:///c%3A/Program%20Files/kalast/a%20b.py";
            assert!(same_path(&path_of(pyright).unwrap(), &path));
        }
        assert!(path_of("untitled:Untitled-1").is_none());
    }

    #[test]
    fn a_message_is_framed_by_its_length() {
        let body = r#"{"jsonrpc":"2.0","id":1,"result":{"x":"é"}}"#;
        let stream = format!(
            "Content-Length: {}\r\nContent-Type: application/vscode-jsonrpc; charset=utf-8\r\n\r\n{body}",
            body.len()
        );
        let mut reader = std::io::BufReader::new(stream.as_bytes());
        let message = read_message(&mut reader).unwrap().unwrap();
        assert_eq!(message["result"]["x"], "é");
        assert!(read_message(&mut reader).unwrap().is_none());
    }

    #[test]
    fn snippets_flatten_to_their_defaults() {
        assert_eq!(flatten_snippet("print(${1:value})$0"), ("print(value)".to_string(), Some(6)));
        assert_eq!(flatten_snippet("fn $1()"), ("fn ()".to_string(), Some(3)));
        assert_eq!(flatten_snippet("a \\$b"), ("a $b".to_string(), None));
        assert_eq!(flatten_snippet("${1|one,two|}"), ("one".to_string(), Some(0)));
    }

    #[test]
    fn a_completion_takes_its_edit_over_its_label() {
        let item = completion_item(&json!({
            "label": "linspace",
            "kind": 3,
            "textEdit": {"range": {"start": {"line": 2, "character": 3}, "end": {"line": 2, "character": 6}},
                         "newText": "linspace"},
            "additionalTextEdits": [{"range": {"start": {"line": 0, "character": 0},
                                               "end": {"line": 0, "character": 0}},
                                     "newText": "import numpy\n"}],
            "documentation": {"kind": "markdown", "value": "Evenly spaced."},
        }));
        assert_eq!(item.insert, "linspace");
        assert_eq!(item.range.unwrap().start, Position { line: 2, character: 3 });
        assert_eq!(item.additional.len(), 1);
        assert_eq!(item.documentation.text, "Evenly spaced.");
        assert_eq!(item.filter_text, "linspace");
    }

    #[test]
    fn signature_parameters_are_found_in_the_label() {
        let help = signature_help(
            &json!({
                "signatures": [{"label": "f(a: int, b: str) -> None",
                                "parameters": [{"label": "a: int"}, {"label": [10, 16]}]}],
                "activeParameter": 1,
            }),
            Encoding::Utf16,
        )
        .unwrap();
        let s = &help.signatures[0];
        assert_eq!(s.parameters[0].0, (2, 8));
        assert_eq!(s.parameters[1].0, (10, 16));
        assert_eq!(s.active_parameter, Some(1));
    }

    #[test]
    fn a_command_splits_on_spaces_outside_quotes() {
        assert_eq!(split_command("pyright-langserver --stdio"), vec!["pyright-langserver", "--stdio"]);
        assert_eq!(
            split_command(r#""C:\Program Files\x\ra.exe" --a "b c""#),
            vec![r"C:\Program Files\x\ra.exe", "--a", "b c"]
        );
    }

    /// The real thing, when a Python server is installed: started, asked,
    /// and answering -- a completion after `os.`, the hover of a function,
    /// an error published -- as the editor will ask.
    #[test]
    fn a_python_server_answers() {
        let Some(spec) = find("python", "", None) else {
            eprintln!("no Python language server on this machine; skipped");
            return;
        };
        let dir = std::env::temp_dir().join(format!("kalast-lsp-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("t.py");
        let text = "import os\nx: int = \"a\"\nos.pa";
        std::fs::write(&file, text).unwrap();
        let mut server = Server::start(spec.clone(), "python", &dir, settings(None, &[]), || {}).unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
        let mut replies = Vec::new();
        let mut asked = None;
        while std::time::Instant::now() < deadline {
            replies.extend(server.poll());
            server.sync(&file, text);
            if server.ready() && asked.is_none() {
                let end = position(text, text.chars().count(), server.encoding);
                let completion = server.completion(end, None).unwrap();
                let hover = server.hover(Position { line: 2, character: 0 }).unwrap();
                asked = Some((completion, hover));
            }
            let done = asked.is_some_and(|(c, h)| {
                replies.iter().any(|r| matches!(r, Reply::Completion(id, _) if *id == c))
                    && replies.iter().any(|r| matches!(r, Reply::Hover(id, _) if *id == h))
            });
            if done && !server.diagnostics.is_empty() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        let (c, h) = asked.expect("the server never became ready");
        let items = replies
            .iter()
            .find_map(|r| match r {
                Reply::Completion(id, list) if *id == c => Some(list.items.clone()),
                _ => None,
            })
            .expect("no completion reply");
        assert!(items.iter().any(|i| i.label == "path"), "{} items, no `path`", items.len());
        let hover = replies
            .iter()
            .find_map(|r| match r {
                Reply::Hover(id, hover) if *id == h => Some(hover.clone()),
                _ => None,
            })
            .flatten()
            .expect("no hover over `os`");
        assert!(hover.contents.text.contains("os"), "{:?}", hover.contents);
        assert!(
            server.diagnostics.iter().any(|d| d.severity == 1 && d.range.start.line == 1),
            "no error on the str assigned to an int: {:?}",
            server.diagnostics
        );
        drop(server);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Settings are looked up by their dotted section, as pyright asks.
    #[test]
    fn configuration_is_answered_by_section() {
        let s = settings(Some(Path::new("/venv/bin/python")), &[PathBuf::from("/bundle/python/Lib/site-packages")]);
        let reply = answer(
            "workspace/configuration",
            Some(&json!({"items": [{"section": "python"}, {"section": "python.analysis"}, {"section": "nope"}]})),
            &s,
            "file:///root",
        )
        .unwrap();
        assert_eq!(reply[0]["pythonPath"], "/venv/bin/python");
        assert_eq!(reply[1]["typeCheckingMode"], "standard");
        assert_eq!(reply[1]["extraPaths"][0], "/bundle/python/Lib/site-packages");
        assert!(reply[2].is_null());
        assert!(answer("something/else", None, &s, "").is_err());
    }
}
