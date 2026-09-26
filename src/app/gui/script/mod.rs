//! The script editor: the text, drawn as VS Code draws its editor, with the
//! helpers VS Code gives it -- completion as you type, a symbol's type and
//! docs on hover, a call's signature, errors underlined and spelled out at
//! the end of their line, go-to-definition -- from a language server
//! (`lsp`). Its keys are VS Code's, or with `app.config.neovim` the user's
//! own Neovim's (`nvim`).
//!
//! Two views of one text. Without Neovim, egui's `TextEdit` edits it, as it
//! always did. With it, Neovim edits it and this module draws what Neovim
//! reports -- the lines, the cursor and its shape, the selection, the
//! command line -- since a `TextEdit` cannot be told where a block cursor
//! sits or what a visual selection covers. What is drawn over the text --
//! the ruler, the diagnostics, the popups, the status bar -- is the same in
//! both.

pub mod lsp;
mod markdown;
pub mod nvim;

use super::code::{self, Lang, Palette};
use super::theme;
use crate::app::config::UiTheme;
use egui::text::{CCursor, CCursorRange, LayoutJob, TextFormat};
use egui::{Color32, FontId, Pos2, Rect, Stroke};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// The one id both views give the text, so the keyboard focus survives
/// switching Neovim on or off.
fn text_id() -> egui::Id {
    egui::Id::new("kalast script editor")
}

/// Neovim's window is this many columns wide whatever kalast shows, so it
/// never scrolls sideways on its own: kalast scrolls the view, and a mouse
/// column stays a buffer column.
const COLUMNS: u32 = 500;
const STATUS_HEIGHT: f32 = 22.0;
/// How long the pointer rests on a word before its hover is asked for:
/// VS Code's `editor.hover.delay`.
const HOVER_DELAY: f64 = 0.3;

static PYTHON: Mutex<Option<PathBuf>> = Mutex::new(None);

/// The interpreter scripts run in, which the Python language server resolves
/// `import kalast` and numpy with. Set from Python -- `sys.executable` --
/// when kalast runs under it.
///
/// Not when that is the `kalast` program carrying the interpreter inside it
/// -- a release bundle's `kalast.exe` -- whose `sys.executable` is itself.
/// Given that as its Python, basedpyright ran `kalast.exe -c "..."` for the
/// version and the search paths, and each run opened another kalast window
/// -- five, each saying it did not know what to do with the code -- and then
/// could resolve no `import numpy`. The bundle's packages are handed to the
/// server as search paths instead; see `ensure_server`. Only then: under a
/// real interpreter, `sys.executable` can be this very process too -- a
/// `python.exe` with no venv, or a venv's `python`, a symlink to the one
/// running -- and is exactly what the server wants.
pub fn set_python(path: PathBuf) {
    if !is_kalast_itself(&path) {
        *PYTHON.lock().unwrap() = Some(path);
    }
}

/// The interpreter scripts run in is linked into this program, the `kalast`
/// binary, rather than being the program. Said by that binary at startup.
static EMBEDDED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn set_python_embedded() {
    EMBEDDED.store(true, std::sync::atomic::Ordering::Relaxed);
}

/// `path` is the `kalast` program that carries its interpreter, which is no
/// `python` to run.
fn is_kalast_itself(path: &Path) -> bool {
    if !EMBEDDED.load(std::sync::atomic::Ordering::Relaxed) {
        return false;
    }
    let Ok(exe) = std::env::current_exe() else { return false };
    match (path.canonicalize(), exe.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => path == exe,
    }
}

fn python() -> Option<PathBuf> {
    if let Some(p) = PYTHON.lock().unwrap().clone() {
        return Some(p);
    }
    // Before any script has run -- a `.py` opened in `kalast.exe` and not
    // played yet -- Python has not said. The interpreter kalast.exe would
    // run it with, then: the one `KALAST_PYTHON` names, or the bundle's own.
    if let Some(p) = std::env::var_os("KALAST_PYTHON").filter(|p| !p.is_empty()).map(PathBuf::from) {
        return (!is_kalast_itself(&p)).then_some(p);
    }
    if let Some(dir) = crate::app::bundled_python_dir() {
        let p = if cfg!(windows) { dir.join("python.exe") } else { dir.join("bin").join("python3") };
        if p.is_file() {
            return Some(p);
        }
    }
    // A virtual environment this process was started from.
    let venv = PathBuf::from(std::env::var_os("VIRTUAL_ENV")?);
    let p = if cfg!(windows) {
        venv.join("Scripts").join("python.exe")
    } else {
        venv.join("bin").join("python")
    };
    p.is_file().then_some(p)
}

/// What the editor takes from `app.config`.
pub struct Settings<'a> {
    pub neovim: bool,
    pub neovim_path: &'a str,
    pub ruler: u32,
    pub language_servers: bool,
    pub python_language_server: &'a str,
    pub rust_language_server: &'a str,
    pub theme: UiTheme,
}

/// What happened in a frame, for the window to act on.
#[derive(Default, Debug)]
pub struct Outcome {
    /// The text changed: typed, a completion accepted, `dd`.
    pub changed: bool,
    /// Under Neovim, whether the text now differs from the file as last
    /// read or saved: `u` back to it is not an edit to save.
    pub modified: Option<bool>,
    /// `:w`, or Ctrl+S under Neovim.
    pub save: bool,
    /// `:q`: back to the scene.
    pub quit: bool,
    /// A definition's file, opened from its peek.
    pub open: Option<PathBuf>,
}

enum Slot {
    Running(lsp::Server),
    /// Not installed, would not start, or died: said once in the log and not
    /// tried again until its setting changes.
    Failed,
}

/// The completion list, as it stands while a word is typed.
struct Completion {
    list: lsp::CompletionList,
    /// Where the word being completed starts, in `char`s.
    anchor: usize,
    /// `list.items` passing the filter, best first, with the characters of
    /// each label that matched.
    shown: Vec<(usize, Vec<usize>)>,
    selected: usize,
    /// The first row in view.
    scroll: usize,
    /// The selection the view was last brought to. The view follows the
    /// selection when that moves -- the arrows, a refilter -- and is the
    /// wheel's otherwise: brought back to it every frame, the list could not
    /// be scrolled.
    followed: Option<usize>,
    /// Wheel travel not yet a whole row.
    wheel: f32,
}

/// A hover open on screen.
struct Hover {
    /// Diagnostics first, as VS Code puts them, then the server's answer.
    parts: Vec<(Option<u8>, lsp::Markup)>,
    /// Where it hangs from: under the word, on screen.
    anchor: Pos2,
    /// The word hovered, in `char`s, for a pointer hover; `None` for `K`,
    /// which stays until the next key.
    word: Option<(usize, usize)>,
}

/// A definition in another file, shown where it is asked for rather than
/// opened: the editor holds one script, and that is the one being run.
struct Peek {
    path: PathBuf,
    line: usize,
    first: usize,
    text: String,
    anchor: Pos2,
}

/// What a key asked for, done once the view says where the cursor is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum KeyAction {
    Complete,
    Definition,
    Hover,
    NextProblem,
    PrevProblem,
}

/// The text as drawn this frame: what everything over it is placed by.
struct View {
    galley: Arc<egui::Galley>,
    /// Where the galley's top left is on screen.
    origin: Pos2,
    /// The part of the screen showing text, past the gutter.
    clip: Rect,
    /// The text the galley was laid out from, its characters, and where
    /// each line starts among them.
    text: String,
    chars: Vec<char>,
    line_starts: Vec<usize>,
    cursor: usize,
    row_height: f32,
    char_width: f32,
}

impl View {
    fn new(
        galley: Arc<egui::Galley>,
        origin: Pos2,
        clip: Rect,
        text: String,
        cursor: usize,
        row_height: f32,
        char_width: f32,
    ) -> Self {
        let chars: Vec<char> = text.chars().collect();
        let mut line_starts = vec![0];
        line_starts.extend(chars.iter().enumerate().filter(|(_, c)| **c == '\n').map(|(i, _)| i + 1));
        Self { galley, origin, clip, text, chars, line_starts, cursor, row_height, char_width }
    }

    /// Where `index` is drawn, on screen.
    fn rect(&self, index: usize) -> Rect {
        self.galley.pos_from_cursor(CCursor::new(index)).translate(self.origin.to_vec2())
    }

    fn line_of(&self, index: usize) -> usize {
        self.line_starts.partition_point(|&s| s <= index).saturating_sub(1)
    }

    /// Where `line` ends: its line break, or the text's end.
    fn line_end(&self, line: usize) -> usize {
        self.line_starts.get(line + 1).map(|&s| s - 1).unwrap_or(self.chars.len())
    }

    /// `(line, byte column)`, as Neovim counts, to a `char` index.
    fn index(&self, line: usize, byte: usize) -> usize {
        let Some(&start) = self.line_starts.get(line) else { return self.chars.len() };
        let end = self.line_end(line);
        let mut bytes = 0;
        let mut i = start;
        while i < end && bytes < byte {
            bytes += self.chars[i].len_utf8();
            i += 1;
        }
        i
    }

    /// A `char` index as `(line, byte column)`.
    fn line_col(&self, index: usize) -> (usize, usize) {
        let line = self.line_of(index);
        let start = self.line_starts[line];
        (line, self.chars[start..index.min(self.chars.len())].iter().map(|c| c.len_utf8()).sum())
    }

    fn lsp_index(&self, p: lsp::Position, encoding: lsp::Encoding) -> usize {
        let Some(&start) = self.line_starts.get(p.line as usize) else { return self.chars.len() };
        let end = self.line_end(p.line as usize);
        let mut units = 0;
        let mut i = start;
        while i < end {
            let w = match encoding {
                lsp::Encoding::Utf8 => self.chars[i].len_utf8(),
                lsp::Encoding::Utf16 => self.chars[i].len_utf16(),
            } as u32;
            if units + w > p.character {
                break;
            }
            units += w;
            i += 1;
        }
        i
    }

    fn lsp_position(&self, index: usize, encoding: lsp::Encoding) -> lsp::Position {
        let line = self.line_of(index);
        let start = self.line_starts[line];
        let character = self.chars[start..index.min(self.chars.len())]
            .iter()
            .map(|c| match encoding {
                lsp::Encoding::Utf8 => c.len_utf8() as u32,
                lsp::Encoding::Utf16 => c.len_utf16() as u32,
            })
            .sum();
        lsp::Position { line: line as u32, character }
    }

    /// The character under `pos`, if the pointer is over one -- not past
    /// the end of a line, where there is nothing to ask about.
    fn index_at(&self, pos: Pos2) -> Option<usize> {
        if !self.clip.contains(pos) {
            return None;
        }
        let line = ((pos.y - self.origin.y) / self.row_height).floor();
        if line < 0.0 || line as usize >= self.line_starts.len() {
            return None;
        }
        let line = line as usize;
        let local = pos - self.origin;
        let (start, end) = (self.line_starts[line], self.line_end(line));
        let cursor = self.galley.cursor_from_pos(local).index.0;
        // `cursor_from_pos` snaps to the nearest gap; the character is the
        // one whose cell holds the point.
        let index = if cursor > start && local.x < self.galley.pos_from_cursor(CCursor::new(cursor)).left() {
            cursor - 1
        } else {
            cursor
        };
        (index >= start && index < end && !self.chars[index].is_whitespace()).then_some(index)
    }

    /// The rectangles covering `start..end` of the text, one per line.
    fn span_rects(&self, start: usize, end: usize) -> Vec<Rect> {
        let mut rects = Vec::new();
        let mut line = self.line_of(start);
        let mut s = start;
        while s < end && line < self.line_starts.len() {
            let e = end.min(self.line_end(line));
            let a = self.rect(s);
            let right = if e > s { self.rect(e).left() } else { a.left() + self.char_width };
            rects.push(Rect::from_min_max(a.min, egui::pos2(right.max(a.left() + 2.0), a.min.y + self.row_height)));
            line += 1;
            s = match self.line_starts.get(line) {
                Some(&next) => next,
                None => break,
            };
        }
        rects
    }
}

/// The script editor's state between frames: its language servers, Neovim,
/// and whatever popup is open.
#[derive(Default)]
pub struct ScriptEditor {
    servers: HashMap<&'static str, (String, Slot)>,
    nvim: Option<nvim::Neovim>,
    /// The Neovim that could not be started, and why: not tried again until
    /// `neovim_path` changes.
    nvim_failed: Option<(String, String)>,
    nvim_exits: Vec<std::time::Instant>,
    /// The file Neovim holds and the text it was last in step with. A script
    /// that no longer hashes to this was changed from outside -- opened,
    /// reloaded -- and goes to Neovim again.
    synced: Option<(String, u64)>,
    /// The script as last read or saved, by its hash: what "unsaved" is
    /// measured against. Not Neovim's `modified`, which starts clean at
    /// whatever it was given -- a script with unsaved edits, when Neovim is
    /// switched on over one -- and would have said there was nothing to save.
    clean: Option<u64>,

