# 2026-09-23 — the swapchain on its own thread

Continues `2026-09-22_present_paced_by_the_display.md`: a visible window paced
the loop at about two iterations per refresh of whichever display it was on,
because `get_current_texture` blocks for the rest of every frame, and the
three ways round it tried on the loop's thread all failed on measurement.

## The spike

A standalone program (`/tmp/kalast-screen/spike`, wgpu 30.0.1, winit
0.30.13, the crate versions kalast uses): the winit thread renders a
changing colour into a texture every frame; a second thread loops on
`get_current_texture`, copies that texture into the drawable, submits and
presents. Visible window, the built-in 120 Hz panel, 8 s each:

| main thread | frames/s | presents/s | longest acquire |
|---|---|---|---|
| `run_app` (the `start()` shape) | 2,540 | 120 | 17 ms |
| `pump_app_events` + 5 ms sleep between pumps (the `step()` shape) | 750 | **0** | -- |
| pump with a 5 ms timeout, main thread idle *inside* the run loop | 190 | 120 | 20 ms |
| frame pump, then a 1000 us idle pump with no redraw, then 3 ms of "script" | 500 | 120 | 20 ms |
| same with a 200 us idle pump | 505 | 120 | 24 ms |
| same with a 0 us idle pump | 120 | 120 | 17 ms |

Two facts. wgpu 30 does let one thread render while another acquires,
copies and presents -- the first row -- so yesterday's hang in kalast was not
wgpu-core's locking after all. And on macOS a thread's `queue.present`
completes only while the main thread is *waiting in its run loop*: with the
main thread out of the loop between pumps (running Python, or here a
sleep), not one present in eight seconds; give the loop a moment after each
frame -- a pump with a timeout and no redraw requested -- and the display is
fed at its own rate while the frames run free. 200 us is enough; a
zero-timeout pump is not (last row: the pump services the present
synchronously and the main thread pays the wait).

That is exactly why the previous evening's presenter thread hung in
`queue.present` after "submitted": the loop's thread was the Python script
between pumps, never waiting.

## The change

`window::Presenter`, a thread owning the acquire-copy-present sequence,
fed by the loop through `Window::frame_done(&texture)` (a mutex holding the
newest complete frame and a serial, so nothing is presented twice or held
while waiting for a frame). The loop never touches the swapchain:
`get_surface_texture` is the housekeeping that remains -- a surface the
presenter found outdated is reconfigured, a lost one recreated with a new
presenter -- and returns `None`. A terminal run hands over `render_texture`
after `Window::render` (the HUD is drawn into it, as the editor's viewport
path already did; export still copies before the HUD). The editor draws
its UI into `ui_texture`, the window's size in the surface's format, at
most 120 times a second, and hands that over. The `debug.window` line is
`presents/s, frames/s`.

`App::step()`, after the frame's pump: if the presenter is inside
`queue.present` (an atomic it sets around the call), a 200 us pump with
`idle_pump` set so `about_to_wait` requests no redraw -- the main thread's
moment in its run loop. At most once per present, so at most ~120 times a
second. `start()` needs nothing: `run_app` is the run loop.

## In kalast: measured, sampled, and reverted

The same presenter in kalast (`window::Presenter`, the loop handing over
`render_texture`, the editor its UI texture, `step()` giving the run loop
200 us after a frame when the presenter was inside `present`):

| loop | frames/s | presents/s |
|---|---|---|
| `step()`, `_10k` example, plain | 100 | **0** |
| `step()`, `_10k` example, UI app | 95 | **0** |
| `start()` + callback, light scene | 2,900 | **10** |

Not the pump, then, or not only. A stack sample of the pumped run
(`sample <pid>`) says where the presenter thread sits: not in Metal but in
`objc2_foundation::run_on_main` -> `dispatch_sync` to the **main queue**.
`wgpu-hal-30.0.1/src/metal/surface.rs`, `acquire_texture`: before
`nextDrawable` it looks up the hosting `NSWindow` and reads its
`occlusionState` (the workaround for wgpu issue 8309, a 1 s hang in
`nextDrawable` on an occluded window), and `NSWindow` is main-thread-only,
so from any other thread that read is a synchronous hop to the main thread.
The spike's presenter shows the same frames -- 63 of 718 samples in that
hop, the rest in `nextDrawable` -- and gets 240 presents/s because its main
thread services the queue every pass. Kalast's does not: in the pumped loop
the main thread is in the frame handler or in Python, never waiting in the
run loop; under `run_app` it serviced the hop about ten times a second, and
why that differs from the spike's loop was not found.

One more thing the sample showed, for the record: while the presenter waited,
kalast's main thread spent 355 of 584 samples blocked *creating a Metal
command buffer* (`draw_text_overlay` -> `queue.write_buffer` ->
`-[MTLCommandQueue commandBuffer]`), the queue's in-flight limit reached --
the frames were being submitted faster than a queue with a stalled
presentation retired them. A second symptom of the same stall.

