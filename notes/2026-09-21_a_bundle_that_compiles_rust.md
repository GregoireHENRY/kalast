# A `.rs` example runs from a release too

2026-09-21, Mac, the same day as `2026-09-21_a_bundle_that_runs_python.md`
and the answer to the same objection: *"rust examples need to run too, that's
the whole point of the bundle."*

They could not, for two independent reasons, and both are fixed.

## 1. The wrapper depended on kalast by path

Hosting a `.rs` generates a crate that wraps the example and exports
`kalast_abi` / `kalast_example`, and that crate said

```toml
kalast = { path = "<the working directory>" }
```

which only resolves inside a clone. From a bundle, cargo said *"failed to
read `<bundle>/Cargo.toml`"* -- a file the user had never been told to have.

It now depends on the **release of the exact version running**:

```toml
kalast = { version = "=0.5.0", default-features = false }
```

`=0.5.0` and not `0.5`, because `abi_fingerprint` hashes
`CARGO_PKG_VERSION`: a guest resolved one patch ahead would compile and then
be refused at load, which is a worse failure than not compiling.

**And `default-features = false`, which was a live bug.** `python` is a
*default* feature. The old wrapper only ever added features, never turned
defaults off, so a `--no-default-features` host -- which is exactly what a
release bundle is -- would have handed cargo a guest *with* pyo3. Different
`Shared` layout, and every hosted build from a bundle refused at load with
"built against a different kalast". It could never show up in the
repository, where the host has `python` too. `dependency_tests` pins it now.

Measured, in a bundle with no source tree anywhere near it:

```
kalast = { version = "=0.5.0", default-features = false }
    Finished `release` profile [optimized] target(s) in 1m 15s
guest fingerprint af5af4cdd9cd31f3
host  fingerprint af5af4cdd9cd31f3
OK: a bundle compiles a .rs against crates.io and the host accepts it
```

Same number on both sides, so the host loads it. Done through the real
`write_wrapper` / `find_or_install_cargo` / `dylib_path_for`, from a harness
standing in for the window, so no GUI was involved and nothing was
re-implemented for the test.

**This is why `release` now `needs: crates`.** A bundle published before the
crate it compiles against exists is a bundle whose Rust half cannot work.

## 2. There might be no cargo

Telling the user to go and install Rust would make the executable a set of
instructions rather than a program. So it installs one: rustup's own
installer, `--profile minimal --no-modify-path`, into `toolchain/` beside the
executable.

Order of preference, and the middle one matters more than it looks:

1. `cargo` on `PATH`;
2. `$CARGO_HOME/bin`, `~/.cargo/bin` -- **a bundle double-clicked in Finder
   inherits `launchd`'s `PATH`, not the shell's**, so a machine with a
   perfectly good toolchain looks empty from there, and downloading a second
   one would be the wrong answer to the right question;
3. `toolchain/` from a previous run;
4. install one.

**Not at first launch.** Most people never open a `.rs`, and this is not
small. The trigger is compiling one.

Verified with `env -i`, no `PATH` to any cargo and an empty `HOME`:

| | |
|---|---|
| installed | `toolchain/cargo/bin/cargo` → `cargo 1.98.1` |
| size | 458 MB (rustup 447, cargo 11) |
| written to `$HOME` | **nothing** |

The build directory adds ~600 MB on top, also inside the folder. That is a
lot, and it is the honest price of compiling Rust; what matters is that it is
all in the folder the user unpacked and goes away when they delete it.

## What still needs the machine's help

The linker. rustup cannot supply one, so on macOS without command line tools
rustc fails after a five-minute compile with a wall of `ld` output. Checked
up front with `xcode-select -p`, **before** the toolchain download rather than
after, and the message names `xcode-select --install`. Linux desktops
essentially always have `cc`; on Windows rustup refuses without the MSVC
tools and explains itself better than this could.

## 3. And they are compiled before they ship

Everything above is for someone who *edits* an example. Someone who just
opens the one in the archive should not wait five minutes or download a
toolchain at all, so the bundle carries the libraries already built.

`kalast --precompile a.rs b.rs` builds each one and exits, no window, which
is what the release workflow runs -- **with the executable it is about to
ship**. That matters more than it sounds: the libraries have to come out of
the same `write_wrapper`, the same feature set and the same `build_dir()`
the editor will look in, and a second recipe written in YAML would drift
from all three. The way it would show up is the editor silently recompiling
everything the bundle shipped, which is indistinguishable from not shipping
them.

It checks `is_current` before building, so it is idempotent -- and that
makes it the verification as well. Run inside the assembled bundle it must
report **0 built**; if a path or a timestamp were wrong it would instead try
to compile, and from a bundle that means crates.io, which does not have the
version until the `crates` job publishes it. So it fails rather than quietly
shipping an archive that recompiles on first use.

| | |
|---|---|
| maintained `.rs` examples | 2, both under `examples/crater_self_shadow/` |
| each library | 16.6 MB, so 32 MB added to the archive |
| in the bundle | `up to date` ×2, `0 built, 0 failed` |
| host accepts them | `af5af4cdd9cd31f3` on both sides |

And run for real, from the bundle, with no toolchain and no network:

```
$ ./kalast examples/crater_self_shadow/step.rs
loaded target/kalast-hosted/plain/release/libcrater_self_shadow_step.dylib
loading model: "res/plane_crater_1024-5000_h=0.437.obj"
```

It rendered to iteration 10000 and closed itself, which is what that example
does.

**`examples/old/` is not precompiled**, and "all the Rust examples" is
therefore two of seven. Running `--precompile` over all seven: *2 of 7
built*. The other five are the superseded versions README already describes
as not user-facing, and they do not compile against the current API at all
-- `gpu::win`, bare `winit`, `env_logger`. They stay in the archive to read.

## Not done

**Stripping the libraries.** `strip -x` takes one from 16.6 to 13.5 MB, so
about 6 MB off the archive. Not obviously worth losing the symbol names in a
crash.
