egui-winit 0.36.1 as crates.io has it (MIT OR Apache-2.0, https://github.com/emilk/egui), with kalast's changes for
the browser, each marked "kalast's patch" in the source. `web/Cargo.toml` patches it in for the browser's build alone;
the desktop's uses crates.io's.

- `src/dropped_file.rs`: on wasm32, egui's `DroppedFile` asks for `bytes_async` rather than `bytes`, and 0.36.1 does
  not compile there without it.
- `src/web.rs`, used from `src/lib.rs` and `src/clipboard.rs`: what winit does not give a page and eframe's web backend
  would. A Mac is told from the browser's user agent, so that Cmd is the command key there (`Modifiers::command`) and
  egui's idea of the platform follows. Copy and cut go to the system clipboard as well (`navigator.clipboard`), and a
  paste asks it, which answers later: the text comes as a paste at the next frame, the page's own clipboard where the
  browser refuses.

Drop the first when kalast moves to an egui whose egui-winit builds for wasm32; the second as long as kalast's page is
drawn through winit rather than eframe.
