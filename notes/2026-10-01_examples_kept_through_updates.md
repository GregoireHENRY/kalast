# 2026-10-01 — the examples a user changed, kept through updates

Asked: "i want to make sure the users of the current public version v0.5.10
wont loose their customs examples / for this update and next ones, if users
have custom examples in their examples folder of kalast, that differ from the
default examples, they should be moved in a backup folder in the new version
updated in scripts folder".

## Why the fix could not be in the updater

An update runs the *installed* version's `install_bundle`, and v0.5.10's
swaps every top-level entry of the new archive into the bundle, the replaced
ones into `.previous`, then calls `clean_previous` straight away: whatever
`examples/` held, edits and new examples, was deleted before the new version
ever started. v0.5.11-beta's updater is the same code (`git diff v0.5.10
v0.5.11-beta -- src/app/update.rs` is empty); the Trash for the old examples
came after it, on main only.

Nothing 0.5.11 does can change what 0.5.10's code does -- only what the
archive gives it. v0.5.10 replaces only the entries the archive holds, so the
archive no longer holds `examples`: the defaults ship as `res/examples`
(release.yml, "Assemble the rest of the bundle"), and the `Archive` step
refuses a bundle with a top-level `examples`. `res/` itself is still replaced
whole, as it always was.

## The first start puts them in place

`update::install_examples(dir, CURRENT)`, from `App::editor_start` in a
bundle -- before the kalast tab's first lines and before a script named on
the command line is read. Not in `--python-check`, `--precompile`,
`--language-server` or `--update`: those return before `editor_start`, and CI
runs the first three on the stage, which must still hold `res/examples` when
it is archived.

When `res/examples` exists:

1. Each top-level entry of `examples/` -- an example's folder, or a file such
   as `README.md` -- is *as shipped* when every file under it has a
   fingerprint `res/examples-shipped.txt` lists for its path, and every folder
   under it is one some release had. Litter does not count: `.DS_Store`,
   `Thumbs.db`, `desktop.ini`, `__pycache__`. A link is never followed and is
   the user's; so is `examples` itself when it is a link or a file.
2. Every other entry is renamed, whole, into
   `scripts/examples-before-v<version>/` (`-2`, `-3` if taken: a beta and its
   release are one version). Whole, because a script reads its siblings --
   meshes, kernels -- by relative path, and a lone edited `main.py` in
   `scripts/` would not run.
3. What is left is kalast's own: renamed to `.previous/examples`, then
   `res/examples` renamed to `examples`, then the aside removed (back in
   place if the second rename failed). An error leaves `res/examples` for the
   next start; the message says where anything already moved is.

Renames, so every file keeps its time: `is_current` compares the shipped
libraries' times with the examples', and `package_name` reads the last two
components of the path, the same under `res/examples` and `examples`.

The kalast tab: "installed the examples of v0.5.11", read, when nothing was
kept; with a dot when something was -- "the 3 examples you had changed or
added are in scripts/examples-before-v0.5.11: cube, lightcurve, mine" -- or
when it failed.

`install_bundle` now skips `examples` as well as `scripts` (`swap_in`,
tested), and the Trash for the old examples is gone: they are never replaced
by the swap.

## The list of what kalast shipped

`res/examples-shipped.txt`, compiled in: `<fingerprint> <path under
examples/>`, 223 lines, 12 KB. `tools/gen_examples_shipped.py` writes it from
every `v*` tag and the working tree (tracked and untracked-not-ignored, what
`build-dist` copies), `examples/old/` included since bundles ship it, and
never drops a line: a moved beta tag's files stay known.
`tests/test_examples_shipped.py` fails when a tag's or the tree's file is
missing -- run the tool after changing an example.

The fingerprint is FNV-1a, 64 bits, of the file with each `\r\n` read as
`\n`: the Windows runner's checkout writes CRLF into the bundle, the tags hold
LF. No hashing crate needed; `example_fingerprint` and the tool agree on
FNV's published vectors and on line endings, tested on both sides.

## Tests

`update::tests`: `the_fingerprint_is_the_tools`,
`the_shipped_examples_are_compiled_in`,
`a_new_bundles_examples_are_put_in_place`,
`the_examples_a_user_changed_are_kept_in_scripts` (CRLF, litter, an edited
file, an added file, a new example, an edited README),
`examples_as_shipped_are_replaced_quietly`,
`a_folder_kept_before_is_not_written_into`, `the_examples_keep_their_times`,
`a_link_is_kept_and_not_followed`,
`an_update_never_replaces_the_examples_or_the_scripts`. Library: 300 of 300.

End to end, in /tmp: the published v0.5.10 macOS bundle, `cube/color_map.py`
edited, `lightcurve/out.csv` and `examples/mine/` added, a `__pycache__` in
`two_spheres`, `scripts/test/main.py`. The new-layout bundle swapped in as
v0.5.10's `install_bundle` does it, `.previous` removed: `examples/` untouched,
edits and all. The first start: `cube`, `lightcurve` and `mine` whole in
`scripts/examples-before-v0.5.11/`, `two_spheres` replaced, `scripts/test`
intact, `examples/` identical to the repository's, a script named on the
command line seeing the new ones, and `--precompile` "2 up to date, 0 built".
A second start moved nothing; a fresh unpack got its examples and no
`scripts/`.

`tools/lsp_check.py` starts ty in `examples/`, or `res/examples` in a bundle
not yet started, as CI's is.

## Open

- Windows and Linux not run end to end; the code is the same renames, and
  the CRLF case is a unit test.
- The real v0.5.10 → v0.5.11 path, through v0.5.10's own download, can only
  run once v0.5.11 is released; the beta before it goes through the same
  code with the beta's updater.
