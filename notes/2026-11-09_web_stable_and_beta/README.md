# 2026-11-09 — kalast.greghy.com: the latest release, and the beta behind "offer betas"

The user (9 November):

- "can you make the web use the latest version non beta like the latest releases?"
- "but clicking on "offer betas" in the settings would show the latest beta build, so the live version we are working
  on"
- "and can you show me how to run local version on web to see how it looks before pushing beta"

## The site

| address | page |
|---|---|
| `kalast.greghy.com/` | the stable page: the newest release's |
| `kalast.greghy.com/beta/` | the beta: main's, as it is deployed |

`web/build.sh` builds both into `web/dist/`:

- **The beta**, from the working tree, as the page was built before.
- **The stable page**, from the newest release tag whose tree has `web/`. It is built in a git worktree of the tag,
  by the tag's own `build.sh --page`, once, and kept in `web/target/pages/<tag>/`.
  - With this tree's `Cargo.lock` copied in. git keeps no `Cargo.lock` here (`.gitignore`), and without one the first
    attempt resolved newer dependencies than the beta's (egui-wgpu 0.36.2 against 0.36.1).
  - In its own target folder, `web/target/stable/`: the first build compiles everything, about 4 minutes.
- **`_headers`**, written for the two pages' folders: both pages asked for again each visit, the content-named folders
  kept for a year. A pattern such as `/:version/*` would also match `/beta/index.html`, so the build names the two
  folders.

`./build.sh --page DIR` builds the working tree's page alone, into `DIR`. A page built alone switches nothing.

## No release has the page yet

- v0.5.14 (14 October) and v0.5.15-beta (23 October) predate `web/`, which came on 30 October (ecf6561). 0.5.15 will
  be the first release with a page.
- Until then the stable page is ded8145's: the page as it was when the beta was split from it, with everything it
  had that day. From 0.5.15's tag on, `build.sh` builds the release's page by itself.
- So, from now on, what is deployed reaches `/beta/` only. The stable page, and the TIRI site's Kalast tab, which
  shows `/` in a frame, change with releases.

## "offer betas" in the browser

- **On arrival.** `index.html` reads the remembered settings before it loads kalast: `update_betas = true` in
  `.kalast/settings.toml`, which the page keeps in the browser's storage (`kalast:file:.kalast/settings.toml`). The
  stable page sends a visitor with it ticked to `beta/`, the address's query (`?embed=…`) and file (`#examples/…`)
  kept.
- **When ticked or unticked.** The page is told each time the app keeps its settings, which is when one is changed in
  the app (`on_settings_kept`, `web/src/lib.rs`). Ticked on the stable page, the beta opens; unticked on the beta, the
  stable page opens.
- **A link to `/beta/`** shows the beta whatever the setting.
- **Shared.** The two pages are one site to the browser: the scripts kept, the settings and the visitor's folder are
  the same on both, as the desktop's settings are the same across its versions.
- **In a frame.** A page shown inside another keeps no settings, so it never switches itself. Its storage is
  kalast.greghy.com's (greghy.com is the same site), so it opens as the beta for a visitor who ticked the box on
  kalast.greghy.com.

The option's documentation says so (`docs/CONFIG.md`, `update_betas`), as do the README and the changelog.

### Which page the log says

Both pages printed `kalast v0.5.15 started`: until 0.5.15 is released both are 0.5.15, the stable page frozen and the
beta following main. The user, told so: "yes add beta to the log line". The beta's kalast tab now opens on `kalast
v0.5.15 beta (built 2026-11-09) started`:

- `build.sh` writes the page's channel and the day it is built (UTC) on its `<html>`: `data-channel`, `data-built`.
  `index.html`'s script reads the channel there too.
- `web/src/lib.rs` hands "beta (built <day>)" to `update::set_page_build` before the UI app starts, and
  `App::editor_start` writes `update::this_build()`.
- The desktop's line is unchanged: a desktop build learns that it is a beta only from GitHub, whose answer the log
  gives after (`update::message`).
- The stable page, ded8145's, still says `kalast v0.5.15 started`.

## Looking at the local version before it goes up

```sh
web/build.sh
python3 -m http.server 8777 --bind 127.0.0.1 --directory web/dist
```

Then `http://127.0.0.1:8777/beta/` is the working tree's page, and `http://127.0.0.1:8777/` the stable one. These are
the files `npx wrangler deploy` (from `web/`) sends. The browser's storage at that address is its own, so the box
starts unticked there.

## Checks

On a copy of the site served locally, the same page as both, in headless Chrome:

- a first visit stays on the stable page;
- ticking the box opens `beta/`, the file in the address kept, and the settings say `update_betas = true`;
- with it ticked, `/` opens `beta/`, and so does `/?embed=test#examples/sphere/tpm.py`, both kept;
- unticking it on the beta opens `/`;
- `/beta/` opened directly with it unticked stays.

The site as built, served locally under a sub-path (`/dist/`, so the relative addresses are tried too): both pages
ran `sphere/tpm.py`'s top level in Python, and the box switched page both ways.

Live, after the deploy:

- `/` and `/beta/` answer with `no-cache`, `/beta` sends to `/beta/`, and the two pages' folders are kept a year;
- in headless Chrome, from a new profile: the stable page ran `sphere/tpm.py`'s top level; ticking the box opened
  `/beta/#examples/sphere/tpm.py`, which ran it too; `kalast.greghy.com` opened again went to `/beta/`; unticking the
  box there opened `/`;
- a page on 127.0.0.1 showing `kalast.greghy.com/?embed=test` in a frame got `{kalast: "ready"}` from it, 0.8 s after
  the frame's request.

Deployed: the stable page 39a9be93c5e8 (from ded8145), the beta 8727bd55fecd. The page before, 14650500d87a, is gone:
a tab opened before the deploy needs a reload.

## Open

- The stable page is built on this machine, from the tag. The release workflow could build it and attach it to the
  release instead.
