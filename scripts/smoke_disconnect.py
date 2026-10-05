"""Check real TCP resets and retained protocol errors. Run with uv on Windows."""

import argparse
import json
import os
from pathlib import Path
import socket
import struct
import subprocess
import tempfile
import time


def receive_exact(peer, length):
    data = bytearray()
    while len(data) < length:
        part = peer.recv(length - len(data))
        if not part:
            raise RuntimeError("Bridge closed before its response was complete")
        data.extend(part)
    return bytes(data)


def ping(address):
    with socket.create_connection(address, timeout=5) as peer:
        body = json.dumps({"id": 731, "operation": {"type": "ping"}}).encode()
        peer.sendall(struct.pack("!I", len(body)) + body)
        length = struct.unpack("!I", receive_exact(peer, 4))[0]
        reply = json.loads(receive_exact(peer, length))
        assert reply["id"] == 731 and reply["result"] == {"Ok": "pong"}, reply


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--launcher", type=Path, required=True)
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix="efi-disconnect-") as directory:
        log = Path(directory) / "stderr.log"
        env = dict(os.environ, EFI_AGENT_API_BASE="http://127.0.0.1:1/v1",
                   EFI_AGENT_API_KEY="test", EFI_AGENT_MODEL="test")
        with log.open("wb") as stderr:
            process = subprocess.Popen(
                [str(args.launcher.resolve()), "serve", directory, "127.0.0.1:0"],
                env=env, stdout=subprocess.DEVNULL, stderr=stderr,
            )
            try:
                deadline = time.monotonic() + 10
                while True:
                    text = log.read_text(encoding="utf-8")
                    if "HostBridge listening on " in text:
                        port = int(text.split("HostBridge listening on ")[1].splitlines()[0].rsplit(":", 1)[1])
                        break
                    if process.poll() is not None or time.monotonic() >= deadline:
                        raise RuntimeError(f"Bridge did not start: {text}")
                    time.sleep(0.02)
                address = ("127.0.0.1", port)
                ping(address)
                # Exercise reset before a frame, in its header, and in its body.
                # A following ping proves the preceding connection was retired.
                linger = struct.pack("HH" if os.name == "nt" else "ii", 1, 0)
                for fragment in (b"", b"\x00\x00", struct.pack("!I", 731) + b"{"):
                    with socket.create_connection(address, timeout=5) as peer:
                        if fragment:
                            peer.sendall(fragment)
                        peer.setsockopt(socket.SOL_SOCKET, socket.SO_LINGER, linger)
                    ping(address)
                assert log.read_text(encoding="utf-8") == text, log.read_text(encoding="utf-8")
                print("PASS idle, partial-header and partial-body TCP resets; reconnect")
                # Graceful EOF inside a frame is malformed, unlike an idle EOF.
                with socket.create_connection(address, timeout=5) as peer:
                    peer.sendall(b"\x00\x00")
                    peer.shutdown(socket.SHUT_WR)
                ping(address)
                with socket.create_connection(address, timeout=5) as peer:
                    peer.sendall(struct.pack("!I", 0))
                    peer.shutdown(socket.SHUT_WR)
                ping(address)
                diagnostics = log.read_text(encoding="utf-8").splitlines()[1:]
                assert len(diagnostics) == 2, diagnostics
                assert "Invalid frame length" in diagnostics[1], diagnostics
                print("PASS partial-frame EOF and invalid length remain visible errors")
            finally:
                process.terminate()
                process.wait(timeout=10)


if __name__ == "__main__":
    main()
