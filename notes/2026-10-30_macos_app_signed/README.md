# 2026-10-30 — kalast.app signed (ad hoc), for macOS 26.5

A user, through the user (30 October): v0.5.14's `kalast.app` on an M5 Mac with macOS Tahoe 26.5.1 would not open.
macOS said "Vérifiez auprès du développeur que kalast fonctionne avec cette version de macOS. Il se peut que vous soyez
amené à réinstaller l'application. [...]". That is the dialog for an app that dies while it is being launched.

## Progress

- [x] v0.5.14's arm64 bundle checked here (macOS 26.4.1, M-series):
  - The executable `kalast` is ad hoc and linker-signed, and its signature is valid.
  - 225 native files, none invalid. Six are unsigned (below).
  - `kalast.app/Contents/MacOS/kalast --python-check` runs ("embedded interpreter OK: kalast 0.5.14").
- [x] `kalast.app` itself is "not signed at all": a shell-script launcher, with nothing sealing it.
  - Two reports describe macOS 26.5 refusing exactly that kind of app on Apple Silicon:
    - an unsealed bundle refused on 26.5.1 and later ([anthropics/claude-code#70647](https://github.com/anthropics/claude-code/issues/70647));
    - an unsigned shell-script launcher app killed at launch, fixed by ad hoc signing
      ([odysseus-dev/odysseus#3815](https://github.com/odysseus-dev/odysseus/issues/3815)).
  - Not reproduced here: this Mac has 26.4.1.
- [x] `tools/macos_app.py` signs the app it makes, ad hoc (`codesign --force --sign -`, no certificate).
  - `release.yml` checks the seal (`codesign --verify --deep --strict`) before the launcher's `--python-check`.
  - The seal is files in `Contents/_CodeSignature`, not extended attributes. So it goes into the tar archive and
    survives the README's `xattr -cr`: checked on a sealed app packed and unpacked here.
- [ ] Confirmation from the user on 26.5.1. A workaround to run first, then double-click `kalast.app`:
  `xattr -cr <folder>` then `codesign --force --deep -s - <folder>/kalast.app`.
  - If it still fails, the crash report says why: the dialog's "Signaler…" button, or Console's crash reports.
  - `./kalast` from a terminal in the folder bypasses the app altogether.
- [ ] A CHANGELOG line once confirmed.
- [ ] Six fontTools extensions in `python/lib/python3.14/site-packages/fontTools/` are not signed at all: `feaLib/lexer`,
      `qu2cu/qu2cu`, `misc/bezierTools`, `cu2qu/cu2qu`, `varLib/iup`, `pens/momentsPen`.
  - On Apple Silicon a process loading unsigned code is killed. This could hit a script whose matplotlib reaches for
    fontTools (font subsetting in PDF or PS output).
  - Not reached at start-up, so not this report.
