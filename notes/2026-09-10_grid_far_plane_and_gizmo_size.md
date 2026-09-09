# The infinite grid was not infinite, and the gizmo was too big

Windows machine, 10 September. Short session on top of the 9 September axes
work.

## The grid stopped at the far plane, and the clamp was why

Noticed by eye: "the infinite horizontal plane is not infinite cus of the cam
zfar probably". Right about the far plane, and the mechanism is worth writing
down because yesterday's note claims the opposite in good faith.

`grid.wgsl` ended with

    out.depth = clamp(clip.z / clip.w, 0.0, 1.0);

and a comment saying that past the far plane this pins to the farthest depth
so bodies still occlude it. The pinning is right. What it missed is what the
farthest depth *is here*: depth is reversed, so far is `0.0`, `DEPTH_CLEAR` is
`0.0`, and `DEPTH_COMPARE` is `Greater`. A fragment clamped to exactly `0.0`
therefore fails `0.0 > 0.0` **against the cleared background** — no geometry
involved. The grid was being thrown away by the depth test at precisely the
far plane, which is indistinguishable from being clipped to it.

The fix is one character's worth of epsilon:

    out.depth = clamp(clip.z / clip.w, 1e-7, 1.0);

Real geometry sits far above that floor — reversed depth at the far plane is
`near/far`, order 1e-3 for the ratios `fit_projection` produces — so a body
still occludes the ground everywhere it should. Verified: the cube's
silhouette and its shadow still cut the grid correctly, and the grid now runs
to every edge of the frame.

### Why the obliquity fade was the wrong suspect

The first hypothesis was `grid_fade_near`, which defaults to `0.5` and so
starts fading 30 degrees off horizontal — aggressive enough to look like the
cause. Setting it to `0.9` produced a **pixel-identical** frame, which is what
ruled it out. Worth keeping as a method: the fade and the depth test both
bound the grid from the same direction, and only re-rendering separates them.

The obliquity fade is still correct and still needed; it is just not what was
cutting the plane.

## The gizmo, smaller and top-right

Asked for directly. `gizmo_size` 54 -> 40, the ball ratio 0.26 -> 0.22 (so
balls went 14.0 px -> 8.8 px radius), `gizmo_label_size` 13 -> 9 so the letters
still sit inside a ball, and `gizmo_anchor` `TopLeft` -> `TopRight`.

Hit-testing was the thing at risk, since the target is `ball_radius + 3`, now
11.8 px. Checked rather than assumed, with yesterday's posted-message
technique: a click at the computed `+Z` ball takes the camera from
`(-0.562, 0.723, -0.402)` to `(0, 0, -1)`.

## `maturin develop` no longer fails with `os error 32`

The standing Windows annoyance — "the process cannot access the file because
it is being used by another process" when copying to `kalast/_rs.pyd`. It does
not happen on macOS because unlinking a mapped file is allowed there; the old
inode stays with whoever has it open and the new file takes the name. Windows
holds a mandatory lock on a mapped image and refuses the write.

The holder is usually **not** a kalast window. Here it was VS Code's Jedi
language server, which imports the package for completions and then keeps it
mapped for the rest of the editor session — so this is the normal state, not
an accident, and "close everything and retry" was never a fix.

Windows blocks writing a mapped image but still permits **renaming** it: the
lock is on the contents, not on the directory entry. So `tools/develop.py`
moves `_rs.pyd` aside to `_rs.pyd.old-<stamp>` before running maturin, which
frees the name; whoever had it open keeps running against the file they
already had, and the next process to start gets the new build. Stale
`.old-*` files are swept on the next run, once nothing holds them, and are
gitignored. If the build fails and leaves no module, the old one is moved back
rather than leaving a broken import.

    python tools/develop.py [--release]

Rejected: killing the holder. It is the editor's language server, it respawns
immediately, and killing processes by image name is exactly what this repo's
own rules say not to do.

## Open

- The epsilon floor is right for the ratios `fit_projection` produces. A scene
  contrived to need `far/near > 1e7` would put real geometry underneath the
  floor and let the ground draw over a body at the far plane. Not reachable
  with the fitted planes today; worth remembering if the fit ever changes.
- The gizmo is still burned into exported frames, and still unclicked inside
  the editor's letterboxed viewport. Both carried over from yesterday.
