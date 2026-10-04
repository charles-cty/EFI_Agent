"""Test the native UEFI mode on OVMF/KVM with a writable FAT boot volume.

Uses SimpleText and QMP keyboard input, no VM marker or serial terminal.
Needs uv, QEMU, the host launcher, and mtools. The relay is deterministic.
This checks native protocol execution under firmware, not physical hardware.
"""
import argparse
import json
from pathlib import Path
import socket
import subprocess
import tempfile
import threading
import time


def run(*args):
    return subprocess.check_output(args)


class Relay:
    def __init__(self):
        self.listener = socket.socket()
        self.listener.bind(("127.0.0.1", 0))
        self.listener.listen()
        self.port = self.listener.getsockname()[1]
        self.done = threading.Event()
        self.waiting = threading.Event()
        self.cancelled = threading.Event()
        self.waiting_body = threading.Event()
        self.stop = threading.Event()
        self.error = None
        self.step = 0
        self.thread = threading.Thread(target=self.serve, daemon=True)
        self.thread.start()

    @staticmethod
    def exact(connection, count):
        output = bytearray()
        while len(output) < count:
            data = connection.recv(count - len(output))
            if not data:
                raise EOFError("Guest disconnected")
            output.extend(data)
        return output

    def request(self, connection, length):
        request = json.loads(self.exact(connection, length))
        while request["operation"]["type"] == "model_config":
            reply = json.dumps({"id": request["id"], "result": {"Ok": "Model configured"}}).encode()
            connection.sendall(len(reply).to_bytes(4, "big") + reply)
            length = int.from_bytes(self.exact(connection, 4), "big")
            request = json.loads(self.exact(connection, length))
        return request

    def serve(self):
        try:
            self.listener.settimeout(0.5)
            while not self.stop.is_set():
                try:
                    connection, _ = self.listener.accept()
                    break
                except TimeoutError:
                    continue
            else:
                return
            # Deliver only part of the first reply's header, then wait for
            # cancellation. The next prompt must preserve the frame boundary.
            connection.settimeout(10)
            length = int.from_bytes(self.exact(connection, 4), "big")
            delayed = self.request(connection, length)
            assert delayed["operation"]["messages"][-1]["content"] == "delay native"
            stale = json.dumps({"id": delayed["id"], "result": {"Ok": json.dumps({
                "role": "assistant", "content": "STALE_NATIVE_RESPONSE"
            })}}).encode()
            stale_frame = len(stale).to_bytes(4, "big") + stale
            connection.sendall(stale_frame[:2])
            self.waiting.set()
            length = int.from_bytes(self.exact(connection, 4), "big")
            self.cancelled.set()
            connection.sendall(stale_frame[2:])
            resumed = self.request(connection, length)
            assert resumed["operation"]["messages"][-1]["content"] == "delay body"
            stale = json.dumps({"id": resumed["id"], "result": {"Ok": json.dumps({
                "role": "assistant", "content": "STALE_NATIVE_BODY_RESPONSE"
            })}}).encode()
            stale_frame = len(stale).to_bytes(4, "big") + stale
            connection.sendall(stale_frame[:17])
            self.waiting_body.set()
            length = int.from_bytes(self.exact(connection, 4), "big")
            connection.sendall(stale_frame[17:])
            resumed = self.request(connection, length)
            with connection:
                connection.settimeout(10)
                calls = [
                    ("read", {"path": "seed.txt"}),
                    ("edit", {"path": "seed.txt", "old_text": "original 731", "new_text": "native 中 419"}),
                    ("write", {"path": "created.txt", "content": "UEFI native file\n"}),
                    ("read", {"path": "../outside.txt"}),
                    ("read", {"path": "."}),
                ]
                for step in range(6):
                    length = int.from_bytes(self.exact(connection, 4), "big") if step else 1
                    if not 0 < length <= 1024 * 1024:
                        raise AssertionError("Invalid RPC frame length")
                    request = self.request(connection, length) if step else resumed
                    if request["operation"]["type"] != "complete":
                        raise AssertionError("Native files were sent to the model relay")
                    messages = request["operation"]["messages"]
                    if step == 0:
                        assert messages[-1]["content"] == "exercise native tools"
                        assert len(messages) == 4 and messages[1]["content"] == "delay native"
                        assert messages[2]["content"] == "delay body"
                    else:
                        last = messages[-1]
                        assert last["role"] == "tool" and last["tool_call_id"] == f"native-{step}"
                        if step <= 3:
                            assert last["content"] == ["original 731", "File edited", "File saved"][step - 1]
                        elif step == 4:
                            assert "Tool error:" in last["content"] and "workspace" in last["content"]
                        else:
                            assert "seed.txt" in last["content"] and "created.txt" in last["content"]
                    if step < len(calls):
                        name, arguments = calls[step]
                        message = {"role": "assistant", "content": None, "tool_calls": [{"id": f"native-{step+1}", "type": "function", "function": {"name": name, "arguments": json.dumps(arguments)}}]}
                    else:
                        message = {"role": "assistant", "content": "NATIVE_TOOLS_VERIFIED"}
                    if step == len(calls):
                        for text in ["NATIVE_", "TOOLS_", "VERIFIED"]:
                            progress = json.dumps({"id": request["id"], "result": {"Ok": ""}, "delta": text}).encode()
                            frame = len(progress).to_bytes(4, "big") + progress
                            for offset in range(0, len(frame), 7):
                                connection.sendall(frame[offset:offset + 7])
                    response = json.dumps({"id": request["id"], "result": {"Ok": json.dumps(message)}}).encode()
                    connection.sendall(len(response).to_bytes(4, "big") + response)
                    self.step = step + 1
                self.done.set()
        except Exception as error:
            self.error = error
            self.done.set()

    def close(self):
        self.stop.set()
        self.thread.join(timeout=2)
        self.listener.close()


