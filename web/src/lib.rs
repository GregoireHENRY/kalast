//! kalast's UI app in a browser: the editor, its panels and its renderer,
//! compiled to WebAssembly and drawn with WebGPU, every computation on the
//! visitor's machine.
//!
//! A page has no file system and no compiler, so it brings both halves of
//! what the desktop app finds on the disk: its files -- the repository's
//! examples and the resources they read, compiled in (`build.rs`), and what
//! the visitor makes or saves, kept in the browser's storage for the next
//! visit -- and its Rust examples, compiled in too, which is how Play runs
//! one here. It opens as the UI app opens, on an empty scene; an address
//! naming one of its files -- `kalast.greghy.com/#examples/sphere/tpm.py` --
//! opens that, as `kalast <file>` does on the desktop.

use kalast::app::App;
use kalast::fs::Change;
use wasm_bindgen::prelude::*;

mod config_gen;
mod convert;
mod folder;
mod python;

include!(concat!(env!("OUT_DIR"), "/files.rs"));

/// The storage's keys for what the visitor made, saved or took away, each
/// followed by the path.
const FILE: &str = "kalast:file:";
const FOLDER: &str = "kalast:folder:";
const GONE: &str = "kalast:gone:";

#[wasm_bindgen(start)]
pub fn start() {
    // A panic stops the page's loop, and the canvas with it: said over it,
    // as well as on the console.
    std::panic::set_hook(Box::new(|info| {
        console_error_panic_hook::hook(info);
        let status = web_sys::window().and_then(|w| w.document()).and_then(|d| d.get_element_by_id("status"));
        if let Some(status) = status {
            status.set_text_content(Some(&format!("kalast stopped: {info}\n\nReloading the page starts it again.")));
        }
    }));
    let _ = console_log::init_with_level(log::Level::Warn);

    for (path, bytes) in FILES {
        kalast::fs::add(path, bytes.to_vec());
    }
    // The visitor's own folder, as a bundle has it, empty.
    kalast::fs::add_folder("scripts");
    restore();
    kalast::fs::on_change(keep);
    add_examples();

    let named = web_sys::window()
        .and_then(|w| w.location().hash().ok())
        .map(|h| h.trim_start_matches('#').to_string())
        .and_then(|h| js_sys::decode_uri_component(&h).ok().and_then(|h| h.as_string()))
        // A file of the page's: not nothing, which is the root folder.
        .filter(|h| !h.is_empty() && kalast::fs::read(h).is_ok());
    // The beta says so in the kalast tab's first line, with the day it was
    // built: index.html's `data-channel` and `data-built`, which build.sh sets.
    let html = web_sys::window().and_then(|w| w.document()).and_then(|d| d.document_element());
    let attribute = |name: &str| html.as_ref().and_then(|e| e.get_attribute(name));
    if attribute("data-channel").as_deref() == Some("beta") {
        kalast::app::update::set_page_build(match attribute("data-built").filter(|d| !d.starts_with("__")) {
            Some(day) => format!("beta (built {day})"),
            None => "beta".to_string(),
        });
    }
    let mut app = App::new();
    app.editor_start(named.as_slice());
    // Shown inside another page that says so (`?embed=<who>`, embed.js), as
    // the user asked of it: egui's dark theme, and meshes dressed to be looked
    // at. The toolbar and the log as kalast.greghy.com shows them (the user,
    // 3 November: "kalast tab show top and bottom panel too"; folded from 1
    // November). After `editor_start`, whose remembered settings would undo
    // the theme; and nothing changed there is remembered (`settings::save`),
    // so that kalast.greghy.com itself, which shares the storage, keeps its
    // own.
    let embedded = web_sys::window()
        .and_then(|w| w.location().search().ok())
        .is_some_and(|q| q.trim_start_matches('?').split('&').any(|kv| kv.split('=').next() == Some("embed")));
    if embedded {
        kalast::app::gui::set_embedded(true);
        app.config.borrow_mut().theme = kalast::app::config::UiTheme::Dark;
    }
    // What the page's Python reaches, before the page's loop takes the app.
    python::install_app(&app);
    app.spawn();
}

fn storage() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok()?
}

