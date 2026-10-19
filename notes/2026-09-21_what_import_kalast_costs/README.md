# `import kalast` was loading 174 MB nothing had asked for

2026-09-21, after v0.5.2, from a question about the bundle: *why does Python
have to be a whole folder, and why is the same Rust library in it four
times?* Measuring to answer that turned up something larger.

## The measurement

`kalast/__init__.py` imports every subpackage. With it emptied in a
throwaway venv, each on its own:

| | pulls in |
|---|---|
| `app`, `math`, `mesh`, `astro`, `scattering`, `io` | nothing heavy |
| `entity`, `util` | numpy |
| `spice` | numpy, spiceypy |
| `tpm` | numpy, **scipy** |
| `plot` | numpy, scipy, **matplotlib**, **pyarrow** |

So a script that renders a mesh and nothing else loaded matplotlib, scipy
and pyarrow. In the shipped bundle's 283 MB of site-packages:

```
120 MB  pyarrow      only kalast/plot/tool.py wants it
 54 MB  scipy        plot/tool.py, plot/smap.py, tpm/implicit.py
 27 MB  matplotlib   all of plot/
 14 MB  PIL          matplotlib
 14 MB  fontTools    matplotlib
---
229 MB  against numpy 18, kalast 19, spiceypy 7 for the part that renders
```

## What was done

`kalast/plot/__init__.py` and `kalast/tpm/__init__.py` defer their
submodules through PEP 562 `__getattr__`. `kalast.plot.cbar.Params()` and
`kalast.tpm.implicit` read exactly as before -- that is how the examples
spell it -- and the import happens on first attribute access.

`import kalast` now reaches **numpy and spiceypy**, and nothing else.
`tests/test_lazy_imports.py` pins both halves: the heavy modules stay out of
`sys.modules`, and every old spelling still resolves. Each check runs in a
fresh interpreter, because `sys.modules` is process-wide and one test
importing matplotlib would hide the regression from the next.

**Deferring alone saves nothing.** It makes the packages optional; the
bundle shrinks only if they also stop being shipped, and then whatever needs
them stops working there. So this was a judgement about the examples, not
just about imports:

| | saves | breaks in the bundle |
|---|---|---|
| defer only | 0 MB | nothing |
| **+ drop pyarrow** | **120 MB** | `kalast.plot.tool` -- no shipped example calls it |
| + drop scipy | 174 MB | also two shipped examples |
| + drop matplotlib | 229 MB | also all three that plot |

`misc/cbar.py`, `analytical/sinusoidal.py` and `analytical/slab_relaxation.py`
use `kalast.plot`; the last two need scipy, one through `kalast.tpm.implicit`
and one directly. So **pyarrow goes and the rest stays**.

**A correction while here.** `tpm/implicit.py`'s docstring says *"Nothing in
the repository called any of it"*, and I repeated that as a reason to treat
the module as dead. It is a historical note about a bug that was fixed --
`analytical/sinusoidal.py` calls it today.

## And stripping

14 MB, measured: the executable 19.3 -> 16.2, the extension module
18.8 -> 15.1, each hosted library 18.9 -> ~15.4. The cost is symbol names in
a crash backtrace.

Verified rather than assumed, because a library that lost `kalast_abi` would
still *load* and the failure would reach a user instead of the job: after
`strip -x`, both hosted libraries still export `kalast_abi` and
`kalast_example`, the executable still starts its embedded interpreter, and
a stripped library still loads into it. The workflow checks the exports on
every build.

## What was not done, and why

**One shared `libkalast.dylib`.** The bundle carries four near-identical
copies of the engine -- executable 19.3 MB, the wheel's `_rs.abi3.so` 18.8,
and 18.9 for each prebuilt example -- because each is an independent static
link. Rust's `dylib` crate type would collapse them and save ~54 MB.

It is the wrong trade. **Rust's dylib ABI is unstable**: everything has to be
built by the same compiler with the same flags. That holds for the three
artefacts one CI job builds together, and fails for the thing shipped this
afternoon -- editing a `.rs` in a bundle compiles it with *the user's*
rustc, possibly one rustup fetched minutes earlier. Linking that against a
`libkalast.dylib` built on a runner in September is undefined, and the
failure would not be clean. The C ABI across `HostApi`, with
`abi_fingerprint` checked at load, exists exactly so the two sides can be
compiled separately and a mismatch is *detected*. 54 MB of 428 is not worth
giving that up.

**Dropping the wheel's `_rs.abi3.so`** would save 18.8 MB and the UI app
never loads it -- `append_to_inittab` hands the embedded interpreter the
executable's own bindings, so there are not two engines in one process. It
is there so `python/bin/python3 -m kalast script.py` works as a second way
in. 4 % of the bundle to keep a working entry point is a good trade.