class Qmp:
    def __init__(self, path):
        self.socket = socket.socket(socket.AF_UNIX)
        self.socket.connect(str(path))
        self.socket.settimeout(10)
        self.stream = self.socket.makefile("rwb", buffering=0)
        json.loads(self.stream.readline())
        self.command("qmp_capabilities")

    def command(self, name, arguments=None):
        request = {"execute": name}
        if arguments is not None:
            request["arguments"] = arguments
        self.stream.write(json.dumps(request).encode() + b"\n")
        while True:
            reply = json.loads(self.stream.readline())
            if "error" in reply:
                raise RuntimeError(reply["error"])
            if "return" in reply:
                return reply["return"]

    def key(self, key):
        self.command("send-key", {"keys": [{"type": "qcode", "data": key}], "hold-time": 20})
        time.sleep(0.04)

    def close(self):
        self.stream.close()
        self.socket.close()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--qemu", default="qemu-system-x86_64")
    for name in ("code", "vars", "efi", "launcher", "output"):
        parser.add_argument(f"--{name}", type=Path, required=True)
    args = parser.parse_args()
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    relay = Relay()
    qemu = None
    qmp = None
    try:
        with tempfile.TemporaryDirectory(prefix="efi-native-") as temporary:
            work = Path(temporary)
            image = work / "esp.img"
            tree = work / "tree"
            (tree / "EFI/BOOT").mkdir(parents=True)
            (tree / "EFI/AGENT").mkdir()
            (tree / "work").mkdir()
            (tree / "EFI/BOOT/BOOTX64.EFI").write_bytes(args.efi.read_bytes())
            config = tree / "EFI/AGENT/NATIVE.JSON"
            config.write_text(json.dumps({"relay_address": [10, 0, 2, 100], "relay_port": 7420, "workspace": "\\work"}), encoding="utf-8")
            seed = tree / "work/seed.txt"
            seed.write_text("original 731", encoding="utf-8")
            outside = tree / "outside.txt"
            outside.write_text("must not be read", encoding="utf-8")
            run(str(args.launcher.resolve()), "pack", str(tree), str(image))
            image_spec = f"{image}@@1048576"
            qmp_path = work / "qmp.sock"
            qemu = subprocess.Popen([
                args.qemu, "-machine", "q35,accel=kvm", "-m", "256", "-display", "none",
                "-serial", "none", "-monitor", "none", "-no-reboot",
                "-qmp", f"unix:{qmp_path},server=on,wait=off",
                "-drive", f"if=pflash,format=raw,readonly=on,file={args.code.resolve()}",
                "-drive", f"if=pflash,format=raw,snapshot=on,file={args.vars.resolve()}",
                "-drive", f"if=none,id=esp,format=raw,file={image}", "-device", "virtio-blk-pci,drive=esp",
                "-netdev", f"user,id=network,guestfwd=tcp:10.0.2.100:7420-tcp:127.0.0.1:{relay.port}",
                "-device", "virtio-net-pci,netdev=network",
            ], cwd=work, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)
            deadline = time.monotonic() + 10
            while not qmp_path.exists():
                if qemu.poll() is not None or time.monotonic() > deadline:
                    raise RuntimeError("QEMU did not start")
                time.sleep(0.1)
            qmp = Qmp(qmp_path)
            # Native SimpleText has no serial readiness marker. Observe the
            # rendered framebuffer, wait for it to stabilize, then inject keys.
            previous = None
            stable = 0
            deadline = time.monotonic() + 20
            while time.monotonic() < deadline:
                qmp.command("screendump", {"filename": str(output / "boot.ppm")})
                data = (output / "boot.ppm").read_bytes()
                if data == previous and len(set(data[-100000:])) > 1:
                    stable += 1
                else:
                    stable = 0
                previous = data
                if stable >= 5:
                    break
                time.sleep(0.3)
            for character in "delay native":
                qmp.key("spc" if character == " " else character)
            qmp.key("ret")
            if not relay.waiting.wait(40):
                raise TimeoutError("Native delayed request did not reach the relay")
            cancelled_at = time.monotonic()
            qmp.key("esc")
            time.sleep(0.2)
            qmp.command("screendump", {"filename": str(output / "cancelled.ppm")})
            for character in "delay body":
                qmp.key("spc" if character == " " else character)
            qmp.key("ret")
            if not relay.cancelled.wait(3):
                raise TimeoutError("Native Escape did not permit a fresh model request")
            if time.monotonic() - cancelled_at > 3:
                raise AssertionError("Native cancellation was not responsive")
            print(f"PASS native SimpleText Esc cancellation and new request in {time.monotonic()-cancelled_at:.2f}s", flush=True)
            if not relay.waiting_body.wait(3):
                raise TimeoutError("Native partial-body response did not begin")
            qmp.key("esc")
            time.sleep(0.2)
            for character in "exercise native tools":
                qmp.key("spc" if character == " " else character)
            qmp.key("ret")
            if not relay.done.wait(60):
                raise TimeoutError(f"Native tool loop stopped at step {relay.step}")
            if relay.error:
                raise relay.error
            # Give SimpleText a frame to display the returned final response.
            time.sleep(0.5)
            qmp.command("screendump", {"filename": str(output / "final.ppm")})
            qmp.command("quit")
            qemu.communicate(timeout=10)
            edited = run("mtype", "-i", image_spec, "::/work/seed.txt").decode("utf-8")
            created = run("mtype", "-i", image_spec, "::/work/created.txt").decode("utf-8")
            unchanged = run("mtype", "-i", image_spec, "::/outside.txt").decode("utf-8")
            assert edited == "native 中 419"
            assert created == "UEFI native file\n"
            assert unchanged == "must not be read"
            (output / "results.json").write_text(json.dumps({"model_rounds": relay.step, "edited": edited, "created": created, "outside": unchanged}), encoding="utf-8")
            print("PASS native SimpleText input, TCP4 model relay, local FAT read/edit/write/list, and traversal rejection")
    finally:
        if qmp:
            qmp.close()
        if qemu:
            if qemu.poll() is None:
                qemu.terminate()
            _, errors = qemu.communicate(timeout=10)
            (output / "qemu.log").write_bytes(errors)
        relay.close()


if __name__ == "__main__":
    main()