    completion: Option<Completion>,
    /// A request for completions in flight, and the word start it is for.
    completion_request: Option<(i64, usize)>,
    resolve_request: Option<(i64, usize)>,
    /// A completion clicked, accepted next frame.
    clicked: Option<usize>,
    signature: Option<lsp::SignatureHelp>,
    signature_request: Option<i64>,
    hover: Option<Hover>,
    hover_request: Option<(i64, Pos2, Option<(usize, usize)>)>,
    /// The word under a resting pointer, and since when.
    rest: Option<((usize, usize), f64)>,
    definition_request: Option<(i64, Pos2)>,
    peek: Option<Peek>,
    keys: Vec<KeyAction>,
    /// The text and the cursor the last trigger check saw: `(hash, cursor,
    /// length)`.
    checked: Option<(u64, usize, usize)>,
    /// A cursor to put in the `TextEdit` next frame: a jump, a completion.
    place_cursor: Option<usize>,
    /// A completion was just put in through Neovim: the change it makes,
    /// arriving a frame or two later, is not typing and opens no new list.
    accepted: bool,
    scroll_to_cursor: bool,
    /// Last frame's popups, so the pointer moving onto one keeps it open.
    popup_rects: Vec<Rect>,
    /// A word for the status bar, for a moment: "no definition found".
    status: Option<(String, f64)>,

    /// The Neovim view's scroll: its top line, eased toward Neovim's, and
    /// how far the text is scrolled sideways.
    top: f32,
    left: f32,
    wheel: f32,
    pressed: Option<(usize, usize)>,

    /// Lines for the kalast tab of the log.
    pub log: Vec<String>,
    said: HashSet<String>,
}

