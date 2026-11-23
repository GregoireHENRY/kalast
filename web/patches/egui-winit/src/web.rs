//! kalast's patch (see `PATCH.md`): in a browser, the platform and the
//! clipboard, which winit does not give and eframe's web backend would.

use std::sync::Mutex;

/// Whether the page runs on a Mac, by the browser's word: Cmd is the
/// command key there, and egui's shortcuts follow it.
pub fn is_mac() -> bool {
    static MAC: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *MAC.get_or_init(|| user_agent().is_some_and(|ua| ua.contains("Mac")))
}

pub fn user_agent() -> Option<String> {
    web_sys::window()?.navigator().user_agent().ok()
}

/// What the system clipboard gave since the last frame, for a paste.
static PASTED: Mutex<Option<String>> = Mutex::new(None);

/// Ask the system clipboard for its text, which a browser answers later:
/// it comes as a paste at a frame after (`take_pasted`) -- `fallback`, the
/// page's own clipboard, where the browser refuses.
pub fn ask_paste(fallback: String) {
    let Some(window) = web_sys::window() else { return };
    let promise = window.navigator().clipboard().read_text();
    wasm_bindgen_futures::spawn_local(async move {
        let text = match wasm_bindgen_futures::JsFuture::from(promise).await {
            Ok(text) => text.as_string().unwrap_or(fallback),
            Err(_) => fallback,
        };
        *PASTED.lock().unwrap_or_else(|e| e.into_inner()) = Some(text);
    });
}

pub fn take_pasted() -> Option<String> {
    PASTED.lock().unwrap_or_else(|e| e.into_inner()).take()
}

/// Text copied or cut, onto the system clipboard too.
pub fn copy(text: &str) {
    if let Some(window) = web_sys::window() {
        let _ = window.navigator().clipboard().write_text(text);
    }
}