So with wgpu-hal 30 as it is, on macOS, a thread cannot own the swapchain
for a loop whose main thread is not continuously in its run loop -- which
is every `step()` script. Reverted; `main` presents every frame on the
loop's thread as before, and the display paces a visible light scene at
about two iterations per refresh. Heavy scenes are slower than that and do
not notice; a covered or `open_in_background` window never presents and
runs free.

## What would actually fix it

1. **Upstream, in wgpu-hal:** do the occlusion check without a main-thread
   hop -- cache `occlusionState` from `NSWindowDidChangeOcclusionStateNotification`
   on the main thread and read the cached flag in `acquire_texture`. Then
   the presenter thread here works as the spike does (`nextDrawable` off the
   main thread is documented as fine). The spike and the samples are the
   reproduction; the spike's source is in `notes/spikes/2026-09-23_presenter_thread/`.
2. **In kalast, without waiting for upstream:** a raw-Metal presenter --
   `objc2-quartz-core` for `nextDrawable` and present on the thread, the
   copy from `render_texture` through `wgpu::Texture::as_hal` into the
   drawable's `MTLTexture`. Bypasses wgpu's surface entirely on macOS. A
   few hundred lines and a second code path to keep in step with the
   Windows/Linux one, where the plain `Presenter` above should already work
   (no main-thread hop in those backends; unverified).
3. **Do nothing:** the pacing costs a visible window on a light scene, which
   is the UI app being watched; nothing a run cares about.

Recommendation: 1, filed with the reproduction, and 3 meanwhile.

## The morning after: it was the GPU queue, and the fix is on the main thread

The presenter thread is dead on macOS with wgpu-hal 30 (above), but the
spike had one more thing to say. Its **main-thread** gate -- present from
the redraw handler only once per refresh interval, nothing on any other
thread -- gives 2,300 frames/s, 116 presents/s, acquisitions under 1 ms,
under `run_app` *and* under a pumped loop (1,120 frames/s, 110 presents/s).
The same gate in kalast: 6-10 presents/s and 130-240 ms acquisitions, text
pass or not, `COPY_SRC` or not, kalast's window size or not, frame latency
1, 2 or 3 -- every surface setting was copied into the spike and none of it
moved.

What did: measuring submit-to-completion latency inside kalast with
`Queue::on_submitted_work_done`. In the gated, visible run it climbed from
250 ms to 780 ms with 57-60 command buffers in flight -- Metal's limit is
64, which is the blocked `-[MTLCommandQueue commandBuffer]` the stack
sample had shown -- on a scene whose GPU frame is 0.94 ms (`gpu_timing`:
shadow 0.56, render 0.87, text 0.85, overlapped). The loop, no longer
throttled by presenting every frame, submits frames the GPU has not started
and a presenting frame's copy into the drawable queues behind all of them;
the next acquisition then waits for that drawable chain. Force the CPU to
wait for the GPU after each frame (`device.poll(Wait)` on the frame's
submission) and the same run reads 95 presents/s at 95 frames/s, longest
acquire 0-2.6 ms, no backlog. The "panel dozing" reading of 22 September
was this.

Why the queue backs up at all when the GPU has 9 ms of slack per frame is
not established -- a command buffer that writes a drawable is scheduled
only when the drawable is ready, and in-order queues wait behind it, is the
shape of it -- but bounding the depth removes it regardless, and the bound
is right on its own terms: `step()` used to return with the GPU up to 64
frames behind, which is the queueing that
`feedback: benchmarks need a sync` warned every timing about.

**The change.** `Window::render` keeps the last two frames' submission
indices and, before letting a third in, waits for the oldest
(`device.poll(PollType::Wait { submission_index })`): CPU and GPU stay
overlapped, the queue stays two deep. The redraw handler acquires the
swapchain only once per refresh interval of the window's current display
(winit's `refresh_rate_millihertz`, read at creation and on every move and
resize); the frames between run as an occluded window's do. Nothing on any
other thread. `start()` and `step()` alike.

## Measured, visible window, built-in 120 Hz panel

| | frames/s (= it/s) | presents/s | longest acquire |
|---|---|---|---|
| light scene, `app.start()` + callback | 2,850 | 118 | 1.1 ms |
| light scene, UI app, `step()` script | 2,878 | -- | -- |
| `_10k` Didymos, plain `step()` loop | 106 | 74 | 14 ms |
| `_10k` Didymos, UI app | 93 | 71 | 15 ms |
| 20-facet mesh, covered window (the loop's ceiling) | 3,060 | 0 | -- |

Before, the same visible light scene read 300 it/s here and 120 exactly on
the 60 Hz monitor. The `_10k` example is bound by its four SPICE calls per
iteration, ~10 ms, and presents on most frames; the `_10k` GPU frame is
under a millisecond. On a 60 Hz display the gate presents ~60 times a
second and the loop does not change -- the interval is re-read on every
move. `test_editor_startup` (one `step()` is one frame), stubs, bindings,
far Sun, PCF, shadow layers and facet shadow pass; `cargo test --release`
both feature sets.
