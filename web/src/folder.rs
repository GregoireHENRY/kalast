//! A folder of the visitor's, which the page works in once connected, as the
//! desktop app works in its own: the Rust half of `folder.js`, which reaches
//! it through the browser (the File System Access API, Chrome and Edge).
//!
//! The folder's files go into the page's over its own (`folder_put`), quietly
//! -- they came from there -- and what the page saves or a script writes goes
//! back to it rather than into the browser's storage (`takes`). The files tab
//! asks through `kalast::app::PageFolder`.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use kalast::fs::Change;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

/// The object `folder.js` installed, and what it said of the folder.
struct Folder {
    js: JsValue,
    connected: RefCell<Option<String>>,
    remembered: RefCell<Option<String>>,
}

thread_local! {
    static FOLDER: RefCell<Option<Rc<Folder>>> = const { RefCell::new(None) };
    /// A change the folder made itself, which is not written back to it.
    static QUIET: Cell<bool> = const { Cell::new(false) };
}

fn folder() -> Option<Rc<Folder>> {
    FOLDER.with(|f| f.borrow().clone())
}

fn call(js: &JsValue, name: &str, args: &[JsValue]) -> JsValue {
    let f = js_sys::Reflect::get(js, &JsValue::from_str(name)).ok().and_then(|f| f.dyn_into::<js_sys::Function>().ok());
    let Some(f) = f else { return JsValue::UNDEFINED };
    let args: js_sys::Array = args.iter().cloned().collect();
    f.apply(js, &args).unwrap_or_else(|e| {
        log::warn!("folder.js {name}: {e:?}");
        JsValue::UNDEFINED
    })
}

impl kalast::app::PageFolder for Folder {
    fn connected(&self) -> Option<String> {
        self.connected.borrow().clone()
    }

    fn remembered(&self) -> Option<String> {
        self.remembered.borrow().clone()
    }

    fn pick(&self) {
        call(&self.js, "pick", &[]);
    }

    fn reconnect(&self) {
        call(&self.js, "reconnect", &[]);
    }

    fn disconnect(&self) {
        call(&self.js, "disconnect", &[]);
    }

    fn on_disk(&self, path: &str) -> bool {
        call(&self.js, "has", &[JsValue::from_str(path)]).as_bool().unwrap_or(false)
    }

    fn write(&self, path: &str, bytes: &[u8]) {
        call(&self.js, "write", &[JsValue::from_str(path), js_sys::Uint8Array::from(bytes).into()]);
    }

    fn names(&self, dir: &str) -> Vec<String> {
        call(&self.js, "names", &[JsValue::from_str(dir)])
            .dyn_into::<js_sys::Array>()
            .map(|names| names.iter().filter_map(|n| n.as_string()).collect())
            .unwrap_or_default()
    }
}

/// `folder.js` gives the page its way to a folder, where the browser has one:
/// `pick`, `reconnect`, `disconnect`, `has`, `names`, and `write`, `mkdir`,
/// `remove` for what the page changes.
#[wasm_bindgen]
pub fn folder_install(js: JsValue) {
    let folder = Rc::new(Folder { js, connected: RefCell::new(None), remembered: RefCell::new(None) });
    FOLDER.with(|f| *f.borrow_mut() = Some(folder.clone()));
    kalast::app::set_page_folder(folder);
}

/// The folder the page works in, or the one from an earlier visit that the
/// browser wants allowed again: their names.
#[wasm_bindgen]
pub fn folder_state(connected: Option<String>, remembered: Option<String>) {
    if let Some(f) = folder() {
        *f.connected.borrow_mut() = connected;
        *f.remembered.borrow_mut() = remembered;
    }
}

/// A file of the folder's, into the page's files, over the page's own.
#[wasm_bindgen]
pub fn folder_put(path: &str, bytes: Vec<u8>) {
    kalast::fs::add(path, bytes);
    crate::python::dirty(path);
}

/// A folder of the folder's, into the page's files.
#[wasm_bindgen]
pub fn folder_dir(path: &str) {
    kalast::fs::add_folder(path);
}

/// Gone from the folder, taken away from the visitor's own tools: from the
/// page's files too.
#[wasm_bindgen]
pub fn folder_gone(path: &str) {
    QUIET.with(|q| q.set(true));
    let _ = kalast::fs::remove_all(path);
    QUIET.with(|q| q.set(false));
}

/// Files the folder changed, which the editor may have open.
#[wasm_bindgen]
pub fn folder_changed(paths: js_sys::Array) {
    for path in paths.iter().filter_map(|p| p.as_string()) {
        kalast::app::gui::changed_outside(&path);
    }
}

/// A change that came from the folder itself.
pub fn quiet() -> bool {
    QUIET.with(|q| q.get())
}

/// Written to the visitor's folder rather than kept in the browser's
/// storage: a change in the page's files while the page works in a folder,
/// but the settings', which stay the page's. A file's removal is a rename's
/// other half, the files tab refusing to throw away what is in the folder.
pub fn takes(path: &str, change: &Change<'_>) -> bool {
    let Some(f) = folder() else { return false };
    if f.connected.borrow().is_none() || path == ".kalast" || path.starts_with(".kalast/") {
        return false;
    }
    let at = JsValue::from_str(path);
    match change {
        Change::File(bytes) => call(&f.js, "write", &[at, js_sys::Uint8Array::from(*bytes).into()]),
        Change::Folder => call(&f.js, "mkdir", &[at]),
        Change::Removed => call(&f.js, "remove", &[at]),
    };
    true
}
