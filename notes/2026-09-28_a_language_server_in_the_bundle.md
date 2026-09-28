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

## No errors reported in a bundle under `dist/`: ty's project

Reported: "i dont see ty identifying live the python errors i type", in
`examples/cube/light.py` of the local bundle, which sits in the repository's
`dist/`. Completions worked.

ty checks only the files of its project. It finds the project by walking up
from the workspace, the script's folder, to the nearest `ty.toml` or
`pyproject.toml`. For the local bundle that was the repository's, and a
project leaves out what `.gitignore` ignores and ty's default excludes, among
them `**/dist/`. The script was outside the project: ty still answered
completions and hover for it, and published an empty list of diagnostics for
every version. Nothing on kalast's side: the editor sends the text every
frame and draws what is published, in Neovim mode too. Replayed through the
protocol, an undefined name and an unknown attribute appended in a
`didChange`:

| script | project ty found | indexed | errors |
|---|---|---|---|
| `examples/cube/light.py`, in the repository | the repository | 246 | both |
| the bundle's copy, in `dist/` | the repository | 246, not the copy | none |
| a copy in a folder of its own | that folder | 1 | both |
| a copy under `dist/` of a project with no git | that project | 1, not the copy | none |
| the bundle under a project's `dist/`, `ty.toml` at its root | the bundle | 2633 | both |
| the same, the `ty.toml` excluding `python` | the bundle | 153 | both |

The fourth line shows it is not git alone: any bundle unpacked in another
project's `dist/`, or in a folder that project's git ignores, was silent the
same way. The bundle's ty and mason's behaved the same throughout. (A copy
under `/tmp` looked silent too, for another reason: `/tmp` is a link to
`/private/tmp`, and the probe's URI named the link.)

The bundle now carries `tools/bundle-ty.toml` as `ty.toml` at its root: a
project of its own wherever it is unpacked. It excludes the bundle's
`python/`, whose 2480 files are kalast's environment, still read through
`environment.python`, not scripts to check. A user's own script in a folder
their project excludes stays unchecked. That is ty's rule in any editor, and
`docs/CONFIG.md` says so.

`tools/lsp_check.py` had missed it: its probe was in a temporary folder, a
project of its own, and it asked only for completions, which work on
excluded files. It now starts the server as the editor does for a script
among the bundle's examples, with that folder as working directory and
workspace. It opens a script there, in memory only, and expects both
`not_defined_anywhere` reported and `bodies` completed. In CI the bundle is
in the checkout's `dist/`, the case reported. Run on a copy with no
`ty.toml`, the check fails ("Indexed 0 file(s)", nothing published); with
it, it passes.
