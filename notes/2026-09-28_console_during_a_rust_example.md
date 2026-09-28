# 2026-09-28 — the python tab while a Rust example runs

**Symptom.** The user: "the python tab console doesnt seem to work on a rust
example". Lines typed while `examples/crater_self_shadow/main.rs` ran did
nothing.

**Cause, three layers deep.**

1. The console is served by the front door (`python -m kalast`, the bundle) on
   `EditorTick::Console`, between two turns of `editor_tick`. A Rust example
   ran *inside* a turn: `editor_tick` called `load_example`, which called the
   example's `main`, and a driven one -- `while app.is_running() { ...
   app.step() }` -- does not return until its loop ends. So the front door
   never got a turn, and the lines waited for the example to finish.
2. Serving them from inside that turn would not have done either: the front
   door held the app borrowed (`inner.borrow_mut().editor_tick()`) for the
   whole of the example's run, so `app.anything` typed at the console would
   have raised "already borrowed".
3. And `app.simulation` was not the scene on screen. A Python `App` keeps the
   handles it was made with, while adopting a Rust example's scene replaces
   the host's simulation outright (`adopt_scene`: the host has to render the
   example's own `Rc`, the one its loop writes into). A line that ran would
   have seen the empty scene from before the example.

   The same stale handle hid an older bug. A Python script run after a Rust
   example built its scene into the app's original simulation while the window
   went on showing the example's, reset and empty: the viewport stayed blank.

**Fix.**

- `editor_tick` hands a load out as `EditorTick::Example { path, release }`, as
  it hands a script out as `Run`, instead of running it in place.
- `App::run_example(&Rc<RefCell<App>>, path, release, between)` runs it from
  the front door. The host's side of `HostApi` now works through a context,
  `hosted::host::Host`, rather than an `App` pointer. The context borrows the
  app for each call the guest makes and never across one, and runs `between`
  after each frame, with nothing borrowed.
  - `log`, `close`, `is_running` and `superseded` go through the app's
    `Shared` alone: a log line can come from inside a frame, where the app is
    borrowed.
  - The guest passes the pointer back verbatim, so nothing changes on its
    side, and `HostApi`'s size is the same.
- The Python and bundle front doors pass a `between` that serves the console
  when `console_pending()`. They pay nothing per frame otherwise: rule 8, the
  UI app must not slow a run. The engine's own `run_editor`, which has no
  Python, still calls `load_example` in place.
- The app keeps `home_simulation`, the one it was made with. A script's run,
  Reset and the next example's load put it back (`restore_home_simulation`,
  with `meshes_dirty`). The next example's load does it before the old library
  is unloaded, since the host's handle on the example's simulation is the last
  one and its drop has to run while that code is still loaded.
- The Python `App.simulation` getter reads the app's current simulation
  whenever the app can be borrowed: between frames, and so at the console. It
  falls back on the handles held beside `inner` inside a frame. There, with
  the restore above, the simulation is the app's own again.

**Verified.** A scratch probe queued a line with `console_submit` and opened
`crater_self_shadow/main.rs`, in a window in the background. The line
recorded what it saw and called `app.close()`. It printed `1 bodies,
iteration 1, paused True`: the example's crater, held after iteration 0, seen
while its loop ran. The loop then ended on the close.

Before the getter change the same probe read `0 bodies`. Before the tick
change the line would never have run while the loop did.

`the_apps_own_simulation_comes_back_after_an_example` covers the restore
after a script, a Reset and another example.

**Not done.** The bundle's arm is the same engine path with its own
`console_serve`, and was only built, not driven: a queued line needs a
script to queue it. A Python script run in the same session before the
example leaves its variables as the console's namespace (`_script_globals`).
There, too, `app` is the live app.
