# Handoff — one of three release failures fixed, and the shadow proxy measured

Windows machine, 18 September, continuing the same day's Mac handoff
(`2026-09-18_HANDOFF_release_v0.5.0.md`). Everything below is committed and
pushed. Suite green: **110 Rust tests, 19 Python files**.

## The release: one down, two to go

### ✅ `wheel windows-x86_64` — fixed and verified

Reproduced on this machine, which is the whole diagnosis:

```
maturin build --release --out dist
💥 maturin failed
  Caused by: Need a Python interpreter to compile for Windows without
             PyO3's `generate-import-lib` feature
```

Windows links against a Python import library rather than resolving symbols at
load time, and without that feature pyo3 needs a real interpreter to derive
one from. **`maturin develop` always has one — the active venv — so nothing in
the daily loop exercises the gap.** `maturin build` in the release workflow is
the first thing that does, which is why this surfaced at a tag rather than in
months of development.

`generate-import-lib` added to pyo3's features in `Cargo.toml`. The same
command now produces `kalast-0.5.0-cp314-abi3-win_amd64.whl`. It is also what
makes an abi3 wheel cross-compilable, so it is the right answer rather than
pinning an interpreter into the job.

### ⏳ `publish to crates.io` — needs you, and it is not the versions

I checked the hypothesis that a version mismatch caused it. **It did not.**
`Cargo.toml` and `macros/Cargo.toml` are both `0.5.0`, the dependency is
`kalast_macros = { path = "macros", version = "0.5" }`, and crates.io has
`0.4.1` for both crates. Nothing there is wrong.

That leaves the missing `CARGO_REGISTRY_TOKEN`, exactly as the incoming
handoff guessed. That is account setup, not code:

- crates.io → Account Settings → API Tokens → New Token, scope
  `publish-update`
- GitHub → Settings → Secrets and variables → Actions →
  `CARGO_REGISTRY_TOKEN`

### ❌ `wheel linux-x86_64` — **not diagnosed, and deliberately not guessed at**

`gh` is not authenticated on this machine, so I could not read the job log.
I did not change the `before-script-linux` on a hypothesis: the incoming
handoff was explicit that its suspicions were "hypotheses, not diagnoses", and
editing CI blind is how a one-line fix becomes three red runs.

**To unblock it**, authenticate in this terminal:

```
! gh auth login
```

then the log the previous handoff already identified:

```sh
gh run view --job=105695993789 --log     # linux wheel
```

The standing suspicion is still the manylinux container's headers —
`vulkan-headers` in particular may not exist in that image, and a CentOS-7
based image also has dead yum mirrors unless the vault repos are pinned. Both
are guesses.

Note **`wheel macos-arm64` succeeded**, so the build itself is sound; this is
environmental.

## The shadow proxy: 1.56x, and not free

The incoming handoff's other loose end — "`shadow_path` is unexplored and
probably the next win … Nobody has measured it."

**1062 → 1656 it/s**, a factor 1.56, on the Didymos pair rendered at 100k
either way with the 10k model as `shadow_path`. Repeats inside 1 %, the two
configurations interleaved.

**And it changes the illumination, which `API.md` said it did not.** That
paragraph claimed a coarser occluder buys performance "without touching
per-facet science data". The shadow map decides which fragments are lit, and
`facet_shadow` reads that same map — so a body rendered at 100k is
depth-tested against a 10k version of itself, with bias constants fitted for a
body against its own geometry.

| | |
|---|---|
| facets whose `facet_shadow` differs | 2.90 % |
| flipped by ≥ 0.5 | 0.69 % |
| shadowed fraction | 0.4651 → 0.4715, **1.4 % relative bias** |

A bias, not noise, so it does not average out over a rotation. In the image it
is speckle on the self-shadowed limb — self-shadowing acne — while the
Dimorphos-onto-Didymos shadow lands identically.

**So: good for interactive work and figures, bad for anything reading
`facet_shadow`.** `API.md` now says so.

**The fix that keeps both** is a proxy for *other* bodies only, leaving the
full mesh in each body's own layer. The shadow array is already allocated per
body (`0505957`), so the layers exist to do it, and on the Didymos pair the
mutual casters are most of the saving. Not implemented — this is the next
thing to try, and it is small.

## Open, in the order I would take them

1. **Read the Linux wheel log** and fix it. Needs `gh auth login` here, or do
   it on the Mac where `gh` works.
2. **`CARGO_REGISTRY_TOKEN`**, and register the PyPI pending publisher while
   you are there — the `pypi` job never ran, so it is untested.
3. **Re-run `gh run rerun 35374536810 --failed`.** Nothing published, so no
   version is burned and the same tag can be reused. A *code* fix means the
   tag has to move, and the Windows fix committed here is exactly that:
   `v0.5.0` currently points at `b4429b0`, which does **not** contain
   `c382f5e`. Delete and re-push the tag before re-running, or the Windows
   wheel will fail again.
4. **The per-body shadow proxy** above.
5. Unchanged from the Mac handoff: nothing reads real photometry, a binary
   still needs its geometry placed per epoch, the LS+Lambert mix has no phase
   function, the audit's remaining test backlog (radiance band integration,
   Kuehrt roughness, transient conduction), and the smaller audit items.

## Practical notes

- **The tag must move before re-running.** `v0.5.0` → `b4429b0`; the Windows
  fix is `c382f5e`, two commits later. `git tag -d v0.5.0 && git push origin
  :refs/tags/v0.5.0 && git tag -a v0.5.0 -m "v0.5.0" && git push origin
  v0.5.0`. Safe only because nothing published; once a registry has 0.5.0 the
  next attempt is 0.5.1.
- **Local data paths re-applied** after the pull: the root swap, the two
  `*_local.tm` meta-kernel names, and the `_100k` meshes, which upstream
  renamed to `g_01165mm_spc_didy_v003_100k.obj` where this machine's are
  `g_01165mm_spc_obj_didy_0000n00000_v003_decimated_100k.obj`. 48 references
  resolve; 11 do not and are trees absent from this machine per
  `local_paths.toml` — the full-resolution Didymos and Dimorphos models
  among them, so `examples/didymos/main.py` at full resolution cannot run
  here.
- `gh` is **not** authenticated on this machine and there is no `GH_TOKEN`.
