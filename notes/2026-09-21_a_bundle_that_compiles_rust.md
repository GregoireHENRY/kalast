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

## Not done

**Prebuilding the examples into the bundle.** There are only two real `.rs`
examples, and shipping their cdylibs would make the common case -- run the
example as shipped -- instant and toolchain-free, leaving cargo for people
who edit one. Not measured; a cdylib that actually pulls in wgpu is tens of
megabytes, so it trades against the download. Worth a look before the next
tag.
