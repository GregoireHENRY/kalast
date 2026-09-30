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
    /// The selection, Cmd+C or Cmd+X: for the clipboard.
    Copy(String),
}

/// A search's match, as Neovim highlights it on its screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Found {
    /// `Search`: every match, while `hlsearch` is on.
    Match,
    /// `CurSearch` or `IncSearch`: the one under the cursor, or the one an
    /// incremental search has reached.
    Current,
}

/// A grid of Neovim's screen, by the highlight each cell is drawn with --
/// all kalast keeps of it, since it draws the text itself from the buffer:
/// enough to find what Neovim highlights there.
#[derive(Debug, Default)]
struct Grid {
    width: usize,
    height: usize,
    /// Row by row.
    cells: Vec<u64>,
}

impl Grid {
    fn resize(&mut self, width: usize, height: usize) {
        let mut cells = vec![0; width * height];
        for r in 0..height.min(self.height) {
            for c in 0..width.min(self.width) {
                cells[r * width + c] = self.cells[r * self.width + c];
            }
        }
        *self = Grid { width, height, cells };
    }

    /// `grid_line`'s cells from `col` on, as runs of `(highlight, count)`.
    fn line(&mut self, row: usize, col: usize, runs: &[(u64, usize)]) {
        if row >= self.height {
            return;
        }
        let mut c = col;
        for &(hl, n) in runs {
            for _ in 0..n {
                if c < self.width {
                    self.cells[row * self.width + c] = hl;
                }
                c += 1;
            }
        }
    }

