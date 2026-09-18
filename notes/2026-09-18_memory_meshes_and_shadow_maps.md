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

And after the streamed read and the sliced upload as well, the full pair at
8192: RSS 1.61 GB after Didymos, 2.87 after Dimorphos, **4.72 GB after the
first frame** (was 5.33), peak 4.74 (was 5.35); footprint 6.07 GB.

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

The parse residue is a reader's working set, freed but not returned to the
OS, and reusable: the peak during the parse equals the RSS after it, `del`
of the mesh gives nothing back, and a second parse adds only 354 MB because
it lands in the first one's pages. Read against `Mesh::load` and tobj 4.0.5
it is: the whole 163 MB file read into a `String` and held for the length of
the parse; tobj's list of every face as `Face::Triangle(VertexIndices × 3)`,
~80 bytes a face, ~250 MB; the `single_index` de-duplication map, ~100 MB;
tobj's output vectors, ~57 MB. The first of those was kalast's choice and
is gone -- the file is streamed through a `BufReader` now -- and the parse
peak fell from 973 to 812 MB. The rest is tobj's design.

The first frame's transient was the other avoidable piece: the GPU copies
were built whole (`extract_geometry`, `extract_attribs`: 56 + 32 bytes a
vertex, 830 MB per mesh) and then staged whole again by
`create_buffer_init`. They are written in slices of 2¹⁸ vertices now, so the
transient is one slice, 23 MB. For the pair the first frame added 1.85 GB of
RSS instead of 2.36; what remains is the vertex buffers themselves, which
on unified memory are RSS and on a discrete card are VRAM.

What `flatten()` keeps for `smoothen()` is small: the shared vertices (114
MB for this model) and the shared indices (36 MB), 150 MB against the 682 MB
flat array, which is the flattening itself. The loader no longer keeps it --
see the next section -- and an explicit `Mesh.flatten()` still does.

## Loading flat, directly

The decision: flat is the default and is built as flat; smooth is asked for
and built as smooth; neither is made from the other. `Mesh::load_flat`
reads the file, parses the plain `v`/`f` triangle format shape models use
**in parallel over the bytes** (chunks cut at newlines, one thread each, the
face indices checked against the vertex count once joined), drops the text,
and builds the flat vertices and the facets **in parallel** in one pass --
each corner takes its facet's normal, the centre, normal and area computed
alongside, bit-for-bit what `flatten` and `compute_facets` produce, which a
Rust test asserts on the three bundled meshes. Nothing is kept to go back to;
`smoothen()` on such a mesh returns `false`, and the Python binding warns.
Anything the format allows beyond that -- texture coordinates, normals,
materials, several objects, indices with slashes or negative, polygons past
an octagon -- hands over to tobj and the old `load` + `flatten`, minus the
kept copy, so nothing changes for those files.

One trap on the way: `is_flat()` was *defined* as "the kept shared copy is
non-empty". Dropping the copy would have turned every loaded mesh into an
indexed one for the GPU, silently. It is an explicit `flat` flag now.

Measured on the 3M-facet Didymos model, 8 cores:

| | before | after |
|---|---|---|
| `load_mesh` (default, flat) | 1.01 s (0.85 of it tobj, one core) | **0.20 s** |
| RSS per loaded mesh | ~1.5 GB | **~0.95 GB** (0.80 GB is the flat mesh itself) |
| `load_mesh(smooth=True)` | 0.94 s | **0.17 s**, RSS +0.70 → +0.45 GB |

The smooth path got the same construction the same day: `Mesh::load` builds
the shared mesh straight from the parallel parse, numbering vertices by first
appearance in the faces and dropping unreferenced ones -- tobj's own order --
with the facets in parallel and the vertex normals summed in `smoothen`'s
order, so it is bit-for-bit tobj's output (`load_matches_tobj_bitwise`) and
tobj remains as `load_via_tobj`, the fallback and the reference.

## Under 100 ms: the sidecar cache

Asked for next: a 3M-facet load in under 100 ms. `KALAST_TIMING=1` prints
the phases (`[LOAD] read 21 ms | parse 78–129 ms | build 90 ms | indices 3
ms`), which named the two levers.

**The parse goes away.** The parsed, canonical positions and triangles --
55 MB for this model against 163 MB of text -- are written beside the OBJ as
`<file>.kmesh`, keyed on the OBJ's size and modification time, and read
straight into the destination arrays (one copy; a byte buffer in between was
half the read). An edited or replaced model is re-parsed and the cache
rewritten; a truncated or foreign cache is refused; a directory that cannot
be written just does not get one; `KALAST_MESH_CACHE=0` turns it off. The
first load pays the parse plus 22 ms to write; every load after reads it in
15 ms. `*.kmesh` is gitignored.

**The build loses its pre-fill.** `vec![Vertex::default(); 9.4 M]` wrote the
684 MB once before the threads wrote it again, a third of the build; the
vectors are `MaybeUninit` now and written exactly once.

| 3M-facet model | cold (parse, write cache) | cached |
|---|---|---|
| flat | 211 ms | **75–95 ms** (cache 15, build 55–75, indices 3) |
| smooth | ~200 ms | **72–100 ms** |

The build's spread is page faults: it is 684 MB of fresh memory, and it runs
slower once the process is large and the GPU holds wired buffers (55 ms with
an empty window, 95–120 with a 3M mesh already uploaded, once 344).

**What is left is bandwidth, not code.** The frame that uploads a 3M mesh
takes 260–320 ms after the conversion went parallel (356 before): 830 MB of
`GeometryVertex` + `AttribVertex` -- 88 bytes a corner, three corners a
facet, of which texture coordinates, tangent and bitangent are zero and the
normal is the facet's repeated three times -- copied into staging, copied to
the GPU, and faulted in. Below that floor the *format* has to change, and
that is a design decision, not a tuning:

- **Expand on the GPU.** Upload the canonical 57 MB and let a compute pass
  write the fat vertex buffers -- upload ~40 ms, memory unchanged, the
  vertex format untouched, one shader.
- **A compact flat vertex.** Positions plus a facet index per corner, with
  normal, colour, value and mode per *facet* in a storage buffer: 830 →
  ~210 MB of VRAM per 3M mesh and a proportionally faster upload; touches the
  shaders, the wireframe and the facet index map.
- **A leaner CPU mesh.** Positions, triangles and facets (0.14 GB per 3M)
  instead of 76 bytes a corner (0.68 GB), with the per-vertex Python views
  derived; the largest change, and the one that cuts RAM fivefold.

What is left of the cached load is the float-free build itself; the remaining RSS above the mesh is the parse's
positions and triangles, freed before the flat build and reused by it. The
first frame's upload is separate, and memory-bound.

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
