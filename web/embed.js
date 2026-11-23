// kalast shown inside another page of greghy.com's, or of the visitor's own
// machine: that page sends a mesh, which kalast opens as its files tab opens
// an .obj. Messages from anywhere else are not listened to.
//
//   kalast -> page, once started:   {kalast: "ready"}
//   page -> kalast:                 {kalast: "open", name: "<file>.obj", bytes: ArrayBuffer}
//   kalast -> page:                 {kalast: "opened", name, path}, or {kalast: "error", name, message}
//   kalast -> page, if it cannot start (no WebGPU, ...): {kalast: "unavailable", reason}  (index.html)
//   page -> kalast, a pinch over the frame: {tiri: "wheel", deltaX, deltaY, deltaMode, ctrlKey: true, x, y}
//     (the ctrl+wheel event the page got, as it got it; from the parent page only)
//
// The file goes into the page's files, sent/<name>, for this visit only.

const LARGEST = 256 * 1024 * 1024; // bytes

function allowed(origin) {
  return /^https:\/\/([a-z0-9-]+\.)*greghy\.com$/.test(origin) || /^http:\/\/(localhost|127\.0\.0\.1)(:\d+)?$/.test(origin);
}

// A pinch over this frame: Chrome gives it to the page around it, as ctrl+wheel events, and the TIRI site's sends them
// on. Their deltaY is -100 ln of the step's scale (Chrome's), in pixels: kalast takes the step as the desktop's
// trackpad pinch. Other modes are no pinch's.
function pinched(kalast, d) {
  if (d.ctrlKey !== true || d.deltaMode !== 0 || !Number.isFinite(d.deltaY)) return;
  kalast.pinch(Math.expm1(-d.deltaY / 100));
}

export function install(kalast) {
  if (window.parent === window) return;
  // The canvas takes the focus as the pointer comes over it, and when the page around gives this frame the focus (its
  // iframe.focus()), as a click on it would: keys reach kalast without a click, as on a page of its own, where nothing
  // else holds the focus (the user: "kalast tab i have to click to focus"). A field of the page around loses it then.
  // Chrome leaves no element of the frame focused once the page around has had the focus.
  const focus = () => {
    const canvas = document.querySelector("canvas"), active = document.activeElement;
    if (!canvas || (active === canvas && document.hasFocus())) return;
    if (!active || active === document.body || active === canvas) canvas.focus({ preventScroll: true });
  };
  window.addEventListener("pointermove", focus, { capture: true, passive: true });
  window.addEventListener("focus", focus);
  window.addEventListener("message", (e) => {
    if (!allowed(e.origin)) return;
    if (e.data?.tiri === "wheel" && e.source === window.parent) return pinched(kalast, e.data);
    if (e.data?.kalast !== "open") return;
    const { name, bytes } = e.data;
    const reply = (message) => e.source?.postMessage(message, e.origin);
    try {
      if (typeof name !== "string" || !(bytes instanceof ArrayBuffer)) {
        throw new Error("expected a name and the file's bytes, an ArrayBuffer");
      }
      if (bytes.byteLength > LARGEST) throw new Error(`${bytes.byteLength} bytes, more than the ${LARGEST} taken`);
      reply({ kalast: "opened", name, path: kalast.open_sent(name, new Uint8Array(bytes)) });
    } catch (err) {
      reply({ kalast: "error", name: String(name), message: String(err?.message ?? err) });
    }
  });
  window.parent.postMessage({ kalast: "ready" }, "*");
}