impl ScriptEditor {
    /// Draw the editor into the rest of `ui`, and handle its keys.
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        script: &mut String,
        path: &str,
        lang: Lang,
        palette: &Palette,
        settings: &Settings,
        dirty: bool,
    ) -> Outcome {
        let mut out = Outcome::default();
        if !dirty {
            self.clean = Some(hash(script));
        }
        let ctx = ui.ctx().clone();
        let file = PathBuf::from(path.trim());
        let language = match lang {
            Lang::Python => Some("python"),
            Lang::Rust => Some("rust"),
            Lang::Plain => None,
        }
        .filter(|_| settings.language_servers && !path.trim().is_empty());

        // The language server for this script, started the first time it is
        // wanted. The others keep running, polled so nothing piles up.
        if let Some(language) = language {
            self.ensure_server(&ctx, language, &file, settings);
        }
        let replies = self.poll_servers(language);

        // Neovim, when on, and in step with the script.
        let vim = settings.neovim && self.ensure_nvim(&ctx, settings);
        if !settings.neovim && self.nvim.take().is_some() {
            self.synced = None;
        }
        if vim {
            self.sync_nvim(script, path, lang, &mut out);
        }
        let (diagnostics, encoding) = self.lsp_state(language);

        let full = ui.available_rect_before_wrap();
        let text_rect = Rect::from_min_max(full.min, egui::pos2(full.max.x, full.max.y - STATUS_HEIGHT));
        let status_rect = Rect::from_min_max(egui::pos2(full.min.x, full.max.y - STATUS_HEIGHT), full.max);

        // A completion list takes the keys that move and accept it before
        // the text does, and the text before the rest of the window.
        let insert = !vim || self.nvim.as_ref().is_some_and(|n| n.insert_mode());
        let mut accept = self.clicked.take().is_some_and(|row| {
            if let Some(c) = self.completion.as_mut() {
                c.selected = row;
            }
            true
        });
        if ui.memory(|m| m.has_focus(text_id())) {
            accept |= self.editor_keys(ui, vim, insert);
        }
        if accept {
            let cursor = egui::TextEdit::load_state(&ctx, text_id())
                .and_then(|s| s.cursor.char_range())
                .map(|r| r.primary.index.0);
            self.accept(script, vim, cursor, encoding, &mut out);
        }

        let view = if vim {
            self.vim_view(ui, text_rect, palette, lang, settings, &diagnostics, encoding, &mut out)
        } else {
            self.edit_view(ui, text_rect, script, lang, palette, settings, &diagnostics, encoding, &mut out)
        };

        // The server gets this frame's text before anything is asked of it:
        // synced at the top of the frame, it was a keystroke behind every
        // request made at the bottom -- asked for completions after `np.`
        // in a text that had no `.` yet.
        if let Some(server) = self.server(language) {
            server.sync(&file, script);
        }

        // What Neovim's mappings and the keys above asked for, now that the
        // cursor is known.
        if let Some(n) = self.nvim.as_mut() {
            for action in std::mem::take(&mut n.actions) {
                match action {
                    nvim::Action::Write => out.save = true,
                    nvim::Action::Quit(_) => out.quit = true,
                    nvim::Action::Hover => self.keys.push(KeyAction::Hover),
                    nvim::Action::Definition => self.keys.push(KeyAction::Definition),
                    nvim::Action::NextDiagnostic => self.keys.push(KeyAction::NextProblem),
                    nvim::Action::PrevDiagnostic => self.keys.push(KeyAction::PrevProblem),
                    nvim::Action::Open(path) => out.open = Some(PathBuf::from(path)),
                }
            }
        }
        for action in std::mem::take(&mut self.keys) {
            match action {
                KeyAction::Complete => {
                    self.completion = None;
                    let position = view.lsp_position(view.cursor, encoding);
                    let mut start = view.cursor;
                    while start > 0 && is_word(view.chars[start - 1]) {
                        start -= 1;
                    }
                    if let Some(id) = self.server(language).and_then(|s| s.completion(position, None)) {
                        self.completion_request = Some((id, start));
                    }
                }
                KeyAction::Definition => self.ask_definition(language, &view, view.cursor, encoding),
                KeyAction::Hover => self.ask_hover(language, &view, view.cursor, None, &diagnostics, encoding),
                KeyAction::NextProblem => self.jump_problem(&view, true, &diagnostics, encoding),
                KeyAction::PrevProblem => self.jump_problem(&view, false, &diagnostics, encoding),
            }
        }

        self.replies(replies, &file, &view, &diagnostics, encoding);
        let settled = self.nvim.as_ref().is_none_or(|n| n.settled());
        if settled {
            self.triggers(language, &view, vim, encoding);
        }
        self.pointer_hover(ui, language, &view, &diagnostics, encoding);
        self.draw_popups(ui, &view, palette, settings, &mut out);
        self.status_bar(ui, status_rect, language, &view, settings, &diagnostics);
        ui.allocate_rect(full, egui::Sense::hover());
        out
    }

    /// A save landed: Neovim's buffer is no longer modified, and servers
    /// that check on save do it now.
    pub fn saved(&mut self) {
        if let Some(n) = self.nvim.as_mut() {
            n.saved();
        }
        for (_, slot) in self.servers.values() {
            if let Slot::Running(s) = slot {
                s.saved();
            }
        }
    }

    /// Say `line` in the log, once per session.
    fn say(&mut self, line: impl Into<String>) {
        let line = line.into();
        if self.said.insert(line.clone()) {
            self.log.push(line);
        }
    }

    fn server(&mut self, language: Option<&str>) -> Option<&mut lsp::Server> {
        match self.servers.get_mut(language?) {
            Some((_, Slot::Running(s))) if s.ready() => Some(s),
            _ => None,
        }
    }

    /// The current language server's diagnostics and position encoding.
    fn lsp_state(&self, language: Option<&str>) -> (Vec<lsp::Diagnostic>, lsp::Encoding) {
        match language.and_then(|l| self.servers.get(l)) {
            Some((_, Slot::Running(s))) if s.ready() => (s.diagnostics.clone(), s.encoding),
            _ => (Vec::new(), lsp::Encoding::Utf16),
        }
    }

    /// Take every server's news: this language's replies, and the others'
    /// dropped, so nothing piles up.
    fn poll_servers(&mut self, language: Option<&str>) -> Vec<lsp::Reply> {
        let mut replies = Vec::new();
        let mut died = Vec::new();
        let mut said = Vec::new();
        for (id, (_, slot)) in self.servers.iter_mut() {
            let Slot::Running(server) = slot else { continue };
            let r = server.poll();
            said.append(&mut server.messages);
            if Some(*id) == language {
                replies = r;
            }
            if let lsp::Status::Gone(why) = &server.status {
                died.push((*id, why.clone()));
            }
        }
        for line in said {
            self.say(line);
        }
        for (id, why) in died {
            self.say(format!("{why}; the editor goes on without it"));
            if let Some((_, slot)) = self.servers.get_mut(id) {
                *slot = Slot::Failed;
            }
        }
        replies
    }

    fn ensure_server(&mut self, ctx: &egui::Context, language: &'static str, file: &Path, settings: &Settings) {
        let configured = match language {
            "python" => settings.python_language_server,
            _ => settings.rust_language_server,
        };
        let python = python();
        // A bundle's packages, found by path: its interpreter is inside
        // `kalast.exe`, which answers no `-c`. See `set_python`.
        let search: Vec<PathBuf> = crate::app::bundled_site_packages().into_iter().collect();
        let key = format!("{configured}|{}", python.as_ref().map(|p| p.display().to_string()).unwrap_or_default());
        if self.servers.get(language).is_some_and(|(k, _)| *k == key) {
            return;
        }
        let slot = match lsp::find(language, configured, python.as_deref()) {
            None => {
                self.say(match language {
                    "python" => "no Python language server found, so no completion or hover: install one \
                                 with `uv tool install basedpyright` (or pip), or name one in \
                                 app.config.python_language_server"
                        .to_string(),
                    _ => "no rust-analyzer found, so no completion or hover in Rust: \
                          `rustup component add rust-analyzer`"
                        .to_string(),
                });
                Slot::Failed
            }
            Some(spec) => {
                let root = root_for(language, file);
                let repaint = ctx.clone();
                let name = spec.name.clone();
                let settings = lsp::settings(python.as_deref(), &search);
                let started = lsp::Server::start(spec, language, &root, settings, move || {
                    repaint.request_repaint()
                });
                match started {
                    Ok(server) => {
                        self.say(format!("{name} started for {language}, in {}", root.display()));
                        Slot::Running(server)
                    }
                    Err(e) => {
                        self.say(e);
                        Slot::Failed
                    }
                }
            }
        };
        self.servers.insert(language, (key, slot));
    }

    fn ensure_nvim(&mut self, ctx: &egui::Context, settings: &Settings) -> bool {
        if let Some(n) = &self.nvim {
            let Some(why) = n.gone.clone() else { return true };
            // It went: `:qa`, a crash, a config calling `:quit`. Once is a
            // restart; three times in a minute is a loop to get out of.
            self.nvim = None;
            self.synced = None;
            self.nvim_exits.retain(|t| t.elapsed().as_secs() < 60);
            self.nvim_exits.push(std::time::Instant::now());
            if self.nvim_exits.len() >= 3 {
                let why = format!("{why}, three times in a minute; the editor goes on without it");
                self.say(why.clone());
                self.nvim_failed = Some((settings.neovim_path.to_string(), why));
                return false;
            }
            self.log.push(format!("{why}; started again"));
        }
        match &self.nvim_failed {
            Some((path, _)) if path == settings.neovim_path => return false,
            Some(_) => self.nvim_failed = None,
            None => {}
        }
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let repaint = ctx.clone();
        match nvim::Neovim::start(settings.neovim_path, &cwd, (COLUMNS, 40), move || repaint.request_repaint()) {
            Ok(n) => {
                self.nvim = Some(n);
                self.synced = None;
                true
            }
            Err(e) => {
                self.say(e.clone());
                self.nvim_failed = Some((settings.neovim_path.to_string(), e));
                false
            }
        }
    }

    /// Take Neovim's edits into the script, or give Neovim a script changed
    /// from outside.
    fn sync_nvim(&mut self, script: &mut String, path: &str, lang: Lang, out: &mut Outcome) {
        let Some(n) = self.nvim.as_mut() else { return };
        n.poll();
        let current = hash(script);
        match &self.synced {
            Some((p, h)) if p == path && *h == current => {
                if n.take_edited() {
                    *script = n.text();
                    let now = hash(script);
                    self.synced = Some((path.to_string(), now));
                    out.changed = true;
                    out.modified = Some(self.clean != Some(now));
                }
            }
            synced => {
                let same_file = synced.as_ref().is_some_and(|(p, _)| p == path);
                let filetype = match lang {
                    Lang::Python => "python",
                    Lang::Rust => "rust",
                    Lang::Plain => "",
                };
                n.load(script, path.trim(), filetype, same_file.then_some(n.cursor.0));
                self.synced = Some((path.to_string(), current));
                self.completion = None;
                self.signature = None;
                self.checked = None;
            }
        }
    }

    /// The keys the editor answers itself, before the text sees them: the
    /// completion list's, Ctrl+Space, F12, F8, and Escape over a popup.
    /// Returns whether the selected completion was accepted.
    fn editor_keys(&mut self, ui: &mut egui::Ui, vim: bool, insert: bool) -> bool {
        use egui::{Key, Modifiers};
        let consume = |ui: &mut egui::Ui, m: Modifiers, k: Key| ui.input_mut(|i| i.consume_key(m, k));
        let mut accept = false;
        if insert && self.completion.as_ref().is_some_and(|c| !c.shown.is_empty()) {
            let mut step: i64 = 0;
            if consume(ui, Modifiers::NONE, Key::ArrowDown) || consume(ui, Modifiers::CTRL, Key::N) {
                step += 1;
            }
            if consume(ui, Modifiers::NONE, Key::ArrowUp) || consume(ui, Modifiers::CTRL, Key::P) {
                step -= 1;
            }
            if consume(ui, Modifiers::NONE, Key::PageDown) {
                step += 9;
            }
            if consume(ui, Modifiers::NONE, Key::PageUp) {
                step -= 9;
            }
            accept = consume(ui, Modifiers::NONE, Key::Tab)
                || consume(ui, Modifiers::NONE, Key::Enter)
                || consume(ui, Modifiers::CTRL, Key::Y);
            let mut dismiss = consume(ui, Modifiers::CTRL, Key::E);
            // Escape closes the list; under Neovim it leaves insert mode as
            // well, as it does with blink.cmp.
            if vim {
                dismiss |= ui.input(|i| i.key_pressed(Key::Escape));
            } else {
                dismiss |= consume(ui, Modifiers::NONE, Key::Escape);
            }
            if let Some(c) = self.completion.as_mut() {
                let n = c.shown.len() as i64;
                c.selected = (c.selected as i64 + step).rem_euclid(n.max(1)) as usize;
            }
            if dismiss {
                self.completion = None;
            }
        }
        if insert && consume(ui, Modifiers::CTRL, Key::Space) {
            self.keys.push(KeyAction::Complete);
        }
        if !vim && consume(ui, Modifiers::NONE, Key::Escape) {
            self.hover = None;
            self.signature = None;
            self.peek = None;
        }
        if consume(ui, Modifiers::NONE, Key::F12) {
            self.keys.push(KeyAction::Definition);
        }
        if consume(ui, Modifiers::NONE, Key::F8) {
            self.keys.push(KeyAction::NextProblem);
        }
        if consume(ui, Modifiers::SHIFT, Key::F8) {
            self.keys.push(KeyAction::PrevProblem);
        }
        accept
    }

    /// Put the selected completion in: in the text, or through Neovim.
    fn accept(&mut self, script: &mut String, vim: bool, cursor: Option<usize>, encoding: lsp::Encoding, out: &mut Outcome) {
        let Some(c) = self.completion.take() else { return };
        let Some(&(index, _)) = c.shown.get(c.selected) else { return };
        let item = &c.list.items[index];
        if vim {
            let Some(n) = self.nvim.as_mut() else { return };
            let text = n.lines.join("\n");
            let cursor = char_index(&text, n.cursor.0, n.cursor.1);
            let (edits, at) = completion_edits(&text, cursor, c.anchor, item, encoding);
            let mut after = text.clone();
            apply(&mut after, &edits);
            let edits: Vec<_> = edits.iter().map(|(s, e, t)| (line_col(&text, *s), line_col(&text, *e), t.clone())).collect();
            n.apply(&edits, line_col(&after, at));
            self.accepted = true;
        } else {
            let cursor = cursor.unwrap_or_else(|| script.chars().count());
            let (edits, at) = completion_edits(script, cursor, c.anchor, item, encoding);
            apply(script, &edits);
            self.place_cursor = Some(at);
            self.scroll_to_cursor = true;
            out.changed = true;
        }
        self.signature_request = None;
        self.checked = None;
    }

    /// The text in egui's `TextEdit`, with the gutter, the band on the
    /// cursor's line and the ruler around it.
    #[allow(clippy::too_many_arguments)]
    fn edit_view(
        &mut self,
        ui: &mut egui::Ui,
        rect: Rect,
        script: &mut String,
        lang: Lang,
        palette: &Palette,
        settings: &Settings,
        diagnostics: &[lsp::Diagnostic],
        encoding: lsp::Encoding,
        out: &mut Outcome,
    ) -> View {
        let font = FontId::monospace(13.0);
        let (row_height, char_width) = ui.ctx().fonts_mut(|f| (f.row_height(&font), f.glyph_width(&font, '0')));
        let mut layouter = |ui: &egui::Ui, buf: &dyn egui::TextBuffer, _wrap: f32| {
            ui.ctx().fonts_mut(|f| f.layout_job(code::layout(buf.as_str(), lang, palette, font.clone())))
        };
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect).layout(egui::Layout::top_down(egui::Align::Min)));
        let mut view = None;
        // The code fills the middle, straight on its card: framed and 24
        // rows tall, it was a panel inside the panel.
        egui::ScrollArea::both().id_salt("script scroll").auto_shrink([false, false]).show(&mut child, |ui| {
            let id = text_id();
            let ctx = ui.ctx().clone();
            let focused = ui.memory(|m| m.has_focus(id));
            if let Some(at) = self.place_cursor.take() {
                if let Some(mut state) = egui::TextEdit::load_state(&ctx, id) {
                    state.cursor.set_char_range(Some(CCursorRange::one(CCursor::new(at))));
                    state.store(&ctx, id);
                }
            }
            // Tab and Shift+Tab indent, as in VS Code: taken before the edit
            // sees them, which would put in a tab character -- and a tab in
            // Python is an error waiting beside four spaces.
            if focused {
                let (tab, untab) = ui.input_mut(|i| {
                    (
                        i.consume_key(egui::Modifiers::NONE, egui::Key::Tab),
                        i.consume_key(egui::Modifiers::SHIFT, egui::Key::Tab),
                    )
                });
                if let (true, Some(mut state)) = (tab || untab, egui::TextEdit::load_state(&ctx, id)) {
                    if let Some(range) = state.cursor.char_range() {
                        let range = if tab { code::tab(script, range) } else { code::untab(script, range) };
                        state.cursor.set_char_range(Some(range));
                        state.store(&ctx, id);
                        out.changed = true;
                    }
                }
            }
            let enter = focused && ui.input(|i| i.key_pressed(egui::Key::Enter) && !i.modifiers.any());
            let command = ui.input(|i| i.modifiers.command);

            // The gutter: wide enough for the last line's number.
            let digits = (script.lines().count() + 1).to_string().len().max(2);
            let gutter = digits as f32 * char_width + 28.0;
            // Reserved behind the text: the cursor line's band, and the ruler
            // and the tint of lines with errors.
            let band = ui.painter().add(egui::Shape::Noop);
            let behind = ui.painter().add(egui::Shape::Noop);
            ui.horizontal_top(|ui| {
                ui.add_space(gutter);
                let output = egui::TextEdit::multiline(script)
                    .id(id)
                    .code_editor()
                    .frame(egui::Frame::NONE)
                    .layouter(&mut layouter)
                    .desired_width(f32::INFINITY)
                    .min_size(ui.available_size())
                    .show(ui);
                // The cursor where it was left when the text has no focus
                // -- a dialog up, a click on the panel -- not the start.
                let mut cursor = output
                    .cursor_range
                    .or_else(|| output.state.cursor.char_range())
                    .map(|r| r.primary.index.0)
                    .unwrap_or(0);
                if output.response.changed() {
                    out.changed = true;
                    // Enter keeps the indentation, and opens a level after
                    // `:` or `{`.
                    if let (true, Some(range)) = (enter, output.cursor_range) {
                        let at = code::indent_new_line(script, range.primary.index.0, lang);
                        let mut state = output.state.clone();
                        state.cursor.set_char_range(Some(CCursorRange::one(CCursor::new(at))));
                        state.store(ui.ctx(), id);
                        cursor = at;
                    }
                }
                // Ctrl+click: the definition, as in VS Code.
                if output.response.clicked() && command {
                    self.keys.push(KeyAction::Definition);
                }

                // Line numbers, the current one lit and those with errors in
                // their colour, and the band across the whole width -- VS
                // Code's.
                let current = output.cursor_range.map(|r| output.galley.pos_from_cursor(r.primary).center().y);
                let painter = ui.painter();
                let worst = worst_by_line(diagnostics);
                for (n, row) in output.galley.rows.iter().enumerate() {
                    let y = row.rect().y_range();
                    let here = current.is_some_and(|c| y.contains(c));
                    let y = egui::Rangef::new(y.min + output.galley_pos.y, y.max + output.galley_pos.y);
                    let color = match worst.get(&n) {
                        Some(&s) if s <= 2 => severity_color(s, settings.theme),
                        _ if here => palette.gutter_current,
                        _ => palette.gutter,
                    };
                    painter.text(
                        egui::pos2(output.galley_pos.x - 14.0, y.center()),
                        egui::Align2::RIGHT_CENTER,
                        (n + 1).to_string(),
                        font.clone(),
                        color,
                    );
                    if here {
                        let band_rect = Rect::from_x_y_ranges(ui.clip_rect().x_range(), y);
                        painter.set(band, egui::Shape::rect_filled(band_rect, 0.0, palette.current_line));
                    }
                }
                let clip = Rect::from_min_max(egui::pos2(output.galley_pos.x, ui.clip_rect().top()), ui.clip_rect().max);
                let v = View::new(output.galley.clone(), output.galley_pos, clip, script.clone(), cursor, row_height, char_width);
                painter.set(behind, egui::Shape::Vec(behind_text(&v, settings, diagnostics)));
                draw_diagnostics(painter, &v, diagnostics, encoding, settings.theme);
                if std::mem::take(&mut self.scroll_to_cursor) {
                    ui.scroll_to_rect(v.rect(cursor), Some(egui::Align::Center));
                }
                view = Some(v);
            });
        });
        view.expect("the text is drawn")
    }

    /// The text as Neovim has it: drawn here, every key and click sent there.
    #[allow(clippy::too_many_arguments)]
    fn vim_view(
        &mut self,
        ui: &mut egui::Ui,
        rect: Rect,
        palette: &Palette,
        lang: Lang,
        settings: &Settings,
        diagnostics: &[lsp::Diagnostic],
        encoding: lsp::Encoding,
        out: &mut Outcome,
    ) -> View {
        let font = FontId::monospace(13.0);
        let (row_height, char_width) = ui.ctx().fonts_mut(|f| (f.row_height(&font), f.glyph_width(&font, '0')));
        let n = self.nvim.as_mut().expect("Neovim is running");
        let text = n.lines.join("\n");
        let galley = ui.ctx().fonts_mut(|f| f.layout_job(code::layout(&text, lang, palette, font.clone())));
        let digits = n.lines.len().to_string().len().max(2);
        let gutter = digits as f32 * char_width + 28.0;
        let text_rect = Rect::from_min_max(egui::pos2(rect.left() + gutter, rect.top()), rect.max);

        let id = text_id();
        let response = ui.interact(rect, id, egui::Sense::click_and_drag());
        if response.clicked() || response.drag_started() || response.is_pointer_button_down_on() {
            response.request_focus();
        }
        // egui's own record of the focus, not `Response::has_focus`, which is
        // also false whenever the OS window is not the active one: the keys
        // go where egui sends them, and whether the window is active only
        // decides how the cursor is drawn.
        let focused = ui.memory(|m| m.has_focus(id));
        let window_active = ui.input(|i| i.focused);
        if focused {
            // Every key is Neovim's: Tab, the arrows and Escape included,
            // which egui would otherwise take to move the focus.
            ui.memory_mut(|m| {
                m.set_focus_lock_filter(
                    id,
                    egui::EventFilter { tab: true, horizontal_arrows: true, vertical_arrows: true, escape: true },
                )
            });
        }
        if response.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Text);
        }

        // Neovim's window is as tall as the text shown, so `H`, `L`,
        // `<C-d>` and `scrolloff` count what is on screen.
        let rows = (text_rect.height() / row_height).floor().max(1.0) as u32;
        n.resize(COLUMNS, rows);

        // The keys, all of them, in Neovim's notation.
        if focused {
            let (keys, pastes, save) = collect_keys(ui, n.insert_mode() || n.cmdline().is_some());
            let typed = !keys.is_empty() || !pastes.is_empty();
            n.input(&keys);
            for p in pastes {
                n.paste(&p);
            }
            out.save |= save;
            if typed {
                // A key closes what `K` opened, as Neovim closes its floats.
                if self.hover.as_ref().is_some_and(|h| h.word.is_none()) {
                    self.hover = None;
                }
                self.peek = None;
            }
        }

        // The view follows Neovim's top line, eased so a jump of a page
        // reads as a scroll rather than a cut.
        let target = n.topline as f32;
        if (target - self.top).abs() > 0.01 {
            self.top += (target - self.top) * 0.35;
            if (target - self.top).abs() < 0.02 {
                self.top = target;
            }
            ui.ctx().request_repaint();
        }
        let view_text = text.clone();
        let chars_cursor = {
            let v = View::new(galley.clone(), Pos2::ZERO, Rect::NOTHING, view_text, 0, row_height, char_width);
            let c = v.index(n.cursor.0, n.cursor.1);
            (v, c)
        };
        let (mut view, cursor) = chars_cursor;
        view.cursor = cursor;
        let cursor_rect = galley.pos_from_cursor(CCursor::new(cursor));
        // Sideways is kalast's own: the cursor kept in view with a margin.
        let width = text_rect.width();
        if cursor_rect.left() - self.left < 0.0 {
            self.left = (cursor_rect.left() - 4.0 * char_width).max(0.0);
        } else if cursor_rect.right() - self.left > width - char_width {
            self.left = cursor_rect.right() - width + 4.0 * char_width;
        }
        let origin = egui::pos2(text_rect.left() - self.left, rect.top() - self.top * row_height);
        view.origin = origin;
        view.clip = text_rect;

        // The pointer: pressed, dragged and released on a cell, and the
        // wheel as Neovim scrolls -- `mousescroll` lines a notch.
        let cell = |pos: Pos2| -> (usize, usize) {
            let line = ((pos.y - origin.y) / row_height).floor().max(0.0) as usize;
            let col = ((pos.x - origin.x) / char_width).round().max(0.0) as usize;
            (line, col)
        };
        let modifiers = ui.input(|i| {
            let mut m = String::new();
            if i.modifiers.ctrl {
                m.push('C');
            }
            if i.modifiers.alt {
                m.push('A');
            }
            if i.modifiers.shift {
                m.push('S');
            }
            m
        });
        let down = ui.input(|i| i.pointer.primary_down());
        if let Some(pos) = ui.input(|i| i.pointer.interact_pos()) {
            let starts_here = response.is_pointer_button_down_on() && text_rect.contains(pos);
            if down && (starts_here || self.pressed.is_some()) {
                let at = cell(pos);
                match self.pressed {
                    None => n.mouse("left", "press", &modifiers, at.0, at.1),
                    Some(last) if last != at => n.mouse("left", "drag", &modifiers, at.0, at.1),
                    _ => {}
                }
                self.pressed = Some(at);
            } else if let Some((line, col)) = self.pressed.take() {
                n.mouse("left", "release", &modifiers, line, col);
            } else if response.clicked() && text_rect.contains(pos) {
                // Pressed and released within one frame -- a tap, or a
                // click faster than the frame: both halves at once.
                let (line, col) = cell(pos);
                n.mouse("left", "press", &modifiers, line, col);
                n.mouse("left", "release", &modifiers, line, col);
            }
            // Ctrl+click: the definition, as in VS Code.
            if response.clicked() && ui.input(|i| i.modifiers.command) {
                self.keys.push(KeyAction::Definition);
            }
        }
        if response.hovered() {
            let delta = ui.input(|i| i.smooth_scroll_delta);
            if delta.x != 0.0 {
                self.left = (self.left - delta.x).max(0.0);
            }
            self.wheel += delta.y;
            let notch = 3.0 * row_height;
            while self.wheel.abs() >= notch {
                let up = self.wheel > 0.0;
                self.wheel -= notch.copysign(self.wheel);
                n.mouse("wheel", if up { "up" } else { "down" }, &modifiers, n.topline, 0);
            }
        }

        let painter = ui.painter_at(rect);
        let row_y = |line: usize| origin.y + line as f32 * row_height;
        let editing = n.cmdline().is_none();
        // The cursor's line: VS Code's band, Neovim's `cursorline`.
        if editing {
            let y = row_y(n.cursor.0);
            painter.rect_filled(
                Rect::from_min_max(egui::pos2(rect.left(), y), egui::pos2(rect.right(), y + row_height)),
                0.0,
                palette.current_line,
            );
        }
        let text_painter = painter.with_clip_rect(text_rect);
        for shape in behind_text(&view, settings, diagnostics) {
            text_painter.add(shape);
        }
        // The selection, as Neovim holds it.
        if let Some(v) = n.visual {
            let selection = ui.visuals().selection.bg_fill;
            for r in selection_rects(&view, v) {
                text_painter.rect_filled(r, 0.0, selection);
            }
        }
        text_painter.galley(origin, galley.clone(), ui.visuals().text_color());
        draw_diagnostics(&text_painter, &view, diagnostics, encoding, settings.theme);

        // The cursor, in the mode's shape: a block over its character in
        // normal mode, a bar in insert -- hollow while the editor does not
        // have the keyboard.
        if editing && n.ready {
            let cell = view.rect(cursor);
            let next = view.rect(cursor + 1);
            let w = if next.top() > cell.top() + 1.0 || next.left() <= cell.left() { char_width } else { next.left() - cell.left() };
            let color = cursor_color(settings.theme);
            match n.shape() {
                nvim::Shape::Block => {
                    let r = Rect::from_min_size(cell.min, egui::vec2(w, row_height));
                    if focused && window_active {
                        text_painter.rect_filled(r, 1.0, color);
                        if let Some(c) = view.chars.get(cursor).filter(|c| !c.is_whitespace()) {
                            text_painter.text(r.left_center(), egui::Align2::LEFT_CENTER, c, font.clone(), ui.visuals().extreme_bg_color);
                        }
                    } else {
                        text_painter.rect_stroke(r, 1.0, Stroke::new(1.0, color), egui::StrokeKind::Inside);
                    }
                }
                nvim::Shape::Vertical(f) => {
                    let r = Rect::from_min_size(cell.min, egui::vec2((w * f).max(2.0), row_height));
                    text_painter.rect_filled(r, 0.0, color);
                }
                nvim::Shape::Horizontal(f) => {
                    let h = (row_height * f).max(2.0);
                    let r = Rect::from_min_size(egui::pos2(cell.left(), cell.top() + row_height - h), egui::vec2(w, h));
                    text_painter.rect_filled(r, 0.0, color);
                }
            }
        }

        // The gutter, over any text scrolled under it: line numbers,
        // relative ones when Neovim's `relativenumber` says so.
        let gutter_rect = Rect::from_min_max(rect.min, egui::pos2(text_rect.left() - 4.0, rect.bottom()));
        painter.rect_filled(gutter_rect, 0.0, ui.visuals().panel_fill);
        if editing {
            let y = row_y(n.cursor.0);
            painter.rect_filled(
                Rect::from_min_max(egui::pos2(rect.left(), y), egui::pos2(gutter_rect.right(), y + row_height)),
                0.0,
                palette.current_line,
            );
        }
        let worst = worst_by_line(diagnostics);
        let first = self.top.floor().max(0.0) as usize;
        for line in first..(first + rows as usize + 2).min(n.lines.len()) {
            let here = line == n.cursor.0;
            let label = if n.relativenumber && !here {
                line.abs_diff(n.cursor.0).to_string()
            } else if n.number || n.relativenumber {
                (line + 1).to_string()
            } else {
                String::new()
            };
            let color = match worst.get(&line) {
                Some(&s) if s <= 2 => severity_color(s, settings.theme),
                _ if here => palette.gutter_current,
                _ => palette.gutter,
            };
            // The current line's number sits left under `relativenumber`,
            // as Neovim sets it apart.
            let (x, align) = if n.relativenumber && here {
                (gutter_rect.left() + 8.0, egui::Align2::LEFT_CENTER)
            } else {
                (gutter_rect.right() - 10.0, egui::Align2::RIGHT_CENTER)
            };
            painter.text(egui::pos2(x, row_y(line) + row_height / 2.0), align, label, font.clone(), color);
        }

        // Where the view is in the file, as a thin thumb on the right.
        let total = n.lines.len().max(1) as f32;
        if total > rows as f32 {
            let track = rect.height();
            let h = (rows as f32 / total * track).max(16.0);
            let y = rect.top() + (self.top / total) * track;
            painter.rect_filled(
                Rect::from_min_size(egui::pos2(rect.right() - 6.0, y), egui::vec2(4.0, h)),
                2.0,
                ui.visuals().widgets.inactive.bg_fill,
            );
        }
        view
    }

    /// Replies from the server, into the popups they answer.
    fn replies(&mut self, replies: Vec<lsp::Reply>, file: &Path, view: &View, diagnostics: &[lsp::Diagnostic], encoding: lsp::Encoding) {
        for reply in replies {
            match reply {
                lsp::Reply::Completion(id, list) => {
                    let Some((wanted, anchor)) = self.completion_request else { continue };
                    if wanted != id {
                        continue;
                    }
                    self.completion_request = None;
                    let mut c = Completion { list, anchor, shown: Vec::new(), selected: 0, scroll: 0, followed: None, wheel: 0.0 };
                    filter(&mut c, view);
                    self.completion = (!c.shown.is_empty()).then_some(c);
                }
                lsp::Reply::Resolved(id, item) => {
                    let Some((wanted, index)) = self.resolve_request else { continue };
                    if wanted != id {
                        continue;
                    }
                    self.resolve_request = None;
                    if let Some(target) = self.completion.as_mut().and_then(|c| c.list.items.get_mut(index)) {
                        if !item.documentation.is_empty() {
                            target.documentation = item.documentation;
                        }
                        if !item.detail.is_empty() {
                            target.detail = item.detail;
                        }
                        // Resolved once is resolved.
                        target.raw = serde_json::Value::Null;
                    }
                }
                lsp::Reply::Hover(id, hover) => {
                    let Some((wanted, anchor, word)) = self.hover_request else { continue };
                    if wanted != id {
                        continue;
                    }
                    self.hover_request = None;
                    let at = word.map(|w| w.0).unwrap_or(view.cursor);
                    let mut parts = diagnostics_at(diagnostics, view, at, encoding);
                    if let Some(h) = hover {
                        parts.push((None, h.contents));
                    }
                    self.hover = (!parts.is_empty()).then_some(Hover { parts, anchor, word });
                }
                lsp::Reply::Signature(id, help) => {
                    if self.signature_request != Some(id) {
                        continue;
                    }
                    self.signature_request = None;
                    self.signature = help;
                }
                lsp::Reply::Definition(id, locations) => {
                    let Some((wanted, anchor)) = self.definition_request else { continue };
                    if wanted != id {
                        continue;
                    }
                    self.definition_request = None;
                    let Some(location) = locations.into_iter().next() else {
                        self.flash("no definition found");
                        continue;
                    };
                    if lsp::same_path(file, &location.path) {
                        let at = view.lsp_index(location.range.start, encoding);
                        self.jump(view, at);
                    } else if let Ok(source) = std::fs::read_to_string(&location.path) {
                        let line = location.range.start.line as usize;
                        let first = line.saturating_sub(3);
                        let text = source.lines().skip(first).take(14).collect::<Vec<_>>().join("\n");
                        self.peek = Some(Peek { path: location.path, line, first, text, anchor });
                    } else {
                        self.flash(&format!("defined in {}", location.path.display()));
                    }
                }
            }
        }
    }

    /// A word on the status bar for a few seconds.
    fn flash(&mut self, message: &str) {
        self.status = Some((message.to_string(), f64::NAN));
    }

    /// Move the cursor to `at`: through Neovim, keeping `''` for `<C-o>`,
    /// or in the `TextEdit`.
    fn jump(&mut self, view: &View, at: usize) {
        if let Some(n) = self.nvim.as_mut().filter(|n| n.ready) {
            let (line, col) = view.line_col(at);
            n.jump(line, col);
        } else {
            self.place_cursor = Some(at);
            self.scroll_to_cursor = true;
        }
    }

    fn ask_hover(
        &mut self,
        language: Option<&str>,
        view: &View,
        at: usize,
        word: Option<(usize, usize)>,
        diagnostics: &[lsp::Diagnostic],
        encoding: lsp::Encoding,
    ) {
        let anchor = view.rect(word.map(|w| w.0).unwrap_or(at)).left_bottom() + egui::vec2(-6.0, 3.0);
        let position = view.lsp_position(at, encoding);
        match self.server(language).and_then(|s| s.hover(position)) {
            Some(id) => self.hover_request = Some((id, anchor, word)),
            // No server: the diagnostics alone, if there are any.
            None => {
                let parts = diagnostics_at(diagnostics, view, at, encoding);
                self.hover = (!parts.is_empty()).then_some(Hover { parts, anchor, word });
            }
        }
    }

    fn ask_definition(&mut self, language: Option<&str>, view: &View, at: usize, encoding: lsp::Encoding) {
        let anchor = view.rect(at).left_bottom() + egui::vec2(-6.0, 3.0);
        let position = view.lsp_position(at, encoding);
        match self.server(language).and_then(|s| s.definition(position)) {
            Some(id) => self.definition_request = Some((id, anchor)),
            None => self.flash("no language server for definitions"),
        }
    }

    /// `]d` and `[d`, F8 and Shift+F8: the next error or warning after the
    /// cursor, or the one before, with its message.
    fn jump_problem(&mut self, view: &View, forward: bool, diagnostics: &[lsp::Diagnostic], encoding: lsp::Encoding) {
        let mut starts: Vec<usize> = diagnostics
            .iter()
            .filter(|d| d.severity <= 2 && !d.unnecessary)
            .map(|d| view.lsp_index(d.range.start, encoding))
            .collect();
        starts.sort_unstable();
        starts.dedup();
        let target = if forward {
            starts.iter().find(|&&s| s > view.cursor).or(starts.first())
        } else {
            starts.iter().rev().find(|&&s| s < view.cursor).or(starts.last())
        };
        let Some(&at) = target else {
            self.flash("no problems");
            return;
        };
        self.jump(view, at);
        let anchor = view.rect(at).left_bottom() + egui::vec2(-6.0, 3.0);
        let parts = diagnostics_at(diagnostics, view, at, encoding);
        self.hover = (!parts.is_empty()).then_some(Hover { parts, anchor, word: None });
    }

    /// After an edit: open, refilter or close the completion list, and the
    /// signature over a call.
    fn triggers(&mut self, language: Option<&str>, view: &View, vim: bool, encoding: lsp::Encoding) {
        if vim && self.nvim.as_ref().is_some_and(|n| !n.insert_mode()) {
            self.completion = None;
            self.signature = None;
            self.checked = None;
            return;
        }
        let text_hash = hash(&view.text);
        let len = view.text.len();
        let Some((last_hash, last_cursor, last_len)) = self.checked.replace((text_hash, view.cursor, len)) else {
            return;
        };
        let changed = last_hash != text_hash;
        let grew = changed && len > last_len;
        let moved = last_cursor != view.cursor;
        if !changed && !moved {
            return;
        }
        if changed && std::mem::take(&mut self.accepted) {
            return;
        }
        let cursor = view.cursor.min(view.chars.len());
        let before = cursor.checked_sub(1).map(|i| view.chars[i]);
        let mut start = cursor;
        while start > 0 && is_word(view.chars[start - 1]) {
            start -= 1;
        }
        let position = view.lsp_position(cursor, encoding);
        let Some(server) = self.server(language) else { return };
        let triggers = server.capabilities.completion_triggers.clone();
        let signature_triggers = server.capabilities.signature_triggers.clone();

        // The completion list: filtered as the word grows, asked for again
        // if the server said its list was incomplete, closed when the cursor
        // leaves the word.
        let typed_trigger = before.map(|c| c.to_string()).filter(|c| grew && triggers.contains(c));
        let mut request = None;
        if let Some(c) = self.completion.as_mut() {
            let inside = cursor >= c.anchor && view.chars[c.anchor..cursor].iter().all(|&ch| is_word(ch));
            if !inside || typed_trigger.is_some() {
                self.completion = None;
            } else {
                filter(c, view);
                if c.list.incomplete && grew {
                    request = Some((None, c.anchor));
                } else if c.shown.is_empty() {
                    self.completion = None;
                }
            }
        }
        if self.completion.is_none() && request.is_none() {
            if let Some(t) = &typed_trigger {
                request = Some((Some(t.clone()), cursor));
            } else if grew
                && before.is_some_and(is_word)
                && !view.chars[start].is_ascii_digit()
                && self.completion_request.is_none_or(|(_, a)| a != start)
            {
                // A word being typed: VS Code's quick suggestions.
                request = Some((None, start));
            } else if !grew {
                self.completion_request = None;
            }
        }
        if let Some((trigger, anchor)) = request {
            if let Some(id) = self.server(language).and_then(|s| s.completion(position, trigger.as_deref())) {
                self.completion_request = Some((id, anchor));
            }
        }

        // The signature over a call: asked as `(` or `,` is typed, and asked
        // again as the cursor moves while it shows.
        let signature_trigger = before.map(|c| c.to_string()).filter(|c| grew && signature_triggers.contains(c));
        let showing = self.signature.is_some();
        let asked = match signature_trigger {
            Some(t) => self.server(language).and_then(|s| s.signature(position, Some(&t), showing)),
            None if showing => self.server(language).and_then(|s| s.signature(position, None, true)),
            None => return,
        };
        self.signature_request = asked;
    }

    /// A pointer resting on a word asks for its hover; moving off the word
    /// and its popup closes it.
    fn pointer_hover(&mut self, ui: &egui::Ui, language: Option<&str>, view: &View, diagnostics: &[lsp::Diagnostic], encoding: lsp::Encoding) {
        let now = ui.input(|i| i.time);
        let pointer = ui.input(|i| i.pointer.hover_pos());
        let over_popup = pointer.is_some_and(|p| self.popup_rects.iter().any(|r| r.expand(6.0).contains(p)));
        let busy = ui.input(|i| i.pointer.any_down()) || self.completion.is_some();
        let index = pointer.filter(|_| !over_popup && !busy).and_then(|p| view.index_at(p));
        let word = index.map(|i| word_at(&view.chars, i).unwrap_or((i, i + 1)));
        // Off the word the hover was for, and off its popup: gone.
        if self.hover.as_ref().is_some_and(|h| h.word.is_some() && !over_popup && h.word != word) {
            self.hover = None;
        }
        match word {
            Some(w) => {
                let since = match self.rest {
                    Some((r, t)) if r == w => t,
                    _ => {
                        self.rest = Some((w, now));
                        now
                    }
                };
                let shown = self.hover.as_ref().is_some_and(|h| h.word == Some(w));
                let asked = self.hover_request.is_some_and(|(_, _, r)| r == Some(w));
                if now - since >= HOVER_DELAY {
                    if !shown && !asked {
                        self.ask_hover(language, view, w.0, Some(w), diagnostics, encoding);
                    }
                } else {
                    ui.ctx().request_repaint_after(std::time::Duration::from_secs_f64(HOVER_DELAY));
                }
            }
            None if !over_popup => self.rest = None,
            None => {}
        }
    }

    fn draw_popups(&mut self, ui: &mut egui::Ui, view: &View, palette: &Palette, settings: &Settings, out: &mut Outcome) {
        let ctx = ui.ctx().clone();
        let screen = ctx.content_rect();
        let t = settings.theme;
        self.popup_rects.clear();

        // Neovim's own menu -- `<C-n>`, the command line's `<Tab>` -- drawn
        // where Neovim would.
        if let Some(popup) = self.nvim.as_ref().and_then(|n| n.popup.clone()) {
            let rows = popup.items.len().min(12);
            let height = rows as f32 * 20.0 + 12.0;
            let pos = if popup.cmdline {
                egui::pos2(view.clip.left() + popup.col as f32 * view.char_width, view.clip.bottom() - height)
            } else {
                let cursor = view.rect(view.cursor);
                if cursor.bottom() + height < screen.bottom() {
                    cursor.left_bottom()
                } else {
                    cursor.left_top() - egui::vec2(0.0, height)
                }
            };
            let first = popup.selected.map(|s| (s + 1).saturating_sub(rows)).unwrap_or(0);
            let (_, selected_fill, _) = theme::list(t);
            let r = egui::Area::new(egui::Id::new("kalast nvim menu"))
                .order(egui::Order::Foreground)
                .fixed_pos(pos)
                .show(&ctx, |ui| {
                    popup_frame(ui).show(ui, |ui| {
                        ui.spacing_mut().item_spacing.y = 0.0;
                        for (i, (word, kind, menu)) in popup.items.iter().enumerate().skip(first).take(rows) {
                            let (rect, _) = ui.allocate_exact_size(egui::vec2(260.0, 20.0), egui::Sense::hover());
                            if popup.selected == Some(i) {
                                ui.painter().rect_filled(rect, 3.0, selected_fill);
                            }
                            ui.painter().text(
                                rect.left_center() + egui::vec2(6.0, 0.0),
                                egui::Align2::LEFT_CENTER,
                                word,
                                FontId::monospace(13.0),
                                ui.visuals().strong_text_color(),
                            );
                            let tail = format!("{kind} {menu}");
                            ui.painter().text(
                                rect.right_center() - egui::vec2(6.0, 0.0),
                                egui::Align2::RIGHT_CENTER,
                                tail.trim(),
                                FontId::proportional(12.0),
                                ui.visuals().weak_text_color(),
                            );
                        }
                    })
                })
                .response
                .rect;
            self.popup_rects.push(r);
        }

        // The completion list, under the cursor -- over it when there is no
        // room below -- with the selected item's documentation beside it.
        let mut resolve = None;
        let mut clicked = None;
        if let Some(c) = self.completion.as_mut() {
            let rows = c.shown.len().min(10);
            if c.followed != Some(c.selected) {
                if c.selected < c.scroll {
                    c.scroll = c.selected;
                } else if c.selected >= c.scroll + rows {
                    c.scroll = c.selected + 1 - rows;
                }
                c.followed = Some(c.selected);
            }
            c.scroll = c.scroll.min(c.shown.len() - rows);
            let row_h = 22.0;
            let width = 420.0;
            let height = rows as f32 * row_h + 12.0;
            let cursor = view.rect(view.cursor);
            let x = view.rect(c.anchor.min(view.cursor)).left() - 32.0;
            let pos = if cursor.bottom() + 2.0 + height < screen.bottom() {
                egui::pos2(x, cursor.bottom() + 2.0)
            } else {
                egui::pos2(x, cursor.top() - 2.0 - height)
            };
            let (hover_fill, selected_fill, _) = theme::list(t);
            let accent = theme::accent(t);
            let list = egui::Area::new(egui::Id::new("kalast completion"))
                .order(egui::Order::Foreground)
                .fixed_pos(pos)
                .show(&ctx, |ui| {
                    popup_frame(ui).show(ui, |ui| {
                        ui.set_width(width);
                        ui.spacing_mut().item_spacing.y = 0.0;
                        // The pointer moving over a row selects it, so its
                        // documentation shows beside the list and `Enter`
                        // takes what is lit. Only moving: a pointer resting
                        // on the list while a word is typed, or while the
                        // wheel scrolls rows under it, would otherwise take
                        // the selection from the keyboard.
                        let moved = ui.input(|i| i.pointer.delta() != egui::Vec2::ZERO);
                        for row in c.scroll..(c.scroll + rows).min(c.shown.len()) {
                            let (index, matched) = &c.shown[row];
                            let item = &c.list.items[*index];
                            let (rect, response) = ui.allocate_exact_size(egui::vec2(width, row_h), egui::Sense::click());
                            if moved && response.hovered() {
                                c.selected = row;
                            }
                            let selected = row == c.selected;
                            if selected {
                                ui.painter().rect_filled(rect, 3.0, selected_fill);
                            } else if response.hovered() {
                                ui.painter().rect_filled(rect, 3.0, hover_fill);
                            }
                            if response.clicked() {
                                clicked = Some(row);
                            }
                            let (icon, color) = kind_icon(item.kind, t);
                            ui.painter().text(
                                rect.left_center() + egui::vec2(13.0, 0.0),
                                egui::Align2::CENTER_CENTER,
                                icon,
                                FontId::proportional(15.0),
                                color,
                            );
                            // The label, the letters typed in the accent.
                            let text = ui.visuals().text_color();
                            let weak = ui.visuals().weak_text_color();
                            let base = TextFormat {
                                font_id: FontId::monospace(13.0),
                                color: text,
                                strikethrough: if item.deprecated { Stroke::new(1.0, weak) } else { Stroke::NONE },
                                ..Default::default()
                            };
                            let mut job = LayoutJob::default();
                            for (i, ch) in item.label.chars().enumerate() {
                                let color = if matched.contains(&i) { accent } else { text };
                                job.append(&ch.to_string(), 0.0, TextFormat { color, ..base.clone() });
                            }
                            if !item.label_detail.is_empty() {
                                job.append(&item.label_detail, 0.0, TextFormat { color: weak, ..base.clone() });
                            }
                            // The selected item's detail at the row's end, as
                            // VS Code puts it, when it is short.
                            let detail = (selected && !item.detail.is_empty() && item.detail.chars().count() < 36)
                                .then(|| ui.painter().layout_no_wrap(item.detail.clone(), FontId::proportional(12.0), weak));
                            let room = width - 34.0 - detail.as_ref().map(|g| g.size().x + 16.0).unwrap_or(0.0);
                            job.wrap = egui::text::TextWrapping::truncate_at_width(room);
                            let galley = ui.painter().layout_job(job);
                            ui.painter().galley(
                                egui::pos2(rect.left() + 28.0, rect.center().y - galley.size().y / 2.0),
                                galley,
                                text,
                            );
                            if let Some(g) = detail {
                                ui.painter().galley(
                                    egui::pos2(rect.right() - 8.0 - g.size().x, rect.center().y - g.size().y / 2.0),
                                    g,
                                    weak,
                                );
                            }
                        }
                        // The wheel, by whole rows, the selection staying where
                        // it is, as VS Code's list scrolls. egui spreads a
                        // notch over several frames, a few points each, which
                        // rounded to rows one frame at a time was nothing.
                        if ui.rect_contains_pointer(ui.min_rect()) {
                            c.wheel += ui.input(|i| i.smooth_scroll_delta.y);
                            let steps = (c.wheel / row_h).trunc();
                            if steps != 0.0 {
                                c.wheel -= steps * row_h;
                                let last = (c.shown.len() - rows) as i64;
                                c.scroll = (c.scroll as i64 - steps as i64).clamp(0, last) as usize;
                            }
                        } else {
                            c.wheel = 0.0;
                        }
                    })
                })
                .response
                .rect;
            self.popup_rects.push(list);

            // The documentation, beside the list: fetched from the server the
            // first time an item is selected, as blink.cmp and VS Code show it.
            if let Some(&(index, _)) = c.shown.get(c.selected) {
                let item = &c.list.items[index];
                if !item.raw.is_null() && item.documentation.is_empty() {
                    resolve = Some(index);
                }
                let long_detail = item.detail.chars().count() >= 36;
                if !item.documentation.is_empty() || long_detail {
                    let (detail, docs) = (item.detail.clone(), item.documentation.clone());
                    let side = if list.right() + 400.0 < screen.right() {
                        egui::pos2(list.right() + 4.0, list.top())
                    } else {
                        egui::pos2(list.left() - 404.0, list.top())
                    };
                    let r = egui::Area::new(egui::Id::new("kalast completion docs"))
                        .order(egui::Order::Foreground)
                        .fixed_pos(side)
                        .show(&ctx, |ui| {
                            popup_frame(ui).show(ui, |ui| {
                                ui.set_max_width(380.0);
                                egui::ScrollArea::vertical().max_height(300.0).min_scrolled_height(300.0).show(ui, |ui| {
                                    if !detail.is_empty() {
                                        let lang = if detail.contains("fn ") || detail.contains("::") { Lang::Rust } else { Lang::Python };
                                        let mut job = code::layout(&detail, lang, palette, FontId::monospace(12.5));
                                        job.wrap.max_width = 370.0;
                                        ui.add(egui::Label::new(job));
                                    }
                                    if !docs.is_empty() {
                                        if !detail.is_empty() {
                                            ui.separator();
                                        }
                                        markdown::show(ui, &docs, palette, 370.0);
                                    }
                                });
                            })
                        })
                        .response
                        .rect;
                    self.popup_rects.push(r);
                }
            }
        }
        self.clicked = clicked;
        if let Some(index) = resolve.filter(|&i| self.resolve_request.is_none_or(|(_, r)| r != i)) {
            let item = self.completion.as_ref().map(|c| c.list.items[index].clone());
                if let Some(item) = item {
                let id = self
                    .servers
                    .values_mut()
                    .find_map(|(_, s)| match s {
                        Slot::Running(s) if s.ready() && s.capabilities.resolve => s.resolve(&item),
                        _ => None,
                    });
                if let Some(id) = id {
                    self.resolve_request = Some((id, index));
                }
            }
        }

        // The signature, over the cursor's line.
        if let Some(sig) = self.signature.as_ref().and_then(|h| h.signatures.get(h.active).map(|s| (s, h))) {
            let (sig, help) = sig;
            let cursor = view.rect(view.cursor);
            let accent = theme::accent(t);
            let r = egui::Area::new(egui::Id::new("kalast signature"))
                .order(egui::Order::Foreground)
                .pivot(egui::Align2::LEFT_BOTTOM)
                .fixed_pos(egui::pos2(cursor.left() - 8.0, cursor.top() - 4.0))
                .show(&ctx, |ui| {
                    popup_frame(ui).show(ui, |ui| {
                        ui.set_max_width(520.0);
                        let plain = TextFormat { font_id: FontId::monospace(13.0), color: ui.visuals().text_color(), ..Default::default() };
                        let strong = TextFormat { color: accent, underline: Stroke::new(1.0, accent), ..plain.clone() };
                        let span = sig.active_parameter.and_then(|p| sig.parameters.get(p)).map(|(s, _)| *s);
                        let mut job = LayoutJob::default();
                        for (i, ch) in sig.label.chars().enumerate() {
                            let f = if span.is_some_and(|(s, e)| i >= s && i < e) { strong.clone() } else { plain.clone() };
                            job.append(&ch.to_string(), 0.0, f);
                        }
                        job.wrap.max_width = 510.0;
                        ui.add(egui::Label::new(job));
                        if help.signatures.len() > 1 {
                            ui.label(egui::RichText::new(format!("{} of {}", help.active + 1, help.signatures.len())).weak().small());
                        }
                        if let Some((_, docs)) = sig.active_parameter.and_then(|p| sig.parameters.get(p)) {
                            if !docs.is_empty() {
                                markdown::show(ui, docs, palette, 510.0);
                            }
                        }
                        if !sig.documentation.is_empty() {
                            ui.separator();
                            egui::ScrollArea::vertical().max_height(160.0).min_scrolled_height(160.0).show(ui, |ui| {
                                markdown::show(ui, &sig.documentation, palette, 510.0);
                            });
                        }
                    })
                })
                .response
                .rect;
            self.popup_rects.push(r);
        }

        // The hover: the diagnostics under the pointer, then the server's
        // answer -- the symbol's type and its docs.
        if let Some(hover) = &self.hover {
            let r = egui::Area::new(egui::Id::new("kalast hover"))
                .order(egui::Order::Foreground)
                .fixed_pos(hover.anchor)
                .show(&ctx, |ui| {
                    popup_frame(ui).show(ui, |ui| {
                        ui.set_max_width(560.0);
                        egui::ScrollArea::vertical().max_height(340.0).min_scrolled_height(340.0).show(ui, |ui| {
                            for (i, (severity, markup)) in hover.parts.iter().enumerate() {
                                if i > 0 {
                                    ui.separator();
                                }
                                match severity {
                                    Some(s) => {
                                        ui.horizontal_top(|ui| {
                                            ui.label(egui::RichText::new(severity_icon(*s)).color(severity_color(*s, t)).size(15.0));
                                            ui.add(egui::Label::new(egui::RichText::new(&markup.text).color(ui.visuals().text_color())).wrap());
                                        });
                                    }
                                    None => markdown::show(ui, markup, palette, 540.0),
                                }
                            }
                        });
                    })
                })
                .response
                .rect;
            self.popup_rects.push(r);
        }

        // A definition elsewhere, peeked at.
        let mut open = false;
        if let Some(peek) = &self.peek {
            let lang = Lang::of(&peek.path.to_string_lossy());
            let r = egui::Area::new(egui::Id::new("kalast peek"))
                .order(egui::Order::Foreground)
                .fixed_pos(peek.anchor)
                .show(&ctx, |ui| {
                    popup_frame(ui).show(ui, |ui| {
                        ui.set_max_width(680.0);
                        let name = format!("{}:{}", peek.path.display(), peek.line + 1);
                        open = ui
                            .add(egui::Label::new(egui::RichText::new(name).small().color(theme::accent(t))).sense(egui::Sense::click()))
                            .on_hover_text("Open this file in the editor")
                            .clicked();
                        ui.separator();
                        let mut job = LayoutJob::default();
                        for (i, line) in peek.text.lines().enumerate() {
                            let n = peek.first + i;
                            let number = TextFormat { font_id: FontId::monospace(12.0), color: palette.gutter, ..Default::default() };
                            job.append(&format!("{:>4}  ", n + 1), 0.0, number);
                            let lj = code::layout(line, lang, palette, FontId::monospace(12.5));
                            for section in &lj.sections {
                                let mut f = section.format.clone();
                                if n == peek.line {
                                    f.background = palette.current_line;
                                }
                                job.append(&lj.text[section.byte_range.start.0..section.byte_range.end.0], 0.0, f);
                            }
                            job.append("\n", 0.0, TextFormat::default());
                        }
                        ui.add(egui::Label::new(job));
                    })
                })
                .response
                .rect;
            self.popup_rects.push(r);
        }
        if open {
            out.open = self.peek.take().map(|p| p.path);
        }
        // A click anywhere else closes the peek, and a hover `K` opened.
        if ui.input(|i| i.pointer.any_pressed()) {
            let p = ui.input(|i| i.pointer.interact_pos());
            if p.is_some_and(|p| !self.popup_rects.iter().any(|r| r.contains(p))) {
                self.peek = None;
                if self.hover.as_ref().is_some_and(|h| h.word.is_none()) {
                    self.hover = None;
                }
            }
        }
    }

    /// The bar under the text: Neovim's mode, keys and messages or its
    /// command line on the left; the problems, the position and the language
    /// server on the right -- VS Code's status bar.
    fn status_bar(&mut self, ui: &mut egui::Ui, rect: Rect, language: Option<&str>, view: &View, settings: &Settings, diagnostics: &[lsp::Diagnostic]) {
        let t = settings.theme;
        let now = ui.input(|i| i.time);
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, theme::side_fill(t));
        painter.hline(rect.x_range(), rect.top(), Stroke::new(1.0, theme::outline(t)));
        let font = FontId::proportional(12.0);
        let weak = ui.visuals().weak_text_color();
        let text_color = ui.visuals().text_color();
        let y = rect.center().y;

        // The right side first, so the left knows where to stop.
        let mut right = rect.right() - 10.0;
        let server = language.and_then(|l| match self.servers.get(l) {
            Some((_, Slot::Running(s))) => Some(match (&s.status, s.progress()) {
                (lsp::Status::Starting, _) => format!("{} starting\u{2026}", s.spec.name),
                (_, Some(p)) => format!("{}: {p}", s.spec.name),
                _ => s.spec.name.clone(),
            }),
            _ => None,
        });
        if let Some(label) = server {
            place_right(&painter, &mut right, y, &label, weak, &font, 14.0);
        }
        if let Some(n) = &self.nvim {
            place_right(&painter, &mut right, y, if n.ready { "Neovim" } else { "Neovim starting\u{2026}" }, weak, &font, 14.0);
        } else if settings.neovim && self.nvim_failed.is_some() {
            place_right(&painter, &mut right, y, "Neovim did not start, see the log", severity_color(2, t), &font, 14.0);
        }
        let line = view.line_of(view.cursor);
        let column = view.cursor.min(view.chars.len()) - view.line_starts[line];
        place_right(&painter, &mut right, y, &format!("Ln {}, Col {}", line + 1, column + 1), text_color, &font, 14.0);
        if self.server(language).is_some() {
            let icon_font = FontId::proportional(14.0);
            let errors = diagnostics.iter().filter(|d| d.severity == 1).count();
            let warnings = diagnostics.iter().filter(|d| d.severity == 2).count();
            place_right(&painter, &mut right, y, &warnings.to_string(), text_color, &font, 4.0);
            place_right(&painter, &mut right, y, severity_icon(2), if warnings > 0 { severity_color(2, t) } else { weak }, &icon_font, 10.0);
            place_right(&painter, &mut right, y, &errors.to_string(), text_color, &font, 4.0);
            place_right(&painter, &mut right, y, severity_icon(1), if errors > 0 { severity_color(1, t) } else { weak }, &icon_font, 14.0);
        }

        let mut x = rect.left() + 6.0;
        // A word said for a moment -- "no definition found".
        let flash = match &mut self.status {
            Some((m, since)) => {
                if since.is_nan() {
                    *since = now;
                }
                (now - *since < 3.0).then(|| m.clone())
            }
            None => None,
        };
        if flash.is_none() {
            self.status = None;
        } else {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(500));
        }

        let Some(n) = &self.nvim else {
            if let Some(m) = flash {
                painter.text(egui::pos2(x, y), egui::Align2::LEFT_CENTER, m, font, text_color);
            }
            return;
        };
        // The mode, as lualine paints it.
        use theme::palette as p;
        let (name, color) = match n.mode.as_str() {
            m if m.starts_with("insert") => ("INSERT", p::GREEN),
            m if m.starts_with("replace") => ("REPLACE", p::RED),
            m if m.starts_with("cmdline") => ("COMMAND", p::PEACH),
            _ if n.visual.is_some() => (
                match n.visual.map(|v| v.kind) {
                    Some('V') => "V-LINE",
                    Some('\x16') => "V-BLOCK",
                    _ => "VISUAL",
                },
                p::MAUVE,
            ),
            "operator" => ("O-PENDING", p::BLUE),
            _ => ("NORMAL", p::BLUE),
        };
        let g = painter.layout_no_wrap(name.to_string(), FontId::monospace(12.0), p::CRUST);
        let pill = Rect::from_min_size(egui::pos2(x, rect.top() + 3.0), egui::vec2(g.size().x + 14.0, rect.height() - 6.0));
        painter.rect_filled(pill, 3.0, color);
        painter.galley(pill.center() - g.size() / 2.0, g, p::CRUST);
        x = pill.right() + 10.0;

        // The command line being typed, with its cursor; otherwise the keys
        // pending and the last message.
        if let Some(c) = n.cmdline() {
            let mono = FontId::monospace(13.0);
            let head = format!("{}{}{}", c.firstc, c.prompt, " ".repeat(c.indent));
            let strong = ui.visuals().strong_text_color();
            let g = painter.layout_no_wrap(format!("{head}{}", c.content), mono.clone(), strong);
            painter.galley(egui::pos2(x, y - g.size().y / 2.0), g, strong);
            let before = format!("{head}{}", c.content.get(..c.pos).unwrap_or(&c.content));
            let w = painter.layout_no_wrap(before, mono, strong).size().x;
            painter.rect_filled(Rect::from_min_size(egui::pos2(x + w, y - 8.0), egui::vec2(2.0, 16.0)), 0.0, cursor_color(t));
        } else {
            for text in [n.showmode.trim(), n.showcmd.trim()] {
                if !text.is_empty() {
                    let g = painter.layout_no_wrap(text.to_string(), FontId::monospace(12.0), text_color);
                    painter.galley(egui::pos2(x, y - g.size().y / 2.0), g.clone(), text_color);
                    x += g.size().x + 12.0;
                }
            }
            let foreign = n.foreign.then(|| {
                ("Neovim is in another buffer, not shown here: Ctrl+^ goes back to the script".to_string(), severity_color(2, t))
            });
            let message = flash.map(|m| (m, text_color)).or(foreign).or_else(|| {
                n.messages.iter().rev().find(|m| !m.is_list()).map(|m| {
                    let color = match m.kind.as_str() {
                        "emsg" | "echoerr" | "lua_error" | "rpc_error" => severity_color(1, t),
                        "wmsg" => severity_color(2, t),
                        _ => text_color,
                    };
                    (m.text.trim().to_string(), color)
                })
            });
            if let Some((text, color)) = message {
                let mut job = LayoutJob::single_section(text, TextFormat { font_id: font.clone(), color, ..Default::default() });
                job.wrap = egui::text::TextWrapping::truncate_at_width((right - x).max(20.0));
                let g = painter.layout_job(job);
                painter.galley(egui::pos2(x, y - g.size().y / 2.0), g, color);
            }
        }

        // A message of several lines -- `:ls`, `:reg`, `:messages` -- in a
        // panel over the text, until the next key.
        let lists: Vec<&str> = n.messages.iter().filter(|m| m.is_list()).map(|m| m.text.trim_end()).collect();
        if !lists.is_empty() {
            let text = lists.join("\n");
            let lines = text.lines().count().min(24) as f32;
            let height = lines * 16.0 + 14.0;
            egui::Area::new(egui::Id::new("kalast nvim messages"))
                .order(egui::Order::Foreground)
                .fixed_pos(egui::pos2(rect.left(), rect.top() - height))
                .show(ui.ctx(), |ui| {
                    egui::Frame::NONE
                        .fill(theme::side_fill(t))
                        .inner_margin(6.0)
                        .stroke(Stroke::new(1.0, theme::outline(t)))
                        .show(ui, |ui| {
                            ui.set_width(rect.width() - 12.0);
                            egui::ScrollArea::vertical().max_height(height).stick_to_bottom(true).show(ui, |ui| {
                                ui.label(egui::RichText::new(text).monospace().color(text_color));
                            });
                        });
                });
        }
    }
}

