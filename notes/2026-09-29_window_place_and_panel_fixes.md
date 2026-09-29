# 2026-09-29 — the window where it was left; names whole; READMEs as pages; pandas in the bundle

Four requests, in the order they came.

## The UI app opens where it was closed

Asked for: "kalast app should remember which screen it was last closed and
window size, else it should also be forced as an option".

- `settings::Place` -- the screen, by name and by its origin on the desktop
  (two screens of one model share a name), the window's corner from the
  screen's, its size, and whether it was maximised -- is kept in
  `settings.toml` beside what the app already remembered (`window_*` keys).
  `save_place` writes it as the app closes (`App::exit`), keeping the rest of
  the file; `save`, from the app tab, keeps the place.
- Only the UI app remembers -- started by `editor_start`, the executable and
  `python -m kalast` both. A script's window, editor layout or not, neither
  opens where the app was left nor saves anything, as a script's settings
  never are.
- Fullscreen or maximised, the window's frame is the screen's: the place it
  had before on that screen is kept, the screen itself updated.
- `place::opening`, on plain numbers so it tests without a screen: the screen
  `app.config.monitor` names -- a number from 1, or part of a name -- else the
  one the app was left on, else the main one. The size `width` and `height`
  give, else the one left (kept inside a screen that may have shrunk), else
  85 % of the screen. The place left applies on its own screen only, clamped
  onto it; otherwise the window's in the screen's middle.
- `app.config.monitor`, live: set with the window open, it moves there,
  leaving fullscreen or maximised and coming back to it. No widget: typed into
  the panel, the window would jump at every letter to whichever screen the
  letters so far matched.

What winit 0.30.13 does on macOS, and had to be worked around:

- A new window takes its position for its *content's* corner
  (`initWithContentRect`), where `outer_position` and `set_outer_position`
  mean the frame's. Restored as read, the window crept up by a title bar each
  time the app opened. The frame's corner is now set again once the window
  exists, except for a maximised one, which that would un-maximise.
- A new window's position and size are converted from pixels at the *main*
  screen's scale. On a laptop's 2x screen beside a 1x monitor, a window meant
  for the monitor landed on the laptop, at half its size. On macOS kalast now
  hands winit points, at the target screen's own scale: a screen's origin in
  winit's pixels is its corner in points times its own scale.
- A screen's `name()` is `Monitor #` and the model's number. `macos::screen_name`
  asks `NSScreen` for the name System Settings shows ("Built-in Retina
  Display", "PHL 279P1"), matched by the `NSScreenNumber` in its description
  to winit's `native_id`. Elsewhere winit's names stand: `DP-1` on Linux,
  `\\.\DISPLAY1` on Windows, where a number is the better way to name one.

Checked on this Mac, its built-in screen and a Philips beside it, each run a
UI app opened in the background by a script that closed it at once, against
a scratch `KALAST_SETTINGS`: the place written with no file; the old
`Monitor #41038` entry found by its origin and written back under its name,
with no creep; a place written by hand read back unchanged, twice; `monitor =
"DELL"` said no such screen and listed the two; `monitor = "PHL"` opened on
the Philips, centred at 85 % (3264 × 1836 at 288, 162 on its 3840 × 2160), and
the next runs, nothing forced, opened there again.

### Remember last window, or not

Asked for next: an option "remember last window" -- position, size and
fullscreen -- and, unticked, options to fix them for every start.

- `app.config.remember_window`, on by default and remembered. Off, the
  window opens where `monitor`, `window_x`, `window_y` (new; `-1` centres),
  `width`, `height` and `start_fullscreen` (new) say, all remembered as set
  in the app tab. `start_fullscreen` is its own field: `F` and the green
  button change `fullscreen`, the window's, and would otherwise have changed
  the start too.
- `Remembered::apply` gives the config the six only while unticked;
  ticked, it leaves them at their defaults so that nothing forces the
  remembered place, and a script's own still wins over both.
- The six are hand-written in the app tab under the generated Window rows
  (`gui::window_start`), shown only while unticked -- the generator has no
  "shown when" -- the screen a list of those connected. Unticking fills them
  from the window as it stands (`App::realise`), which is where one wants to
  start from.
- `window_x` and `window_y` move the window live, as `width` and `height`
  resize it and `monitor` moves it.
- The place's keys in `settings.toml` became `left_*`, `window_x` now being
  a setting; nothing had shipped with the old ones.

Checked with the box unticked in a scratch settings file, `monitor = "PHL"`,
`(100, 80)`, `1500 x 900`: the window list put the frame at (-1870, 40)
points, 750 x 478 -- the Philips' origin at -1920 points plus 50 and 40, and
1500 x 900 pixels under a 28-point title bar at the Philips' 2x. (Both
screens here are 2x, so the per-screen scale conversion is read off winit's
source rather than seen on a mixed pair.)

## A setting's name whole where there is room

Reported with a screenshot: `panels_f…`, `toolbar_…`, `simulati…` cut short
in a panel with room to spare, and the toolbar text's field running past the
panel's edge.

`widgets::setting` gave the names a fixed share of the row, 38 % clamped to
72..170 points. A section's names now share a column as wide as the widest
of them (`names_column`), cut only to leave the control 48 points
(`name_width`). 120 at first -- a slider's room -- still cut `simulation_folded`
beside a checkbox in a side panel of 235 points; a slider makes do with less. The widths are measured as the rows are drawn and used from
the next pass; a pass that finds the column wider asks egui to draw the
frame again (`request_discard`), so no frame is shown askew. A text field
took egui's own 280 points whatever the row; it now takes the row.

## A README opens in the documentation tab

Asked for: a README opened in the files tab goes to the documentation, and
is added to its pages for the run.

- The files tab counts Markdown among what it opens -- no longer dimmed --
  and a click on one goes to `Docs::open`, not the editor: read from the
  disk, parsed, its pictures loaded from beside it, and listed after the
  tab's own pages until kalast closes. Opened again, it is read again, not
  listed twice. A path that is one of the tab's own pages -- the README at
  the root -- shows that page, as this build has it.
- A link from a page to Markdown on this disk opens it the same way; it used
  to go to GitHub.

## A folder named `mesh`

Reported: the examples' `mesh` folder had the database folder's icon. That
was kalast's own mapping (`KALAST_FOLDERS` in `tools/gen_icons.py`), not
Catppuccin's; `mesh` and `meshes` are plain folders now. The icons were
fetched again and did not change.

## pandas in the bundle

Reported: `landmark_tracking/main.py` stopped on `No module named 'pandas'`
in the editor, though pandas is in `pyproject.toml`. That lists what a
developer installs; a bundle carries `tools/bundle-requirements.txt`, which
had missed pandas although the rule written there is "what the shipped
examples call". Added: pandas 3 requires numpy and python-dateutil only, not
pyarrow, which stays out on purpose. A scan of the shipped examples' imports
against the bundle finds three more absent: astropy and cosmoscripting for
the Hera scripts, which need the mission's data or Cosmographia anyway, and
pymeshlab for `mesh/decimate.py`. Left out.

## Tests

`place::tests` (the screen by number and name, where it was left, a place
off a shrunk screen, the config winning), `settings::tests` (the place read
back, and the settings and the place each keeping the other),
`widgets::tests` (one column the width of the widest name, drawn twice only
the first time; a name cut only for its control), and
`docs::tests::a_markdown_file_opened_becomes_a_page`.
