# 2026-09-28 — the macOS 26 SDK paced the pump to the display

**Symptom.** The user: "barely 100 fps on empty scene on mac now", against
3,000–5,000 on Friday. Today's figure was the local bundle,
`dist/kalast-dev-ff4b23a-macos-arm64`, opened through `kalast.app`. Friday's
was `python -m kalast`.

**What it was not.**

- Not today's pull: the 23 September `target/release/kalast` does the same, 122 fps.
- Not `kalast.app` or LaunchServices: the bundle's `kalast` started from a
  terminal gives 134.
- Not presenting: `debug.window` read 125 frames/s and 73 presents/s, with the
  acquisition at 0.1 ms.
- Not the process's priority: `taskpolicy -l 0 -t 0` changed nothing.
- Not extra pumps: counted, it is one `pump_app_events` per frame.

Each pump took about 7.5 ms, with the main thread 93 % busy, all of it inside
`[NSApp run]`. The run loop kept turning: winit's poll timer re-armed,
`cleared()` ran again and again, and `_NS_SetBasicPasteTelemetry` ran for each
event dequeued. `run` returned only at the display's refresh.

**Measured.** Visible window, empty scene, the 120 Hz built-in display:

| main executable | linked against | frames/s |
|---|---|---|
| `python -m kalast` (uv's CPython 3.14.3) | SDK 15.5 | ~3,000 |
| the bundle's `kalast`, built with Xcode 26.5 | SDK 26.5 | 122–134 |
| the same file after `vtool -set-build-version macos 11.0 15.5` | SDK 15.5, stamp only | ~3,900 |

**Cause.** AppKit picks its "linked on or after" behaviour from the SDK the
*main executable* was linked against. Linked against the macOS 26 SDK,
`[NSApp run]` returns once per display refresh, even after winit's `stop:` and
the event it posts. So one `pump_app_events` per frame is paced to the display.

The bundle runs scripts in that process too, so a driven `while app.step():`
there was capped the same way.

What escapes it:

- `python -m kalast`: the main program is Python, linked against SDK 15.5, and
  an extension module's own SDK is not consulted.
- The released bundles: the v0.5.10 beta's `kalast` is stamped SDK 14.5 on
  arm64 (`macos-14`, Xcode 15.4) and 15.5 on x86_64 (`macos-15-intel`).

**Fix, first.** The local bundle was rebuilt against the Command Line Tools'
SDK 14.5, the one CI's arm64 runner uses
(`SDKROOT=/Library/Developer/CommandLineTools/SDKs/MacOSX14.5.sdk`). It then ran
at 2,900–4,000 frames/s and 118 presents/s under `debug.window`. Opened through
the root `kalast.app` alias, as a double-click opens it, its toolbar read
3,759 fps. That fixed one machine's bundle, and nothing else.

**Fix, for every build.** `build.rs` passes the linker a second
`-Wl,-platform_version,macos,<min>,15.5` for the binaries and the examples.
The linker keeps the last one it is given, without a warning, so the record
reads 15.5 whatever SDK the toolchain links against. `<min>` repeats rustc's
own minimum: `MACOSX_DEPLOYMENT_TARGET`, raised to 11.0 on Apple silicon or
10.12 on Intel, which is also the default. Without it the x86_64 bundle would
lose macOS 10.12–10.15.

Tests and benches are left out: cargo refuses `rustc-link-arg-tests` in a
package with no test target, and the library's unit tests are not one. The
Python module is left out on purpose, since Python is its main executable.

Measured with Xcode 26.5 and no `SDKROOT`:

- `cargo build --release --bin kalast` gives minos 11.0, sdk 15.5: 3,468 fps
  on the empty scene.
- `--example crater_main` is recorded the same way.

The release workflow gets a step, "AppKit sees an SDK before 26", after the
executable goes into the bundle. It fails a macOS bundle recorded against SDK
26 or later. It reads `LC_BUILD_VERSION`, or `LC_VERSION_MIN_MACOSX`, which is
what the linker writes for x86_64's 10.12 minimum.

The local build no longer pins `SDKROOT`, so it goes through `build.rs` as
`cargo run` does, and it checks the same record. It also unsets
`LIBRARY_PATH`. The shell here lists `/opt/local/lib` there, and every local
build linked MacPorts' libiconv, the morning's bundle included. A bundle built
that way starts only where MacPorts is installed. Cargo does not track
`LIBRARY_PATH`, so a binary already linked has to be relinked (touch
`src/bin/kalast.rs`) before the change shows. The build now refuses any file
in the bundle that links a library under `/opt/` or `/usr/local/`.

**Open.**

- A program of someone else's that links kalast as a library is recorded
  against its own toolchain's SDK: a dependency's `rustc-link-arg` does not
  reach the dependent's binary. With Xcode 26 it is paced until it records an
  older SDK itself, with the same `-platform_version` argument.
- Recording 15.5 also keeps the pre-26 window style. CI's bundles had it
  already.