fn hash(text: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::hash::DefaultHasher::new();
    text.hash(&mut h);
    h.finish()
}

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// The word around `index`, in `char`s: `(start, end)`.
fn word_at(chars: &[char], index: usize) -> Option<(usize, usize)> {
    if !chars.get(index).is_some_and(|&c| is_word(c)) {
        return None;
    }
    let mut start = index;
    while start > 0 && is_word(chars[start - 1]) {
        start -= 1;
    }
    let mut end = index;
    while end < chars.len() && is_word(chars[end]) {
        end += 1;
    }
    Some((start, end))
}

/// A `char` index as `(line, byte column)`, as Neovim counts.
fn line_col(text: &str, index: usize) -> (usize, usize) {
    let mut line = 0;
    let mut col = 0;
    for c in text.chars().take(index) {
        if c == '\n' {
            line += 1;
            col = 0;
        } else {
            col += c.len_utf8();
        }
    }
    (line, col)
}

/// `(line, byte column)` back to a `char` index, clamped to the line.
fn char_index(text: &str, line: usize, col: usize) -> usize {
    let mut index = 0;
    let mut current = 0;
    let mut bytes = 0;
    for c in text.chars() {
        if current == line {
            if c == '\n' || bytes >= col {
                return index;
            }
            bytes += c.len_utf8();
        } else if c == '\n' {
            current += 1;
        }
        index += 1;
    }
    index
}

