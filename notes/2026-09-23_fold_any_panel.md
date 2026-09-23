# Every panel folds, and one at a time

Follow-up to the toolbar-height fix in `2026-09-23_fullscreen_second_screen.md`.
The user asked for three things: the toolbar should fold with `N`, it should
be foldable with the mouse, and there should be a command per panel.

## The toolbar as a collapsible panel

The docked toolbar was `Panel::top("toolbar").show(..)`: not resizable, no
handle, always there, excluded from `toggle_panels` and from the config fold
(`docked_open[1..]`). It is now shown with `show_collapsible` on
`docked_open[0]`, and `resizable(true)` -- which is what gives an egui panel
its handle and with it drag-to-collapse, double-click-to-toggle, and the
invisible-until-hovered handle a folded panel keeps at its edge.

`resizable` does not make it taller. An egui panel is the size of its frame,
and the frame is the size of its content: the log, script and config panels
follow a drag because a scroll area or a text edit fills whatever it is
given, and one row of buttons does not. Dragging the toolbar's lower edge
down therefore changes nothing; dragging it up past egui's 20-point minimum
folds it. The minimum was left at the default: the panel cannot shrink below
its row anyway, so the drag has no intermediate state to show, and a lower
threshold only means a slightly longer pull before it folds.

The three places that iterated `docked_open[1..]` -- `toggle_panels`, the
config-driven fold and the read-back into `panels_folded` -- iterate all
four now, so `panels_folded` means all four and reads `True` only then.

## Arrow keys

`Editor::toggle_panel(side)` flips one entry, and `window_event` binds the
arrows by edge: up the toolbar, down the log, left the script, right the
simulation panel. The arrows were unbound. egui reports a key as consumed
whenever a text field has the focus, and the app returns before its `match`
on a consumed key, so the caret can be moved in the script without folding
anything. Docked layout only, like `N`: focus mode has nothing docked.

## From a script

Asked for straight after: `app.config.toolbar_folded`, `log_folded`,
`script_folded`, `simulation_folded`. Each works like `panels_folded`: the
editor keeps what the field said last frame, applies a change on the config
side to `docked_open`, and writes the panel's state back every frame -- so a
key press or a drag shows up in the field, and `panels_folded` reads `True`
only while all four do. Flat fields rather than a `panels` group, because
`panels_folded` already exists flat and a script reads them side by side.