/// What earlier visits took away, made and saved, over the page's own files.
fn restore() {
    let Some(storage) = storage() else { return };
    let keys: Vec<String> = (0..storage.length().unwrap_or(0)).filter_map(|i| storage.key(i).ok().flatten()).collect();
    for path in keys.iter().filter_map(|k| k.strip_prefix(GONE)) {
        let _ = kalast::fs::remove_all(path);
    }
    for path in keys.iter().filter_map(|k| k.strip_prefix(FOLDER)) {
        kalast::fs::add_folder(path);
    }
    for (key, path) in keys.iter().filter_map(|k| Some((k, k.strip_prefix(FILE)?))) {
        if let Ok(Some(text)) = storage.get_item(key) {
            kalast::fs::add(path, text.into_bytes());
        }
    }
}

/// Keep what was made, saved or taken away for the next visit -- in the
/// folders the files tab shows and the settings', not what a script writes
/// elsewhere for itself. A file of the page's taken away is remembered as
/// gone, or it would be back. Into the visitor's folder instead, while the
/// page works in one (`folder::takes`); and for Python's own files, which
/// take it before the next run.
fn keep(path: &str, change: Change<'_>) {
    python::dirty(path);
    if folder::quiet() || folder::takes(path, &change) {
        return;
    }
    if !["scripts", "examples", ".kalast"].iter().any(|d| path == *d || path.starts_with(&format!("{d}/"))) {
        return;
    }
    let Some(storage) = storage() else { return };
    let kept = match change {
        Change::File(bytes) => match std::str::from_utf8(bytes) {
            Ok(text) => storage.set_item(&format!("{FILE}{path}"), text).and(storage.remove_item(&format!("{GONE}{path}"))),
            Err(_) => {
                log::warn!("{path} is not text, and is kept until the page closes");
                Ok(())
            }
        },
        Change::Folder => storage.set_item(&format!("{FOLDER}{path}"), "").and(storage.remove_item(&format!("{GONE}{path}"))),
        Change::Removed => {
            let ours = FILES.iter().any(|(p, _)| *p == path || p.starts_with(&format!("{path}/")));
            storage
                .remove_item(&format!("{FILE}{path}"))
                .and(storage.remove_item(&format!("{FOLDER}{path}")))
                .and(if ours { storage.set_item(&format!("{GONE}{path}"), "") } else { Ok(()) })
        }
    };
    if kept.is_err() {
        log::warn!("{path} could not be kept for the next visit: the browser's storage is full or refused");
    } else if kalast::app::settings::path().is_some_and(|p| p == std::path::Path::new(path)) {
        SETTINGS_KEPT.with(|f| f.borrow().as_ref().map(|f| f.call0(&JsValue::NULL)));
    }
}

thread_local! {
    static SETTINGS_KEPT: std::cell::RefCell<Option<js_sys::Function>> = const { std::cell::RefCell::new(None) };
}

/// Have `f` called each time the app's settings are kept for the next visit,
/// which is when one is changed in the app: `index.html` takes a visitor who
/// ticks "offer betas" in the app tab to the beta, and back once unticked.
#[wasm_bindgen]
pub fn on_settings_kept(f: js_sys::Function) {
    SETTINGS_KEPT.with(|s| *s.borrow_mut() = Some(f));
}

/// A mesh sent by the page this one is shown in (`embed.js`): put among the
/// page's files, in `sent/`, for this visit only -- neither in the browser's
/// storage nor in a folder of the visitor's -- and sent to the renderer as
/// the files tab sends an `.obj`. Its path, or why not.
#[wasm_bindgen]
pub fn open_sent(name: &str, bytes: Vec<u8>) -> Result<String, JsValue> {
    let name = name.rsplit(['/', '\\']).next().unwrap_or_default();
    let fits = name.len() <= 128
        && !name.starts_with('.')
        && name.to_ascii_lowercase().ends_with(".obj")
        && name.chars().all(|c| c.is_ascii_alphanumeric() || "._- ".contains(c));
    if !fits {
        return Err(JsValue::from_str(&format!(
            "{name:?}: expected a file name ending in .obj, of letters, digits, spaces and ._-"
        )));
    }
    let path = format!("sent/{name}");
    kalast::fs::add(&path, bytes);
    python::dirty(&path);
    kalast::app::gui::send_mesh(&path);
    Ok(path)
}

/// A pinch the page caught (`wheel.js`, `embed.js`), as the desktop's
/// trackpad reports one: its magnification, `+1.0` for twice as big. Taken
/// between frames as the desktop's pinch is taken.
#[wasm_bindgen]
pub fn pinch(magnification: f64) {
    if magnification.is_finite() && magnification > -1.0 {
        kalast::app::gui::send_pinch(magnification);
    }
}
