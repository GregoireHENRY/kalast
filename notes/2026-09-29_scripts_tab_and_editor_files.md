# 2026-09-29 — the scripts tab, the editor's files, and render as its own act

A run of requests, taken as one design:

- right clicks in the explorer: a new example, a new file, rename, delete --
  to the Trash, not `rm -r`;
- any text file opened in the editor; a README into the documentation only
  by "Open as documentation";
- the editor keeping a list of its files, as the documentation keeps its
  pages, and an outline of the script, as VS Code has one;
- a render button, so that opening a file never changes what is rendered;
  compile, debug or release, beside it for Rust;
- then: the explorer as **scripts**, two folders -- `examples`, the bundle's,
  and `scripts`, the user's -- an update replacing the first and keeping the
  second; and a button to open any file from anywhere.

## The scripts tab

`FileTree` shows two roots, `examples` and `scripts`, beside the executable
in a release bundle -- wherever it was started from -- and in the working
directory otherwise (`scripts_base`). `scripts` is shown before it exists and
made by the first New folder or New file in it.

A row returns a `TreeAction`: Open, Docs, Render, Trash, Created, Renamed,
served after the frame. The menu (`FileTree::menu`) is by row: `examples`
takes New example; `scripts` New folder and New file; a folder in either New
file and Delete; a file Rename and Delete, Send to renderer for a script or a
mesh, Open as documentation for Markdown. A name is typed in a row of its
own, VS Code's way (`Naming`): Enter makes it, Escape gives up, a click away
gives up an empty or refused one; a name taken or not a name is said on the
field, kept for another try. Delete asks, then `trash::delete`: Finder's
Trash, the Recycle Bin, the freedesktop trash (the `trash` crate, without
`chrono`; its objc2 is kalast's).

The side panel's folder button, right of the tabs, opens the system's
dialog (`rfd`) for any file: into the editor, or -- a mesh too big to edit --
to the scene.

## The editor's files

`Buffers` -- a view on the `Editor`'s `script`, `script_path`, `script_dirty`
and on `opened` and `stash` -- does what opening, switching, closing and
renaming do, testable without a window. A file opened is read if it is text
(`read_text`: no NUL in its first 8 KiB, UTF-8, at most 4 MiB), and the one
it replaces goes to the stash with its edits; switching back takes it out.
Closing an edited one asks: save and close, close without saving, cancel.
Quitting asks over every edited file, "Save all and quit" writing the others
first. The untitled buffer is listed too, and "Save as" renames it.

The column on the editor's left, `editor_nav`, is the documentation's: the
files open -- the shown one lit, an edited one dotted, the renderer's marked
with the render icon, a cross under the pointer -- and the outline.

## The outline

`script::outline`, read off the text rather than asked of the language
server, so that it is there at once and without one. Python: `class`, `def`,
`async def`, nested by indentation, and the names a module or a class body
assigns (constants by their capitals), never a function's; strings and
continuation lines stepped over, so a `def` in a docstring is not one. Rust:
items nested by braces, a `fn` in an `impl` or a `trait` a method, strings,
chars and comments stripped. Markdown: headings outside fenced code. A click
goes to the line: `ScriptEditor::go_to_line`, through Neovim or in the
`TextEdit`.

## Render as its own act

Opening used to render: a `.py` was built to iteration 0, a `.rs` loaded, a
`.obj` shown. Now `Editor::rendering` is the script sent to the renderer --
by the editor's render button (top right), the tree's Send to renderer, the
command line, `app.set_script` -- and Play, Restart and Step act on it
(`render_source`: its text as the editor has it, saved or not, else the
file), whatever the editor shows. The toolbar names it. The Rust status
(`rust_key`, `rust_built`) follows it, keyed on a hash of its text so an edit
in a file set aside counts; Compile builds the file shown. A mesh sent to the
renderer is `mesh_request`, and leaves nothing to play.
`app.open_script()` keeps its word -- read again and shown -- through the
same send.

Compile moved beside render: a bug or a rocket for the profile, the chevron
to choose, both greyed but for a Rust file.

Rendering again starts clean (asked next): `send_to_renderer` raises
`reset_before_render`, which the app takes with the run as the toolbar's
Reset -- a new app's settings and camera, which Restart keeps on purpose --
before the run, between frames. A mesh sent waits for that reset too
(`Shared::mesh_requested`, shown by `show_sent_mesh`): loaded in the frame as
before, the reset after it would have cleared it.

Folders rename too (asked next): the folder's menu has Rename; the files open
from under it, and the renderer's script, follow it (`moved`), and the folders
shown open in it stay open.

## Updates

`update::install_bundle` swapped every entry of the new bundle in and the old
ones out to `.previous`, then removed it: edits to `examples` were lost for
good, and anything the archive did not carry, kept. It now never touches
`scripts`, whatever a release ships, and sends the old `examples` to the
Trash. (Replaced on 1 October: an archive no longer carries `examples`, and
the examples a user changed are kept in `scripts/`; see
`2026-10-01_examples_kept_through_updates.md`.)

## Tests

`scripts_tests`: `switching_files_keeps_their_edits`,
`names_typed_in_the_tree_make_what_they_say`, `only_text_is_read_for_the_editor`;
`outline::tests` for Python, Rust and Markdown. The Rust library: 243 of 243.

Not done: the editor keeps no cursor per file -- a file switched to opens at
its top -- and Neovim's undo history is the buffer's, so it goes on a switch.