/// Apply `edits` -- `(start, end, text)` in `char`s, last first -- to `text`.
fn apply(text: &mut String, edits: &[(usize, usize, String)]) {
    for (start, end, new) in edits {
        let byte = |i: usize| text.char_indices().nth(i).map(|(b, _)| b).unwrap_or(text.len());
        let (s, e) = (byte(*start), byte(*end));
        text.replace_range(s..e.max(s), new);
    }
}

/// A completion as edits to the text, last first, and where the cursor
/// ends: its own text over the word typed so far, and any it brings along,
/// such as an import at the top.
fn completion_edits(
    text: &str,
    cursor: usize,
    anchor: usize,
    item: &lsp::CompletionItem,
    encoding: lsp::Encoding,
) -> (Vec<(usize, usize, String)>, usize) {
    let (start, end) = match item.range {
        Some(r) => {
            let s = lsp::index(text, r.start, encoding);
            (s.min(cursor), lsp::index(text, r.end, encoding).max(cursor))
        }
        None => (anchor.min(cursor), cursor),
    };
    let mut edits = vec![(start, end, item.insert.clone())];
    let mut at = start + item.cursor.unwrap_or_else(|| item.insert.chars().count());
    for (range, new) in &item.additional {
        let (s, e) = (lsp::index(text, range.start, encoding), lsp::index(text, range.end, encoding));
        if e <= start {
            at = at + new.chars().count() - (e - s);
        }
        edits.push((s, e, new.clone()));
    }
    edits.sort_by(|a, b| b.0.cmp(&a.0));
    (edits, at)
}

