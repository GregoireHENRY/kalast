# 2026-11-06 — kalast.greghy.com: Python as the page opens, and why the page starts, loads and runs as it does

The user (6 November), of kalast.greghy.com:

- "why do i see "starting kalast" at the beginning on the web?"
- "also why do examples take more time to load on th web than on the app?"
- "also why do examples on the web seem to be less stable in fps/its than on the app?"
- "also i see "16:49:15.680 starting Python, once: CPython in the page and numpy, about 15 MB" i think python should be
  loaded by default just like it works on the app"

Measured in headless Chrome 157 with WebGPU on this Mac (M1 Pro), on the live site unless said otherwise. The
machine was busy throughout: load 9 to 12, from another session's jobs and a browser.

## Python as the page opens

`web/python.js` starts Python once the page has drawn a frame, rather than when the first script runs:

- Pyodide, numpy and kalast's package, then `import kalast.editor`. The python tab is greeted, as the UI app greets
  it as it opens.
- Nothing goes to the log when it works, as the UI app says nothing of its interpreter. The browser's console has
  `Python 3.14.2 started in 1.5 s`.
- A script, or a line at the python tab, asked for before Python is ready waits for it. The log then says "Python is
  still starting: the script runs once it is ready".
- A start that fails is said in the log, and tried again when a script runs.
- In a browser without JavaScript Promise Integration, the first run says that `step()` cannot be held for a frame.
  This used to be said on the line that announced the start.

Measured:

- **Python is ready 1.5 to 2.4 s after the page opens**, with Pyodide in the browser's cache. A first visit downloads
  about 9 MB from jsDelivr: CPython 3.4 MB, its library 2.5, numpy 2.9.
- **A script then starts at once.** `sphere/tpm.py` printed its first line 0.04 s after its run was asked, where it
  took 1.65 s with Pyodide cached and 4.2 s on a first visit. `two_spheres/main.py` was iterating 0.13 s after.
  - The desktop: 0.11 s from its interpreter's start to the same line.
- **The cost**: the page holds its frames once for about 0.3 s, 1.4 s after it opens. That is numpy's import, 287 ms
  in one piece. Importing numpy's core first does not split it: a submodule imports its package first. Every first
  run used to pay it.
- Every visitor now downloads Python, including the pages that show kalast in a frame (the TIRI site's Kalast tab).

## "Starting kalast…"

It is the page's message while the browser fetches, compiles and starts kalast's WebAssembly, then loads the page's
four small scripts one after the other. The WebAssembly is 19 MB, 8 MB over the wire. It holds the engine, egui, and
the examples and resources compiled in.

- First visit: shown 0.7 s, of which the download took 0.4 s here.
- Later visits: 0.15 s, the WebAssembly cached and compiled from the browser's code cache.

The desktop app shows nothing: its executable is on the disk, and its window appears once it is ready.

## Loading an example

- **Python examples** waited for Python's start (above), which the page now does as it opens.
- **Rust examples** are compiled into the page and load at once. `didymos/tpm.rs`'s two bodies (20,000 facets) were
  there by the time the message went.
- What remains:
  - a package a script imports, other than numpy, downloads at its first import, several MB each: matplotlib (24
    examples), scipy (9);
  - SPICE has no build for the page (34 examples).

## The iteration rate

Iterations a second over 15 s, in 1 s windows as the toolbar averages them; the spread is their standard deviation.

| example | page | desktop |
|---|--:|--:|
| `didymos/tpm.rs` | 577 ± 2 % | |
| `sphere/tpm.py` | 489 to 527 ± 3 to 7 % | 481 ± 12 % |
| `two_spheres/main.py` | 885 ± 17 % | 1,189 ± 9 % |
| `crater_self_shadow/fn.py` | 1,261 ± 19 % | 411 ± 9 % |

- The desktop's column: the example run with the repository's Python, `App()` in a window of its own, in the
  background, not through the UI app.
- In headless Chrome the browser's frames come at a steady 60 a second.

Why the page's rate moves more:

- **The GPU answers late.** A page cannot wait on the GPU. Chrome's GPU process tells it when a frame is done,
  through messages, as a task of the page's.
  - For an empty frame, measured alone: 1.3 ms at the median, 3.5 ms at the 95th percentile, up to 13 to 16 ms.
  - The page keeps at most three frames' work queued (`Window::gpu_backlogged`). The desktop waits the same way, for
    the frame two back, but hears in microseconds.
  - With three queued, Chrome's answers alone allow about 1,900 empty frames a second; with eight, 3,100. A light
    scene runs into that bound, with its jitter.
- **One thread.** The engine, Python, the browser's own frames and its garbage collector take turns on the page's one
  thread. On the desktop, Python has a thread of its own.
  - In `two_spheres/main.py`, the collector took 2.5 % of 5 s, in pauses of up to 9 to 12 ms.
  - The page's loop turned 40,000 times a second, most turns finding the GPU behind or the script not ready: the
    thread was busy 92 % of the time.
- **Rust examples are steady**: their frames take longer than Chrome's answer.
- **`crater_self_shadow/fn.py` is faster in the page.** Its `facet_illumination` hands back an earlier frame's
  shadows there, where the desktop waits for the frame just drawn.

## Built for speed: tried, no gain

The page is built for size (`opt-level = "s"`), without SIMD. A build at `opt-level = 3` with SIMD (`+simd128`) was
tried in scratch, served locally beside the size build, the two run in turn:

- **Size**: 22.0 MB rather than 19.2, 7.5 MB rather than 7.2 with Brotli at quality 9.
- **Rates**: no gain beyond the noise. `sphere/tpm.py` gave 700 and 760 it/s with each build, `didymos/tpm.rs` 550 to
  600 with both, and `two_spheres/main.py` 3 and 13 % more in two runs.

These examples wait on the GPU's answer and on Python, not on the engine's arithmetic. The size build stays.

## Why one thread in the page

The user, told so: "why is it one thread of everything on the browser but not on the app?"

- **A page's code runs on one thread**: the page's main thread, which also takes the input, the browser's frames
  (`requestAnimationFrame`) and the garbage collector. Other threads are Web Workers, which share no memory with
  the page by default.
- **Shared memory, which Rust's threads need, is allowed only on a page that is cross-origin isolated**
  (`SharedArrayBuffer`). Such a page is served with two headers (COOP, COEP), and nothing from another site loads in
  it unless that site allows it.
  - Pyodide from jsDelivr would have to allow it.
  - So would the page around kalast when it is shown in a frame, the TIRI site's Kalast tab.
- **Rust's standard library for the browser cannot start a thread** (`std::thread::spawn` fails on
  `wasm32-unknown-unknown`). Threads take a rebuild of it with atomics, nightly only, and workers started for them.