    /// `grid_scroll`: the region's content moves up by `rows`, down when it
    /// is negative; the rows it leaves are redrawn after.
    fn scroll(&mut self, top: usize, bot: usize, left: usize, right: usize, rows: i64) {
        let (w, bot, right) = (self.width, bot.min(self.height), right.min(self.width));
        let n = rows.unsigned_abs() as usize;
        if rows > 0 {
            for r in top..bot.saturating_sub(n) {
                for c in left..right {
                    self.cells[r * w + c] = self.cells[(r + n) * w + c];
                }
            }
        } else {
            for r in (top + n..bot).rev() {
                for c in left..right {
                    self.cells[r * w + c] = self.cells[(r - n) * w + c];
                }
            }
        }
    }
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
    /// `hl_attr_define`: which of a search's highlights, if any, the
    /// highlight is made of.
    HlAttr(u64, Option<Found>),
    GridResize(i64, usize, usize),
    GridClear(i64),
    GridDestroy(i64),
    /// A row's cells from a column on, as runs of `(highlight, count)`.
    GridLine(i64, usize, usize, Vec<(u64, usize)>),
    GridScroll { grid: i64, top: usize, bot: usize, left: usize, right: usize, rows: i64 },
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
    /// The buffer's `tabstop`: how many screen columns a tab takes.
    tabstop: usize,
    /// Neovim's screen, by grid, and which of its highlights are a
    /// search's.
    grids: HashMap<i64, Grid>,
    found_by: HashMap<u64, Found>,
    /// What Neovim's screen shows of a search, in the buffer's terms:
    /// `(line, first byte, byte after, how)`.
    pub found: Vec<(usize, usize, usize, Found)>,
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
-- The mode goes with it, each time it changes: back to Normal mode too,
-- which it once did not say, and kalast went on taking the keys for text.
local last
local function selection()
  local m = vim.api.nvim_get_mode().mode
  local c = m:sub(1, 1)
  if c == 'v' or c == 'V' or c == '\22' or c == 's' or c == 'S' or c == '\19' then
    local s = vim.fn.getpos('v')
    local e = vim.api.nvim_win_get_cursor(0)
    last = m
    notify('visual', m, s[2] - 1, s[3] - 1, e[1] - 1, e[2])
  elseif m ~= last then
    last = m
    notify('visual', m)
  end
end
vim.api.nvim_create_autocmd({ 'ModeChanged', 'CursorMoved' }, { group = group, callback = selection })
local function options()
  local info = vim.fn.getwininfo(win)[1]
  notify('options', vim.wo[win].number, vim.wo[win].relativenumber, info and info.textoff or 0, vim.bo[buf].tabstop)
end
vim.api.nvim_create_autocmd('OptionSet', { group = group, callback = function() vim.schedule(options) end })
vim.api.nvim_create_autocmd({ 'BufWinEnter', 'WinResized', 'VimResized' }, { group = group, callback = function() vim.schedule(options) end })
-- What a language server answers, on the keys VS Code's Neovim extension
-- gives them and an LspAttach usually does.
for lhs, action in pairs({ K = 'hover', gh = 'hover', gd = 'definition', gD = 'definition',
                           ['<C-]>'] = 'definition', [']d'] = 'next_diagnostic', ['[d'] = 'prev_diagnostic' }) do
  vim.keymap.set('n', lhs, function() notify(action) end, { buffer = buf, desc = 'kalast: ' .. action })
end
-- macOS's editing keys that mean one thing in one mode and another in the
-- next: kalast sends them as calls to these, which act in the mode Neovim is
-- in when it reads them (`nvim::typed`). The selection's text goes to kalast
-- for the clipboard, which needs no clipboard tool on the system.
local function keys(k) return vim.api.nvim_replace_termcodes(k, true, false, true) end
local function selected()
  local mode = vim.fn.mode()
  if not mode:match('^[vV\22sS\19]') then return nil end
  local kind = ({ s = 'v', S = 'V', ['\19'] = '\22' })[mode:sub(1, 1)] or mode:sub(1, 1)
  local ok, lines = pcall(vim.fn.getregion, vim.fn.getpos('v'), vim.fn.getpos('.'), { type = kind })
  if not ok then return nil end
  return table.concat(lines, '\n') .. (kind == 'V' and '\n' or ''), mode
end
kalast_keys = {
  -- Cmd+C, Cmd+X: the selection copied, and cut -- still selected after a
  -- copy, as in VS Code.
  copy = function(cut)
    local text, mode = selected()
    if not text then return end
    notify('copy', text)
    if cut then vim.api.nvim_feedkeys(keys(mode:match('^[sS\19]') and '<C-g>d' or 'd'), 'in', false) end
  end,
  -- Option+Backspace, Cmd+Backspace: a word, or the line, before the cursor
  -- -- where text is being typed, and nowhere else.
  erase = function(what)
    local key = ({ word = '<C-w>', line = '<C-u>' })[what]
    if key and vim.fn.mode():match('^[iRc]') then vim.api.nvim_feedkeys(keys(key), 'in', false) end
  end,
}
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
/// comment string. Returns the `tabstop` the filetype gave it.
const LOAD: &str = r#"
local buf, lines, name, ft, eol, crlf, line = ...
local levels = vim.bo[buf].undolevels
vim.bo[buf].undolevels = -1
vim.api.nvim_buf_set_lines(buf, 0, -1, false, lines)
vim.bo[buf].undolevels = levels
-- A script with no file yet is named anyway: `:w` on a buffer without a name
-- says E32 before BufWriteCmd is asked, and kalast is what asks where to
-- save it.
if name == '' then
  pcall(vim.api.nvim_buf_set_name, buf, 'kalast://untitled')
else
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
return vim.bo[buf].tabstop
"#;

/// The Neovim config kalast ships -- its author's -- compiled in, by its
/// path in the config folder. `neovim_config = "kalast"`, the default,
/// writes it out (`kalast_config`) and has Neovim read it there.
const KALAST_CONFIG: &[(&str, &str)] = &[
    ("init.lua", include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/res/neovim/init.lua"))),
    ("lazy-lock.json", include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/res/neovim/lazy-lock.json"))),
    ("lua/settings.lua", include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/res/neovim/lua/settings.lua"))),
    ("lua/plugins.lua", include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/res/neovim/lua/plugins.lua"))),
    ("lua/plugins/completion.lua", include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/res/neovim/lua/plugins/completion.lua"))),
    ("lua/plugins/editor.lua", include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/res/neovim/lua/plugins/editor.lua"))),
    ("lua/plugins/lsp.lua", include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/res/neovim/lua/plugins/lsp.lua"))),
    ("lua/plugins/treesitter.lua", include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/res/neovim/lua/plugins/treesitter.lua"))),
    ("lua/plugins/ui.lua", include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/res/neovim/lua/plugins/ui.lua"))),
];

/// Neovim's name for kalast's config: its folder, and the one its plugins
/// and state go in -- `~/.local/share/kalast-nvim` -- apart from the user's
/// own Neovim's.
const KALAST_APPNAME: &str = "kalast-nvim";

/// Write kalast's config into `base/kalast-nvim`, each file where it is
/// missing or differs, and return `base`: what `XDG_CONFIG_HOME` is set to
/// for Neovim to find it. The copy is kalast's, rewritten from the binary;
/// a config of one's own is `neovim_config`.
fn kalast_config_in(base: &Path) -> Result<PathBuf, String> {
    let dir = base.join(KALAST_APPNAME);
    for (name, text) in KALAST_CONFIG {
        let path = dir.join(name);
        if std::fs::read_to_string(&path).ok().as_deref() == Some(*text) {
            continue;
        }
        let written = path
            .parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .and_then(|()| std::fs::write(&path, text));
        written.map_err(|e| format!("cannot write kalast's Neovim config to {}: {e}", path.display()))?;
    }
    Ok(base.to_path_buf())
}

/// What `nvim` is given for the config `neovim_config` names: environment
/// variables, or `-u` and a file.
///
/// - `"kalast"`, or empty: the config kalast ships, written beside the
///   app's settings (`kalast_config_in`) and found through
///   `XDG_CONFIG_HOME` and `NVIM_APPNAME`, so that its plugins install
///   apart from the user's own Neovim's.
/// - `"user"`: the user's own, where Neovim looks for it -- nothing given.
/// - a path: a config folder, holding `init.lua` or `init.vim`, read the
///   same way as kalast's; or one file, read with `-u`.
fn config_choice(config: &str) -> Result<(Vec<String>, Vec<(&'static str, std::ffi::OsString)>), String> {
    let folder = |dir: &Path| -> Result<Vec<(&'static str, std::ffi::OsString)>, String> {
        let (Some(parent), Some(name)) = (dir.parent(), dir.file_name()) else {
            return Err(format!("{} is no config folder Neovim can be pointed at", dir.display()));
        };
        Ok(vec![("XDG_CONFIG_HOME", parent.as_os_str().to_owned()), ("NVIM_APPNAME", name.to_owned())])
    };
    match config.trim() {
        "" | "kalast" => {
            let base = crate::app::settings::path()
                .and_then(|p| p.parent().map(|d| d.join("neovim")))
                .ok_or("no folder to write kalast's Neovim config in")?;
            let base = kalast_config_in(&base)?;
            Ok((Vec::new(), folder(&base.join(KALAST_APPNAME))?))
        }
        "user" => Ok((Vec::new(), Vec::new())),
        path => {
            let path = match path.strip_prefix("~/").or_else(|| path.strip_prefix("~\\")) {
                Some(rest) => std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
                    .map(|home| PathBuf::from(home).join(rest))
                    .unwrap_or_else(|| PathBuf::from(path)),
                None => PathBuf::from(path),
            };
            if path.is_dir() {
                Ok((Vec::new(), folder(&path)?))
            } else if path.is_file() {
                Ok((vec!["-u".to_string(), path.to_string_lossy().into_owned()], Vec::new()))
            } else {
                Err(format!("no Neovim config at {}: app.config.neovim_config", path.display()))
            }
        }
    }
}

impl Neovim {
    /// Start `program` -- `nvim` found on the PATH if empty -- in `cwd`,
    /// with a grid of `size` cells, reading the config `config` names
    /// (`config_choice`). Returns at once; the setup lands in later frames,
    /// through `poll`.
    pub fn start(
        program: &str,
        config: &str,
        cwd: &Path,
        size: (u32, u32),
        repaint: impl Fn() + Send + 'static,
    ) -> Result<Self, String> {
        let (args, env) = config_choice(config)?;
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        Self::spawn_with(program, cwd, size, &args, &env, repaint)
    }

    /// `start`, with more arguments for `nvim`: `--clean`, for a test that
    /// must not depend on whose config is installed.
    #[cfg(test)]
    pub(super) fn spawn(
        program: &str,
        cwd: &Path,
        size: (u32, u32),
        extra: &[&str],
        repaint: impl Fn() + Send + 'static,
    ) -> Result<Self, String> {
        Self::spawn_with(program, cwd, size, extra, &[], repaint)
    }

    fn spawn_with(
        program: &str,
        cwd: &Path,
        size: (u32, u32),
        extra: &[&str],
        env: &[(&'static str, std::ffi::OsString)],
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
            .envs(env.iter().map(|(k, v)| (*k, v)))
            .current_dir(cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        crate::app::without_bundled_python_home(&mut command);
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
            tabstop: 8,
            grids: HashMap::new(),
            found_by: HashMap::new(),
            found: Vec::new(),
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
            // Each highlight with the groups it is made of: a search's
            // match on a keyword is `Search` over the keyword's colour.
            "ext_hlstate",
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
        // status bar until Neovim clears or replaces it, or a mode begins
        // over it (`begins_over_the_message`).
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
                if let Some(ts) = result.as_u64() {
                    self.tabstop = (ts as usize).max(1);
                }
            }
            Call::Attach | Call::Other => {}
        }
    }

    fn redraw(&mut self, event: Ui) {
        match event {
            Ui::ModeInfo(shapes) => self.shapes = shapes.into_iter().collect(),
            Ui::Mode(mode) => {
                // A mode begun over the message goes with it, as in a
                // terminal, where `-- INSERT --` or the command line is
                // written on the message line. kalast shows those apart,
                // and with 'showmode' off nothing was written at all. Not
                // on the way back to Normal: an error comes with that
                // change, and has yet to be read.
                if mode != self.mode && begins_over_the_message(&mode) {
                    self.messages.clear();
                }
                self.mode = mode;
            }
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
            Ui::MsgShowmode(text) => {
                // In a terminal `-- INSERT --` is written over the message
                // line, so an error goes as the mode changes. kalast shows
                // the mode apart, and the error stayed beside it.
                if text != self.showmode {
                    self.messages.clear();
                }
                self.showmode = text;
            }
            Ui::MsgShowcmd(text) => self.showcmd = text,
            Ui::MsgHistory(messages) => self.messages = messages,
            Ui::PopupShow(popup, _grid) => self.popup = Some(popup),
            Ui::PopupSelect(selected) => {
                if let Some(p) = &mut self.popup {
                    p.selected = selected;
                }
            }
            Ui::PopupHide => self.popup = None,
            Ui::HlAttr(id, found) => match found {
                Some(f) => {
                    self.found_by.insert(id, f);
                }
                None => {
                    self.found_by.remove(&id);
                }
            },
            Ui::GridResize(grid, width, height) => self.grids.entry(grid).or_default().resize(width, height),
            Ui::GridClear(grid) => {
                if let Some(g) = self.grids.get_mut(&grid) {
                    g.cells.fill(0);
                }
            }
            Ui::GridDestroy(grid) => {
                self.grids.remove(&grid);
            }
            Ui::GridLine(grid, row, col, runs) => {
                if let Some(g) = self.grids.get_mut(&grid) {
                    g.line(row, col, &runs);
                }
            }
            Ui::GridScroll { grid, top, bot, left, right, rows } => {
                if let Some(g) = self.grids.get_mut(&grid) {
                    g.scroll(top, bot, left, right, rows);
                }
            }
            Ui::Flush => {
                self.unflushed = false;
                self.found = match self.grid.and_then(|g| self.grids.get(&g)) {
                    Some(g) if !self.found_by.is_empty() => {
                        found_spans(g, &self.found_by, &self.lines, self.topline, self.textoff, self.tabstop)
                    }
                    _ => Vec::new(),
                };
            }
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
            "copy" => {
                if let Some(t) = args.get(1).map(text) {
                    self.actions.push(Action::Copy(t));
                }
            }
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
                if let Some(ts) = args.get(4).and_then(Value::as_u64) {
                    self.tabstop = (ts as usize).max(1);
                }
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

/// Whether a mode, as `mode_change` names it, is written over the message
/// line in a terminal as it begins: Insert and Replace with their `-- INSERT
/// --`, Visual and Select with theirs, and the command line itself -- `:`,
/// `/`, `?`. Not Normal, nor the operator pending after `d`.
fn begins_over_the_message(mode: &str) -> bool {
    ["insert", "replace", "visual", "cmdline"].iter().any(|m| mode.starts_with(m))
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
        "hl_attr_define" => calls.map(|a| Ui::HlAttr(a.first().and_then(Value::as_u64).unwrap_or(0), found_in(a.get(3)))).collect(),
        "grid_resize" => calls.map(|a| Ui::GridResize(int(a, 0), int(a, 1).max(0) as usize, int(a, 2).max(0) as usize)).collect(),
        "grid_clear" => calls.map(|a| Ui::GridClear(int(a, 0))).collect(),
        "grid_destroy" => calls.map(|a| Ui::GridDestroy(int(a, 0))).collect(),
        "grid_line" => calls
            .filter_map(|a| {
                // `[text, highlight, repeat]`, the highlight left out when
                // it is the one before, the repeat when it is one.
                let mut hl = 0;
                let runs = a
                    .get(3)?
                    .as_array()?
                    .iter()
                    .filter_map(Value::as_array)
                    .map(|cell| {
                        if let Some(h) = cell.get(1).and_then(Value::as_u64) {
                            hl = h;
                        }
                        (hl, cell.get(2).and_then(Value::as_u64).unwrap_or(1) as usize)
                    })
                    .collect();
                Some(Ui::GridLine(int(a, 0), int(a, 1).max(0) as usize, int(a, 2).max(0) as usize, runs))
            })
            .collect(),
        "grid_scroll" => calls
            .map(|a| Ui::GridScroll {
                grid: int(a, 0),
                top: int(a, 1).max(0) as usize,
                bot: int(a, 2).max(0) as usize,
                left: int(a, 3).max(0) as usize,
                right: int(a, 4).max(0) as usize,
                rows: int(a, 5),
            })
            .collect(),
        "flush" => vec![Ui::Flush],
        _ => Vec::new(),
    }
}

/// Which of a search's highlights a highlight is made of, from
/// `ext_hlstate`'s account of its groups.
fn found_in(info: Option<&Value>) -> Option<Found> {
    let mut how = None;
    for item in info?.as_array()? {
        let name = item
            .as_map()
            .and_then(|m| m.iter().find(|(k, _)| k.as_str() == Some("ui_name")))
            .and_then(|(_, v)| v.as_str());
        match name {
            Some("CurSearch" | "IncSearch") => return Some(Found::Current),
            Some("Search") => how = Some(Found::Match),
            _ => {}
        }
    }
    how
}

/// What a grid shows of a search, as spans of the buffer's lines. Each row
/// is a line from `top` on -- kalast's window is too wide to wrap and folds
/// nothing -- and the first `textoff` columns are the line numbers.
fn found_spans(
    grid: &Grid,
    found_by: &HashMap<u64, Found>,
    lines: &[String],
    top: usize,
    textoff: usize,
    tabstop: usize,
) -> Vec<(usize, usize, usize, Found)> {
    let mut spans = Vec::new();
    for row in 0..grid.height {
        let Some(text) = lines.get(top + row) else { break };
        let cells = &grid.cells[row * grid.width..(row + 1) * grid.width];
        let mut col = textoff;
        while col < cells.len() {
            let Some(&how) = found_by.get(&cells[col]) else {
                col += 1;
                continue;
            };
            let start = col;
            while col < cells.len() && found_by.get(&cells[col]) == Some(&how) {
                col += 1;
            }
            let (a, b) = bytes_at(text, start - textoff, col - textoff, tabstop);
            spans.push((top + row, a, b, how));
        }
    }
    spans
}

/// The bytes of `text` its screen columns `from..to` show, as Neovim lays it
/// out: a tab to the next multiple of `tabstop`, a wide character over two
/// columns, an accent that combines with the one before it.
fn bytes_at(text: &str, from: usize, to: usize, tabstop: usize) -> (usize, usize) {
    let (mut a, mut b) = (None, text.len());
    let mut column = 0;
    for (i, c) in text.char_indices() {
        let w = if c == '\t' {
            tabstop - column % tabstop
        } else {
            unicode_width::UnicodeWidthChar::width(c).unwrap_or(0)
        };
        if w > 0 && column >= to {
            b = i;
            break;
        }
        if a.is_none() && column + w > from {
            a = Some(i);
        }
        column += w;
    }
    let a = a.unwrap_or(text.len());
    (a, b.max(a))
}

/// A key that types nothing, by Neovim's name for it: `Esc`, `CR`, `Left`.
/// `None` for the keys that type a character, Space among them.
fn named(key: egui::Key) -> Option<&'static str> {
    use egui::Key::*;
    Some(match key {
        Escape => "Esc",
        Enter => "CR",
        Tab => "Tab",
        Backspace => "BS",
        Delete => "Del",
        Insert => "Insert",
        Home => "Home",
        End => "End",
        PageUp => "PageUp",
        PageDown => "PageDown",
        ArrowLeft => "Left",
        ArrowRight => "Right",
        ArrowUp => "Up",
        ArrowDown => "Down",
        F1 => "F1",
        F2 => "F2",
        F3 => "F3",
        F4 => "F4",
        F5 => "F5",
        F6 => "F6",
        F7 => "F7",
        F8 => "F8",
        F9 => "F9",
        F10 => "F10",
        F11 => "F11",
        F12 => "F12",
        _ => return None,
    })
}

/// A key egui reports, in Neovim's notation -- or `None` for one that
/// arrives as text instead, and for keys Neovim has no name for.
///
/// Plain characters come through egui's `Text` events, which already
/// account for the keyboard layout and Shift; a key is only named here when
/// it is not a character (`<Esc>`, `<CR>`, `<Left>`) or carries Ctrl or Alt,
/// which suppress the text.
pub fn key(key: egui::Key, m: egui::Modifiers) -> Option<String> {
    let special = named(key).or((key == egui::Key::Space && (m.ctrl || m.alt)).then_some("Space"));
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

/// macOS's editing keys that mean one thing in one mode and another in the
/// next -- `<C-w>` erases a word in Insert mode and begins a window command
/// in Normal mode -- sent as calls to `kalast_keys` (SETUP), which act in
/// the mode Neovim is in when it reads them, not the one kalast last saw.
/// Nothing in angle brackets inside: `nvim_input` would read it as a key.
const COPY: &str = "<Cmd>lua kalast_keys.copy(false)<CR>";
const CUT: &str = "<Cmd>lua kalast_keys.copy(true)<CR>";
const ERASE_WORD: &str = "<Cmd>lua kalast_keys.erase('word')<CR>";
const ERASE_LINE: &str = "<Cmd>lua kalast_keys.erase('line')<CR>";
/// Cmd+A, from whatever mode: all of it, by lines.
const SELECT_ALL: &str = "<C-\\><C-n>ggVG";

/// A frame's keyboard, for Neovim.
#[derive(Debug, Default, PartialEq)]
pub struct Typed {
    /// Keys, in Neovim's notation, for `nvim_input`.
    pub keys: String,
    /// Text from the clipboard, for `nvim_paste`.
    pub pastes: Vec<String>,
    /// Ctrl+S, or Cmd+S: kalast saves, as VS Code does with its Neovim
    /// extension.
    pub save: bool,
    /// An input method's composition -- a dead key's accent waiting for its
    /// letter -- when the frame changed it; empty once it is done.
    pub preedit: Option<String>,
}

/// The frame's keyboard events as Neovim has them from a terminal, and
/// macOS's editing shortcuts as VS Code does them.
///
/// - Text is text: what the layout, Shift and, on macOS, Option made of a
///   key. Option+5 is `{` on a French Mac; sent as `<M-{>` it left Insert
///   mode, since Neovim reads an Alt key nothing maps as Escape and the key.
/// - Ctrl, and Alt outside macOS, with a key are that key: `<C-r>`, `<M-x>`.
/// - Command on macOS is the system's, as in a terminal, which never passes
///   it on -- apart from its editing keys, VS Code's: Cmd with the arrows to
///   the ends of the line or the file, with Backspace to the start of the
///   line, Cmd+A, and Cmd+C and Cmd+X on the selection; Option with the
///   arrows and Backspace goes by words. None of them may reach Neovim as
///   `<D-z>` or `<M-Left>`: unmapped, `<D-z>` is typed out in Insert mode,
///   `<D-c>` is `c` in Visual mode, and `<M-Left>` leaves Insert mode.
///
/// `inserting`: Neovim takes text -- Insert mode, the command line -- where
/// Ctrl+V pastes; elsewhere it begins a Visual block. `ctrl`: the frame's
/// Ctrl, which a paste does not carry.
pub fn typed(events: &[egui::Event], mac: bool, inserting: bool, ctrl: bool) -> Typed {
    use egui::Key::*;
    let mut t = Typed::default();
    let mut skip_text: Option<String> = None;
    for event in events {
        match event {
            egui::Event::Text(text) => {
                // Alt with a character came as a key already, `<M-x>`.
                if skip_text.take().is_some_and(|s| s.eq_ignore_ascii_case(text)) {
                    continue;
                }
                t.keys.push_str(&text_keys(text));
            }
            egui::Event::Key { key, pressed: true, modifiers: m, .. } => {
                let key = *key;
                skip_text = None;
                if m.command && key == S {
                    t.save = true;
                } else if mac && m.mac_cmd {
                    t.keys.push_str(match key {
                        ArrowLeft => "<Home>",
                        ArrowRight => "<End>",
                        ArrowUp => "<C-Home>",
                        ArrowDown => "<C-End>",
                        Backspace => ERASE_LINE,
                        A => SELECT_ALL,
                        _ => "",
                    });
                } else if mac && m.alt && !m.ctrl {
                    match key {
                        ArrowLeft => t.keys.push_str("<C-Left>"),
                        ArrowRight => t.keys.push_str("<C-Right>"),
                        Backspace => t.keys.push_str(ERASE_WORD),
                        // Its character comes as text -- a dead key's from
                        // the input method, once composed.
                        _ if named(key).is_none() => {}
                        // Option means nothing to the other keys that type
                        // nothing, as in a terminal.
                        _ => t.keys.push_str(&self::key(key, egui::Modifiers { alt: false, ..*m }).unwrap_or_default()),
                    }
                } else if let Some(k) = self::key(key, *m) {
                    t.keys.push_str(&k);
                    if m.alt && !m.ctrl {
                        skip_text = Some(if key == Space { " ".into() } else { key.symbol_or_name().into() });
                    }
                }
            }
            egui::Event::Copy => t.keys.push_str(if mac { COPY } else { "<C-c>" }),
            egui::Event::Cut => t.keys.push_str(if mac { CUT } else { "<C-x>" }),
            egui::Event::Paste(text) => {
                // Ctrl+V is Visual block outside Insert mode; pasting there
                // is `p`, or Shift+Insert. Cmd+V pastes anywhere.
                if ctrl && !inserting {
                    t.keys.push_str("<C-v>");
                } else {
                    t.pastes.push(text.clone());
                }
            }
            egui::Event::Ime(egui::ImeEvent::Preedit { text, .. }) => t.preedit = Some(text.clone()),
            egui::Event::Ime(egui::ImeEvent::Commit(text)) => {
                t.keys.push_str(&text_keys(text));
                t.preedit = Some(String::new());
            }
            _ => {}
        }
    }
    t
}

/// Typed text in Neovim's notation: every `<` spelled out, as `nvim_input`
/// reads the rest as keys.
pub fn text_keys(text: &str) -> String {
    text.replace('<', "<lt>")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// kalast's config is written out whole, as compiled in, and written
    /// again over a copy that was changed; a second start rewrites nothing.
    #[test]
    fn kalast_config_is_written_out_as_shipped() {
        let base = std::env::temp_dir().join(format!("kalast-nvim-config-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        assert_eq!(kalast_config_in(&base).unwrap(), base);
        let dir = base.join(KALAST_APPNAME);
        for (name, text) in KALAST_CONFIG {
            assert_eq!(std::fs::read_to_string(dir.join(name)).unwrap(), *text, "{name}");
        }
        std::fs::write(dir.join("init.lua"), "-- changed").unwrap();
        kalast_config_in(&base).unwrap();
        assert_eq!(std::fs::read_to_string(dir.join("init.lua")).unwrap(), KALAST_CONFIG[0].1);
        std::fs::remove_dir_all(&base).unwrap();
    }

    /// `"user"` gives Neovim nothing; a folder is read as a config folder,
    /// through `XDG_CONFIG_HOME` and `NVIM_APPNAME`; a file with `-u`; and a
    /// path to nothing is said so.
    #[test]
    fn neovim_config_names_how_neovim_finds_it() {
        assert_eq!(config_choice("user").unwrap(), (Vec::new(), Vec::new()));

        let base = std::env::temp_dir().join(format!("kalast-nvim-choice-{}", std::process::id()));
        let folder = base.join("mine");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("init.lua"), "").unwrap();
        let (args, env) = config_choice(folder.to_str().unwrap()).unwrap();
        assert!(args.is_empty());
        assert_eq!(env, vec![("XDG_CONFIG_HOME", base.clone().into_os_string()), ("NVIM_APPNAME", "mine".into())]);

        let file = folder.join("init.lua");
        let (args, env) = config_choice(file.to_str().unwrap()).unwrap();
        assert_eq!(args, vec!["-u".to_string(), file.to_string_lossy().into_owned()]);
        assert!(env.is_empty());

        assert!(config_choice(base.join("nothing").to_str().unwrap()).unwrap_err().contains("no Neovim config at"));
        std::fs::remove_dir_all(&base).unwrap();
    }

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

    /// What each key a keyboard gives becomes, on macOS and elsewhere: the
    /// events egui has from winit for it, in the order it has them.
    #[test]
    fn keys_reach_neovim_as_a_terminal_or_vs_code_sends_them() {
        use egui::{Event, Key, Modifiers};
        let press = |key: Key, modifiers: Modifiers| Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers };
        let text = |t: &str| Event::Text(t.into());
        let none = Modifiers::NONE;
        let alt = Modifiers::ALT;
        let cmd = Modifiers { mac_cmd: true, command: true, ..Default::default() };
        let keys = |events: &[Event], mac: bool| typed(events, mac, false, false).keys;

        for mac in [true, false] {
            // Space and Enter are text and a key, whatever the pointer does.
            assert_eq!(keys(&[press(Key::Space, none), text(" ")], mac), " ");
            assert_eq!(keys(&[press(Key::Enter, none)], mac), "<CR>");
            assert_eq!(keys(&[press(Key::R, Modifiers::CTRL)], mac), "<C-r>");
            assert_eq!(keys(&[press(Key::A, Modifiers::SHIFT), text("A")], mac), "A");
            assert_eq!(keys(&[text("<")], mac), "<lt>");
        }

        // Option+5 on a French Mac: `{`, which `<M-{>` never typed. Alt
        // elsewhere is Meta, the text it comes with dropped.
        let brace = [press(Key::OpenCurlyBracket, alt), text("{")];
        assert_eq!(keys(&brace, true), "{");
        assert_eq!(keys(&brace, false), "<M-{>");
        // A dead key -- Option+N, `~` -- types nothing yet on macOS.
        assert_eq!(keys(&[press(Key::N, alt)], true), "");
        assert_eq!(keys(&[press(Key::N, alt)], false), "<M-n>");
        assert_eq!(keys(&[press(Key::Space, alt), text(" ")], false), "<M-Space>");

        // macOS's editing keys, VS Code's.
        assert_eq!(keys(&[press(Key::ArrowLeft, alt)], true), "<C-Left>");
        assert_eq!(keys(&[press(Key::ArrowRight, alt)], true), "<C-Right>");
        assert_eq!(keys(&[press(Key::Backspace, alt)], true), ERASE_WORD);
        assert_eq!(keys(&[press(Key::ArrowUp, alt)], true), "<Up>");
        assert_eq!(keys(&[press(Key::ArrowLeft, cmd)], true), "<Home>");
        assert_eq!(keys(&[press(Key::ArrowRight, cmd)], true), "<End>");
        assert_eq!(keys(&[press(Key::ArrowUp, cmd)], true), "<C-Home>");
        assert_eq!(keys(&[press(Key::ArrowDown, cmd)], true), "<C-End>");
        assert_eq!(keys(&[press(Key::Backspace, cmd)], true), ERASE_LINE);
        assert_eq!(keys(&[press(Key::A, cmd)], true), SELECT_ALL);
        // The rest of Command is the system's, as in a terminal: `<D-z>`
        // would be typed out in Insert mode.
        assert_eq!(keys(&[press(Key::Z, cmd)], true), "");
        assert_eq!(keys(&[press(Key::Enter, cmd)], true), "");
        // Alt elsewhere, as a terminal sends it.
        assert_eq!(keys(&[press(Key::ArrowLeft, alt)], false), "<M-Left>");
        assert_eq!(keys(&[press(Key::Backspace, alt)], false), "<M-BS>");

        // The clipboard: Cmd+C and Cmd+X act on the selection; Ctrl+C and
        // Ctrl+X are Neovim's own.
        assert_eq!(keys(&[Event::Copy], true), COPY);
        assert_eq!(keys(&[Event::Cut], true), CUT);
        assert_eq!(keys(&[Event::Copy], false), "<C-c>");
        assert_eq!(keys(&[Event::Cut], false), "<C-x>");
        let paste = [Event::Paste("x = 1".into())];
        assert_eq!(typed(&paste, true, false, false).pastes, ["x = 1"]);
        assert_eq!(typed(&paste, false, false, true).keys, "<C-v>", "Visual block in Normal mode");
        assert_eq!(typed(&paste, false, true, true).pastes, ["x = 1"], "a paste in Insert mode");
        assert!(typed(&[press(Key::S, cmd)], true, false, false).save);
        assert!(typed(&[press(Key::S, Modifiers { ctrl: true, command: true, ..Default::default() })], false, false, false).save);

        // An input method: the accent shown while it waits, then the letter.
        let waiting = typed(&[Event::Ime(egui::ImeEvent::Preedit { text: "ˆ".into(), active_range_chars: None })], true, true, false);
        assert_eq!((waiting.keys.as_str(), waiting.preedit.as_deref()), ("", Some("ˆ")));
        let done = typed(
            &[Event::Ime(egui::ImeEvent::Preedit { text: String::new(), active_range_chars: None }), Event::Ime(egui::ImeEvent::Commit("ê".into()))],
            true,
            true,
            false,
        );
        assert_eq!((done.keys.as_str(), done.preedit.as_deref()), ("ê", Some("")));
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

    /// An error stays until the next thing is begun, and goes then, as in a
    /// terminal, where `-- INSERT --` or the command line is written over
    /// it: Insert mode, or a new command after `:`. Asked for: "after a
    /// neovim error for example if i type :W instead of :w, should be
    /// removed if i press I for insert mode or if i type : again". With
    /// 'showmode' off too, where no `-- INSERT --` comes to say so.
    #[test]
    fn an_error_goes_when_insert_or_a_new_command_begins() {
        if find("").is_none() {
            eprintln!("no nvim on this machine; skipped");
            return;
        }
        for showmode in ["showmode", "noshowmode"] {
            let dir = std::env::temp_dir().join(format!("kalast-nvim-error-{showmode}-{}", std::process::id()));
            std::fs::create_dir_all(&dir).unwrap();
            let file = dir.join("t.py");
            let set = format!("set {showmode}");
            let mut n = Neovim::spawn("", &dir, (80, 20), &["--clean", "-c", &set], || {}).unwrap();
            n.load("a = 1\n", &file.to_string_lossy(), "python", None);
            wait(&mut n, "set up", |n| n.ready && n.loading.is_none() && n.grid.is_some());
            let error = |n: &Neovim| n.messages.iter().any(|m| m.kind == "emsg");

            n.input(":W<CR>");
            wait(&mut n, "the error", |n| n.mode == "normal" && error(n));
            // Back in Normal mode, and moving about, it stays to be read.
            n.input("l");
            wait(&mut n, "moved", |n| n.cursor == (0, 1));
            for _ in 0..8 {
                n.poll();
                std::thread::sleep(std::time::Duration::from_millis(25));
            }
            assert!(error(&n), "{showmode}: the error went by itself: {:?}", n.messages);

            n.input("i");
            wait(&mut n, "Insert mode, the error gone", |n| n.mode == "insert" && n.messages.is_empty());

            n.input("<Esc>:W<CR>");
            wait(&mut n, "the error again", |n| n.mode == "normal" && error(n));
            n.input(":");
            wait(&mut n, "a new command, the error gone", |n| n.mode.starts_with("cmdline") && n.messages.is_empty());
            n.input("<Esc>");
            wait(&mut n, "back", |n| n.mode == "normal");

            drop(n);
            let _ = std::fs::remove_dir_all(&dir);
        }
    }

    /// macOS's editing keys, as `typed` sends them, each acting in the mode
    /// Neovim is in when it reads it: Cmd+C copies the selection and keeps
    /// it, Cmd+X cuts it, Cmd+A takes everything from Insert mode, and
    /// Option+Backspace erases a word in Insert mode but nothing in Normal
    /// mode, where `<C-w>` would begin a window command and eat the next key.
    #[test]
    fn macos_editing_keys_act_in_the_mode_neovim_is_in() {
        if find("").is_none() {
            eprintln!("no nvim on this machine; skipped");
            return;
        }
        let dir = std::env::temp_dir().join(format!("kalast-nvim-keys-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("t.py");
        let mut n = Neovim::spawn("", &dir, (80, 20), &["--clean"], || {}).unwrap();
        n.load("abc def\nghi\njkl\n", &file.to_string_lossy(), "python", None);
        wait(&mut n, "set up", |n| n.ready && n.loading.is_none() && n.grid.is_some());

        n.input(&format!("vl{COPY}"));
        wait(&mut n, "copied", |n| n.actions.contains(&Action::Copy("ab".into())));
        wait(&mut n, "still selected", |n| n.visual.is_some_and(|v| v.kind == 'v'));

        n.input(&format!("<Esc>A{SELECT_ALL}"));
        wait(&mut n, "all of it", |n| n.visual.is_some_and(|v| v.kind == 'V' && v.start.0 == 0 && v.end.0 == 2));

        n.input(&format!("<Esc>ggjVj{CUT}"));
        wait(&mut n, "cut", |n| n.lines == ["abc def"] && n.actions.contains(&Action::Copy("ghi\njkl\n".into())));

        n.input(&format!("A{ERASE_WORD}"));
        wait(&mut n, "a word erased", |n| n.lines == ["abc "] && n.insert_mode());
        n.input(&format!("<Esc>0{ERASE_WORD}x"));
        wait(&mut n, "nothing erased in Normal mode, and x still x", |n| n.lines == ["bc "] && n.mode == "normal");
        n.input(&format!("A{ERASE_LINE}"));
        wait(&mut n, "the line erased", |n| n.lines == [""] && n.insert_mode());

        drop(n);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Neovim's screen columns back to a line's bytes: a tab to the next
    /// stop, a wide character over two columns, an accent with its letter.
    #[test]
    fn screen_columns_are_the_lines_bytes() {
        assert_eq!(bytes_at("foo bar foo", 8, 11, 8), (8, 11));
        assert_eq!(bytes_at("\tfoo baz", 8, 11, 8), (1, 4), "past a tab of 8");
        assert_eq!(bytes_at("\tfoo baz", 4, 7, 4), (1, 4), "past a tab of 4");
        assert_eq!(bytes_at("ab\tc", 2, 4, 4), (2, 3), "a tab from column 2 fills to the stop at 4");
        assert_eq!(bytes_at("ab\tc", 4, 5, 4), (3, 4), "and c comes after it");
        // 日 and 本 are two columns each: `本x` is columns 2..5.
        assert_eq!(bytes_at("日本x", 2, 5, 8), (3, 7));
        // e and a combining acute are one column, and stay together; é made
        // as one character is one column of two bytes.
        assert_eq!(bytes_at("e\u{301}e", 0, 1, 8), (0, 3));
        assert_eq!(bytes_at("\u{e9}e", 0, 1, 8), (0, 2));
        // Past the end: a match of nothing, or of the line's end.
        assert_eq!(bytes_at("abc", 3, 4, 8), (3, 3));
    }

    /// `ext_hlstate` says which groups a highlight is made of.
    #[test]
    fn a_search_highlight_is_known_by_its_groups() {
        let info = |names: &[&str]| {
            Value::Array(
                names
                    .iter()
                    .map(|n| Value::Map(vec![("kind".into(), "ui".into()), ("ui_name".into(), (*n).into())]))
                    .collect(),
            )
        };
        assert_eq!(found_in(Some(&info(&["Search"]))), Some(Found::Match));
        assert_eq!(found_in(Some(&info(&["Normal", "CurSearch"]))), Some(Found::Current));
        assert_eq!(found_in(Some(&info(&["IncSearch"]))), Some(Found::Current));
        assert_eq!(found_in(Some(&info(&["Visual"]))), None);
        assert_eq!(found_in(Some(&Value::Array(vec![]))), None);

        let line = Value::Array(vec![
            "grid_line".into(),
            Value::Array(vec![
                2.into(),
                3.into(),
                4.into(),
                Value::Array(vec![
                    Value::Array(vec!["f".into(), 7.into()]),
                    Value::Array(vec!["o".into()]),
                    Value::Array(vec![" ".into(), 0.into(), 3.into()]),
                ]),
                false.into(),
            ]),
        ]);
        match redraw_batch(&line).as_slice() {
            [Ui::GridLine(2, 3, 4, runs)] => assert_eq!(runs, &[(7, 1), (7, 1), (0, 3)]),
            other => panic!("{other:?}"),
        }
    }

    /// A grid's rows move as Neovim scrolls them, up or down.
    #[test]
    fn a_grid_scrolls_as_neovim_says() {
        let mut g = Grid::default();
        g.resize(2, 4);
        for r in 0..4 {
            g.line(r, 0, &[(r as u64 + 1, 2)]);
        }
        g.scroll(0, 4, 0, 2, 1);
        assert_eq!(g.cells[..6], [2, 2, 3, 3, 4, 4], "up by one");
        g.scroll(0, 4, 0, 2, -2);
        assert_eq!(g.cells[4..], [2, 2, 3, 3], "down by two");
    }

    /// Search, on Neovim's own terms, as kalast now shows it: every match
    /// while `hlsearch` is on and the current one apart, none after `:noh`,
    /// and the matches of a pattern still being typed -- past a tab, which
    /// takes columns the line's bytes do not.
    #[test]
    fn a_search_shows_as_neovim_highlights_it() {
        if find("").is_none() {
            eprintln!("no nvim on this machine; skipped");
            return;
        }
        let dir = std::env::temp_dir().join(format!("kalast-nvim-search-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("t.py");
        let mut n = Neovim::spawn("", &dir, (80, 20), &["--clean"], || {}).unwrap();
        n.load("foo bar foo\n\tfoo baz\n", &file.to_string_lossy(), "python", None);
        wait(&mut n, "set up", |n| n.ready && n.loading.is_none() && n.grid.is_some());

        n.input("/foo<CR>");
        let every = [(0, 0, 3, Found::Match), (0, 8, 11, Found::Current), (1, 1, 4, Found::Match)];
        wait(&mut n, "/foo", |n| n.found == every);

        n.input(":noh<CR>");
        wait(&mut n, ":noh", |n| n.found.is_empty() && n.cmdline().is_none());

        n.input("/ba");
        wait(&mut n, "/ba, typed", |n| n.found == [(0, 4, 6, Found::Match), (1, 5, 7, Found::Current)]);
        n.input("<Esc>");
        wait(&mut n, "given up", |n| n.found.is_empty() && n.cmdline().is_none());

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
