# 2026-09-28 — Neovim in the editor: keys as typed, not clicks

Reported: "when i press space at the end of a line it goes to the next line
instead of adding a space", and asked for Neovim in the editor to behave as
in a terminal or in VS Code. The same cause was behind the fast typing lost
earlier the same day: `Go# edited in Neovim` came out as `# edited in im`,
with E35.

## Space and Enter were clicks

egui counts Space and Enter on the focused widget as a click on it:
`Response::clicked()` includes `FAKE_PRIMARY_CLICKED` (egui 0.36.1,
`context.rs`, for buttons reached by Tab). The Neovim view is one focused
widget, and `vim_view` sent Neovim a press and a release at the pointer
whenever `clicked()` was true. So each space went in -- unseen at the end of
a line -- and a click then moved the cursor to the line under the pointer.
Two spaces within `mousetime` on one cell were a double click, a word
selected in Visual mode, and the letters after it commands: `N` searched
backwards with no pattern, E35, and an error flushes what is queued.

The editor now takes the mouse's clicks only,
`clicked_by(PointerButton::Primary)`: the tap in `vim_view`, its Ctrl+click,
and the plain editor's Ctrl+click, where Cmd+Enter jumped to a definition.

## The rest of the keyboard

Replayed on `nvim --clean` through `nvim_input`, as kalast sends keys, in
Insert mode at the end of `abc def`:

| sent | result |
|---|---|
| a space, then a click on the next line | the space at the end, the cursor on the next line |
| that twice within half a second | a double click: a word selected, the next key applied to it |
| `<M-{>`, what Option+5 (`{` on a French Mac) became | Insert mode left, `{` run as a motion |
| `<C-M-{>` | the same |
| `<M-n>`, a dead key's Option+N (`~`) | Insert mode left, `n` run |
| `<C-c>`, what Cmd+C became | Insert mode left, nothing copied |
| `<M-Left>`, `<M-BS>` | Insert mode left |
| `<D-z>`, a Cmd key nothing maps | `<D-z>` typed out |
| `<D-c>` in Visual mode, unmapped | `c`: the selection deleted |

Neovim reads an Alt key nothing maps in Insert mode as Escape and the key,
and a Cmd key nothing maps as its text, or as the plain key. So:

- On macOS, Option types what the system made of the key -- the text event
  egui has with it -- and never `<M-x>`. A dead key types nothing until the
  input method has its letter. Elsewhere Alt stays Neovim's `<M-x>`, the
  text it comes with dropped, as a terminal sends it.
- Command is the system's, as in a terminal, apart from VS Code's editing
  keys: Cmd with the arrows goes to `<Home>`, `<End>`, `<C-Home>`,
  `<C-End>`; Option with the arrows to `<C-Left>`, `<C-Right>`; Cmd+A is
  `<C-\><C-n>ggVG`. Those mean the same in every mode. Nothing is sent as
  `<D-x>`.
- The keys whose meaning depends on the mode -- Cmd+C, Cmd+X,
  Option+Backspace, Cmd+Backspace -- go as `<Cmd>lua kalast_keys.…<CR>`, to
  functions SETUP defines. They act in the mode Neovim is in when it reads
  them, not the one kalast last saw: `<C-w>` erases a word in Insert mode
  and begins a window command in Normal mode, where it would eat the next
  key. `nvim_feedkeys(…, 'in')` keeps their keys in order with anything
  typed after. Nothing in angle brackets goes inside the call:
  `nvim_input` reads `<C-w>` as the key even there.
- Cmd+C and Cmd+X send the selection's text (`getregion`) to kalast, which
  puts it on the clipboard itself: no pbcopy, win32yank or xclip is needed.
  A copy keeps the selection, as in VS Code.
- Windows' AltGr needed nothing: winit 0.30.13 clears Ctrl and Alt for the
  right Alt on a layout that has AltGr (`filter_out_altgr`), so its
  characters arrive as text. The left Ctrl and Alt together, as AltGr,
  still give `<C-M-x>`.

The translation is `nvim::typed`, a function of egui's events and the
platform, so both platforms' rules are tested on either.

## The input method, while Neovim takes text

A dead key composes only through the input method, and egui turns it on only
for a frame that asks, as its text fields do. The Neovim view asks while
Neovim takes text -- Insert and Replace modes, the command line -- and draws
the composition underlined at the cursor. Not in Normal mode: with it on,
macOS offers a held key's accents instead of repeating it, and `l` held
would stop.

That showed a stale mode. The selection autocmd told kalast the mode in full
(`mode()`) but never that Normal mode was back after Insert mode, so
`insert_mode()` stayed true: the input method stayed on, and on Windows and
Linux Ctrl+V pasted instead of beginning a Visual block. Every change of mode
is now said.

## Tests

- `keys_reach_neovim_as_a_terminal_or_vs_code_sends_them`: what each key
  becomes, on macOS and elsewhere, from the events egui has for it.
- `macos_editing_keys_act_in_the_mode_neovim_is_in`, on `nvim --clean`: a
  copy keeps the selection, a cut removes it, Cmd+A from Insert mode, and
  Option+Backspace erases a word in Insert mode and nothing in Normal mode,
  where the `x` after it is still `x`.
- `space_and_enter_are_typed_not_clicked`: the editor itself, run through
  egui with `nvim --clean`, the pointer resting eight lines down. Before the
  fix it failed with the cursor at line 9, the pointer's, after a space typed
  on line 0. It also checks that the input method is on in Insert mode and
  off after Escape, and that a dead key's accent and letter type `ê`, once.

Not done: Ctrl+V with an empty clipboard on Windows and Linux (egui-winit
swallows the paste shortcut when there is nothing to paste, so no Visual
block then); Neovim's right-click menu and the middle button; a click on the
line numbers; Cmd+Z.