- **The canvas and its events are the main thread's** (winit's web backend).
- **Python could run in a worker, but kalast's Python calls the engine directly.** Each `body.mat`,
  `sim.state.iteration` is a call into kalast's WebAssembly, answered at once (`web/src/python.rs`). From a worker,
  each would be a message there and back, or need the shared memory above to wait for its answer.
- **The desktop app is an ordinary program, which starts what threads it likes.** The window and the renderer have
  the main thread, scripts and the python tab the scripts' thread (`App::set_script_thread`), and the GPU's driver its
  own. It waits on the GPU in its own process (`device.poll`).

## A deeper GPU queue

The user, asked whether to try it: "yeah i guess".

The page now queues up to eight frames' work on the GPU while that is little of the GPU's time (`PAGE_MAX_IN_FLIGHT`,
`Window::gpu_backlogged`). The GPU's time over a frame is the spacing of its answers for frames queued behind another,
averaged (`Window::count_in_flight`). Once three frames hold more than 4 ms of it (`PAGE_QUEUED_MS`), the bound is
three, as before and as on the desktop. A heavy scene's queue would otherwise hold the browser's frame, and the
panels, behind it: the desktop's two frames behind came from presents queued behind 64 command buffers
(`notes/2026-09-23_presenter_thread/`).

Built locally and run in turn with the build before it (1193f9d3cfb7), 8 s each, it/s over 1 s windows:

| scene | three frames | up to eight | |
|---|--:|--:|---|
| `crater_self_shadow/fn.py`, three runs, the machine idle | 1,703, 1,709, 1,719 | 1,916, 1,881, 1,910 | +10 to 12 % |
| `two_spheres/main.py`, the same runs | 1,372, 1,645, 1,638 | 1,705, 1,716, 1,709 | +4 to 24 % |
| `crater_self_shadow/fn.py`, the machine busy | 609, 598 | 667, 652 | +9 % |
| `sphere/tpm.py`, busy | 581, 625 | 611, 643 | +3 to 5 % |
| `two_spheres/main.py`, busy | 471, 566 | 495, 559 | -1 to +5 % |
| a step and nothing else (`ico2.obj`), busy | 1,413, 1,320, 2,208 | 1,414, 2,069, 2,276 | 0 to +57 % |
| a heavy scene, busy | 84, 86 | 86, 85 | the same |

- The heavy scene: the Didymos pair under the most accurate shadows, maps at 8192 with PCF 4, about 12 ms of GPU a
  frame. The browser's frames came at 58 to 60 a second with either build.
- The spread within a run went from 9 to 11 % to 5 to 8 % on the idle machine; on the busy one it did not change
  beyond the noise.
- The same build gave 560 and 970 it/s in two runs a minute apart while the machine was busy: the page's one thread,
  shared with the rest of the machine, sets the rate more than the GPU's answer does, once that is queued deep enough.
- crater's per-facet shadows (`facet_illumination`) come back from frames further behind: up to eight, where they
  came from up to three.

The desktop is unchanged: everything new is in the page's build alone (`cfg(target_arch = "wasm32")`).

## Open

- The message could show only when the start takes longer than about a second, so that later visits never flash it.

## Checks

- Locally and live: the page with no script named had Python ready 1.5 to 2.4 s in, and nothing on the log.
- A script named by the address (`#examples/sphere/tpm.py`), asked for before Python was ready, ran once it was, at
  1.70 s from the page's start, against 1.80 s before.
- In the first 5 s, only two of the page's tasks took over 30 ms: kalast's start (51 to 55 ms) and numpy's import
  (283 to 326 ms).

- The deeper queue, live: `crater_self_shadow/fn.py` at 1,896 it/s, and the heavy scene at 80 it/s with the
  browser's frames at 59 a second.

Deployed: 1193f9d3cfb7 (Python as the page opens), then 14650500d87a (the deeper queue). The scripts are in the
session's scratch folder (`webperf/`): `run.mjs` times the page through Chrome's DevTools protocol, with
`--then`/`--source` for a script of one's own (`light.py`, `heavy.py`); `gpulatency.mjs` times Chrome's answers.
