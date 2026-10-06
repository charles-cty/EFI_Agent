# /// script
# dependencies = ["pyte>=0.8.2,<0.9", "cryptography>=44"]
# ///
"""Verify native UEFI text input and direct HTTPS file tools under OVMF."""
import argparse
import json
from pathlib import Path
import socket
import ssl
import subprocess
import tempfile
import threading
import time
from http.server import ThreadingHTTPServer
from smoke_vm import certificates, Provider, DNS, DNSService

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
    for name in ("launcher", "efi", "code", "vars", "output"):
        parser.add_argument(f"--{name}", type=Path, required=True)
    parser.add_argument("--qemu", default="qemu-system-x86_64")
    args = parser.parse_args()
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="efi-native-direct-") as temporary:
        temporary = Path(temporary)
        certificates(temporary)
        provider = ThreadingHTTPServer(("127.0.0.1", 0), Provider)
        provider.daemon_threads = True
        tls = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
        tls.load_cert_chain(temporary / "cert.pem", temporary / "key.pem")
        provider.socket = tls.wrap_socket(provider.socket, server_side=True)
        dns = DNSService(("127.0.0.1", 0), DNS)
        threading.Thread(target=provider.serve_forever, daemon=True).start()
        threading.Thread(target=dns.serve_forever, daemon=True).start()
        tree = temporary / "esp"
        (tree / "EFI/BOOT").mkdir(parents=True)
        (tree / "EFI/AGENT").mkdir()
        (tree / "work").mkdir()
        (tree / "EFI/BOOT/BOOTX64.EFI").write_bytes(args.efi.read_bytes())
        (tree / "EFI/AGENT/CA.DER").write_bytes((temporary / "ca.der").read_bytes())
        config = {"api_base": f"https://provider.test:{provider.server_port}/v1", "api_key": "smoke-key", "model": "smoke-model", "dns_address": [10, 0, 2, 2], "dns_port": dns.server_address[1], "workspace": "\\work", "ca_certificate": "\\EFI\\AGENT\\CA.DER"}
        (tree / "EFI/AGENT/CONFIG.JSON").write_text(json.dumps(config))
        (tree / "work/needle.txt").write_text("SEED_731")
        (tree / "work/ambiguous.txt").write_text("aaa")
        image = temporary / "native.img"
        subprocess.run([str(args.launcher.resolve()), "pack", str(tree), str(image)], check=True)
        qmp_path = temporary / "qmp.sock"
        qemu = subprocess.Popen([args.qemu, "-machine", "q35,accel=kvm", "-m", "256", "-display", "none", "-serial", "none", "-monitor", "none", "-no-reboot",
                                 "-drive", f"if=pflash,format=raw,readonly=on,file={args.code.resolve()}",
                                 "-drive", f"if=pflash,format=raw,snapshot=on,file={args.vars.resolve()}",
                                 "-drive", f"if=none,id=esp,format=raw,file={image}", "-device", "virtio-blk-pci,drive=esp",
                                 "-qmp", f"unix:{qmp_path},server=on,wait=off",
                                 "-netdev", "user,id=network", "-device", "virtio-net-pci,netdev=network", "-cpu", "max"], stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)
        qmp = None
        try:
            deadline = time.monotonic() + 30
            while not qmp_path.exists():
                if qemu.poll() is not None or time.monotonic() > deadline:
                    raise RuntimeError("QEMU did not start")
                time.sleep(.1)
            qmp = Qmp(qmp_path)
            previous = None
            stable = 0
            while time.monotonic() < deadline:
                qmp.command("screendump", {"filename": str(output / "boot.ppm")})
                data = (output / "boot.ppm").read_bytes()
                stable = stable + 1 if data == previous and len(set(data[-100000:])) > 1 else 0
                previous = data
                if stable >= 5:
                    break
                time.sleep(.3)
            for character in "Exercise the file tools":
                if character.isupper():
                    qmp.command("send-key", {"keys": [{"type": "qcode", "data": "shift"}, {"type": "qcode", "data": character.lower()}], "hold-time": 20})
                    time.sleep(.04)
                else:
                    qmp.key("spc" if character == " " else character)
            qmp.key("ret")
            deadline = time.monotonic() + 60
            while len(Provider.requests) < 5 and time.monotonic() < deadline:
                if Provider.errors:
                    raise AssertionError(Provider.errors)
                time.sleep(.1)
            assert len(Provider.requests) == 5, len(Provider.requests)
            time.sleep(.3)
            qmp.command("screendump", {"filename": str(output / "final.ppm")})
            qmp.command("quit")
            qemu.wait(timeout=10)
            spec = f"{image}@@1048576"
            for name, expected in [("needle.txt", "edited 中 853".encode()), ("created.txt", b"created 419\n"), ("ambiguous.txt", b"aaa")]:
                assert subprocess.check_output(["mtype", "-i", spec, f"::/work/{name}"]) == expected
            print("PASS native SimpleText keyboard, direct DNS/verified HTTPS, five model rounds and actual FAT file bytes", flush=True)
        finally:
            if qmp:
                qmp.close()
            if qemu.poll() is None:
                qemu.terminate()
            _, errors = qemu.communicate(timeout=10)
            (output / "qemu.log").write_bytes(errors)
            provider.shutdown()
            provider.server_close()
            dns.shutdown()
            dns.server_close()


if __name__ == "__main__":
    main()
