# PCF was moving shadows, not blurring them

## The report

`examples/didymos/main.py`, iteration 12907 (2027-03-10 11:07 UTC), camera
`pos=[1.207, -0.329, 0.660] dir=[-0.853, 0.232, -0.467]`: Dimorphos casts a
shadow on Didymos that reaches the night side. At `shadows.pcf = 7` the shadow
detaches from the terminator; at 16 it is a small blob. "I'm not sure that
this is how PCF is supposed to work."

It is not. Percentage-closer filtering averages the depth comparison over a
kernel of texels; it softens an edge and must not move it. The integral of
darkness over the image and its centroid are both conserved by a blur, so a
shadow that shrinks or migrates as the kernel grows is a bias artefact
wearing a filter's clothes.

## The geometry

At that epoch the Sun–Dimorphos separation seen from Didymos is 18°, so
Dimorphos's shadow cylinder passes `1151 sin 18° ≈ 356 m` from Didymos's
centre and just clips the ~400 m limb. The shadow lands at grazing
incidence, and grazing incidence is where a normal offset does the most
damage: lift the lookup point `h` off a sphere of radius `R` and the
shadow's edge, where the shadow cylinder meets the surface near the
terminator, moves along the surface by `sqrt(2 R h)`.

`mesh_shadow.wgsl` lifted the lookup by `lb.x * (1 + pcf) * k`, one texel
diagonal scaled by the kernel radius. One texel is 1.76 m at 512 and 0.11 m at
8192 on Didymos's layer, so:

| resolution | pcf 0 | pcf 7 | pcf 16 |
|---|---|---|---|
| 512 | h 2.5 m → edge 45 m | 20 m → 127 m | 42 m → 185 m |
| 8192 | 0.16 m → 11 m | 1.2 m → 32 m | 2.6 m → 46 m |

At 512 the whole shadow is ~150 m across. That is the report.

## Why the scaling was there

`2026-09-04_shadow_fixes.md` §4 added it to cure a "grey floor" on the crater
at `pcf = 4`, and the number it quotes (7,952 → 388 px) is real -- but the
mechanism is not what that note says. The far taps of a kernel were
self-shadowing a tilted receiver because the per-tap receiver-plane
adjustment was clamped at `GRAD_MAX = 1e-4` of normalised depth, which at
8192 is about one texel's worth: beyond the first tap the adjustment could
not follow the receiver, and the tap compared the centre's depth against
the receiver's own, deeper, surface. Lifting the whole lookup `N` texels
off the surface hid that -- and moved every shadow edge to do it.

## Measured

A probe renders each scene at 1400×1000 with the shadow test off
(`shading.color_mode = 3`) and at pcf 0, 1, 2, 4, 7, 16, exports the frames,
and over the lit surface computes per pcf:

- **centroid**: shift of the darkness-weighted centroid from pcf 0, px. A blur
  conserves it; this is the erosion signature.
- **integral**: Σ(1 − v/v_noshadow), fully-dark-pixel equivalents. A blur
  conserves it too, but darkness blurred into the night side (outside the
  mask) is lost, so it is the weaker of the two.
- **leak / acne**: brightness appearing inside the pcf-0 umbra / darkening
  appearing outside it. A blur moves darkness across the edge, so the two
  are of a size; erosion gives leak ≫ acne, self-shadowing acne ≫ leak.

Variants: **current** (`lb.x (1+N) k`, `GRAD_MAX 1e-4`); **noscale** (one
texel offset, same clamp); **b2** (one texel, no clamp); **b3** (one texel,
slope ceiling tan 80°); **b4** (one texel, slope ceiling tan 85°).

Didymos at 8192 (centroid px | integral | leak/acne):

| pcf | current | noscale | b2 | b3 | b4 |
|---|---|---|---|---|---|
| 4 | 1.2 \| 80,799 \| 625/471 | 14.9 \| 123,621 \| 423/43,090 | 0.2 \| 80,682 \| 483/211 | 9.2 \| 83,547 \| 482/3,075 | 1.1 \| 80,943 \| 483/473 |
| 16 | 28.6 \| 88,400 \| 2,520/9,966 | 36.3 \| 167,107 \| 1,518/87,670 | 2.1 \| 80,016 \| 2,009/1,072 | 23.9 \| 88,238 \| 1,984/9,267 | 3.3 \| 80,840 \| 2,006/1,892 |

