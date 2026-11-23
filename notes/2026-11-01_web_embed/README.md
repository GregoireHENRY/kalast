# kalast.greghy.com in a frame: a mesh sent by the page that shows it

The user asked for a page of their site to show kalast.greghy.com itself, in a frame, and open its own meshes in it:
"the kalast page now should show the actual kalast.greghy.com page to see the shape files inside kalast app instead.
Merge the 3d shape button to kalast page to load mesh in kalast".

The browser does not let a page reach into a frame from another origin, nor the frame use the visitor's folder (the
File System Access picker is refused in a cross-origin frame), so the page sends the mesh's bytes by `postMessage`, and
kalast opens it as its files tab opens an `.obj`: a clean scene, the mesh framed, held at iteration 0
(`Editor::send_to_renderer`, then `App::open_mesh`), the log saying `showing sent/<name>`.

## The messages

| from | message | when |
|---|---|---|
| kalast | `{kalast: "ready"}` | once started, inside a frame; carries nothing |
| the page | `{kalast: "open", name: "<file>.obj", bytes: ArrayBuffer}` | the OBJ file as it is, the buffer transferred |
| kalast | `{kalast: "opened", name, path}` | taken, and sent to the renderer: `path` is `sent/<file>.obj` |
| kalast | `{kalast: "error", name, message}` | refused: the name, the bytes, the size |
| kalast | `{kalast: "unavailable", reason}` | it cannot start, inside a frame: no WebGPU, or the error |
| the page | `{tiri: "wheel", deltaX, deltaY, deltaMode, ctrlKey: true, x, y}` | since 3 November: a pinch over the frame, which Chrome gives the page around; taken as the desktop's pinch (`notes/2026-11-03_web_pinch/`) |

- Only from pages of greghy.com over https (`https://greghy.com`, `https://<sub>.greghy.com`) and of the visitor's own
  machine (`http://localhost[:port]`, `http://127.0.0.1[:port]`); a message from anywhere else is not answered. Replies
  go to the sender's origin only. `ready` and `unavailable` go to any parent: they carry nothing of the visitor's.
- The name: its last part only, ending in `.obj`, letters, digits, spaces and `._-`, at most 128 characters; the file at
  most 256 MB.
- The file goes into the page's files, `sent/<name>`, for the visit only: `kalast::fs::add` is not a change the page
  keeps, so it reaches neither the browser's storage nor a folder of the visitor's. Python's files have it too, for a
  script that wants it.

## Where

- `web/embed.js`: the listener, the origins, the size; installed by `web/index.html` once kalast has started, which also
  says `unavailable`.
- `web/src/lib.rs`, `open_sent`: the name's check, the file put in, the mesh handed on.
- `kalast::app::gui::send_mesh` and `App::page_tick`: the meshes queued and sent to the renderer between frames, as the
  folder's changed files are. In the browser only (`wasm32`): the desktop app is unchanged.

## Checks

In headless Chrome with WebGPU, a local page framing first the build (`web/dist`, served locally) and then the deployed
site: `ready`; a 1.8 MB OBJ sent under a name with `../` in front, `opened` as `sent/<name>` and drawn, the log saying
`showing sent/...`; a `.py` refused with `error`; from a page on `http://[::1]:8767`, outside the origins, `ready` and
then no answer, nothing opened.

## Embedded defaults (`?embed=`)

The user, relayed by work-edds, of the page in their site's Kalast tab: "hide top and bottom panel by default on web
page kalast", "turn on ambiant lighting a bit a put the sun in a better place and wireframe should be off by
default", and a dark theme there only: "default kalast web page outside of tiri page should still use defaut theme
catpuccin". The site frames `kalast.greghy.com/?embed=<who>`; with `embed` in its address the page:

- folds the toolbar and the log (`toolbar_folded`, `log_folded`): the scene the whole frame but the side panel;
  `↑`, `↓` or the folded panel's edge bring them back, as in the UI app. **Shown again since 3 November** (the user:
  "kalast tab show top and bottom panel too"): `notes/2026-11-03_web_embed_focus/`;
- draws in egui's dark theme, set after the remembered settings, and remembers nothing changed there
  (`settings::save` returns), since the frame shares kalast.greghy.com's storage: the page itself keeps the visitor's
  theme, Catppuccin's Mocha by default;
- dresses a mesh it opens to be looked at (`Simulation::dress_for_viewing`, after `frame_all`): wireframe off,
  ambient 0.08, exposure 0.55 so that lit ground is grey rather than white, and the Sun over the viewer's left
  shoulder, 50 degrees left of the camera and 35 above it, a phase of 58 degrees. Tried on the desktop renderer
  beside exposure 1.0 at a phase of 41 degrees, where most of what is lit came out white and the relief with it.

## Freezes

The user: "kalast web page embeded has many freeze that you never see in local app". work-edds measured none in
kalast itself, framed or not, and found the cause in the frame's process: kalast.greghy.com and their site are one
site to the browser (greghy.com), so Chrome runs the frame on the embedding page's main thread, and a busy loop
there stopped kalast's frames for its whole length. Their page's own work is lessened on their side. Here every
response carries `Origin-Agent-Cluster: ?1` (`web/_headers`): the page asks for an agent cluster of its own, which
lets the browser give the frame its own process. Neither page reached into the other, so nothing is lost by it.
Opening the 1.8 MB mesh framed, with this build, in headless Chrome: no frame of kalast's over 50 ms apart.

