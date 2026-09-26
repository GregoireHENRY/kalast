//! Neovim in the script editor, run the way VS Code's Neovim extension runs
//! it: the user's own `nvim` and config, embedded as a process and driven
//! over msgpack-RPC. While it is on, Neovim owns the text -- every key goes
//! to it -- and kalast draws what it reports: the buffer's lines, the mode,
//! the cursor and the view, the selection, the command line, the messages
//! and the completion menu.
//!
//! How it is attached, and why each piece:
//!
//! - `--embed` makes stdin and stdout the RPC channel. Neovim then waits for
//!   a UI before it reads the config, so that startup messages have
//!   somewhere to go.
//! - `nvim_ui_attach` with `ext_multigrid`: each window gets a grid of its
//!   own and a `win_viewport` event carrying its top line and its cursor in
//!   buffer terms, which is what a view drawing the text itself needs. The
//!   grids' cells are never read.
//! - `ext_cmdline`, `ext_messages`, `ext_popupmenu`: the command line, the
//!   messages and the menu arrive as events rather than as cells, for kalast
//!   to draw in its own status bar and popup.
//! - `nvim_buf_attach` streams every change to the buffer as lines replaced,
//!   which keeps kalast's copy -- the script it runs -- the same as Neovim's.
//!
//! The selection has no event of its own, so an autocmd sends it; `:w`
//! writes through kalast (`buftype=acwrite`); and the keys a language server
//! answers in VS Code -- `K`, `gd`, `]d` -- are mapped in the buffer to ask
//! kalast's. Everything else is the user's config, loaded with `g:kalast`
//! set so a part of it that has no place here can be skipped.

use rmpv::Value;
use std::collections::HashMap;
use std::io::{BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::{Arc, Mutex};

/// The cursor's shape in a mode, from the user's `guicursor`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Shape {
    Block,
    /// A bar at the cell's left, this fraction of its width.
    Vertical(f32),
    /// A bar along the cell's bottom, this fraction of its height.
    Horizontal(f32),
}

/// A selection, both ends as `(line, byte column)`, `end` where the cursor
/// is. `kind` is the mode's first letter: `v`, `V`, or `\x16` for a block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Visual {
    pub kind: char,
    pub start: (usize, usize),
    pub end: (usize, usize),
}

/// The command line being typed: `:` a command, `/` `?` a search, or an
/// `input()` prompt.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Cmdline {
    pub firstc: String,
    pub prompt: String,
    pub content: String,
    /// The cursor, in bytes into `content`.
    pub pos: usize,
    pub indent: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Message {
    /// Neovim's kind: `emsg` an error, `wmsg` a warning, `echo`, `list_cmd`
    /// for `:ls` and its like, `history` for `:messages`...
    pub kind: String,
    pub text: String,
}

impl Message {
    /// Several lines -- `:ls`, `:reg`, `:messages`, a traceback -- which
    /// the status bar cannot hold and a panel above it shows instead, until
    /// the next key.
    pub fn is_list(&self) -> bool {
        self.text.trim_end().contains('\n') || matches!(self.kind.as_str(), "list_cmd" | "history")
    }
}

/// Neovim's own completion menu: `<C-n>` in insert mode, `<Tab>` on the
/// command line.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Popup {
    /// `(word, kind, menu)`.
    pub items: Vec<(String, String, String)>,
    pub selected: Option<usize>,
    /// Anchored to the command line rather than the text.
    pub cmdline: bool,
    /// Where it opens: a column of the command line, or a place in the text.
    pub row: usize,
    pub col: usize,
}

