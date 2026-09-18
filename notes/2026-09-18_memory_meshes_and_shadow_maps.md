# Memory: what a mesh costs, what the shadow maps cost

Prompted by kalast going out of memory on a 16 GB machine with 13 GB already
in use, running `examples/didymos/main.py` with the full-resolution models.
The question was how much the two meshes take at 10k and at 3M facets. The
answer has two parts, and the larger one at small mesh sizes was not the
meshes.

## Measured

This Mac (32 GB, Apple Silicon, release build), a 1020×1020 window, ten frames
drawn, Didymos + Dimorphos at the geometry of the example. *RSS* is the
process's resident memory; *footprint* is macOS's `phys_footprint`, which is
what it accounts and kills on and which includes the GPU allocations -- on a
PC those are VRAM on a discrete card or system RAM on an integrated GPU.

Before this note's change, with the shadow array at its cap of eight layers:

| meshes | RSS after loading | RSS after the first frame | footprint @ 8192 |
|---|---|---|---|
| 10k + 10k | 0.13 GB | 0.22 GB | **2.39 GB** |
| 100k + 100k | 0.21 GB | 0.37 GB | **2.41 GB** |
| 3M + 3M | 2.9 GB | **5.3 GB** | **6.8–7.0 GB** |

The shadow array alone, isolated by sweeping `shadows.resolution` on the 10k
pair: footprint 0.35 GB at 2048, 0.76 GB at 4096, 2.39 GB at 8192 --
`resolution² × 4 bytes × 8 layers`, 2.1 GB at the default, before a single
mesh was loaded.

After allocating the array at the body count (two layers here):

| meshes | footprint @ 8192 | @ 4096 |
|---|---|---|
| 10k + 10k | **0.87 GB** | 0.37 GB |
| 100k + 100k | **1.0 GB** | -- |
| 3M + 3M | 7.0 GB | 7.0 GB |

The 3M row did not move, and honesty requires saying why: in those runs the
shadow array never appeared in the footprint, before or after -- the total is
RSS plus the mesh buffers (`IOAccelerator` 1.74 GB), with no 2 GB left over
for it. Metal evidently did not commit the texture's pages in that
configuration, while in the small-mesh runs it plainly did. A discrete GPU's
driver allocates the full texture in VRAM at creation whatever the mesh, so
the saving is real there in every case; on this machine it is real for the
cases that matter for interactive work. The 3M peak of 5.3 GB RSS is the
meshes and is untouched by anything about shadows.

## Where the mesh memory goes

The default build is `f32` (`use_f64` is an opt-in feature that doubles every
figure below). A `Vertex` -- position, texture, normal, tangent, bitangent,
colour, two `u32` -- is 76 bytes, and a 3,145,728-facet model loaded flat is
9.4 M of them: 0.68 GB. But each load added 1.3–1.5 GB of RSS, so the vertex
array is only half of it. Measured on the Didymos model with a bare
`kalast.mesh.Mesh`, no window:

| step | RSS added | what it holds |
|---|---|---|
| parse the OBJ (shared, 1.57 M vertices) | **+856 MB** | the shared mesh is ~235 MB (vertices 114, facets 84, indices 36); the other ~620 MB is what parsing a 163 MB text file leaves behind in the allocator |
| `flatten()` | **+682 MB** | the 9.4 M-vertex flat array, exactly; the shared vertices are kept for `smoothen()` |

Then the first frame builds the GPU copy, 72 bytes per vertex, 650 MB per
mesh, and the upload's staging shows up as RSS as well: +2.3 GB for the pair.
So a full-resolution pair is ~5.3 GB of RAM before shadows, and that alone is
more than the 3 GB that machine had free. The 100k models are what
interactive work wants; the full models are for data products.

The parse residue is the one piece that is not spoken for. It is an OBJ
reader's working set -- the file as a string, the tokens, the intermediate
vectors -- freed but not returned to the OS, and it is the same size whether
the mesh is then flattened or not. A streaming parse, or a binary cache of
the flattened mesh, would take it away; neither is done.

## What changed

`shadow_layers_wanted`: the array is allocated at one layer per body (one in
all with `shadows.per_body` off), up to the cap of eight, and **grown** when a
body arrives after the window exists -- the same reallocation a live change
of `shadows.resolution` already did. Never shrunk: a Restart empties the scene
for a frame, and shrinking then would pay the rebuild twice; the session's
high-water mark is the cost, which for any one scene is its body count.

Fixed on the way, because the smaller array would have turned it from wrong
into a crash: `facet_shadow` queried body *i* against layer *i* regardless of
`per_body`. With the per-body fit off only layer 0 is rendered, so body 1's
occlusion was being read from a layer nothing had drawn into -- silently, and
it feeds the TPM. With a one-layer array the same code would have bound the
array view to a 2D binding. The query now takes the layer the body was fitted
into, `min(body, n_layers − 1)`, which is the clamp the fragment shader has
always applied.

`tests/test_shadow_layers.py`: one sphere, a frame, a second sphere added
after the window exists, and the frame after that still renders both while
the first gains the second's shadow through the new layer.

## Rules of thumb

- Shadow maps: `resolution² × 4 bytes` per body -- 268 MB at 8192, 67 MB at
  4096, 17 MB at 2048. On a machine that is short of memory, 4096 first.
- Meshes: 76 bytes per vertex on the CPU and 72 on the GPU in the default
  `f32` build, three vertices per facet when flat -- and, as measured, about
  as much again on the CPU for the OBJ parse's residue and the shared copy.
  Call it **0.5 GB of RAM and 0.2 GB of VRAM per million facets**. Decimate
  for the editor; keep the full model for the product.
