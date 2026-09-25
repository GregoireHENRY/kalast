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

/// Set by the key monitor when `Cmd`-`Q` is pressed; taken by the window.
static QUIT_KEYS: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Watch for `Cmd`-`Q` ahead of the window. While a text field has the focus
/// -- the editor, the console -- egui turns text input on, winit hands every
/// key to the system's text handling, and `Cmd`-`Q` never came back as a key:
/// sent to a UI app with the console focused, it did nothing. A local
/// monitor sees the event before any of that. Once per process; the monitor
/// lives as long as it does.
pub fn watch_quit_keys() {
    use objc2_app_kit::{NSEvent, NSEventMask, NSEventModifierFlags};
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let block = block2::RcBlock::new(|event: std::ptr::NonNull<NSEvent>| -> *mut NSEvent {
            // SAFETY: AppKit hands the monitor a live event for the length
            // of the call.
            let e = unsafe { event.as_ref() };
            // The character, not the key: `Q` is key 12 on a QWERTY board
            // and key 0 on an AZERTY one, and matching 12 left `Cmd`-`Q`
            // dead on the French keyboard it was reported from.
            let q = e
                .charactersIgnoringModifiers()
                .is_some_and(|c| c.to_string().eq_ignore_ascii_case("q"));
            let flags = e.modifierFlags();
            let chord = NSEventModifierFlags::Command
                | NSEventModifierFlags::Shift
                | NSEventModifierFlags::Option
                | NSEventModifierFlags::Control;
            if q && (flags & chord) == NSEventModifierFlags::Command {
                QUIT_KEYS.store(true, std::sync::atomic::Ordering::SeqCst);
                // Swallowed: it is ours, and nothing else should see it.
                return std::ptr::null_mut();
            }
            event.as_ptr()
        });
        // SAFETY: the block returns the event it was given, or null.
        let monitor = unsafe { NSEvent::addLocalMonitorForEventsMatchingMask_handler(NSEventMask::KeyDown, &block) };
        std::mem::forget(monitor);
    });
}

/// Whether `Cmd`-`Q` was pressed since the last call.
pub fn take_quit_keys() -> bool {
    QUIT_KEYS.swap(false, std::sync::atomic::Ordering::SeqCst)
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