/// What a key mapped by kalast asks it to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// `:w`.
    Write,
    /// `:q`, and the `:wq` after its write -- `true` with a bang.
    Quit(bool),
    Hover,
    Definition,
    NextDiagnostic,
    PrevDiagnostic,
    /// A file Neovim went to -- `:e`, a picker -- for kalast to open, as
    /// VS Code opens it in a tab.
    Open(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Call {
    ApiInfo,
    Attach,
    Setup,
    Load,
    Other,
}

/// What the reader thread passes on: responses, the buffer's changes, the
/// UI events kalast draws from, and kalast's own notifications.
#[derive(Debug)]
enum Note {
    Response { id: u32, error: Value, result: Value },
    Lines { buf: i64, first: i64, last: i64, data: Vec<String> },
    Redraw(Vec<Ui>),
    Kalast(Vec<Value>),
    Exited,
}

#[derive(Debug)]
enum Ui {
    ModeInfo(Vec<(String, Shape)>),
    Mode(String),
    Viewport { grid: i64, win: i64, top: i64, line: i64, col: i64 },
    CmdlineShow(Cmdline, u64),
    CmdlinePos(usize, u64),
    CmdlineHide(u64),
    MsgShow { kind: String, text: String, replace_last: bool },
    MsgClear,
    MsgShowmode(String),
    MsgShowcmd(String),
    MsgHistory(Vec<Message>),
    PopupShow(Popup, i64),
    PopupSelect(Option<usize>),
    PopupHide,
    /// The end of a batch: the state is whole.
    Flush,
}

/// One embedded Neovim.
pub struct Neovim {
    child: Option<Child>,
    writer: Arc<Mutex<ChildStdin>>,
    inbox: Arc<Mutex<Vec<Note>>>,
    next_id: u32,
    pending: HashMap<u32, Call>,
    /// Set up and attached to its buffer. Keys typed before it are held.
    pub ready: bool,
    /// Why it is not running any more, once it is not.
    pub gone: Option<String>,
    channel: i64,
    buf: i64,
    win: i64,
    grid: Option<i64>,
    held: String,
    /// A load waiting for the setup to finish.
    held_load: Option<(Vec<String>, String, String, bool, bool, Option<usize>)>,
    size: (u32, u32),

    /// The buffer, as lines without their endings.
    pub lines: Vec<String>,
    /// The text ended with a line ending, and which kind.
    eol: bool,
    crlf: bool,
    /// Changed by an edit since `take_edited` was last asked.
    edited: bool,
    /// A load in flight: the lines it replaces are not an edit.
    loading: Option<u32>,
    /// Lines changed since the last `flush`: the cursor that goes with them
    /// has not arrived yet.
    unflushed: bool,
    /// Neovim's `modified`: `u` back to the saved text clears it.
    pub modified: bool,

    /// `mode_change`'s name: `normal`, `insert`, `visual`, `replace`,
    /// `operator`, `cmdline_normal`...
    pub mode: String,
    /// `mode()`'s short form, from the selection autocmd: `n`, `i`, `v`,
    /// `V`, `\x16`, `no`, `R`...
    pub short_mode: String,
    shapes: HashMap<String, Shape>,
    /// `(line, byte column)`.
    pub cursor: (usize, usize),
    pub topline: usize,
    pub visual: Option<Visual>,
    /// By nesting level; the innermost is the one shown.
    cmdlines: Vec<(u64, Cmdline)>,
    pub messages: Vec<Message>,
    pub showcmd: String,
    pub showmode: String,
    pub popup: Option<Popup>,
    pub number: bool,
    pub relativenumber: bool,
    /// Neovim is in a buffer that is not the script -- help, a file
    /// explorer -- which kalast does not show.
    pub foreign: bool,
    /// The columns before the text in Neovim's window -- its line numbers
    /// and signs -- which a mouse position has to be given in.
    textoff: usize,
    pub actions: Vec<Action>,
}

/// Run once attached, with the channel as its argument: kalast's buffer,
/// its autocmds and mappings. Returns what the rest needs to know.
const SETUP: &str = r#"
local chan = ...
local function notify(...) vim.rpcnotify(chan, 'kalast', ...) end
-- A buffer of kalast's own rather than the one Neovim starts in, which a
-- startup plugin -- a dashboard -- may have taken.
local buf = vim.api.nvim_create_buf(true, false)
vim.api.nvim_set_current_buf(buf)
local win = vim.api.nvim_get_current_win()
-- Written by kalast: `:w` goes through BufWriteCmd.
vim.bo[buf].buftype = 'acwrite'
vim.bo[buf].swapfile = false
-- blink.cmp and mini.completion stand down: kalast draws its own
-- completion, from its own language server, and theirs would be invisible.
vim.b[buf].completion = false
-- kalast draws the status line and the command line.
vim.o.laststatus = 0
vim.o.showtabline = 0
vim.o.cmdheight = 0
vim.o.showcmd = true
local group = vim.api.nvim_create_augroup('kalast', { clear = true })
vim.api.nvim_create_autocmd('BufWriteCmd', { group = group, buffer = buf, callback = function()
  notify('write')
  vim.bo[buf].modified = false
end })
vim.api.nvim_create_autocmd('BufModifiedSet', { group = group, buffer = buf, callback = function()
  notify('modified', vim.bo[buf].modified)
end })
-- The selection: a UI is told where the cursor is, never where a
-- selection began.
local was = false
local function selection()
  local m = vim.api.nvim_get_mode().mode
  local c = m:sub(1, 1)
  if c == 'v' or c == 'V' or c == '\22' or c == 's' or c == 'S' or c == '\19' then
    local s = vim.fn.getpos('v')
    local e = vim.api.nvim_win_get_cursor(0)
    was = true
    notify('visual', m, s[2] - 1, s[3] - 1, e[1] - 1, e[2])
  elseif was or c ~= 'n' then
    was = false
    notify('visual', m)
  end
end
vim.api.nvim_create_autocmd({ 'ModeChanged', 'CursorMoved' }, { group = group, callback = selection })
local function options()
  local info = vim.fn.getwininfo(win)[1]
  notify('options', vim.wo[win].number, vim.wo[win].relativenumber, info and info.textoff or 0)
end
vim.api.nvim_create_autocmd('OptionSet', { group = group, callback = function() vim.schedule(options) end })
vim.api.nvim_create_autocmd({ 'BufWinEnter', 'WinResized', 'VimResized' }, { group = group, callback = function() vim.schedule(options) end })
-- What a language server answers, on the keys VS Code's Neovim extension
-- gives them and an LspAttach usually does.
for lhs, action in pairs({ K = 'hover', gh = 'hover', gd = 'definition', gD = 'definition',
                           ['<C-]>'] = 'definition', [']d'] = 'next_diagnostic', ['[d'] = 'prev_diagnostic' }) do
  vim.keymap.set('n', lhs, function() notify(action) end, { buffer = buf, desc = 'kalast: ' .. action })
end
-- :q and :wq leave the editor, not Neovim, which kalast keeps running.
vim.api.nvim_create_user_command('KalastQuit', function(o) notify('quit', o.bang) end, { bang = true })
vim.api.nvim_create_user_command('KalastWriteQuit', function(o)
  vim.cmd.write({ bang = o.bang })
  notify('quit', o.bang)
end, { bang = true })
local function alias(from, to)
  vim.cmd(('cnoreabbrev <expr> %s (getcmdtype() ==# ":" && getcmdline() ==# "%s") ? "%s" : "%s"'):format(from, from, to, from))
end
for _, c in ipairs({ 'q', 'qu', 'qui', 'quit', 'qa', 'qal', 'qall', 'quita', 'quitall', 'clo', 'clos', 'close' }) do
  alias(c, 'KalastQuit')
end
for _, c in ipairs({ 'wq', 'x', 'xi', 'xit', 'exi', 'exit', 'wqa', 'wqal', 'wqall', 'xa', 'xal', 'xall' }) do
  alias(c, 'KalastWriteQuit')
end
-- The empty buffer Neovim starts in goes. With it there, `:bnext` -- or a
-- config's `<S-h>` -- left the script for a buffer kalast does not show, and
-- `:w` there said E32, no file name.
for _, b in ipairs(vim.api.nvim_list_bufs()) do
  if b ~= buf and vim.api.nvim_buf_get_name(b) == '' and vim.bo[b].buftype == ''
      and not vim.bo[b].modified and vim.api.nvim_buf_line_count(b) == 1
      and vim.api.nvim_buf_get_lines(b, 0, 1, false)[1] == '' then
    pcall(vim.api.nvim_buf_delete, b, { force = true })
  end
end
-- A file Neovim goes to -- `:e`, a picker, a jump -- is opened in kalast, as
-- VS Code opens it in a tab, and Neovim comes back to the script. Anything
-- else -- help, a file explorer -- is said on the status bar, since kalast
-- shows the script alone.
vim.api.nvim_create_autocmd('BufEnter', { group = group, callback = function(ev)
  if ev.buf == buf then
    notify('buffer', true)
    return
  end
  local name = vim.api.nvim_buf_get_name(ev.buf)
  if vim.bo[ev.buf].buftype == '' and name ~= '' then
    notify('open', name)
    vim.schedule(function()
      if vim.api.nvim_buf_is_valid(buf) then pcall(vim.api.nvim_set_current_buf, buf) end
      pcall(vim.api.nvim_buf_delete, ev.buf, { force = true })
    end)
  else
    notify('buffer', false)
  end
end })
vim.api.nvim_create_autocmd({ 'BufDelete', 'BufWipeout' }, { group = group, buffer = buf, callback = function()
  notify('closed')
end })
local info = vim.fn.getwininfo(win)[1]
return { buf, win, vim.wo[win].number, vim.wo[win].relativenumber, info and info.textoff or 0 }
"#;

/// Put the script in the buffer without making it an undoable change, name
/// it after its file, and give it its filetype -- indent rules, `gcc`'s
/// comment string. Returns nothing kalast needs.
const LOAD: &str = r#"
local buf, lines, name, ft, eol, crlf, line = ...
local levels = vim.bo[buf].undolevels
vim.bo[buf].undolevels = -1
vim.api.nvim_buf_set_lines(buf, 0, -1, false, lines)
vim.bo[buf].undolevels = levels
if name ~= '' then
  local full = vim.fn.fnamemodify(name, ':p')
  if vim.api.nvim_buf_get_name(buf) ~= full then
    for _, b in ipairs(vim.api.nvim_list_bufs()) do
      if b ~= buf and vim.api.nvim_buf_get_name(b) == full then
        pcall(vim.api.nvim_buf_delete, b, { force = true })
      end
    end
    pcall(vim.api.nvim_buf_set_name, buf, full)
  end
end
vim.bo[buf].eol = eol
vim.bo[buf].fileformat = crlf and 'dos' or 'unix'
if ft == '' and name ~= '' then ft = vim.filetype.match({ filename = name, buf = buf }) or '' end
if ft ~= '' and vim.bo[buf].filetype ~= ft then vim.bo[buf].filetype = ft end
vim.bo[buf].modified = false
if line ~= vim.NIL and line ~= nil then
  pcall(vim.api.nvim_win_set_cursor, 0, { math.min(line + 1, #lines), 0 })
end
return true
"#;

impl Neovim {
    /// Start `program` -- `nvim` found on the PATH if empty -- in `cwd`,
    /// with a grid of `size` cells. Returns at once; the setup lands in
    /// later frames, through `poll`.
    pub fn start(
        program: &str,
        cwd: &Path,
        size: (u32, u32),
        repaint: impl Fn() + Send + 'static,
    ) -> Result<Self, String> {
        Self::spawn(program, cwd, size, &[], repaint)
    }

    /// `start`, with more arguments for `nvim`: `--clean`, for a test that
    /// must not depend on whose config is installed.
    fn spawn(
        program: &str,
        cwd: &Path,
        size: (u32, u32),
        extra: &[&str],
        repaint: impl Fn() + Send + 'static,
    ) -> Result<Self, String> {
        let program = find(program).ok_or_else(|| {
            if program.trim().is_empty() {
                "Neovim is not on the PATH: install it, or set app.config.neovim_path".to_string()
            } else {
                format!("no Neovim at {program}")
            }
        })?;
        let mut command = Command::new(&program);
        command
            .args(["--embed", "-n", "--cmd", "let g:kalast = 1"])
            .args(extra)
            .current_dir(cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            // No console window: `nvim` is a console program, and a
            // double-clicked kalast has no console for it to share.
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            command.creation_flags(CREATE_NO_WINDOW);
        }
        let mut child = command
            .spawn()
            .map_err(|e| format!("cannot start {}: {e}", program.display()))?;
        let stdin = child.stdin.take().ok_or("no stdin")?;
        let stdout = child.stdout.take().ok_or("no stdout")?;
        let writer = Arc::new(Mutex::new(stdin));
        let inbox = Arc::new(Mutex::new(Vec::new()));
        {
            let writer = writer.clone();
            let inbox = inbox.clone();
            std::thread::Builder::new()
                .name("nvim".into())
                .spawn(move || {
                    let mut reader = BufReader::new(stdout);
                    while let Ok(message) = rmpv::decode::read_value(&mut reader) {
                        if let Some(note) = parse(message, &writer) {
                            inbox.lock().unwrap().push(note);
                            repaint();
                        }
                    }
                    inbox.lock().unwrap().push(Note::Exited);
                    repaint();
                })
                .map_err(|e| e.to_string())?;
        }
        let mut nvim = Self {
            child: Some(child),
            writer,
            inbox,
            next_id: 1,
            pending: HashMap::new(),
            ready: false,
            gone: None,
            channel: 0,
            buf: 0,
            win: 0,
            grid: None,
            held: String::new(),
            held_load: None,
            size,
            lines: vec![String::new()],
            eol: false,
            crlf: false,
            edited: false,
            loading: None,
            unflushed: false,
            modified: false,
            mode: "normal".into(),
            short_mode: "n".into(),
            shapes: HashMap::new(),
            cursor: (0, 0),
            topline: 0,
            visual: None,
            cmdlines: Vec::new(),
            messages: Vec::new(),
            showcmd: String::new(),
            showmode: String::new(),
            popup: None,
            number: true,
            relativenumber: false,
            foreign: false,
            textoff: 0,
            actions: Vec::new(),
        };
        nvim.call(Call::ApiInfo, "nvim_get_api_info", vec![]);
        let options: Vec<(Value, Value)> = [
            "rgb",
            "ext_linegrid",
            "ext_multigrid",
            "ext_cmdline",
            "ext_messages",
            "ext_popupmenu",
        ]
        .into_iter()
        .map(|k| (Value::from(k), Value::from(true)))
        .collect();
        nvim.call(
            Call::Attach,
            "nvim_ui_attach",
            vec![size.0.into(), size.1.into(), Value::Map(options)],
        );
        Ok(nvim)
    }

    fn call(&mut self, call: Call, method: &str, params: Vec<Value>) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        self.pending.insert(id, call);
        let message = Value::Array(vec![0.into(), id.into(), method.into(), Value::Array(params)]);
        let mut bytes = Vec::new();
        let _ = rmpv::encode::write_value(&mut bytes, &message);
        let mut w = self.writer.lock().unwrap();
        let _ = w.write_all(&bytes).and_then(|()| w.flush());
        id
    }

    fn lua(&mut self, call: Call, code: &str, args: Vec<Value>) -> u32 {
        self.call(call, "nvim_exec_lua", vec![code.into(), Value::Array(args)])
    }

    /// Keys, in Neovim's notation: `dd`, `<Esc>`, `<C-r>`, `<lt>`.
    pub fn input(&mut self, keys: &str) {
        if keys.is_empty() {
            return;
        }
        if !self.ready {
            self.held.push_str(keys);
            return;
        }
        // A list -- `:ls`, `:messages` -- is read and dismissed by the next
        // key, as Neovim's own pager is. A one-line message stays on the
        // status bar until Neovim clears or replaces it.
        self.messages.retain(|m| !m.is_list());
        self.call(Call::Other, "nvim_input", vec![keys.into()]);
    }

    /// Text from the clipboard, as a terminal pastes it: one change, and `.`
    /// repeats it.
    pub fn paste(&mut self, text: &str) {
        if self.ready {
            self.call(Call::Other, "nvim_paste", vec![text.into(), false.into(), (-1).into()]);
        }
    }

    /// A mouse event at `(line, display column)` of the buffer, handed to
    /// Neovim's own handling -- a click places the cursor, a drag selects,
    /// a double click takes a word -- in its window's cells.
    pub fn mouse(&mut self, button: &str, action: &str, modifiers: &str, line: usize, column: usize) {
        let Some(grid) = self.grid.filter(|_| self.ready) else { return };
        let row = line as i64 - self.topline as i64;
        let col = (self.textoff + column) as i64;
        self.call(
            Call::Other,
            "nvim_input_mouse",
            vec![button.into(), action.into(), modifiers.into(), grid.into(), row.into(), col.into()],
        );
    }

    /// Match Neovim's window to the part of the editor that shows text, so
    /// `H`, `L`, `<C-d>` and `scrolloff` count the rows kalast shows.
    pub fn resize(&mut self, columns: u32, rows: u32) {
        let size = (columns.max(1), rows.max(1));
        if size != self.size && self.ready {
            self.size = size;
            self.call(Call::Other, "nvim_ui_try_resize", vec![size.0.into(), size.1.into()]);
        }
    }

    /// Replace each `start..end` -- `(line, byte column)`, last in the text
    /// first -- with its text, and put the cursor at `cursor`: a completion
    /// accepted, with the import it brings.
    pub fn apply(&mut self, edits: &[((usize, usize), (usize, usize), String)], cursor: (usize, usize)) {
        if !self.ready {
            return;
        }
        let edits = Value::Array(
            edits
                .iter()
                .map(|(s, e, text)| {
                    Value::Array(vec![
                        s.0.into(),
                        s.1.into(),
                        e.0.into(),
                        e.1.into(),
                        Value::Array(text.split('\n').map(Value::from).collect()),
                    ])
                })
                .collect(),
        );
        let (buf, win) = (self.buf, self.win);
        self.lua(
            Call::Other,
            "local buf, win, edits, cr, cc = ...
             for _, e in ipairs(edits) do vim.api.nvim_buf_set_text(buf, e[1], e[2], e[3], e[4], e[5]) end
             pcall(vim.api.nvim_win_set_cursor, win, { cr + 1, cc })",
            vec![buf.into(), win.into(), edits, cursor.0.into(), cursor.1.into()],
        );
    }

    /// Move the cursor, keeping `''` pointing where it was, so `<C-o>` goes
    /// back: a definition jumped to.
    pub fn jump(&mut self, line: usize, column: usize) {
        if !self.ready {
            return;
        }
        let win = self.win;
        self.lua(
            Call::Other,
            "local win, l, c = ... vim.cmd(\"normal! m'\") pcall(vim.api.nvim_win_set_cursor, win, { l + 1, c })",
            vec![win.into(), line.into(), column.into()],
        );
    }

    /// kalast wrote the file: the buffer is no longer modified.
    pub fn saved(&mut self) {
        if self.ready {
            let buf = self.buf;
            self.lua(Call::Other, "vim.bo[...].modified = false", vec![buf.into()]);
        }
        self.modified = false;
    }

    /// Show `text` from the file `name`: the script opened, or changed from
    /// outside. The cursor goes to `line` if given.
    pub fn load(&mut self, text: &str, name: &str, filetype: &str, line: Option<usize>) {
        let (lines, eol, crlf) = split(text);
        // The copy is not written here: it follows Neovim's own account of
        // the change, which replaces whatever the buffer held -- written
        // here as well, the load landed twice.
        self.eol = eol;
        self.crlf = crlf;
        self.edited = false;
        self.modified = false;
        if !self.ready {
            self.held_load = Some((lines, name.to_string(), filetype.to_string(), eol, crlf, line));
            return;
        }
        self.send_load(lines, name, filetype, eol, crlf, line);
    }

    fn send_load(&mut self, lines: Vec<String>, name: &str, filetype: &str, eol: bool, crlf: bool, line: Option<usize>) {
        let buf = self.buf;
        let id = self.lua(
            Call::Load,
            LOAD,
            vec![
                buf.into(),
                Value::Array(lines.into_iter().map(Value::from).collect()),
                name.into(),
                filetype.into(),
                eol.into(),
                crlf.into(),
                line.map(Value::from).unwrap_or(Value::Nil),
            ],
        );
        self.loading = Some(id);
    }

    /// The buffer as a script: its lines joined with the endings it came with.
    pub fn text(&self) -> String {
        join(&self.lines, self.eol, self.crlf)
    }

    /// Whether an edit changed the buffer since this was last asked.
    pub fn take_edited(&mut self) -> bool {
        std::mem::take(&mut self.edited)
    }

    /// The cursor's shape in the current mode.
    pub fn shape(&self) -> Shape {
        self.shapes.get(&self.mode).copied().unwrap_or(match self.mode.as_str() {
            "insert" | "cmdline_insert" => Shape::Vertical(0.25),
            "replace" | "cmdline_replace" | "operator" => Shape::Horizontal(0.2),
            _ => Shape::Block,
        })
    }

    pub fn cmdline(&self) -> Option<&Cmdline> {
        self.cmdlines.last().map(|(_, c)| c)
    }

    pub fn insert_mode(&self) -> bool {
        self.mode == "insert" || self.short_mode.starts_with('i')
    }

    /// Apply what Neovim sent since the last frame.
    pub fn poll(&mut self) {
        let notes = std::mem::take(&mut *self.inbox.lock().unwrap());
        for note in notes {
            match note {
                Note::Response { id, error, result } => self.response(id, error, result),
                Note::Lines { buf, first, last, data } => {
                    if buf != self.buf {
                        continue;
                    }
                    let first = (first.max(0) as usize).min(self.lines.len());
                    let last = if last < 0 { self.lines.len() } else { (last as usize).min(self.lines.len()) };
                    self.lines.splice(first..last.max(first), data);
                    if self.lines.is_empty() {
                        self.lines.push(String::new());
                    }
                    if self.loading.is_none() {
                        self.edited = true;
                    }
                    self.unflushed = true;
                }
                Note::Redraw(events) => {
                    for event in events {
                        self.redraw(event);
                    }
                }
                Note::Kalast(args) => self.kalast(&args),
                Note::Exited => {
                    self.ready = false;
                    self.gone.get_or_insert_with(|| "Neovim exited".to_string());
                }
            }
        }
    }

    fn response(&mut self, id: u32, error: Value, result: Value) {
        let Some(call) = self.pending.remove(&id) else { return };
        if !error.is_nil() {
            let text = match &error {
                Value::Array(a) => a.get(1).map(text).unwrap_or_default(),
                other => text(other),
            };
            match call {
                Call::Setup | Call::Attach => self.gone = Some(format!("Neovim would not start: {text}")),
                _ => self.messages.push(Message { kind: "emsg".into(), text }),
            }
            if call == Call::Load {
                self.loading = None;
            }
            return;
        }
        match call {
            Call::ApiInfo => {
                self.channel = result.as_array().and_then(|a| a.first()).and_then(Value::as_i64).unwrap_or(0);
                let channel = self.channel;
                self.lua(Call::Setup, SETUP, vec![channel.into()]);
            }
            Call::Setup => {
                let r = result.as_array().cloned().unwrap_or_default();
                self.buf = r.first().and_then(Value::as_i64).unwrap_or(0);
                self.win = r.get(1).and_then(Value::as_i64).unwrap_or(0);
                self.number = r.get(2).and_then(Value::as_bool).unwrap_or(true);
                self.relativenumber = r.get(3).and_then(Value::as_bool).unwrap_or(false);
                self.textoff = r.get(4).and_then(Value::as_u64).unwrap_or(0) as usize;
                self.ready = true;
                let buf = self.buf;
                self.call(Call::Other, "nvim_buf_attach", vec![buf.into(), false.into(), Value::Map(vec![])]);
                if let Some((lines, name, filetype, eol, crlf, line)) = self.held_load.take() {
                    self.send_load(lines, &name, &filetype, eol, crlf, line);
                }
                let held = std::mem::take(&mut self.held);
                self.input(&held);
            }
            Call::Load => {
                if self.loading == Some(id) {
                    self.loading = None;
                }
            }
            Call::Attach | Call::Other => {}
        }
    }

    fn redraw(&mut self, event: Ui) {
        match event {
            Ui::ModeInfo(shapes) => self.shapes = shapes.into_iter().collect(),
            Ui::Mode(mode) => self.mode = mode,
            Ui::Viewport { grid, win, top, line, col } => {
                if win == self.win {
                    self.grid = Some(grid);
                    self.topline = top.max(0) as usize;
                    self.cursor = (line.max(0) as usize, col.max(0) as usize);
                }
            }
            Ui::CmdlineShow(cmdline, level) => {
                self.cmdlines.retain(|(l, _)| *l < level);
                self.cmdlines.push((level, cmdline));
            }
            Ui::CmdlinePos(pos, level) => {
                if let Some((_, c)) = self.cmdlines.iter_mut().find(|(l, _)| *l == level) {
                    c.pos = pos;
                }
            }
            Ui::CmdlineHide(level) => self.cmdlines.retain(|(l, _)| *l < level),
            Ui::MsgShow { kind, text, replace_last } => {
                if replace_last {
                    self.messages.pop();
                }
                // `:s` confirmations and `input()` answers come as echoes of
                // nothing; they would only blank the line.
                if !text.trim().is_empty() || kind == "return_prompt" {
                    self.messages.push(Message { kind, text });
                }
            }
            Ui::MsgClear => self.messages.clear(),
            Ui::MsgShowmode(text) => self.showmode = text,
            Ui::MsgShowcmd(text) => self.showcmd = text,
            Ui::MsgHistory(messages) => self.messages = messages,
            Ui::PopupShow(popup, _grid) => self.popup = Some(popup),
            Ui::PopupSelect(selected) => {
                if let Some(p) = &mut self.popup {
                    p.selected = selected;
                }
            }
            Ui::PopupHide => self.popup = None,
            Ui::Flush => self.unflushed = false,
        }
    }

    /// The lines and the cursor agree: no change is waiting for the redraw
    /// that says where the cursor went. Checked before reading the word
    /// under the cursor, which a keystroke's lines and its cursor, arriving
    /// a frame apart, would otherwise put one character off.
    pub fn settled(&self) -> bool {
        !self.unflushed
    }

    fn kalast(&mut self, args: &[Value]) {
        let Some(what) = args.first().and_then(Value::as_str) else { return };
        let int = |i: usize| args.get(i).and_then(Value::as_i64).unwrap_or(0).max(0) as usize;
        match what {
            "write" => self.actions.push(Action::Write),
            "quit" => self.actions.push(Action::Quit(args.get(1).and_then(Value::as_bool).unwrap_or(false))),
            "hover" => self.actions.push(Action::Hover),
            "definition" => self.actions.push(Action::Definition),
            "next_diagnostic" => self.actions.push(Action::NextDiagnostic),
            "prev_diagnostic" => self.actions.push(Action::PrevDiagnostic),
            "modified" => self.modified = args.get(1).and_then(Value::as_bool).unwrap_or(false),
            "open" => {
                if let Some(path) = args.get(1).map(text).filter(|p| !p.is_empty()) {
                    self.actions.push(Action::Open(path));
                }
            }
            "buffer" => self.foreign = !args.get(1).and_then(Value::as_bool).unwrap_or(true),
            // `:bd` on the script: Neovim has nothing kalast can show, and is
            // started again with the script.
            "closed" => {
                self.ready = false;
                self.gone.get_or_insert_with(|| "the script's buffer was closed in Neovim".to_string());
            }
            "options" => {
                self.number = args.get(1).and_then(Value::as_bool).unwrap_or(true);
                self.relativenumber = args.get(2).and_then(Value::as_bool).unwrap_or(false);
                self.textoff = int(3);
            }
            "visual" => {
                let mode = args.get(1).map(text).unwrap_or_default();
                self.visual = if args.len() >= 6 {
                    let kind = match mode.chars().next() {
                        Some('V') | Some('S') => 'V',
                        Some('\x16') | Some('\x13') => '\x16',
                        _ => 'v',
                    };
                    Some(Visual { kind, start: (int(2), int(3)), end: (int(4), int(5)) })
                } else {
                    None
                };
                self.short_mode = mode;
            }
            _ => {}
        }
    }
}

impl Drop for Neovim {
    /// Close the channel and give Neovim a moment to go: with its stdin
    /// closed it exits by itself. Killed if it has not, on a thread of its
    /// own so nothing waits.
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.stdin.take();
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

/// A script as a buffer's lines, and how it ended them: `(lines, a final
/// line ending, CRLF)`. A buffer always has a line, if an empty one.
fn split(text: &str) -> (Vec<String>, bool, bool) {
    let crlf = text.contains("\r\n");
    let body = if crlf { text.replace("\r\n", "\n") } else { text.to_string() };
    let eol = body.ends_with('\n');
    let body = body.strip_suffix('\n').unwrap_or(&body);
    (body.split('\n').map(str::to_string).collect(), eol, crlf)
}

/// `split` undone.
fn join(lines: &[String], eol: bool, crlf: bool) -> String {
    let ending = if crlf { "\r\n" } else { "\n" };
    let mut text = lines.join(ending);
    if eol {
        text.push_str(ending);
    }
    text
}

/// `nvim` on the PATH, in the usual install directories, or `program` as
/// given.
pub fn find(program: &str) -> Option<PathBuf> {
    let program = program.trim().trim_matches('"');
    if !program.is_empty() {
        let p = PathBuf::from(program);
        return if p.is_file() {
            Some(p)
        } else {
            super::lsp::which(program, &path_dirs())
        };
    }
    let mut dirs = path_dirs();
    if cfg!(windows) {
        for var in ["ProgramFiles", "ProgramFiles(x86)", "LOCALAPPDATA"] {
            if let Some(d) = std::env::var_os(var) {
                dirs.push(PathBuf::from(&d).join("Neovim").join("bin"));
                dirs.push(PathBuf::from(&d).join("Programs").join("Neovim").join("bin"));
            }
        }
    } else {
        dirs.extend(["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin", "/snap/bin"].map(PathBuf::from));
    }
    super::lsp::which("nvim", &dirs)
}

fn path_dirs() -> Vec<PathBuf> {
    std::env::var_os("PATH").map(|p| std::env::split_paths(&p).collect()).unwrap_or_default()
}

/// A string from Neovim, which may not be UTF-8: a buffer holds bytes.
fn text(v: &Value) -> String {
    match v {
        Value::String(s) => s.as_str().map(str::to_string).unwrap_or_else(|| String::from_utf8_lossy(s.as_bytes()).into_owned()),
        Value::Binary(b) => String::from_utf8_lossy(b).into_owned(),
        Value::Nil => String::new(),
        other => other.to_string(),
    }
}

/// A buffer or window handle: an EXT whose payload is the number, or a
/// plain number.
fn handle(v: &Value) -> i64 {
    match v {
        Value::Ext(_, data) => rmpv::decode::read_value(&mut &data[..]).ok().and_then(|v| v.as_i64()).unwrap_or(0),
        other => other.as_i64().unwrap_or(0),
    }
}

/// The text of a highlighted chunk list, `[[attr, text, hl], ...]`.
fn chunks(v: &Value) -> String {
    v.as_array()
        .map(|a| a.iter().filter_map(|c| c.as_array().and_then(|c| c.get(1)).map(text)).collect())
        .unwrap_or_default()
}

/// One message from Neovim, as a note for the UI thread -- or `None` for
/// what kalast does not use. Requests from Neovim are answered with an
/// error here: kalast makes none possible, and one left unanswered would
/// hang Neovim.
fn parse(message: Value, writer: &Mutex<ChildStdin>) -> Option<Note> {
    let Value::Array(parts) = message else { return None };
    match parts.first().and_then(Value::as_u64)? {
        0 => {
            let id = parts.get(1).cloned().unwrap_or(Value::Nil);
            let reply = Value::Array(vec![1.into(), id, "kalast answers no requests".into(), Value::Nil]);
            let mut bytes = Vec::new();
            let _ = rmpv::encode::write_value(&mut bytes, &reply);
            let mut w = writer.lock().unwrap();
            let _ = w.write_all(&bytes).and_then(|()| w.flush());
            None
        }
        1 => Some(Note::Response {
            id: parts.get(1).and_then(Value::as_u64)? as u32,
            error: parts.get(2).cloned().unwrap_or(Value::Nil),
            result: parts.get(3).cloned().unwrap_or(Value::Nil),
        }),
        2 => {
            let method = parts.get(1).and_then(Value::as_str)?;
            let params = parts.get(2).and_then(Value::as_array)?;
            match method {
                "redraw" => {
                    let events: Vec<Ui> = params.iter().flat_map(redraw_batch).collect();
                    (!events.is_empty()).then_some(Note::Redraw(events))
                }
                "nvim_buf_lines_event" => Some(Note::Lines {
                    buf: handle(params.first()?),
                    first: params.get(2)?.as_i64()?,
                    last: params.get(3)?.as_i64()?,
                    data: params.get(4)?.as_array()?.iter().map(text).collect(),
                }),
                "kalast" => Some(Note::Kalast(params.clone())),
                _ => None,
            }
        }
        _ => None,
    }
}

/// One `redraw` batch, `[name, args, args, ...]`, as the events kalast
/// draws from. The grid's own events -- cells, scrolling, highlights -- are
/// the bulk of what Neovim sends and are dropped here, on the reader thread.
fn redraw_batch(batch: &Value) -> Vec<Ui> {
    let Some(batch) = batch.as_array() else { return Vec::new() };
    let Some(name) = batch.first().and_then(Value::as_str) else { return Vec::new() };
    let calls = batch[1..].iter().filter_map(Value::as_array);
    let int = |a: &Vec<Value>, i: usize| a.get(i).and_then(Value::as_i64).unwrap_or(0);
    match name {
        "mode_info_set" => calls
            .map(|a| {
                let modes = a.get(1).and_then(Value::as_array).cloned().unwrap_or_default();
                Ui::ModeInfo(
                    modes
                        .iter()
                        .filter_map(|m| {
                            let map = m.as_map()?;
                            let get = |k: &str| map.iter().find(|(key, _)| key.as_str() == Some(k)).map(|(_, v)| v);
                            let name = get("name").map(text)?;
                            let fraction = get("cell_percentage").and_then(Value::as_u64).unwrap_or(25) as f32 / 100.0;
                            let shape = match get("cursor_shape").and_then(Value::as_str) {
                                Some("vertical") => Shape::Vertical(fraction),
                                Some("horizontal") => Shape::Horizontal(fraction),
                                _ => Shape::Block,
                            };
                            Some((name, shape))
                        })
                        .collect(),
                )
            })
            .collect(),
        "mode_change" => calls.map(|a| Ui::Mode(a.first().map(text).unwrap_or_default())).collect(),
        "win_viewport" => calls
            .map(|a| Ui::Viewport {
                grid: int(a, 0),
                win: a.get(1).map(handle).unwrap_or(0),
                top: int(a, 2),
                line: int(a, 4),
                col: int(a, 5),
            })
            .collect(),
        "cmdline_show" => calls
            .map(|a| {
                let level = a.get(5).and_then(Value::as_u64).unwrap_or(1);
                Ui::CmdlineShow(
                    Cmdline {
                        content: a.first().map(chunks).unwrap_or_default(),
                        pos: int(a, 1).max(0) as usize,
                        firstc: a.get(2).map(text).unwrap_or_default(),
                        prompt: a.get(3).map(text).unwrap_or_default(),
                        indent: int(a, 4).max(0) as usize,
                    },
                    level,
                )
            })
            .collect(),
        "cmdline_pos" => calls
            .map(|a| Ui::CmdlinePos(int(a, 0).max(0) as usize, a.get(1).and_then(Value::as_u64).unwrap_or(1)))
            .collect(),
        "cmdline_hide" => calls.map(|a| Ui::CmdlineHide(a.first().and_then(Value::as_u64).unwrap_or(1))).collect(),
        "msg_show" => calls
            .map(|a| Ui::MsgShow {
                kind: a.first().map(text).unwrap_or_default(),
                text: a.get(1).map(chunks).unwrap_or_default(),
                replace_last: a.get(2).and_then(Value::as_bool).unwrap_or(false),
            })
            .collect(),
        "msg_clear" => vec![Ui::MsgClear],
        "msg_showmode" => calls.map(|a| Ui::MsgShowmode(a.first().map(chunks).unwrap_or_default())).collect(),
        "msg_showcmd" => calls.map(|a| Ui::MsgShowcmd(a.first().map(chunks).unwrap_or_default())).collect(),
        "msg_history_show" => calls
            .map(|a| {
                let entries = a.first().and_then(Value::as_array).cloned().unwrap_or_default();
                Ui::MsgHistory(
                    entries
                        .iter()
                        .filter_map(Value::as_array)
                        .map(|e| Message {
                            kind: e.first().map(text).unwrap_or_default(),
                            text: e.get(1).map(chunks).unwrap_or_default(),
                        })
                        .collect(),
                )
            })
            .collect(),
        "popupmenu_show" => calls
            .map(|a| {
                let items = a
                    .first()
                    .and_then(Value::as_array)
                    .map(|items| {
                        items
                            .iter()
                            .filter_map(Value::as_array)
                            .map(|i| {
                                (
                                    i.first().map(text).unwrap_or_default(),
                                    i.get(1).map(text).unwrap_or_default(),
                                    i.get(2).map(text).unwrap_or_default(),
                                )
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                let selected = int(a, 1);
                let grid = int(a, 4);
                Ui::PopupShow(
                    Popup {
                        items,
                        selected: (selected >= 0).then_some(selected as usize),
                        cmdline: grid == -1,
                        row: int(a, 2).max(0) as usize,
                        col: int(a, 3).max(0) as usize,
                    },
                    grid,
                )
            })
            .collect(),
        "popupmenu_select" => calls
            .map(|a| {
                let s = int(a, 0);
                Ui::PopupSelect((s >= 0).then_some(s as usize))
            })
            .collect(),
        "popupmenu_hide" => vec![Ui::PopupHide],
        "flush" => vec![Ui::Flush],
        _ => Vec::new(),
    }
}

/// A key egui reports, in Neovim's notation -- or `None` for one that
/// arrives as text instead, and for keys Neovim has no name for.
///
/// Plain characters come through egui's `Text` events, which already
/// account for the keyboard layout and Shift; a key is only named here when
/// it is not a character (`<Esc>`, `<CR>`, `<Left>`) or carries Ctrl or Alt,
/// which suppress the text.
pub fn key(key: egui::Key, m: egui::Modifiers) -> Option<String> {
    use egui::Key::*;
    let special = match key {
        Escape => Some("Esc"),
        Enter => Some("CR"),
        Tab => Some("Tab"),
        Backspace => Some("BS"),
        Delete => Some("Del"),
        Insert => Some("Insert"),
        Home => Some("Home"),
        End => Some("End"),
        PageUp => Some("PageUp"),
        PageDown => Some("PageDown"),
        ArrowLeft => Some("Left"),
        ArrowRight => Some("Right"),
        ArrowUp => Some("Up"),
        ArrowDown => Some("Down"),
        F1 => Some("F1"),
        F2 => Some("F2"),
        F3 => Some("F3"),
        F4 => Some("F4"),
        F5 => Some("F5"),
        F6 => Some("F6"),
        F7 => Some("F7"),
        F8 => Some("F8"),
        F9 => Some("F9"),
        F10 => Some("F10"),
        F11 => Some("F11"),
        F12 => Some("F12"),
        Space if m.ctrl || m.alt => Some("Space"),
        _ => None,
    };
    let prefix = |shift_counts: bool| {
        let mut p = String::new();
        if m.ctrl {
            p.push_str("C-");
        }
        if m.alt {
            p.push_str("M-");
        }
        if m.shift && shift_counts {
            p.push_str("S-");
        }
        p
    };
    if let Some(name) = special {
        return Some(format!("<{}{name}>", prefix(true)));
    }
    if !(m.ctrl || m.alt) {
        return None;
    }
    // Ctrl or Alt with a character: the character itself, shifted by hand
    // since there is no text event to say what Shift made of it.
    let name = key.symbol_or_name();
    let c = if name.chars().count() == 1 {
        let c = name.chars().next()?;
        if c.is_ascii_alphabetic() {
            if m.shift { c.to_ascii_uppercase() } else { c.to_ascii_lowercase() }
        } else {
            c
        }
    } else {
        return None;
    };
    let c = match c {
        '<' => "lt".to_string(),
        '\\' => "Bslash".to_string(),
        '|' => "Bar".to_string(),
        c => c.to_string(),
    };
    Some(format!("<{}{c}>", prefix(false)))
}

/// Typed text in Neovim's notation: every `<` spelled out, as `nvim_input`
/// reads the rest as keys.
pub fn text_keys(text: &str) -> String {
    text.replace('<', "<lt>")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_are_named_as_neovim_names_them() {
        let none = egui::Modifiers::NONE;
        let ctrl = egui::Modifiers::CTRL;
        assert_eq!(key(egui::Key::Escape, none).as_deref(), Some("<Esc>"));
        assert_eq!(key(egui::Key::Tab, egui::Modifiers::SHIFT).as_deref(), Some("<S-Tab>"));
        assert_eq!(key(egui::Key::R, ctrl).as_deref(), Some("<C-r>"));
        assert_eq!(key(egui::Key::OpenBracket, ctrl).as_deref(), Some("<C-[>"));
        assert_eq!(key(egui::Key::Space, ctrl).as_deref(), Some("<C-Space>"));
        // A plain letter is text, not a key.
        assert_eq!(key(egui::Key::R, none), None);
        assert_eq!(text_keys("a<b"), "a<lt>b");
    }

    #[test]
    fn a_redraw_batch_keeps_what_kalast_draws() {
        let batch = Value::Array(vec![
            "win_viewport".into(),
            Value::Array(vec![
                2.into(),
                Value::Ext(1, vec![0x03]),
                10.into(),
                40.into(),
                12.into(),
                4.into(),
                100.into(),
                0.into(),
            ]),
        ]);
        match redraw_batch(&batch).as_slice() {
            [Ui::Viewport { grid: 2, win: 3, top: 10, line: 12, col: 4 }] => {}
            other => panic!("{other:?}"),
        }
        let cells = Value::Array(vec!["grid_line".into(), Value::Array(vec![1.into()])]);
        assert!(redraw_batch(&cells).is_empty());
    }

    /// Poll until `done`, or fail with what Neovim is showing.
    fn wait(n: &mut Neovim, what: &str, done: impl Fn(&Neovim) -> bool) {
        for _ in 0..400 {
            n.poll();
            if done(n) {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
        panic!(
            "{what}: lines {:?}, mode {:?}, cursor {:?}, visual {:?}, messages {:?}, gone {:?}",
            n.lines, n.mode, n.cursor, n.visual, n.messages, n.gone
        );
    }

    /// The real thing, when Neovim is installed: a script in, keys in, and
    /// the lines, the mode, the cursor, the selection and kalast's own
    /// mappings back out. `--clean`, so no one's config decides the result.
    #[test]
    fn neovim_edits_the_script() {
        if find("").is_none() {
            eprintln!("no nvim on this machine; skipped");
            return;
        }
        let dir = std::env::temp_dir().join(format!("kalast-nvim-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("t.py");
        let mut n = Neovim::spawn("", &dir, (80, 20), &["--clean"], || {}).unwrap();
        n.load("a = 1\r\nb = 2\r\nc = 3\r\n", &file.to_string_lossy(), "python", None);
        wait(&mut n, "set up", |n| n.ready && n.loading.is_none() && n.grid.is_some());
        assert!(!n.take_edited(), "a load is not an edit");

        n.input("jdd");
        wait(&mut n, "dd", |n| n.lines == ["a = 1", "c = 3"]);
        assert!(n.take_edited());
        assert_eq!(n.text(), "a = 1\r\nc = 3\r\n", "the endings it came with");
        wait(&mut n, "modified", |n| n.modified);

        n.input("ciwx<Esc>");
        wait(&mut n, "ciw", |n| n.lines[1] == "x = 3" && n.mode == "normal");
        n.input("u");
        wait(&mut n, "u", |n| n.lines[1] == "c = 3");

        n.input("ggVj");
        wait(&mut n, "V", |n| n.visual.is_some_and(|v| v.kind == 'V' && v.start.0 == 0 && v.end.0 == 1));
        n.input("<Esc>");
        wait(&mut n, "leaving V", |n| n.visual.is_none() && n.mode == "normal");

        n.input("ggA # end<Esc>");
        wait(&mut n, "append", |n| n.lines[0] == "a = 1 # end" && n.cursor == (0, 10));

        n.input(":w<CR>");
        wait(&mut n, ":w", |n| n.actions.contains(&Action::Write));
        n.input(":q<CR>");
        wait(&mut n, ":q", |n| n.actions.contains(&Action::Quit(false)));
        n.input("K");
        wait(&mut n, "K", |n| n.actions.contains(&Action::Hover));
        assert!(n.gone.is_none(), ":q left Neovim running");

        n.input(":nonsense<CR>");
        wait(&mut n, "an error", |n| n.messages.iter().any(|m| m.kind == "emsg"));

        // `:bprevious` has nowhere to go: the buffer Neovim started in is
        // gone, so `:w` cannot land in a nameless one.
        n.input(":bprevious<CR>");
        n.input(":w<CR>");
        wait(&mut n, "the script still", |n| n.actions.iter().filter(|a| **a == Action::Write).count() == 2);
        assert!(!n.foreign);
        n.input(":enew<CR>");
        wait(&mut n, "another buffer", |n| n.foreign);
        n.input("<C-^>");
        wait(&mut n, "back", |n| !n.foreign);
        let other = dir.join("other.py");
        std::fs::write(&other, "x = 1\n").unwrap();
        n.input(&format!(":e {}<CR>", other.display()));
        wait(&mut n, "a file to open", |n| n.actions.iter().any(|a| matches!(a, Action::Open(p) if p.ends_with("other.py"))));
        wait(&mut n, "back to the script", |n| !n.foreign && n.lines.first().is_some_and(|l| l == "a = 1 # end"));

        // A completion accepted, with an import at the top.
        n.apply(&[((1, 0), (1, 1), "xyz".into()), ((0, 0), (0, 0), "import os\n".into())], (2, 3));
        wait(&mut n, "apply", |n| n.lines == ["import os", "a = 1 # end", "xyz = 3"] && n.cursor == (2, 3));

        drop(n);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The text comes back with the line endings it went in with: this
    /// clone checks its scripts out with CRLF, and a buffer of `^M`s would
    /// have been the first thing anyone saw.
    #[test]
    fn a_script_survives_the_buffer() {
        for text in ["a = 1\r\nb = 2\r\n", "a = 1\nb = 2", "x\n", "", "\n\n"] {
            let (lines, eol, crlf) = split(text);
            assert!(!lines.is_empty());
            assert!(lines.iter().all(|l| !l.contains('\r')), "{text:?}");
            assert_eq!(join(&lines, eol, crlf), text, "{text:?}");
        }
    }
}
