// A mouse wheel, told to kalast as the desktop app is told of it: in lines. kalast reads the device from the kind of
// scroll (app::frame's Controller::scroll): lines are a wheel's notches, which zoom; pixels a trackpad's, which orbit.
// The system tells the desktop app which it was; in a page, Chrome, Edge and Safari report a wheel in pixels too, so
// it orbited. Such a wheel event is stopped here, before winit sees it, and sent again in lines:
//   - a wheel is told from a trackpad by the legacy wheelDeltaX/Y: a wheel's count 120 a notch, a trackpad's are three
//     times its pixels in Chrome, Edge and Safari (40 pixels a tick), and those are left as they are;
//   - on macOS a wheel's pixels are the system's lines times 40, the lines the desktop app gets (NSEvent's deltaY and
//     scrollingDeltaY are one number), so it is sent again as pixels / 40 lines; elsewhere as its notches, a line
//     each, as winit gives the desktop app on Windows and Linux.
// Firefox reports a wheel in lines already. A pinch, ctrl and a wheel event, is left to winit, which zooms with it, but
// never to the browser, which would zoom the page: cancelled wherever the pointer is, kalast started or not.
// Safari tells a pinch as gesture events instead, which winit does not read, and zooms the page unless they are
// cancelled: cancelled here too, and once kalast has started (connect) given to it as the desktop's trackpad gives
// one, each event's scale over the one before (`scale` counts from the gesture's start).

const MAC = /Mac/.test(navigator.userAgentData?.platform ?? navigator.platform ?? navigator.userAgent);
let pinch = null; // kalast's, once started
let gesture = null; // Safari's gesture under way: its scale so far

// The event's lines (x, y, as deltaX and deltaY count them), or null to leave it as it is.
export function wheelLines(e, mac = MAC) {
  if (e.deltaMode !== 0 || e.ctrlKey || (e.deltaX === 0 && e.deltaY === 0)) return null;
  const wx = e.wheelDeltaX, wy = e.wheelDeltaY;
  if (typeof wx !== "number" || typeof wy !== "number") return null;
  if (wx === -3 * e.deltaX && wy === -3 * e.deltaY) return null; // a trackpad's
  return mac ? [e.deltaX / 40, e.deltaY / 40] : [-wx / 120, -wy / 120];
}

export function install() {
  window.addEventListener("wheel", (e) => {
    if (e.ctrlKey) {
      e.preventDefault(); // a pinch: kalast's zoom, never the page's
      if (gesture !== null) e.stopImmediatePropagation(); // during Safari's gesture, taken as the gesture
      return;
    }
    if (!e.isTrusted) return; // the one sent again below
    const lines = wheelLines(e);
    if (!lines) return;
    e.preventDefault();
    e.stopImmediatePropagation();
    e.target.dispatchEvent(new WheelEvent("wheel", {
      deltaX: lines[0], deltaY: lines[1], deltaMode: WheelEvent.DOM_DELTA_LINE,
      clientX: e.clientX, clientY: e.clientY, screenX: e.screenX, screenY: e.screenY,
      ctrlKey: e.ctrlKey, shiftKey: e.shiftKey, altKey: e.altKey, metaKey: e.metaKey, buttons: e.buttons,
      bubbles: true, cancelable: true, composed: true,
    }));
  }, { capture: true, passive: false });
  for (const type of ["gesturestart", "gesturechange", "gestureend"]) {
    window.addEventListener(type, (e) => {
      e.preventDefault();
      if (type === "gesturestart") gesture = 1;
      else if (gesture !== null && e.scale > 0) {
        if (e.scale !== gesture) pinch?.(e.scale / gesture - 1);
        gesture = type === "gestureend" ? null : e.scale;
      }
    }, { capture: true, passive: false });
  }
}

// kalast started: Safari's pinches go to it from now on.
export function connect(kalast) {
  pinch = (magnification) => kalast.pinch(magnification);
}
