# 2026-09-29 — Neovim in the editor: a search's matches highlighted

Reported: "neovim search mode `/` doesnt highlight matches".

kalast draws the script itself, from the buffer's lines, and dropped
Neovim's screen (`grid_line`) unread. Whatever Neovim highlights only on its
screen -- a search's matches, among others -- never reached the view.

## From Neovim's screen, as vscode-neovim does it

Computing the matches again in Lua was the other way, and would have meant
redoing Neovim's rules: `smartcase`, `\v`, offsets like `/foo/e`, which match
is current, `incsearch` while the pattern is typed, `:noh`, which fires no
autocmd, and `n` bringing them back. vscode-neovim reads the grid instead,
and so does kalast now:

- The UI attaches with `ext_hlstate`, so each `hl_attr_define` lists the
  groups a highlight is made of. A match on a keyword is `Search` over the
  keyword's colour, a highlight of its own, known by its `ui_name`:
  `Search` for every match, `CurSearch` or `IncSearch` for the current one.
- `nvim::Grid` keeps each grid's cells by highlight, and nothing else:
  `grid_resize`, `grid_line` (runs of `[text, hl, repeat]`, the highlight
  left out when it repeats), `grid_scroll`, `grid_clear`, `grid_destroy`.
- At each `flush`, the script window's grid becomes spans of the buffer's
  lines, `Neovim::found`: row `r` is line `topline + r` -- the window is 500
  columns wide and nothing is folded in kalast -- and the first `textoff`
  columns are the line numbers. Screen columns become bytes as Neovim lays
  them out: a tab to the next multiple of `tabstop` (now from LOAD and the
  `options` notification), a wide character over two columns
  (`unicode-width`, already built as a dependency of another), an accent
  with its letter.
- The view fills them behind the text, VS Code's find colours in the Dark
  theme and Catppuccin's yellow and peach in Mocha.

`incsearch` while typing, `hlsearch` after Enter, the current match apart,
`:noh`, `n` bringing them back, `*`: all Neovim's, as its screen has them.

Not done: other highlights on Neovim's screen alone -- `MatchParen`,
`:s`'s live preview with `inccommand`, whose text is not the buffer's either.
A folded or wrapped line would shift the rows under it; kalast shows neither.

## Tests

- `a_search_shows_as_neovim_highlights_it`, on `nvim --clean`: `/foo<CR>`
  gives every match with the current one apart, one of them past a tab;
  `:noh` clears them; `/ba` highlights while still typed; Escape gives up.
- `screen_columns_are_the_lines_bytes`, `a_search_highlight_is_known_by_its_groups`
  and `a_grid_scrolls_as_neovim_says`, on the parts.
- `space_and_enter_are_typed_not_clicked` now also searches through the
  editor itself, egui and all, and finds the three matches in the view.
