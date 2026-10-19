# 2026-10-01 — v0.5.11 half published; v0.5.12 in its place

The tag v0.5.11 (`b208c0b`, rehearsal 36789160733 all green) ran as run
36790528692: the gate passed, **PyPI published 0.5.11** from the rehearsal's
wheels and sdist, `kalast_macros` 0.5.11 went to crates.io, and `cargo
publish` of `kalast` failed verifying its package:

    couldn't read `src/app/../../res/colormaps.bin`
    couldn't read `src/app/../../res/examples-shipped.txt`

Both are compiled in (`include_bytes!` in `app::config`, `include_str!` in
`app::update`), both new in this version, and neither was in `Cargo.toml`'s
`include` list. The GitHub release waits on `crates` -- a bundle's `.rs`
compiles against `kalast = "=<version>"` from crates.io -- so it was skipped:
no bundle was published, and v0.5.10's update check was offered nothing.

The same list draws maturin's sdist: PyPI's `kalast-0.5.11.tar.gz` lacks the
two files and cannot build. Its four wheels are complete. A version cannot be
published twice on either registry, so v0.5.11 cannot be finished as it was
tagged: its crate would need another commit, the gate refuses a version PyPI
has, and PyPI's sdist stays broken. Hence **v0.5.12**, the same code with the
list fixed, through the usual rehearsal and tag. CHANGELOG.md's section, the
whole list since v0.5.10, is headed `## v0.5.12`: v0.5.11 has no GitHub
release, so no section. The tag v0.5.11 stays where it is, the source of
what PyPI has.

Why no rehearsal caught it: a rehearsal builds the wheels, the sdist and the
bundles, and never packages the crate -- `crates` runs on tags only, as it
publishes. The sdist was built without complaint; nothing builds *from* it.

The guard: `tests/test_crate_package.py` reads every `include_bytes!`,
`include_str!` and `include_wgsl!` in `src/` -- a literal path relative to its
file, or `concat!(env!("CARGO_MANIFEST_DIR"), ...)` -- and checks each against
`cargo package --list`. 45 files today. Without the fix it names exactly the
two above. The `sdist` job runs it first, so a rehearsal fails on it.
Verified locally as well: `cargo package` builds the packaged crate, and
`maturin sdist` carries both files.

For the user: yanking 0.5.11 on PyPI (its page, "Options", "Yank") keeps pip
from picking it unless pinned; only the account holder can. The pre-release
v0.5.11-beta is not removed by v0.5.12's release, which removes
`v0.5.12-beta`.
