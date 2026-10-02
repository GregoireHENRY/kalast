# 2026-10-01 — examples back at the bundle's root; backups in scripts/backup

v0.5.12 shipped its examples in `res/examples` and moved them into place at
the first start (`2026-10-01_examples_kept_through_updates.md`). The user
refused that layout -- "people who download the bundle will be lost, i dont
want examples to be in res/, you took this decision and proceeded with it
alone" -- and set it:

- the bundle's root holds `res/`, `python/`, `docs/`, `notes/`, `examples/`,
  and an empty `scripts/`;
- users write their scripts in `scripts/`; `examples/` holds the reference
  scripts, not to be edited;
- an update keeps `scripts/` and, when examples were changed or added, saves
  them in `scripts/backup`; none changed, nothing happens.

## The trade-off, and the user's choice

An update runs the *installed* version's updater. With `examples/` back in
the archive, two updaters already out there misbehave:

| updater | an archive's `examples/` |
|---|---|
| v0.5.10 and older, betas before 805d580 | replaces the user's, and deletes it at once (`clean_previous`) |
| beta 805d580 | replaces the user's, which goes to the Trash |
| beta fc57fff, v0.5.12 | skips it: the user's stays, the new one is dropped |

Nothing in a new version can stop the first: v0.5.10's code deletes before
the new version starts. Asked (one manual download for everyone, through a
renamed archive, or keep the update button), the user kept the update button:
users updating straight from v0.5.10, past v0.5.12, lose the examples they
edited or added. And updates stay in place, in the same folder.

## What does it

**The archive** (release.yml, "Assemble the rest of the bundle"): `res`,
`examples`, `docs`, `notes` at the root as before v0.5.12, an empty
`scripts/`, and `examples/.kalast-version` holding the version. The
`Archive` step checks all three. `build-dist.sh` does the same locally.

**This version's updater** (`install_unpacked`, from `install_bundle`):
`keep_changed_examples` first -- each example, a folder of `examples/` or a
file there, not exactly as some release shipped it (`res/examples-shipped.txt`,
the same list as v0.5.12's) moves whole to
`scripts/backup/before-v<new version>/`, `-2` and on if taken; the update
stops there if that fails. Then the swap, `examples` replaced with the rest,
`scripts` never. The log says which examples were kept.

**After v0.5.12's updater**, which dropped the new examples: the bundle's
executable carries its own. `build.rs` packs `examples/` (`KALASTEX`, path and
bytes per file, litter and the version file left out) into `OUT_DIR`, written
only when it changed, and sets `cfg(kalast_examples)`; `update.rs` includes it
under `all(feature = "embed", kalast_examples)`. The crate on crates.io ships
without `build.rs` -- `cargo package` warns "ignoring `package.build` entry"
-- so there it is empty, and the cfg is declared in `[lints.rust]`. Found by
`cargo package` before anything was pushed: the first version read `OUT_DIR`
unconditionally and the packaged crate did not build. 1.6 MB in the bundle's
executable; none in the wheel (`ext`, not `embed`).

`settle_examples`, at the start of a bundle's UI app (`editor_start`, as
`install_examples` was): `examples/.kalast-version` this version's, nothing
-- edits made since stay until the next update keeps them. Another's, or
none: `keep_changed_examples`, then the built-in examples, written beside in
`.previous/examples-new` and renamed into place, the old remainder aside and
removed. Also puts a deleted `examples/` back.

The precompiled Rust examples do not care: `is_current` compares the
`.source-hash` beside the library, never the example's time.

Gone: `install_examples` and the `res/examples` stage; `lsp_check.py` reads
`examples/` again.

## Tests

`update::tests`: `an_update_keeps_the_examples_a_user_changed_in_scripts_backup`,
`an_update_over_examples_as_shipped_keeps_nothing`,
`examples_an_older_update_left_are_replaced_by_kalasts_own` (and the next
start does nothing, edits included), `a_missing_examples_folder_is_put_back`,
`a_backup_never_writes_into_an_earlier_one`, `a_link_is_kept_and_not_followed`,
`a_pack_is_read_back_and_never_leaves_examples`,
`the_examples_built_in_are_the_repositorys` (byte for byte),
`an_update_never_replaces_the_scripts`. Library 302 of 302; without default
features, as CI runs them, 22 of 22 in `update::`. `cargo package` builds.

End to end in /tmp, macOS arm64:

- a fresh unpack of the new bundle: the root as asked, `scripts/` empty,
  version 0.5.13; its first start moved nothing;
- the published v0.5.12, started once, `cube/color_map.py` edited and
  `examples/mine/` added, then v0.5.12's swap of the new bundle (examples and
  scripts skipped, as its code does): the new version's first start put `cube`
  and `mine` in `scripts/backup/before-v0.5.13/`, left `scripts/test`,
  `examples/` identical to the repository's, "2 up to date, 0 built"; a second
  start moved nothing;
- the new bundle's own `kalast --update`, for real, pretending 0.5.11 so
  v0.5.12 was offered: "the 2 examples you had changed or added are in
  scripts/backup/before-v0.5.12: cube, mine", before the swap.

## Open

- Released: nothing yet; the manifests say 0.5.13, the changelog
  `## v0.5.13-beta`.
- Windows and Linux not run end to end.
- `scripts/examples-before-v0.5.12/` folders made by v0.5.12 stay where they
  are: the user's now.

## 2 October: the list frozen, out of `res/`

The user: "this has nothing to do with res/ the resources folder". Since every
bundle from v0.5.13 carries its examples in its executable, an update compares
`examples/` with exactly what that version shipped (`Shipped::new` adds the
built-in pack's fingerprints). The list is only for an `examples/` from
v0.5.12 or older, so it is frozen as v0.5.12 committed it -- every release up
to there -- in `src/app/examples-until-v0.5.12.txt`, beside `update.rs`, and
in the crate's `include`. `res/examples-shipped.txt`,
`tools/gen_examples_shipped.py` and `tests/test_examples_shipped.py` are gone:
editing an example no longer touches any list.
`update::tests::this_versions_own_examples_need_no_list`.
