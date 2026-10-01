#!/usr/bin/env python
"""Check a bundle's own language server, through the protocol.

Starts `kalast --language-server` -- ty, the bundle's -- the way the editor
starts it for a script among the bundle's examples, and checks both things a
user sees of it:

- completions: `bodies` after `app.simulation.`, kalast's API, which ty
  finds because it is told the bundle's Python folder is its environment,
  as the editor tells it;
- diagnostics: a name defined nowhere is reported. ty checks only the files
  of its project, which it looks for above the script, and a bundle
  unpacked in another project's `dist/` or git-ignored folder -- as it is
  here, in a checkout -- is left out of that project unless the bundle's
  `ty.toml` makes it one of its own. Completions go on working without it,
  so only this catches it.

    python tools/lsp_check.py <bundle>/kalast[.exe]

The release workflow runs it on each assembled bundle, where no window can
open. What it proves is what would otherwise break unseen: that ty survived
the pruning and runs on the platform, that the protocol's pipes work, and
that the bundle's scripts are checked at all. The script is opened in
memory, nothing is written into the bundle. Only the standard library: any
Python can drive it.
"""

import json
import pathlib
import queue
import subprocess
import sys
import threading

# A first completion reads kalast's stubs: well under a second here, a
# margin for a slow runner.
TIMEOUT = 120


def frame(message: dict) -> bytes:
    body = json.dumps(message).encode()
    return b"Content-Length: %d\r\n\r\n" % len(body) + body


def read(stream, into: queue.Queue) -> None:
    """Every message the server writes, in order; `None` at its end."""
    while True:
        length = None
        while True:
            line = stream.readline()
            if not line:
                into.put(None)
                return
            if line in (b"\r\n", b"\n"):
                break
            name, _, value = line.decode("ascii").partition(":")
            if name.strip().lower() == "content-length":
                length = int(value)
        into.put(json.loads(stream.read(length)))


def main() -> int:
    if len(sys.argv) != 2:
        print(__doc__)
        return 2
    # Absolute: a relative one would be looked for from `cwd` below.
    kalast = pathlib.Path(sys.argv[1]).resolve()
    bundle = kalast.parent
    # Where a user's scripts are, and so the folder the editor starts the
    # server in and names as its workspace.
    folder = bundle / "examples"
    server = subprocess.Popen(
        [kalast, "--language-server"],
        cwd=folder,
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
    )
    messages: queue.Queue = queue.Queue()
    threading.Thread(target=read, args=(server.stdout, messages), daemon=True).start()

    def send(message: dict) -> None:
        server.stdin.write(frame(message))
        server.stdin.flush()

    # The bundle's Python folder, which ty reads as its environment.
    settings = {"ty": {"configuration": {"environment": {"python": str(bundle / "python")}}}}

    def section(name: str | None):
        value = settings
        for key in (name or "").split(".") if name else []:
            value = value.get(key) if isinstance(value, dict) else None
        return value

    def until(wanted, what: str) -> dict:
        """The first message `wanted` accepts."""
        while True:
            message = messages.get(timeout=TIMEOUT)
            if message is None:
                raise SystemExit(f"the language server stopped before {what}")
            if "method" in message and "id" in message:
                # A request of the server's own: its settings, or something
                # a checker has no part in, answered so it waits on nothing.
                if message["method"] == "workspace/configuration":
                    items = message.get("params", {}).get("items", [])
                    result = [section(item.get("section")) for item in items]
                else:
                    result = None
                send({"jsonrpc": "2.0", "id": message["id"], "result": result})
            elif wanted(message):
                return message

    def reply(id: int) -> dict:
        return until(lambda m: m.get("id") == id and "method" not in m, f"answering request {id}")

    # A script among the examples, with a name nothing defines on its third
    # line and `app.simulation.` to complete on its fourth.
    script = folder / "lsp_check.py"
    uri = script.as_uri()
    text = "from kalast.app import App\napp = App()\nnot_defined_anywhere\napp.simulation."

    root = folder.as_uri()
    send({"jsonrpc": "2.0", "id": 1, "method": "initialize",
          "params": {"processId": None, "rootUri": root,
                     "workspaceFolders": [{"uri": root, "name": folder.name}],
                     "initializationOptions": {},
                     # What the editor says too: without it the server never
                     # asks for its settings, and so never learns where the
                     # bundle's packages are.
                     "capabilities": {"workspace": {"configuration": True}}}})
    capabilities = reply(1).get("result", {}).get("capabilities")
    if not capabilities or "completionProvider" not in capabilities:
        raise SystemExit(f"the language server offers no completion: {capabilities}")
    send({"jsonrpc": "2.0", "method": "initialized", "params": {}})
    send({"jsonrpc": "2.0", "method": "textDocument/didOpen",
          "params": {"textDocument": {"uri": uri, "languageId": "python", "version": 1, "text": text}}})
    # Pushed once the script is checked, as the editor receives them. The
    # only document open, so whatever comes is about it.
    published = until(lambda m: m.get("method") == "textDocument/publishDiagnostics",
                      "publishing diagnostics")["params"].get("diagnostics", [])
    send({"jsonrpc": "2.0", "id": 2, "method": "textDocument/completion",
          "params": {"textDocument": {"uri": uri}, "position": {"line": 3, "character": 15}}})
    result = reply(2).get("result") or []
    items = result.get("items", []) if isinstance(result, dict) else result
    labels = {item["label"] for item in items}

    send({"jsonrpc": "2.0", "id": 3, "method": "shutdown"})
    reply(3)
    send({"jsonrpc": "2.0", "method": "exit"})
    server.stdin.close()
    code = server.wait(timeout=30)

    if not any(d["range"]["start"]["line"] == 2 and "not_defined_anywhere" in d["message"] for d in published):
        raise SystemExit(f"nothing said of a name defined nowhere in {script}, so the bundle's scripts "
                         f"are not checked -- is the bundle's ty.toml there? Published: {published}")
    if "bodies" not in labels:
        raise SystemExit(f"no `bodies` after `app.simulation.`: {sorted(labels)[:20]}")
    print(f"the bundle's language server: `not_defined_anywhere` reported, {len(labels)} completions "
          f"after `app.simulation.`, `bodies` among them; it exited with {code}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