/// A fuzzy match of what was typed against a completion, as VS Code ranks
/// its suggestions: every character in order, better at the start of the
/// label or of a word inside it, better in a run, better in the same case.
/// `None` when it does not match; the matched characters, to light them.
fn fuzzy(query: &str, candidate: &str) -> Option<(i32, Vec<usize>)> {
    if query.is_empty() {
        return Some((0, Vec::new()));
    }
    let q: Vec<char> = query.chars().collect();
    let c: Vec<char> = candidate.chars().collect();
    let mut score = 0;
    let mut matched = Vec::with_capacity(q.len());
    let mut qi = 0;
    for (ci, &ch) in c.iter().enumerate() {
        if qi == q.len() {
            break;
        }
        if !ch.to_lowercase().eq(q[qi].to_lowercase()) {
            continue;
        }
        let word_start =
            ci == 0 || !is_word(c[ci - 1]) || c[ci - 1] == '_' || (ch.is_uppercase() && c[ci - 1].is_lowercase());
        score += 1;
        if ch == q[qi] {
            score += 1;
        }
        if ci == 0 {
            score += 8;
        } else if word_start {
            score += 5;
        }
        if matched.last().is_some_and(|&last| last + 1 == ci) {
            score += 4;
        }
        matched.push(ci);
        qi += 1;
    }
    if qi < q.len() {
        return None;
    }
    score -= (c.len().saturating_sub(q.len()) as i32).min(20) / 4;
    Some((score, matched))
}

