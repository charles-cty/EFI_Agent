# /// script
# dependencies = ["pyte>=0.8.2,<0.9"]
# ///
"""Check optional firmware capabilities and long UEFI residence through real serial I/O."""
import argparse
import codecs
from pathlib import Path
import socket
import subprocess
import time

import pyte


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--qemu", required=True)
    parser.add_argument("--code", type=Path, required=True)
    parser.add_argument("--vars", type=Path, required=True)
    parser.add_argument("--esp", type=Path, required=True)
    parser.add_argument("--accel", choices=["kvm", "whpx"], required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--hold-seconds", type=int, default=310)
    parser.add_argument("--rng", action="store_true")
    args = parser.parse_args()
    if args.hold_seconds < 0:
        parser.error("hold-seconds must be non-negative")
    args.output.mkdir(parents=True, exist_ok=True)
    with socket.socket() as listener:
        listener.bind(("127.0.0.1", 0))
        listener.listen()
        listener.settimeout(30)
        port = listener.getsockname()[1]
        command = [
            args.qemu, "-machine", f"q35,accel={args.accel}", "-m", "128",
            "-display", "none", "-monitor", "none", "-serial", "none", "-no-reboot",
            "-net", "none",
            "-drive", f"if=pflash,format=raw,readonly=on,file={args.code.resolve()}",
            "-drive", f"if=pflash,format=raw,snapshot=on,file={args.vars.resolve()}",
            "-drive", f"if=none,id=esp,format=raw,readonly=on,file=fat:ro:{args.esp.resolve()}",
            "-device", "virtio-blk-pci,drive=esp", "-device", "virtio-serial-pci",
            "-chardev", f"socket,id=terminal,host=127.0.0.1,port={port}",
            "-device", "virtconsole,chardev=terminal",
        ]
        if args.rng:
            command += ["-object", "rng-builtin,id=rng0", "-device", "virtio-rng-pci,rng=rng0"]
        with (args.output / "qemu.log").open("wb") as log:
            child = subprocess.Popen(command, stdout=subprocess.DEVNULL, stderr=log)
            wire = bytearray()
            screen = pyte.Screen(120, 40)
            stream = pyte.Stream(screen)
            decoder = codecs.getincrementaldecoder("utf-8")("replace")
            try:
                connection, _ = listener.accept()
                with connection:
                    connection.settimeout(0.2)
                    def wait_screen(marker, seconds=30):
                        deadline = time.monotonic() + seconds
                        while time.monotonic() < deadline:
                            try:
                                chunk = connection.recv(16384)
                                if not chunk:
                                    raise RuntimeError("Guest disconnected")
                                wire.extend(chunk)
                                stream.feed(decoder.decode(chunk))
                            except socket.timeout:
                                pass
                            if marker in "\n".join(screen.display):
                                return
                            if child.poll() is not None:
                                raise RuntimeError("QEMU exited")
                        raise TimeoutError(f"Guest did not show {marker}")
                    # Wait for readiness before sending any terminal input.
                    deadline = time.monotonic() + 60
                    while b"\x1b]777;efi-agent;ready\x07" not in wire:
                        if time.monotonic() >= deadline:
                            raise TimeoutError("No application readiness marker")
                        try:
                            chunk = connection.recv(16384)
                            if not chunk:
                                raise RuntimeError("Guest disconnected before readiness")
                            wire.extend(chunk)
                            stream.feed(decoder.decode(chunk))
                        except socket.timeout:
                            pass
                    connection.sendall(b"\x1b[8;40;120t")
                    wait_screen("What would you like to build?")
                    connection.sendall(b"/caps\r")
                    wait_screen("Cryptographic RNG:")
                    wait_screen("TCP4 interfaces: 0")
                    print("PASS no-NIC boot and explicit network/RNG capability report", flush=True)
                    print("\n".join(line.rstrip() for line in screen.display if "RNG:" in line), flush=True)
                    started = time.monotonic()
                    while time.monotonic() - started < args.hold_seconds:
                        if child.poll() is not None:
                            raise RuntimeError("VM exited during watchdog residence check")
                        try:
                            chunk = connection.recv(16384)
                            if not chunk:
                                raise RuntimeError("Guest disconnected during residence check")
                            wire.extend(chunk)
                            stream.feed(decoder.decode(chunk))
                        except socket.timeout:
                            pass
                    connection.sendall(b"/clear\r/help\r")
                    wait_screen("Show commands")
                    print(f"PASS UEFI remains interactive after {args.hold_seconds} seconds", flush=True)
                    connection.sendall(b"/quit\r")
                    child.wait(timeout=15)
                    if child.returncode != 0:
                        raise RuntimeError(f"QEMU quit failed: {child.returncode}")
            finally:
                if child.poll() is None:
                    child.terminate()
                    child.wait(timeout=10)
                (args.output / "serial.bin").write_bytes(wire)
                (args.output / "screen.txt").write_text("\n".join(screen.display), encoding="utf-8")


if __name__ == "__main__":
    main()
