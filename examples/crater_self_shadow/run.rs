//! `crater_main` as a program of its own.
//!
//! ```sh
//! cargo run --release --example crater_main
//! ```
//!
//! The scene is in `main.rs`, which is also the cdylib the editor loads. All
//! this adds is the part a hosted example does not have: making an app, and
//! owning the loop.

#[path = "main.rs"]
mod example;

fn main() {
    let mut app = kalast::app::App::new();
    example::scene(&mut app);
    app.start();
}