/// The list's items matching what has been typed since the anchor, best
/// first.
fn filter(c: &mut Completion, view: &View) {
    let cursor = view.cursor.min(view.chars.len());
    let typed: String = view.chars[c.anchor.min(cursor)..cursor].iter().collect();
    let mut scored: Vec<(i32, usize, Vec<usize>)> = c
        .list
        .items
        .iter()
        .enumerate()
        .filter_map(|(i, item)| {
            let (score, _) = fuzzy(&typed, &item.filter_text)?;
            let (_, matched) = fuzzy(&typed, &item.label).unwrap_or((0, Vec::new()));
            Some((score, i, matched))
        })
        .collect();
    scored.sort_by(|a, b| {
        b.0.cmp(&a.0)
            .then_with(|| c.list.items[a.1].sort_text.cmp(&c.list.items[b.1].sort_text))
            .then_with(|| c.list.items[a.1].label.cmp(&c.list.items[b.1].label))
    });
    c.shown = scored.into_iter().map(|(_, i, m)| (i, m)).collect();
    c.selected = 0;
    c.scroll = 0;
    c.followed = None;
}

/// A completion's kind as VS Code draws it: its codicon and colour.
fn kind_icon(kind: u8, t: UiTheme) -> (&'static str, Color32) {
    use theme::palette as p;
    let (purple, blue, orange, text, peach) = match t {
        UiTheme::Dark => (
            Color32::from_rgb(0xb1, 0x80, 0xd7),
            Color32::from_rgb(0x75, 0xbe, 0xff),
            Color32::from_rgb(0xee, 0x9d, 0x28),
            Color32::from_rgb(0xcc, 0xcc, 0xcc),
            Color32::from_rgb(0xee, 0x9d, 0x28),
        ),
        UiTheme::CatppuccinMocha => (p::MAUVE, p::BLUE, p::YELLOW, p::TEXT, p::PEACH),
    };
    match kind {
        2..=4 => ("\u{ea8c}", purple), // method, function, constructor
        5 => ("\u{eb5f}", blue),       // field
        6 => ("\u{ea88}", blue),       // variable
        7 => ("\u{eb5b}", orange),     // class
        8 => ("\u{eb61}", blue),       // interface
        9 => ("\u{ea8b}", text),       // module
        10 => ("\u{eb65}", text),      // property
        11 => ("\u{ea96}", text),      // unit
        12 | 13 => ("\u{ea95}", peach), // value, enum
        14 => ("\u{eb62}", text),      // keyword
        15 => ("\u{eb66}", text),      // snippet
        16 => ("\u{eb5c}", text),      // color
        17 => ("\u{ea7b}", text),      // file
        18 => ("\u{ea94}", text),      // reference
        19 => ("\u{ea83}", text),      // folder
        20 => ("\u{eb5e}", blue),      // enum member
        21 => ("\u{eb5d}", peach),     // constant
        22 => ("\u{ea91}", orange),    // struct
        23 => ("\u{ea86}", orange),    // event
        24 => ("\u{eb64}", text),      // operator
        25 => ("\u{ea92}", orange),    // type parameter
        _ => ("\u{ea93}", text),       // text
    }
}

/// An error's, a warning's or an information's colour.
fn severity_color(severity: u8, t: UiTheme) -> Color32 {
    use theme::palette as p;
    match (severity, t) {
        (1, UiTheme::Dark) => Color32::from_rgb(0xf1, 0x4c, 0x4c),
        (2, UiTheme::Dark) => Color32::from_rgb(0xcc, 0xa7, 0x00),
        (_, UiTheme::Dark) => Color32::from_rgb(0x37, 0x94, 0xff),
        (1, _) => p::RED,
        (2, _) => p::YELLOW,
        _ => p::BLUE,
    }
}

fn severity_icon(severity: u8) -> &'static str {
    match severity {
        1 => "\u{ea87}",
        2 => "\u{ea6c}",
        _ => "\u{ea74}",
    }
}

/// The frame every popup is drawn in: VS Code's suggest widget and hover --
/// the panel colour, a thin outline, rounded a little.
fn popup_frame(ui: &egui::Ui) -> egui::Frame {
    egui::Frame::popup(ui.style())
        .inner_margin(egui::Margin::same(6))
        .corner_radius(egui::CornerRadius::same(5))
}

/// The cursor's colour: Catppuccin's rosewater, VS Code's grey.
fn cursor_color(t: UiTheme) -> Color32 {
    match t {
        UiTheme::CatppuccinMocha => theme::palette::ROSEWATER,
        UiTheme::Dark => Color32::from_gray(0xae),
    }
}

/// `text` right-aligned at `right`, which moves left past it and `gap`.
#[allow(clippy::too_many_arguments)]
fn place_right(painter: &egui::Painter, right: &mut f32, y: f32, text: &str, color: Color32, font: &FontId, gap: f32) {
    let g = painter.layout_no_wrap(text.to_string(), font.clone(), color);
    *right -= g.size().x;
    painter.galley(egui::pos2(*right, y - g.size().y / 2.0), g, color);
    *right -= gap;
}

/// Where a server starts. rust-analyzer's: the cargo workspace the file is
/// in. The Python server's: the nearest folder above the script that
/// configures pyright -- a `pyrightconfig.json`, a `pyproject.toml` with a
/// `[tool.pyright]` or `[tool.basedpyright]` -- or else the script's own.
///
/// Not the folder kalast was started in, which it was. A server indexes its
/// whole workspace for completions that add their own import, and from the
/// kalast repository that was 2877 files, most of them the bundled Python
/// under `dist/`: the first completion waited behind all of them, longer
/// than anyone waits for one.
fn root_for(language: &str, file: &Path) -> PathBuf {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let file = std::path::absolute(file).unwrap_or_else(|_| file.to_path_buf());
    if language != "rust" {
        let configured = |dir: &Path| {
            dir.join("pyrightconfig.json").is_file()
                || std::fs::read_to_string(dir.join("pyproject.toml"))
                    .is_ok_and(|t| t.contains("[tool.pyright]") || t.contains("[tool.basedpyright]"))
        };
        return file
            .ancestors()
            .skip(1)
            .find(|dir| configured(dir))
            .or_else(|| file.parent())
            .map(Path::to_path_buf)
            .unwrap_or(cwd);
    }
    let mut nearest = None;
    for dir in file.ancestors().skip(1) {
        let manifest = dir.join("Cargo.toml");
        if manifest.is_file() {
            nearest.get_or_insert_with(|| dir.to_path_buf());
            if std::fs::read_to_string(&manifest).is_ok_and(|m| m.contains("[workspace]")) {
                return dir.to_path_buf();
            }
        }
    }
    nearest.unwrap_or(cwd)
}

/// The worst severity on each line with a diagnostic.
fn worst_by_line(diagnostics: &[lsp::Diagnostic]) -> HashMap<usize, u8> {
    let mut worst = HashMap::new();
    for d in diagnostics.iter().filter(|d| !d.unnecessary) {
        let e = worst.entry(d.range.start.line as usize).or_insert(d.severity);
        *e = (*e).min(d.severity);
    }
    worst
}

/// The diagnostics covering `index`, for a hover.
fn diagnostics_at(diagnostics: &[lsp::Diagnostic], view: &View, index: usize, encoding: lsp::Encoding) -> Vec<(Option<u8>, lsp::Markup)> {
    diagnostics
        .iter()
        .filter(|d| d.severity <= 3 && !d.unnecessary)
        .filter(|d| {
            let s = view.lsp_index(d.range.start, encoding);
            let e = view.lsp_index(d.range.end, encoding).max(s + 1);
            index >= s && index < e
        })
        .map(|d| {
            let source = if d.source.is_empty() { String::new() } else { format!("  ({})", d.source) };
            (Some(d.severity), lsp::Markup { text: format!("{}{source}", d.message), plain: true })
        })
        .collect()
}

/// Neovim's selection as rectangles over the text.
fn selection_rects(view: &View, v: nvim::Visual) -> Vec<Rect> {
    let (a, b) = if v.start <= v.end { (v.start, v.end) } else { (v.end, v.start) };
    let line_rect = |from: usize, to: usize| {
        let r = view.rect(from);
        let right = view.rect(to).left() + view.char_width;
        Rect::from_min_max(r.min, egui::pos2(right.max(r.left() + view.char_width), r.min.y + view.row_height))
    };
    match v.kind {
        'V' => (a.0..=b.0)
            .filter(|&l| l < view.line_starts.len())
            .map(|line| line_rect(view.line_starts[line], view.line_end(line)))
            .collect(),
        '\x16' => {
            let (c0, c1) = (v.start.1.min(v.end.1), v.start.1.max(v.end.1));
            (a.0..=b.0)
                .filter(|&l| l < view.line_starts.len())
                .map(|line| {
                    let (from, to) = (view.index(line, c0), view.index(line, c1));
                    let r = view.rect(from);
                    let right = if to > from || c1 > c0 { view.rect(to).left() + view.char_width } else { r.left() + view.char_width };
                    Rect::from_min_max(r.min, egui::pos2(right, r.min.y + view.row_height))
                })
                .collect()
        }
        _ => {
            let (s, e) = (view.index(a.0, a.1), view.index(b.0, b.1));
            (a.0..=b.0)
                .filter(|&l| l < view.line_starts.len())
                .map(|line| {
                    let from = if line == a.0 { s } else { view.line_starts[line] };
                    let to = if line == b.0 { e } else { view.line_end(line) };
                    line_rect(from, to)
                })
                .collect()
        }
    }
}

