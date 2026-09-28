# 2026-09-28 — a language server in the bundle: ty, after jedi and basedpyright

**Two reports.** A colleague's fresh v0.5.10 bundle found no Python language
server at all. The editor said to run `uv tool install basedpyright`, and he
asked whether kalast could not do it. On the user's Mac, in a bundle,
basedpyright "started" and exited 70 ms later.

## The exit: `PYTHONHOME` inherited

A bundle sets `PYTHONHOME` for the interpreter linked into kalast
(`point_at_the_bundled_interpreter`), and every child inherited it. The
basedpyright found was Neovim's mason copy, a venv script. Its Python loaded
the bundle's stripped standard library and died on `No module named
'_posixsubprocess'`. Without the variable it starts.

`app::without_bundled_python_home` removes the variable from the commands of
language servers and Neovim, and only when it is the bundle's own value.
Neovim needed it as much: everything it runs in Python broke the same way. A
script's own `subprocess` of another Python still inherits it.

## None installed: ship one

Installing one for the user needs uv, a network and the user's consent. So
the bundle ships one, ahead of anything installed, so that the editor is the
same on every machine. Three were tried, each on kalast's scripts.

1. **jedi-language-server.** Pure Python, 36 MB, run by kalast's own
   interpreter through `kalast --language-server`. jedi works in-process
   when `sys.executable` is not a `python`. Beside basedpyright the user
   found it "way more worse": "the coloring, all the infos". Hovers were
   bare and there was no type checking.
2. **basedpyright.** 1.40.1, with Node from `nodejs-wheel-binaries`. Source
   maps and npm were pruned to leave 45 + 119 MB, 43 MB gzipped. It was
   excellent in the editor.
3. **ty.** 0.0.84, Astral's, a single executable. Measured against
   basedpyright on the same protocol session in the repository
   (`/tmp/kalast-ty/compare.py`, macOS arm64):

| | basedpyright + Node | ty |
|---|---|---|
| initialize | 128 ms | 11 ms |
| completion after `app.simulation.`, first / again | 861 / 205 ms | 82 / 1 ms |
| hover, signature help | up to 676 ms, 206 ms | 1 ms, 2 ms |
| memory after the session | 669 MB | 62 MB |
| download, unpacked | +43 MB, +164 MB | +13 MB, +25 MB |
| `examples/didymos/main.py` | 2 type errors, 2 unused imports | nothing |

In kalast's editor, typing `app.step()`, the two were on a par: the same 58
completions, the same hover and signature.

- **Completion list:** ty shows each name's type in the list. basedpyright
  starts with dunder names.
- **Hover:** basedpyright leaves `**` literal in docstrings. ty renders them,
  but keeps the docstring's own line breaks.

The user chose ty. basedpyright catches more type errors and is one setting
away: `app.config.python_language_server`.

## How ty is wired

- `tools/bundle-requirements.txt` installs `ty`. pip puts its executable in
  `python/bin` (`Scripts` on Windows), and the release keeps that one file
  when it prunes the folder.
- `app::bundled_language_server` finds it. `lsp::find` takes it first in a
  bundle, and `kalast --language-server` runs it for the check.
- ty reads its settings under `ty` through `workspace/configuration`.
  `configuration.environment.python` is the interpreter scripts run with, or
  in a bundle its `python` folder, which ty reads as an installation. All
  three environment variants gave the 58 completions with `bodies` and
  resolved numpy, with nothing else on PATH: that folder, `extra-paths` to
  its site-packages, and both.
- ty's `initializationOptions` are its own (log level, log file), so it is
  sent none. The pyright settings it was handed at first came back as a JSON
  dump in the kalast tab.
- `pip install "kalast[editor]"` brings ty for `python -m kalast`, found
  beside the interpreter. `ty` heads the list of candidates.
- The completion list kept `labelDetails.detail` and `.description` in one
  string drawn against the name, so ty's `step` read as `stepbound method
  App.step() -> bool`. The description is now its own field, drawn after a
  gap.

`tools/lsp_check.py` drives a bundle's server through the protocol. It
initializes, opens a script ending in `app.simulation.`, and expects
`bodies`. It declares the `workspace/configuration` capability: without it a
server never asks for its settings, and basedpyright answered an empty list.
The release workflow runs it on every bundle after pruning, with the
runner's own Python ("The bundle's language server answers"). The local
build runs it too.