Didymos at 512:

| pcf | current | b2 | b4 |
|---|---|---|---|
| 7 | 48.4 \| 69,067 \| 19,774/10,799 | 2.9 \| 69,040 \| 13,658/4,656 | 2.9 \| 69,040 \| 13,658/4,657 |
| 16 | 102.9 \| 51,936 \| 42,965/16,859 | 5.4 \| 55,539 \| 32,387/9,885 | 6.6 \| 54,911 \| 32,613/9,482 |

Crater (`res/plane_crater_1024-5000_h=0.437.obj`, Sun at 33° elevation):

| | pcf | current | noscale | b2 | b4 |
|---|---|---|---|---|---|
| 8192 | 4 | 2.1 \| 29,317 \| 195/520 | 42.4 \| 46,681 \| 191/17,881 | 0.6 \| 29,073 \| 210/291 | 0.5 \| 29,076 \| 192/276 |
| 8192 | 16 | 43.7 \| 37,846 \| 753/9,607 | 78.2 \| 66,254 \| 716/37,978 | 4.9 \| 29,546 \| 711/1,265 | 4.7 \| 29,510 \| 712/1,230 |
| 1024 | 4 | 15.8 \| 30,105 \| 1,323/2,879 | 60.2 \| 52,111 \| 1,082/24,644 | 2.5 \| 28,184 \| 1,201/836 | 2.4 \| 28,256 \| 1,072/779 |
| 1024 | 16 | 61.2 \| 35,964 \| 5,139/12,553 | 91.2 \| 64,577 \| 4,035/40,062 | 28.2 \| 30,436 \| 4,369/6,256 | 27.8 \| 31,297 \| 3,482/6,230 |

Reading it:

- **current** moves the centroid by 29–103 px and its integral is wrong in
  both directions: eroded at 512 (leak ≫ acne), inflated by false darkening
  at 8192 and pcf ≥ 7 (acne ≫ leak) -- the clamped receiver-plane term gives
  out and the lift is no longer enough either.
- **noscale** confirms the scaling was load-bearing: acne explodes (43,090 px
  at pcf 4). Dropping it alone is not the fix.
- **b2** is a filter: centroid within 2–5 px everywhere, integral conserved,
  leak ≈ acne. Its risk is theoretical here but real: at a terminator the
  planar extrapolation is unbounded, and a close occluder -- a boulder's
  shadow at sunset -- would flip.
- **b3** shows the cost of bounding it too tightly: on a sphere most of the
  acne-prone surface is the band just short of the terminator, and tan 80°
  hands it back (211 → 3,075 at pcf 4).
- **b4** keeps b2's numbers to within a few hundred pixels and bounds the
  extrapolation. Chosen.

At 512 the 33-texel kernel of `pcf = 16` is a third of the shadow's width, so
the integral falls in every variant -- that is a blur wider than the feature,
plus darkness blurred into the night side where the metric cannot see it.
The centroid tells the two apart: 103 px against 6.

## What changed

- `normal_offset = lb.x * k`: one texel diagonal, any kernel.
- The receiver-plane gradient is capped as a **slope**, `tan 85°` in
  texel units, instead of the adjustment being capped at a fixed depth. A
  slope cap scales with the tap's distance, so a wall stays a wall out to the
  kernel's edge; it bites only where the planar assumption is already false.
- `layer_bias.w` now carries the layer's texel depth, which the slope cap is
  written in. `ShadowFit` gained `texel_depth`; it is geometry, never pinned.
- `shadows.pcf = 0` is bit-identical to before: the single-tap path uses
  neither term.

## Guard

`tests/test_pcf_filters.py` renders the crater at 1024 and checks pcf 4
against pcf 0 by the centroid (≤ 0.5 % of the image width; the old shader
moved it 1.1 %) and the integral (within 5 %). It decodes the exported PNGs
itself so as to add no dependency.
