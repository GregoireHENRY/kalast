#!/usr/bin/env python
"""Check a bundle's own language server, through the protocol.

Starts `kalast --language-server` -- ty, the bundle's -- opens a script
ending in `app.simulation.`, asks for completions there, and expects
`bodies` among them: kalast's API, which ty finds because it is told the
bundle's Python folder is its environment, as the editor tells it.

    python tools/lsp_check.py <bundle>/kalast[.exe]

The release workflow runs it on each assembled bundle, where no window can
open. What it proves is what would otherwise break unseen: that ty survived
the pruning and runs on the platform, and that the protocol's pipes work. Only the standard library: any Python can drive it.
"""

import json
import pathlib
import queue
import subprocess
import sys
import tempfile
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
    server = subprocess.Popen(
        [sys.argv[1], "--language-server"],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
    )
    messages: queue.Queue = queue.Queue()
    threading.Thread(target=read, args=(server.stdout, messages), daemon=True).start()

    def send(message: dict) -> None:
        server.stdin.write(frame(message))
        server.stdin.flush()

    # The bundle's Python folder, which ty reads as its environment.
    python = pathlib.Path(sys.argv[1]).resolve().parent / "python"
    settings = {"ty": {"configuration": {"environment": {"python": str(python)}}}}

    def section(name: str | None):
        value = settings
        for key in (name or "").split(".") if name else []:
            value = value.get(key) if isinstance(value, dict) else None
        return value

    def reply(id: int) -> dict:
        while True:
            message = messages.get(timeout=TIMEOUT)
            if message is None:
                raise SystemExit(f"the language server stopped before answering request {id}")
            if "method" in message and "id" in message:
                # A request of the server's own: its settings, or something
                # a checker has no part in, answered so it waits on nothing.
                if message["method"] == "workspace/configuration":
                    items = message.get("params", {}).get("items", [])
                    result = [section(item.get("section")) for item in items]
                else:
                    result = None
                send({"jsonrpc": "2.0", "id": message["id"], "result": result})
            elif message.get("id") == id:
                return message

    # A folder of its own, so that the `kalast` read is the bundle's and not
    # a source tree the check happens to run in.
    root = pathlib.Path(tempfile.mkdtemp(prefix="kalast-lsp-check-"))
    uri = (root / "probe.py").as_uri()
    text = "from kalast.app import App\napp = App()\napp.simulation."

    send({"jsonrpc": "2.0", "id": 1, "method": "initialize",
          "params": {"processId": None, "rootUri": root.as_uri(),
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
    send({"jsonrpc": "2.0", "id": 2, "method": "textDocument/completion",
          "params": {"textDocument": {"uri": uri}, "position": {"line": 2, "character": 15}}})
    result = reply(2).get("result") or []
    items = result.get("items", []) if isinstance(result, dict) else result
    labels = {item["label"] for item in items}

    send({"jsonrpc": "2.0", "id": 3, "method": "shutdown"})
    reply(3)
    send({"jsonrpc": "2.0", "method": "exit"})
    server.stdin.close()
    code = server.wait(timeout=30)

    if "bodies" not in labels:
        raise SystemExit(f"no `bodies` after `app.simulation.`: {sorted(labels)[:20]}")
    print(f"the bundle's language server: {len(labels)} completions after `app.simulation.`, "
          f"`bodies` among them; it exited with {code}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
