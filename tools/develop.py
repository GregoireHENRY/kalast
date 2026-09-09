#!/usr/bin/env python
"""`maturin develop`, but it does not fail because something has the .pyd open.

On Windows a DLL that is mapped into any live process cannot be written to or
deleted, and `maturin develop` finishes by copying the fresh build over
`kalast/_rs.pyd` -- so the whole build is thrown away at the last step with
`os error 32`. This does not happen on macOS: there, unlinking a mapped file
is allowed, the existing users keep the old inode, and the new file simply
takes the name. The difference is the filesystem, not the toolchain.

The holder is usually not a kalast window. VS Code's Python language server
imports the package to offer completions and then keeps it mapped for the rest
of the editor session, so the lock is present most of the time on this machine
and a build that "works after closing everything" is not a fix.

The fix is that Windows blocks *writing* a mapped image but still permits
*renaming* it -- the lock is on the contents, not on the directory entry. So
the old module is moved aside before maturin runs. Whoever has it open keeps
running against the renamed file, which is what they already had; the name is
free, so the copy succeeds; and the next process to start gets the new build.

Rejected: killing the holder. It is the editor's language server, it comes
back immediately, and a build script that kills processes by name is exactly
the thing this repo says not to do.

Run:  python tools/develop.py [--release] [any other maturin develop args]
"""

import pathlib
import subprocess
import sys
import time

ROOT = pathlib.Path(__file__).resolve().parent.parent
PYD = ROOT / "kalast" / "_rs.pyd"


def sweep_old() -> int:
    """Delete the leftovers from previous runs, ignoring any still mapped.

    They are only deletable once every process that had them open has gone, so
    a failure here is the normal case and not worth reporting -- the point is
    that they do not accumulate forever.
    """
    freed = 0
    for stale in PYD.parent.glob("_rs.pyd.old-*"):
        try:
            stale.unlink()
            freed += 1
        except OSError:
            pass
    return freed


def move_aside() -> pathlib.Path | None:
    """Free the name, returning where the old module went."""
    if not PYD.exists():
        return None
    # Millisecond suffix: two builds in the same second are ordinary during an
    # edit/run loop, and colliding here would fail for the same reason we are
    # trying to avoid.
    old = PYD.with_name(f"_rs.pyd.old-{time.strftime('%Y%m%d-%H%M%S')}-{int(time.time() * 1000) % 1000:03d}")
    try:
        PYD.rename(old)
    except OSError as exc:
        # Not the mapped-image case -- a rename is refused for other reasons,
        # such as the file being open with FILE_SHARE_DELETE withheld. Let
        # maturin run anyway and report its own error, rather than pre-empting
        # it with a worse message.
        print(f"note: could not move {PYD.name} aside ({exc}); building anyway", flush=True)
        return None
    return old


def main() -> int:
    freed = sweep_old()
    if freed:
        print(f"cleaned up {freed} stale module{'s' if freed > 1 else ''}", flush=True)

    old = move_aside()
    if old is not None:
        print(f"moved {PYD.name} -> {old.name}", flush=True)

    cmd = ["uv", "run", "maturin", "develop", "--uv", *sys.argv[1:]]
    result = subprocess.run(cmd, cwd=ROOT)

    if result.returncode != 0 and old is not None and not PYD.exists():
        # The build failed and left no module, so the package is now broken.
        # Put the working one back rather than leaving an import error behind.
        try:
            old.rename(PYD)
            print(f"build failed; restored {PYD.name}")
        except OSError as exc:
            print(f"build failed and {PYD.name} could not be restored: {exc}")

    return result.returncode


if __name__ == "__main__":
    sys.exit(main())
