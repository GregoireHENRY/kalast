//! The bits of AppKit winit does not reach.
//!
//! Only the window's green button, which is not winit's to give: it is
//! AppKit's `toggleFullScreen:`, wired up by the collection behaviour of the
//! `NSWindow` and with no hook in between.

use objc2::rc::Retained;
use objc2_app_kit::{NSView, NSWindow, NSWindowCollectionBehavior};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};

/// The `NSWindow` behind a winit window.
fn ns_window(window: &winit::window::Window) -> Option<Retained<NSWindow>> {
    let handle = window.window_handle().ok()?;
    let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
        return None;
    };
    // SAFETY: winit hands out a pointer to a live `NSView` it owns, and this
    // only borrows it for long enough to retain its window.
    let view: &NSView = unsafe { handle.ns_view.cast::<NSView>().as_ref() };
    view.window()
}

/// Stop the green button offering *native* fullscreen.
///
/// Native fullscreen moves the window to a Space of its own, which is the
/// behaviour this project spent a day working around: nothing can sit on top
/// of it, reaching it is a swipe, and the Space animation starves the drawable
/// pool for up to a second.
///
/// Clearing `FullScreenPrimary` turns the button back into a plain zoom --
/// instant, no Space -- which `App` then converts into the simple fullscreen
/// the config option gives. `FullScreenAuxiliary` keeps the window able to
/// *join* another app's fullscreen Space, which costs nothing and is what a
/// window that has opted out of primary is supposed to say.
pub fn disable_native_fullscreen(window: &winit::window::Window) {
    let Some(ns) = ns_window(window) else { return };
    let behavior = ns.collectionBehavior();
    ns.setCollectionBehavior(
        (behavior & !NSWindowCollectionBehavior::FullScreenPrimary)
            | NSWindowCollectionBehavior::FullScreenAuxiliary,
    );
}

/// Whether the green button has just zoomed the window.
pub fn is_zoomed(window: &winit::window::Window) -> bool {
    ns_window(window).is_some_and(|ns| ns.isZoomed())
}

/// Put a zoomed window back where it was.
///
/// Called before entering simple fullscreen, so the size the window returns to
/// on the way out is the one it had before the button was pressed, and so
/// `is_zoomed` reads false again rather than reporting the same zoom for ever.
pub fn unzoom(window: &winit::window::Window) {
    if let Some(ns) = ns_window(window) {
        if ns.isZoomed() {
            ns.zoom(None);
        }
    }
}
