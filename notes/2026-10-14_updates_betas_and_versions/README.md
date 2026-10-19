# Updates: betas on request, any version, going back

14 October 2026. Asked: "an option to kalast update to accept beta update",
"an option to revert to previous version", "or to directly select a
version".

## What

- `app.config.update_betas` (app tab, Updates; remembered in
  `settings.toml`): the newest pre-release is offered too, when its version is
  newer than this copy's and than the newest release's -- the beta of a version
  already released is that release's past. Ticking it asks GitHub again. A
  beta bundle was, and still is, offered newer betas of its own version
  without it (`parse_for`).
- The Updates section lists every version GitHub has once the check has
  answered -- or "check now" asks -- newest first, betas among them, each
  release's notes on hover, "(this one)" for this copy's tag; a button installs
  the one chosen, "install" for a newer one or a beta, "go back to" for an
  older one. It goes through `install_release`, the same path as the
  toolbar's update: the archive for this machine, the examples you changed
  kept in `scripts/backup/before-v<version>/`, `.previous`, the toolbar's
  restart. A release with no archive for this machine cannot be chosen; pip
  has the releases alone, not the betas (refused, said).
- Terminal: `kalast --update --beta`, `kalast --update 0.5.12` (or
  `--update=v0.5.12`, or a beta's `0.5.14-beta`), `kalast --versions`. A word
  after `--update` is a version only if it starts with a digit, past its `v`:
  a script's path is not taken for one.

## How

`update::check(current, betas)` asks for 100 releases a page (was 20), so the
list reaches back to v0.5.0. `Update.releases` holds them all, newest first,
behind an `Arc`: the state is copied for the GUI every frame. `State::UpToDate`
keeps its check's answer for the list. The app's `serve_update` takes the
toolbar's update (the newest), the list's install (a tag, found in the
releases) and a check asked for (`spawn_update_check`).

## Checked

Unit tests: betas taken offer a newer version's beta to any build, not the
beta of this very version nor a released version's; the versions listed and
found as typed, the beta not the release; pip refuses a beta. The existing
beta-bundle cases unchanged. From the terminal, the dev build claiming 0.5.13
(`KALAST_UPDATE_PRETEND`): `--versions` lists v0.5.14-beta to v0.5.0, every
one with a macOS arm64 archive; `--update` says 0.5.13 is the latest release,
`--update --beta` offers v0.5.14 beta; `--update 0.5.9` goes to install it --
refused, a source build; `--update 0.4.99`, no such release.

## Open

- Not run end to end from a bundle: installing an older version over a newer
  one is the same swap as an update, but a downgrade across the examples'
  changes (v0.5.12, when bundles began carrying their own) is untried.
