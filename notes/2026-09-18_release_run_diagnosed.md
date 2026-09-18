# The v0.5.0 release run, diagnosed

Three red jobs in run `35374536810`, and a fourth problem that was hiding all
of them. Every cause below comes from a job log, not from reasoning about what
might have gone wrong — which matters, because **two of the three hypotheses
in the handoffs were wrong**, including one of mine.

## The one that hid the others

`gh run view --log` returned nothing on the Mac, and the note guessed "probably
still uploading". It was not. **The run never completed**, so GitHub served no
logs at all: both `macos-x86_64` jobs sat `queued` from 17:28 without ever
starting.

`macos-13` was **retired in December 2025**. A job requesting a dead runner
label does not fail — it waits, until the run's timeout. So the run hung, the
logs stayed unavailable, and everything downstream had to be guessed at.

`macos-15-intel` is the replacement, and the last x86_64 macOS image; it is
available until August 2027, after which that target goes.

**The per-job REST endpoint serves logs while a run is still in progress**,
where `gh run view --log` refuses:

```sh
gh api repos/OWNER/REPO/actions/jobs/JOB_ID/logs --allow-escape-sequences
```

That is the thing to reach for next time a run is stuck.

## `wheel windows-x86_64` — a feature pyo3 needs and nothing else exercises

Reproduced on Windows before touching anything:

```
Caused by: Need a Python interpreter to compile for Windows without
           PyO3's `generate-import-lib` feature
```

Windows links against a Python import library rather than resolving symbols at
load time. Without that feature pyo3 must find a real interpreter to derive
one from — and **`maturin develop` always has one, the active venv.** So the
entire daily loop never touches this path; `maturin build` in the release
workflow is the first thing that does, which is why a year of development
found nothing and the first tag did.

## `wheel linux-x86_64` — the script ran in the wrong place

```
E: Could not open lock file /var/lib/apt/lists/lock (13: Permission denied)
```

`--manylinux auto` was passed through `args`. That reaches maturin and tells
the **action** nothing, so it never started a container: the build ran
natively on ubuntu-24.04 and `before-script-linux` ran on the *host*, as an
unprivileged user, where apt cannot take its lock. The `yum` branch — the one
the script was written for — was unreachable, because there was no container
to be in.

As the action's own `manylinux` input it selects the image and builds inside
it. That also makes the wheel's tag a manylinux one rather than a bare
`linux_x86_64`, which PyPI refuses.

**And the script is dropped, not fixed.** Every Linux system library this
project touches is loaded at runtime: `x11-dl` dlopens libX11, `wayland-sys`
dlopens libwayland through winit's default `wayland-dlopen`, `ash` dlopens
libvulkan. The container needs no `-devel` packages at all. Installing them
was solving a problem the build does not have, while adding one — a
CentOS-7-based image has dead yum mirrors unless its vault repos are pinned,
which was the *other* standing hypothesis and would have been the next
failure.

## `publish to crates.io` — not the token, a manifest typo

Both handoffs expected a missing `CARGO_REGISTRY_TOKEN`. **It was there**:
created 17:22:55, run started 17:28:21. Ruled out by looking, which took one
API call and should have been the first thing either of us did.

The log:

```
error[E0433]: cannot find `Item` in `syn`
error[E0425]: cannot find type `File` in crate `syn`
error[E0425]: cannot find function `parse_file` in crate `syn`
error: failed to verify package tarball
```

`macros/Cargo.toml` said:

```toml
syn = { version = "2", type-features = ["full", "parsing"] }
```

**There is no `type-features` key.** Cargo ignored the list, syn came in with
its defaults, and `full` — which gates `syn::Item`, `syn::File` and
`parse_file`, all three used here — was off. rustc said so plainly, twice:
*"found an item that was configured out"*.

It has compiled anyway for as long as it has existed, because another
workspace member enables those features and cargo unifies them across the
graph. **A dependency declaration only has to be correct when the crate is
built alone**, and the first thing that ever does that is `cargo publish`
verifying its own tarball. So a manifest typo stayed invisible until the first
release, and would have stayed invisible indefinitely without one.

Reproduced and fixed here: `cargo publish --dry-run --manifest-path
macros/Cargo.toml` gave the same three errors before and reaches
"Uploading kalast_macros v0.5.0" after.

### One thing that looks like a fourth bug and is not

A dry run of the *root* crate fails:

```
failed to select a version for the requirement `kalast_macros = "^0.5"`
candidate versions found which didn't match: 0.4.1
```

That is correct behaviour. `kalast` resolves its macro dependency from
crates.io, which has only 0.4.1 until the macro crate is published. The job
publishes `kalast_macros` first and sleeps 30 s for the index, which is the
order that handles it.

## What is left, and it is not code

Nothing published, so no version is burned and `v0.5.0` can be reused.

1. **Move the tag.** It points at `b4429b0`; the three fixes are after it. A
   re-run against the old tag rebuilds the old code and fails identically.
2. **Register the PyPI pending publisher** — `pypi.org/manage/account/publishing/`,
   project `kalast`, owner `GregoireHENRY`, repo `kalast`, workflow
   `release.yml`, environment `pypi`. The `pypi` job never ran, so this is
   still untested.
3. **Cancel the stuck run** if it is still queued, or let it time out.

## The lesson, since it cost two sessions

Three hypotheses were recorded across two handoffs. **One was right** (the
Windows wheel being platform-specific rather than a build problem), one was
wrong in a way that would have made things worse (installing headers into the
container), and one was wrong in a way that would have wasted an afternoon on
account settings (the crates.io token).

All three were resolvable by reading a log, and the log was unreachable only
because of an unrelated fourth problem — a retired runner label — that nobody
had looked for, because a queued job does not look like a failure.
