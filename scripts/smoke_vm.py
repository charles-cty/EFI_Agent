"""Boot a real VM and check serial rendering, guest TCP4, and provider RPC.

Run with uv. The provider is a local deterministic HTTP server; no key is needed.
This exercises guest transport, not the launcher's interactive terminal relay.
"""

import argparse
import json
import os
from pathlib import Path
import re
import socket
import subprocess
import tempfile
import threading
import time
from http.server import BaseHTTPRequestHandler, HTTPServer


FILE_MARKER = "HOST_FILE_731_VERIFIED"
MODEL_MARKER = "MODEL_RESPONSE_419_VERIFIED"
ANSI = re.compile(rb"\x1b\[[0-?]*[ -/]*[@-~]")


class Provider(BaseHTTPRequestHandler):
    received = False

    def do_POST(self):
        body = self.rfile.read(int(self.headers["Content-Length"]))
        request = json.loads(body)
        valid = (
            self.path == "/v1/chat/completions"
            and self.headers["Authorization"] == "Bearer smoke-key"
            and request["model"] == "smoke-model"
            and request["messages"][-1] == {"role": "user", "content": "Say the model marker"}
            and request["stream"] is False
        )
        Provider.received = valid
        response = json.dumps({"choices": [{"message": {"role": "assistant", "content": MODEL_MARKER}}]}).encode()
        self.send_response(200 if valid else 400)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(response)))
        self.end_headers()
        self.wfile.write(response)

    def log_message(self, *_args):
        pass


def unused_port():
    # The standalone serve command takes an address. Close this probe before
    # starting it, then verify the child is listening before booting the guest.
    with socket.socket() as probe:
        probe.bind(("127.0.0.1", 0))
        return probe.getsockname()[1]


def wait_for_service(process, port):
    deadline = time.monotonic() + 10
    while time.monotonic() < deadline:
        if process.poll() is not None:
            raise RuntimeError("HostBridge exited before listening")
        try:
            with socket.create_connection(("127.0.0.1", port), timeout=0.2):
                return
        except OSError:
            time.sleep(0.05)
    raise TimeoutError("HostBridge did not listen")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--qemu", required=True)
    parser.add_argument("--code", type=Path, required=True)
    parser.add_argument("--vars", type=Path, required=True)
    parser.add_argument("--esp", type=Path, required=True)
    parser.add_argument("--launcher", type=Path, required=True)
    parser.add_argument("--accel", choices=["kvm", "whpx"], required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)

    provider = HTTPServer(("127.0.0.1", 0), Provider)
    thread = threading.Thread(target=provider.serve_forever, daemon=True)
    thread.start()
    children = []
    try:
        with tempfile.TemporaryDirectory(prefix="efi-agent-smoke-") as temporary:
            workspace = Path(temporary)
            (workspace / "needle.txt").write_text(FILE_MARKER, encoding="utf-8")
            env = os.environ.copy()
            env.update({
                "EFI_AGENT_API_BASE": f"http://127.0.0.1:{provider.server_port}/v1",
                "EFI_AGENT_API_KEY": "smoke-key",
                "EFI_AGENT_MODEL": "smoke-model",
            })
            rpc_port = unused_port()
            bridge = subprocess.Popen(
                [str(args.launcher.resolve()), "serve", str(workspace), f"127.0.0.1:{rpc_port}"],
                cwd=workspace, env=env, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE,
            )
            children.append(("bridge", bridge))
            wait_for_service(bridge, rpc_port)
            with socket.socket() as listener:
                listener.bind(("127.0.0.1", 0))
                listener.listen()
                listener.settimeout(15)
                console_port = listener.getsockname()[1]
                command = [
                    args.qemu, "-machine", f"q35,accel={args.accel}", "-m", "256",
                    "-display", "none", "-serial", "none", "-monitor", "none", "-no-reboot",
                    "-drive", f"if=pflash,format=raw,readonly=on,file={args.code.resolve()}",
                    "-drive", f"if=pflash,format=raw,snapshot=on,file={args.vars.resolve()}",
                    "-drive", f"if=none,id=esp,format=raw,readonly=on,file=fat:ro:{args.esp.resolve()}",
                    "-device", "virtio-blk-pci,drive=esp", "-device", "virtio-serial-pci",
                    "-chardev", f"socket,id=terminal,host=127.0.0.1,port={console_port}",
                    "-device", "virtconsole,chardev=terminal",
                    "-netdev", f"user,id=network,guestfwd=tcp:10.0.2.100:7420-tcp:127.0.0.1:{rpc_port}",
                    "-device", "virtio-net-pci,netdev=network",
                ]
                qemu = subprocess.Popen(command, cwd=workspace, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)
                children.append(("qemu", qemu))
                connection, _ = listener.accept()
                with connection:
                    connection.settimeout(0.5)
                    output = bytearray()
                    state = "boot"
                    deadline = time.monotonic() + 75
                    while time.monotonic() < deadline:
                        try:
                            data = connection.recv(65536)
                            if not data:
                                raise RuntimeError("Guest terminal disconnected")
                            output.extend(data)
                        except TimeoutError:
                            pass
                        plain = ANSI.sub(b"", output)
                        if state == "boot" and b"What would you like to build?" in plain:
                            print("PASS serial TUI render", flush=True)
                            connection.sendall(b"\x1b[8;29;103t/host-read needle.txt\r")
                            state = "file"
                        elif state == "file" and FILE_MARKER.encode() in plain:
                            print("PASS guest TCP4 host file read", flush=True)
                            connection.sendall(b"Say the model marker\r")
                            state = "model"
                        elif state == "model" and MODEL_MARKER.encode() in plain:
                            if not Provider.received:
                                raise AssertionError("Provider request format was incorrect")
                            print("PASS Chat Completions request and guest response", flush=True)
                            state = "done"
                            break
                        if qemu.poll() is not None:
                            raise RuntimeError("QEMU exited during the test")
                    (args.output / "serial.bin").write_bytes(output)
                    (args.output / "transcript.txt").write_bytes(ANSI.sub(b"", output))
                    if state != "done":
                        raise TimeoutError(f"VM smoke test stopped in state {state}")
    finally:
        for name, child in reversed(children):
            if child.poll() is None:
                child.terminate()
            try:
                _, stderr = child.communicate(timeout=10)
            except subprocess.TimeoutExpired:
                child.kill()
                _, stderr = child.communicate()
            (args.output / f"{name}.log").write_bytes(stderr)
        provider.shutdown()
        provider.server_close()
        thread.join(timeout=2)


if __name__ == "__main__":
    main()
