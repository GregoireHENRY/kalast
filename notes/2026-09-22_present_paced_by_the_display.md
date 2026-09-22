# 2026-09-22 — the display paced the loop; now it is shown at its own rate

**Symptom.** The user: "when I move kalast from my main laptop screen to the
second screen the it/s seems to halve." `examples/didymos/main.py` on the
`_10k` meshes, in the UI app. The built-in panel is a 120 Hz Liquid Retina
XDR; the second screen a Philips 279P1 at 60 Hz, both at 2× scale, so it was
not pixels.

**What it was not.** Not the renderer: the full-resolution pair at 105 it/s
read the same on both screens. Not winit's redraw dispatch: queued redraws
go out synchronously per run-loop iteration. Not the `run_app` loop as
against the pumped `step()` loop: swapping `start()` to a pump changed
nothing -- but every probe I ran here used `open_in_background`, whose
window is covered, and a covered window skips the present entirely. Those
probes could not see the problem, and a first `start()` change measured
against them was reverted.

**Measured by the user, visible window, light scene, both loop shapes**
(`step()` script in the UI app; `app.start()` with a callback):

| | built-in 120 Hz | Philips 60 Hz |
|---|---|---|
| before | 300 it/s | **120 exactly** |
| after | ~3,000 | ~3,000 |

120 on a 60 Hz display is two presents per refresh, which is what a
three-drawable pool allows before `nextDrawable` blocks: wgpu configures the
`CAMetalLayer` with `maximumDrawableCount = 3`, `allowsNextDrawableTimeout =
false` and, for `Immediate`, `displaySyncEnabled = false`; the window server
returns drawables at the pace of the display the window is on. The 300 on the
built-in panel is the same cost in a milder form -- ten times the loop, once
it stopped presenting every frame.

**The change** (`src/app/mod.rs`, the redraw handler). The swapchain is
acquired only when a refresh interval of the window's current display has
elapsed since the last present -- `current_monitor().refresh_rate_millihertz()`,
read at window creation and again on every `Moved` and `Resized`, so
dragging to another screen changes the answer; `None` where the platform
does not say, which presents every frame as before. The frames in between
run exactly as an occluded window's do (`get_surface_texture` returning
`None`): stepped, rendered into `render_texture`, HUD and export included,
not blitted or presented. Both loop shapes go through the same acquisition.

**Kept.** `step()` is still one frame per call (`test_editor_startup`);
exported frames are unchanged (`test_far_sun` reads them); the covered-window
rate is unchanged (2,942 it/s before and after). `cargo test --release`
129/129.

**Corrects an older reading.** `2026-09-08_step_one_frame_and_a_bad_benchmark.md`
found a Rust binary "pinned at 121 it/s exactly" and the Python module not,
and put it down to the two loops. It was this: the binary's window was on
screen and presented every frame, the Python one was measured covered. The
two front doors compare end to end now.

---

**Reverted the same evening.** On the built-in panel the user saw 100 it/s
with a picture like 5 frames a second, and it reproduced here in the
foreground (the counters below are `debug.window` prints added for it):

    [WINDOW] 6 presents/s, 112 frames/s, 0 refused, longest gap 220 ms, longest acquire 211 ms

Every presenting frame blocked 130-220 ms *inside `get_current_texture`*.
The built-in panel is adaptive-refresh: after the loading gap it had dropped
to an idle rate, one present every 8 ms was not enough to wake it, each
acquisition then waited for its slow refresh, and the presents stayed rare
-- a feedback loop that presenting every frame never enters (the panel sees
frames as fast as it releases drawables and ramps up). With the gate removed
the same run reads 105 presents/s, 105 frames/s, longest acquire 10-21 ms.

Which also says what this example's 100 it/s is: the loop's own work is
about 1 ms a frame (with the gate, 100 frames/s went by between 6 presents),
and the rest is the acquisition waiting for the display. The loop is paced
by the display on both screens; the second one only makes it obvious.

The fix that is right on both kinds of display is to take the blocking call
off the loop's thread: acquire the drawable on a helper thread, present
whatever frame is current when one is ready, and never wait. Next.

---

## Three more attempts, all measured in the foreground, none kept

The counters are the `debug.window` line (`presents/s, frames/s, longest
acquire`), the scene `examples/didymos/main.py` on the `_10k` meshes, the
window visible on the 120 Hz built-in panel.

**Present at just under twice the refresh rate** (0.55 × interval, ~218/s
allowed). 9 presents/s, 100 frames/s, every acquisition 90-140 ms. Same
picture as the once-per-refresh gate. So it is not the spacing: with a
present every frame, presents are 10 ms apart and acquisitions take 10-20 ms.
The difference is *where the app is between presents*: presenting every
frame means calling `nextDrawable` again immediately and waiting inside it;
any gate means presenting, computing for a few milliseconds, and asking
then -- and that ask costs ~100 ms every time. Consistent with the layer
recycling its drawables the moment none is requested and paying an IOSurface
allocation on the next request. Whatever the cause, on this panel the pool
is cheap only while continuously asked for.

**Acquire on a thread, present on the loop's thread.** Worked for eight
frames, then acquisitions took 208 ms, 37, 1010, then never returned. It is
wgpu-core: `Surface::get_current_texture` holds `surface.presentation` (a
mutex) for the whole blocking wait, and `Queue::present` takes the same
mutex first thing (`wgpu-core/src/present.rs`, `Queue::present`). So the
loop's present waits for the thread's acquire, the acquire waits for the
display to release a drawable, and the display releases one when the *next
presented* drawable is shown -- which is the present that is waiting. A
three-drawable pool hides it for three frames.

**Acquire, copy and present all on the thread**, the loop handing over its
finished `render_texture` (or the editor's UI texture, drawn at most 120
times a second) through a mutex and a serial. The thread acquired its first
drawable in 1.5 ms, encoded and submitted the copy, and then never returned
from `queue.present`: that path takes `device.snatchable_lock.write()` after
`prepare_surface_texture_for_present`, and something the loop's thread holds
across its frames keeps the writer out -- not identified. `Device`, `Queue`,
`Surface`, `SurfaceTexture` and `Texture` are all `Send + Sync`, so the API
permits it; wgpu-core's locking does not, as of 30.0.1.

## Where it stands

`main` presents every frame, as it did before today: correct picture,
and the loop paced by the display for scenes lighter than the display's
refresh -- at most about two iterations per refresh of the display the
window is on (~240 on the built-in panel, 120 on a 60 Hz monitor), the
rest of each frame spent in `get_current_texture`. A full-resolution run at
100 it/s barely notices; a `_10k` run notices a lot. A covered window
(`open_in_background`, or any window behind another) never presents and runs
at the loop's own rate, which is why every background probe here was fast.

What would fix it properly is the classic split: the simulation and its
rendering on a thread of their own, the winit thread doing events and
presentation only -- the acquire blocks there and nobody cares. That is a
change to how `App` runs a script (Python callbacks off the main thread, the
GIL, the editor's tick), and it wants a plan, not an evening. The cheaper
things tried above are exhausted by the measurements here.