/// What is drawn behind the text: the ruler, and the faint tint VS Code's
/// Error Lens gives a line with an error or a warning.
fn behind_text(view: &View, settings: &Settings, diagnostics: &[lsp::Diagnostic]) -> Vec<egui::Shape> {
    let mut shapes = Vec::new();
    for (line, severity) in worst_by_line(diagnostics) {
        if severity > 2 || line >= view.line_starts.len() {
            continue;
        }
        let r = view.rect(view.line_starts[line]);
        shapes.push(egui::Shape::rect_filled(
            Rect::from_min_max(egui::pos2(view.clip.left(), r.top()), egui::pos2(view.clip.right(), r.top() + view.row_height)),
            0.0,
            severity_color(severity, settings.theme).gamma_multiply(0.08),
        ));
    }
    if settings.ruler > 0 {
        let x = (view.origin.x + settings.ruler as f32 * view.char_width).round() + 0.5;
        let color = match settings.theme {
            UiTheme::CatppuccinMocha => theme::palette::SURFACE1,
            UiTheme::Dark => Color32::from_gray(0x4a),
        };
        shapes.push(egui::Shape::vline(x, view.clip.y_range(), Stroke::new(1.0, color)));
    }
    shapes
}

/// The diagnostics over the text: errors and warnings underlined with a
/// squiggle and the worst of each line spelled out at its end -- Error Lens,
/// which this VS Code has, and Neovim's virtual text -- and unused code
/// faded.
fn draw_diagnostics(painter: &egui::Painter, view: &View, diagnostics: &[lsp::Diagnostic], encoding: lsp::Encoding, t: UiTheme) {
    let mut by_line: HashMap<usize, &lsp::Diagnostic> = HashMap::new();
    let fade = painter.ctx().global_style().visuals.panel_fill.gamma_multiply(0.5);
    for d in diagnostics {
        let s = view.lsp_index(d.range.start, encoding);
        let e = view.lsp_index(d.range.end, encoding).max(s + 1);
        if d.unnecessary {
            for r in view.span_rects(s, e) {
                painter.rect_filled(r, 0.0, fade);
            }
            continue;
        }
        if d.severity > 3 {
            continue;
        }
        let color = severity_color(d.severity, t);
        for r in view.span_rects(s, e) {
            let y = r.bottom() - 1.5;
            let mut points = Vec::new();
            let mut x = r.left();
            let mut up = true;
            while x <= r.right() {
                points.push(egui::pos2(x, if up { y - 1.5 } else { y + 0.5 }));
                x += 2.0;
                up = !up;
            }
            if points.len() >= 2 {
                painter.add(egui::Shape::line(points, Stroke::new(1.0, color)));
            }
        }
        if d.severity <= 2 {
            let slot = by_line.entry(d.range.start.line as usize).or_insert(d);
            if d.severity < slot.severity {
                *slot = d;
            }
        }
    }
    for (line, d) in by_line {
        if line >= view.line_starts.len() {
            continue;
        }
        let r = view.rect(view.line_end(line));
        let color = severity_color(d.severity, t);
        let message = d.message.lines().next().unwrap_or_default();
        // The severity's own icon before the message, as Error Lens puts it:
        // a Codicon, which the UI font lacks nothing of -- U+25CF, Neovim's
        // bullet, it drew as a box.
        let mut job = LayoutJob::default();
        let format = TextFormat { font_id: FontId::proportional(12.0), color, italics: true, ..Default::default() };
        job.append(severity_icon(d.severity), 0.0, TextFormat { italics: false, ..format.clone() });
        job.append(&format!(" {message}"), 0.0, format);
        let x = r.left() + 3.0 * view.char_width;
        job.wrap = egui::text::TextWrapping::truncate_at_width((view.clip.right() - x - 8.0).max(0.0));
        let g = painter.layout_job(job);
        painter.galley(egui::pos2(x, r.top() + (view.row_height - g.size().y) / 2.0), g, color);
    }
}

/// Every key the frame had, in Neovim's notation, and any text pasted --
/// taken out of egui's input so nothing else in the window acts on them.
/// Ctrl+S is kalast's own, as it is VS Code's with the Neovim extension.
fn collect_keys(ui: &mut egui::Ui, inserting: bool) -> (String, Vec<String>, bool) {
    let events = ui.input(|i| i.events.clone());
    let ctrl = ui.input(|i| i.modifiers.ctrl);
    let mut keys = String::new();
    let mut pastes = Vec::new();
    let mut save = false;
    let mut skip_text: Option<String> = None;
    for event in events {
        match event {
            egui::Event::Text(t) => {
                // Alt with a letter came as a key already, `<M-x>`.
                if skip_text.take().is_some_and(|s| s.eq_ignore_ascii_case(&t)) {
                    continue;
                }
                keys.push_str(&nvim::text_keys(&t));
            }
            egui::Event::Key { key, pressed: true, modifiers, .. } => {
                if modifiers.command && key == egui::Key::S {
                    save = true;
                    continue;
                }
                if let Some(k) = nvim::key(key, modifiers) {
                    keys.push_str(&k);
                    if modifiers.alt && !modifiers.ctrl {
                        skip_text = Some(key.symbol_or_name().to_string());
                    }
                }
            }
            egui::Event::Copy => keys.push_str("<C-c>"),
            egui::Event::Cut => keys.push_str("<C-x>"),
            egui::Event::Paste(text) => {
                // Ctrl+V is visual block outside insert mode; pasting there
                // is `p`, or Shift+Insert.
                if ctrl && !inserting {
                    keys.push_str("<C-v>");
                } else {
                    pastes.push(text);
                }
            }
            egui::Event::Ime(egui::ImeEvent::Commit(t)) => keys.push_str(&nvim::text_keys(&t)),
            _ => {}
        }
    }
    ui.input_mut(|i| {
        i.events.retain(|e| {
            !matches!(
                e,
                egui::Event::Text(_)
                    | egui::Event::Key { .. }
                    | egui::Event::Copy
                    | egui::Event::Cut
                    | egui::Event::Paste(_)
                    | egui::Event::Ime(_)
            )
        })
    });
    (keys, pastes, save)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The wheel scrolls the completion list, whole rows at a time, the
    /// selection left where it was; the pointer moved onto a row selects it,
    /// for its documentation; moving the selection brings the view back to
    /// it. The view was pinned to the selection every frame, and a notch --
    /// which egui spreads over frames, a few points each -- was rounded to
    /// rows one frame at a time, to nothing.
    #[test]
    fn the_wheel_scrolls_the_completion_list() {
        let ctx = egui::Context::default();
        let mut editor = ScriptEditor::default();
        let mut script = String::from("x = 1
");
        let palette = code::palette(UiTheme::CatppuccinMocha);
        let settings = Settings {
            neovim: false,
            neovim_path: "",
            ruler: 80,
            language_servers: false,
            python_language_server: "",
            rust_language_server: "",
            theme: UiTheme::CatppuccinMocha,
        };
        let mut time = 0.0;
        let mut frame = |editor: &mut ScriptEditor, events: Vec<egui::Event>| {
            time += 1.0 / 60.0;
            let raw = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(1000.0, 700.0))),
                time: Some(time),
                events,
                ..Default::default()
            };
            let mut output = ctx.run_ui(raw, |ui| {
                editor.show(ui, &mut script, "a.py", Lang::Python, &palette, &settings, false);
            });
            output.textures_delta.clear();
        };
        frame(&mut editor, vec![]);
        let items = (0..30).map(|i| lsp::CompletionItem { label: format!("item{i:02}"), ..Default::default() }).collect();
        editor.completion = Some(Completion {
            list: lsp::CompletionList { incomplete: false, items },
            anchor: 0,
            shown: (0..30).map(|i| (i, Vec::new())).collect(),
            selected: 0,
            scroll: 0,
            followed: None,
            wheel: 0.0,
        });
        frame(&mut editor, vec![]);
        let list = *editor.popup_rects.last().expect("the list is drawn");
        let over = egui::Event::PointerMoved(list.center());
        let notch = egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Line,
            delta: egui::vec2(0.0, -3.0),
            phase: egui::TouchPhase::Move,
            modifiers: egui::Modifiers::default(),
        };
        frame(&mut editor, vec![over.clone(), notch]);
        for _ in 0..60 {
            frame(&mut editor, vec![over.clone()]);
        }
        let c = editor.completion.as_ref().expect("still open");
        assert!(c.scroll >= 3, "scrolled down by rows, got {}", c.scroll);
        assert_eq!(c.selected, 0, "the selection stays");
        let scrolled = c.scroll;

        // Back up past the top: held there.
        let up = egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Line,
            delta: egui::vec2(0.0, 30.0),
            phase: egui::TouchPhase::Move,
            modifiers: egui::Modifiers::default(),
        };
        frame(&mut editor, vec![over.clone(), up]);
        for _ in 0..60 {
            frame(&mut editor, vec![over.clone()]);
        }
        assert_eq!(editor.completion.as_ref().unwrap().scroll, 0, "from {scrolled} to the top");

        // The pointer resting there all along took nothing; moved onto a row,
        // it selects it, for its documentation. Rows start under the frame's
        // 6-point margin, 22 points each.
        assert_eq!(editor.completion.as_ref().unwrap().selected, 0, "a resting pointer selects nothing");
        let fourth = egui::pos2(list.center().x, list.top() + 6.0 + 3.0 * 22.0 + 11.0);
        frame(&mut editor, vec![egui::Event::PointerMoved(fourth)]);
        assert_eq!(editor.completion.as_ref().unwrap().selected, 3, "the row under the pointer");

        // The selection moves -- an arrow's doing -- and the view follows it.
        editor.completion.as_mut().unwrap().selected = 20;
        frame(&mut editor, vec![]);
        let c = editor.completion.as_ref().unwrap();
        assert!((c.scroll..c.scroll + 10).contains(&20), "the selection in view, from row {}", c.scroll);
    }

    #[test]
    fn line_and_column_round_trip_in_bytes() {
        let text = "ab\ncé d\nx";
        assert_eq!(line_col(text, 5), (1, 3));
        assert_eq!(char_index(text, 1, 3), 5);
        assert_eq!(char_index(text, 1, 99), 7, "past the end is the line's end");
        assert_eq!(char_index(text, 9, 0), text.chars().count());
    }

    #[test]
    fn a_completion_replaces_the_word_typed_and_adds_its_import() {
        let text = "x = 1\nnp.lins";
        let item = lsp::CompletionItem {
            label: "linspace".into(),
            insert: "linspace".into(),
            additional: vec![(
                lsp::Range {
                    start: lsp::Position { line: 0, character: 0 },
                    end: lsp::Position { line: 0, character: 0 },
                },
                "import numpy as np\n".into(),
            )],
            ..Default::default()
        };
        let cursor = text.chars().count();
        let (edits, at) = completion_edits(text, cursor, 9, &item, lsp::Encoding::Utf16);
        let mut out = text.to_string();
        apply(&mut out, &edits);
        assert_eq!(out, "import numpy as np\nx = 1\nnp.linspace");
        assert_eq!(at, out.chars().count(), "the cursor after the word, moved down by the import");
    }

    #[test]
    fn fuzzy_ranks_a_prefix_first() {
        let (prefix, _) = fuzzy("lin", "linspace").unwrap();
        let (inside, _) = fuzzy("lin", "polyline").unwrap();
        assert!(prefix > inside);
        assert!(fuzzy("lsp", "linspace").is_some());
        assert!(fuzzy("xyz", "linspace").is_none());
        let (_, matched) = fuzzy("ls", "linspace").unwrap();
        assert_eq!(matched, vec![0, 3]);
    }

    #[test]
    fn a_word_is_letters_digits_and_underscores() {
        let chars: Vec<char> = "a = sim.state_1(".chars().collect();
        assert_eq!(word_at(&chars, 9), Some((8, 15)));
        assert_eq!(word_at(&chars, 2), None);
    }

    /// The view's own indexing agrees with the slow one it replaces.
    #[test]
    fn a_view_indexes_as_the_text_does() {
        let text = "ab\ncé d\n\nxyz".to_string();
        // Fonts exist once a frame has begun.
        let ctx = egui::Context::default();
        let mut galley = None;
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            galley = Some(ui.painter().layout_job(LayoutJob::simple(
                text.clone(),
                FontId::monospace(13.0),
                Color32::WHITE,
                f32::INFINITY,
            )));
        });
        // The font atlas it made, which no renderer is here to take.
        output.textures_delta.clear();
        let galley = galley.unwrap();
        let v = View::new(galley, Pos2::ZERO, Rect::EVERYTHING, text.clone(), 0, 16.0, 8.0);
        for i in 0..=text.chars().count() {
            let (l, c) = line_col(&text, i);
            assert_eq!(v.line_col(i), (l, c), "at {i}");
            assert_eq!(v.index(l, c), char_index(&text, l, c));
            for enc in [lsp::Encoding::Utf8, lsp::Encoding::Utf16] {
                let p = lsp::position(&text, i, enc);
                assert_eq!(v.lsp_position(i, enc), p);
                assert_eq!(v.lsp_index(p, enc), lsp::index(&text, p, enc));
            }
        }
    }
}
