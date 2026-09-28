# 2026-09-28 — the Neovim config kalast ships

**Asked for.** The user asked for his Neovim config to be shipped as the
default, with a way to point to one's own. Then he asked for it updated to
recent tools first: "my neovim config is probably really old".

## Updating the author's config

His config is at `~/config/nvim`, since `XDG_CONFIG_HOME=~/config`. It was
not under git, so it was backed up to `~/config/nvim.backup-2026-09-28`.
Neovim is 0.12.5. It was a lazy.nvim setup of 17 plugins, already partly
current: blink.cmp, `vim.lsp.config`, snacks, conform.

What changed:

- **Treesitter.** nvim-treesitter moved from `master`, which is frozen, to
  `main`, the rewrite. `main` installs parsers and queries, and Neovim itself
  highlights and indents (`vim.treesitter.start` in a `FileType` autocmd).
  Its build needs the tree-sitter CLI, which Homebrew has here. The old
  `<CR>`/`<BS>` incremental selection maps to Neovim 0.12's own `an`/`in`.
  All 29 languages installed in 5 s.
- **Python.** ty and ruff, ty installed through mason. basedpyright is kept
  only where ty is not installed. conform runs `ruff_organize_imports` then
  `ruff_format`, and taplo for TOML.
- **Pickers.** snacks.nvim replaces telescope and its `make`-built fzf
  extension, on the same `<leader>f…` keys. snacks' indent guides replace
  indent-blankline.
- **Commenting.** Comment.nvim is gone; `gc` has been Neovim's own since
  0.10.
- **Versions.** gitsigns' `next_hunk`/`prev_hunk` became `nav_hunk`.
  rustaceanvim went from `^6` to `^9`, and blink.cmp to `1.*`.
- **Inside kalast.** The config became kalast-aware: `defaults.cond` is
  `not vim.g.kalast`, and only gitsigns, nvim-surround and nvim-autopairs are
  `cond = true`. kalast's editor draws the text itself and brings its own
  language server, so the other plugins' windows, colours, statusline and
  completion menu are never seen there, and blink.cmp's unseen menu would
  still have taken `<Tab>` and `<CR>`. lazy's `cond` does not install what it
  excludes. The update checker is off in kalast too.

Checked headless, on `examples/crater_self_shadow/main.py`:

- **Plain Neovim:** ty and ruff attached, treesitter highlighted, 15 of 21
  plugins loaded, no messages.
- **With `g:kalast`:** no clients and three plugins, no messages.

## Shipping it

- `res/neovim/` holds a copy with the one personal line made portable: the
  undo folder is `stdpath('state')/undo`, not `/Users/gregoireh/neovim/undodir`.
- It is compiled in (`nvim::KALAST_CONFIG`) and is in the crate's `include`.
  Every door has it: the bundle, `pip install`, `cargo run`.
- `app.config.neovim_config` is remembered, and takes:
  - `"kalast"` (the default): the copy is written beside the app's settings,
    in `neovim/kalast-nvim`, and rewritten where it differs.
    `XDG_CONFIG_HOME` and `NVIM_APPNAME=kalast-nvim` point Neovim at it, so
    its plugins and state go to `~/.local/share/kalast-nvim`, apart from the
    user's own Neovim's.
  - `"user"`: nothing is given, and Neovim finds the user's own.
  - a path: a folder is read the same way as kalast's; a file is read with
    `-u`.
- A failed start is remembered per `neovim_path` and `neovim_config`, so
  changing either tries again.

A fresh start of the copy took 9 s with scratch data folders: lazy.nvim
bootstrapped itself and installed its three plugins, 9.2 MB in all. The
tests `kalast_config_is_written_out_as_shipped` and
`neovim_config_names_how_neovim_finds_it` cover the writing and the choice.

The copy in `res/neovim` does not follow the author's own config by itself.
To ship a later change, copy it again and keep the undo line portable.
