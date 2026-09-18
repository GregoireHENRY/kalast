# The shadow proxy: 1.56x, and it is not free

`2026-09-18_HANDOFF_release_v0.5.0.md` closed with "`shadow_path` is
unexplored and probably the next win … Nobody has measured it." Measured.

It is a win, and it has a cost that `API.md` said it did not have.

## The speedup

Windows, RTX 5080, release build, `vsync` off, 1020×1020, `shadows.resolution
= 8192`. Didymos + Dimorphos **rendered at 100k each** either way; the proxy
run passes the 10k model as `shadow_path`, so the shadow casters drop 10x
while the rendered geometry is identical. Four repeats a run, first discarded,
runs interleaved:

| | it/s |
|---|---|
| no proxy | 1061.4, 1062.6 |
| 10k proxy | 1653.1, 1659.8 |

**1062 → 1656 it/s, a factor 1.56.** The repeats sit inside 1 %, and
interleaving the two configurations rules out drift.

The full models are not on this machine, so this is a 10x cut in casters
rather than the ~30x a 100k proxy against 3M would give. The ratio is what
transfers; the absolute rate does not.

## The cost, which is the part that was not known

`API.md` said a coarser occluder "buys performance without touching per-facet
science data". The first half is right. The second is not, and the reason is
in the sentence before it: *the shadow map only decides which fragments are
lit* — and `facet_shadow` reads that same map, which is what the
thermophysical model runs on.

A body rendered at 100k is depth-tested against a **10k version of itself**.
The bias constants are fitted for a body against its own geometry, so where
the two surfaces disagree the test does too.

**In the image**: 8.68 % of body pixels differ, 0.10 % by more than 24/255.
The difference is not a displaced or missing shadow — it is *speckle scattered
across the self-shadowed limb*, which is self-shadowing acne, not a mutual
shadow landing in the wrong place. The Dimorphos-onto-Didymos shadow itself is
in the same place in both.

**In `facet_shadow`**, which is what matters:

| | |
|---|---|
| facets differing at all | 2,905 of 100,000 (2.90 %) |
| facets flipped by ≥ 0.5 | **686** (0.69 %) |
| mean \|difference\| | 0.0096 |
| shadowed fraction | 0.4651 full → **0.4715** proxy |

So the proxy biases the illuminated fraction by **1.4 % relative**, toward
shadowed, and flips two thirds of a percent of facets outright.

## What that means for using it

The two uses pull apart cleanly:

- **Interactive work and figures**: take it. 1.56x for speckle on the
  self-shadowed limb is a good trade when you are turning a body around to
  look at it, and the mutual shadow — the thing you are usually looking at —
  is unaffected.
- **Anything reading `facet_shadow`**: do not, or measure first. A 1.4 %
  bias in the illuminated fraction goes straight into the surface energy
  balance, and `2026-09-02_HANDOFF_tiri_deimos.md` records prerolling with
  `facet_shadow` moving the 10th percentile of the visible surface from 150 K
  to ~104 K. This is smaller than that, but it is the same quantity and it is
  a bias rather than noise: it does not average out over a rotation.

**The obvious fix is a proxy for *other* bodies only** — use the full mesh in
a body's own shadow layer and the proxy in everyone else's. The shadow array
is already allocated per body (`0505957`), so the layers exist to do it. That
would keep the mutual-shadow saving, which on the Didymos pair is most of the
casters, and lose the self-shadow acne entirely. Not implemented; it is the
next thing to try here.

## How it was measured

`shadow_proxy_bench.py` for the rate, `shadow_proxy_image.py` for the pixels
and `proxy_facet_shadow.py` for the facet fractions, all in the session
scratchpad rather than the repo, per CLAUDE.md. The image comparison is what
turned a clean speedup into a qualified one: the rate alone said 1.56x and
nothing else, and *"faster" was the whole of what the handoff asked for*.
Exporting a frame each way took ten minutes and changed the recommendation.
