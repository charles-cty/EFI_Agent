# /// script
# dependencies = ["dissect.fat>=3,<4", "pillow>=11,<12", "pyte>=0.8.2,<0.9"]
# ///
"""Boot the packaged Shell, enter Agent, return to Shell, and verify FAT output.

Runs with Windows or Linux native tools. No provider or real credentials needed.
"""
import argparse
import json
import os
import re
from pathlib import Path
import shutil
import socket
import subprocess
import time

from PIL import Image
from dissect.fat import FATFS
from dissect.util.stream import RangeStream
import pyte


class Qmp:
    def __init__(self, port, process):
        deadline = time.monotonic() + 30
        while True:
            try:
                self.socket = socket.create_connection(("127.0.0.1", port), timeout=2)
                break
            except OSError:
                if process.poll() is not None or time.monotonic() >= deadline:
                    raise RuntimeError("QEMU QMP did not start")
                time.sleep(0.1)
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
            result = json.loads(self.stream.readline())
            if "error" in result:
                raise RuntimeError(result["error"])
            if "return" in result:
                return result["return"]

    def text(self, text):
        punctuation = {" ": "spc", "/": "slash", "\\": "backslash", ":": "semicolon",
                       ".": "dot", "-": "minus", "%": "5", ">": "dot", "_": "minus"}
        for character in text:
            key = punctuation.get(character, character.lower())
            keys = [{"type": "qcode", "data": key}]
            if character.isupper() or character in ":%>_":
                keys.insert(0, {"type": "qcode", "data": "shift"})
            self.command("send-key", {"keys": keys, "hold-time": 25})
            time.sleep(0.06)
        self.command("send-key", {"keys": [{"type": "qcode", "data": "ret"}], "hold-time": 25})

    def close(self):
        self.stream.close()
        self.socket.close()


def screen_text(serial):
    screen = pyte.Screen(160, 60)
    pyte.ByteStream(screen).feed(serial.read_bytes() if serial.exists() else b"")
    return "\n".join(screen.display)


def wait_text(serial, marker, process, timeout=60):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        text = screen_text(serial)
        if marker in text:
            return text
        if "UEFI terminal:" in text:
            raise AssertionError(f"Agent console failed:\n{serial.read_bytes().decode(errors='replace')}")
        if process.poll() is not None:
            raise RuntimeError(f"QEMU exited before {marker}")
        time.sleep(0.2)
    raise AssertionError(f"Missing {marker}:\n{screen_text(serial)}")


def binding_count(report):
    return int(re.search(r"Driver bindings: (\d+)", report)[1])


def run_case(args, output, package, decoy, network=True, driver_case=None):
    case_name = driver_case or ("no-network-rng" if not network else "second-volume" if decoy else "first-volume")
    case = output / case_name
    case.mkdir(exist_ok=True)
    image = case / "native.img"
    if driver_case:
        tree = case / "esp"
        if tree.exists():
            shutil.rmtree(tree)
        shutil.copytree(package / "native-esp", tree)
        drivers = tree / "EFI/AGENT/DRIVERS"
        drivers.mkdir(exist_ok=True)
        path = "\\EFI\\AGENT\\DRIVERS\\Probe.efi"
        if driver_case == "driver-resident":
            shutil.copyfile(args.driver, drivers / "Probe.efi")
        elif driver_case == "reject-application":
            shutil.copyfile(args.efi, drivers / "Probe.efi")
            shutil.copyfile(args.driver, drivers / "Later.efi")
        elif driver_case == "reject-manifest":
            path = "fs0:\\EFI\\AGENT\\DRIVERS\\Probe.efi"
        paths = [path]
        if driver_case == "reject-application":
            paths.append("\\EFI\\AGENT\\DRIVERS\\Later.efi")
        (tree / "EFI/AGENT/DRIVERS.JSON").write_text(json.dumps(paths), encoding="utf-8")
        image.unlink(missing_ok=True)
        subprocess.run([str(args.launcher.resolve()), "pack", str(tree), str(image)], check=True)
    else:
        shutil.copyfile(package / "efi-agent-native.img", image)
    serial = case / "serial.bin"
    serial.unlink(missing_ok=True)
    with socket.socket() as reservation:
        reservation.bind(("127.0.0.1", 0))
        port = reservation.getsockname()[1]
    command = [args.qemu, "-machine", "q35,accel=tcg", "-m", "256", "-display", "none",
               "-serial", f"file:{serial}", "-monitor", "none", "-no-reboot",
               "-drive", f"if=pflash,format=raw,readonly=on,file={args.code.resolve()}",
               "-drive", f"if=pflash,format=raw,snapshot=on,file={args.vars.resolve()}",
               "-qmp", f"tcp:127.0.0.1:{port},server=on,wait=off"]
    if network:
        command += ["-netdev", "user,id=network", "-device", "virtio-net-pci,netdev=network",
                    "-device", "virtio-rng-pci"]
    else:
        command += ["-net", "none"]
    if decoy:
        command += ["-drive", f"if=none,id=decoy,format=raw,file={output / 'decoy.img'}",
                    "-device", "virtio-blk-pci,drive=decoy,bootindex=2"]
    command += ["-drive", f"if=none,id=esp,format=raw,file={image}",
                "-device", "virtio-blk-pci,drive=esp,bootindex=1"]
    qmp = None
    with (case / "qemu.log").open("wb") as errors:
        process = subprocess.Popen(command, stdout=subprocess.DEVNULL, stderr=errors)
        try:
            qmp = Qmp(port, process)
            # The application's first frame clears the startup text. Wait for
            # its UI instead of a transient script message.
            wait_text(serial, "Enter send", process)
            qmp.text("/caps")
            text = wait_text(serial, "UEFI Shell: protocol available", process)
            assert f"TCP4 interfaces: {int(network)}" in text, text
            if not network:
                assert "Unsupported; TLS randomness unavailable" in text, text
            if driver_case == "driver-resident":
                assert "Extra drivers started: 1" in text, text
                baseline = (output / "first-volume/caps.txt").read_text(encoding="utf-8")
                assert binding_count(text) == binding_count(baseline) + 1, text
            elif driver_case:
                assert "Driver loading stopped:" in text, text
            (case / "caps.txt").write_text(text, encoding="utf-8")
            screenshot = case / "caps.ppm"
            qmp.command("screendump", {"filename": str(screenshot)})
            rendered = Image.open(screenshot).convert("RGB")
            rendered.save(case / "caps.png")
            # The Agent heading is cyan. A serial-only interface leaves the
            # gray/yellow Shell boot screen on the actual display.
            assert sum(r < 40 and g > 180 and b > 180 for r, g, b in rendered.getdata()) > 20
            qmp.text("/exit")
            wait_text(serial, "EFI Agent returned.", process)
            if driver_case == "driver-resident":
                qmp.text("%homefilesystem%\\EFI\\AGENT\\AGENT.EFI")
                wait_text(serial, "Enter send", process)
                qmp.text("/caps")
                text = wait_text(serial, "Driver already resident:", process)
                assert "Extra drivers started: 0" in text, text
                assert binding_count(text) == binding_count(baseline) + 1, text
                qmp.text("/exit")
                wait_text(serial, "Shell>", process)
            qmp.text("echo SHELL_RETURN_731 > %homefilesystem%\\work\\shell-check.txt")
            time.sleep(1)
            qmp.text("echo %homefilesystem%")
            time.sleep(1)
            text = screen_text(serial)
            (case / "shell.txt").write_text(text, encoding="utf-8")
            if decoy:
                assert "FS1:" in text.upper(), text
            qmp.command("quit")
            process.wait(timeout=10)
        finally:
            if qmp:
                qmp.close()
            if process.poll() is None:
                process.terminate()
                process.wait(timeout=10)
    with image.open("rb") as disk:
        fs = FATFS(RangeStream(disk, 1048576, image.stat().st_size - 1048576))
        data = fs.get("/work/shell-check.txt").open().read()
        assert "SHELL_RETURN_731" in data.decode("utf-16"), data
        if driver_case == "driver-resident":
            assert fs.get("/work/driver-started.txt").open().read() == b"1"
        elif driver_case:
            try:
                fs.get("/work/driver-started.txt")
            except FileNotFoundError:
                pass
            else:
                raise AssertionError("Rejected driver executed")
    print(f"PASS {case.name}: displayed Agent, capability report, /exit -> Shell, FAT command output")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("launcher", "efi", "code", "vars", "output"):
        parser.add_argument(f"--{name}", type=Path, required=True)
    parser.add_argument("--qemu", default="qemu-system-x86_64")
    parser.add_argument("--driver", type=Path, help="Optional EFI boot-service driver fixture")
    parser.add_argument("--case", choices=["driver-resident", "reject-application", "reject-manifest", "missing-driver"],
                        help="Run one driver case (requires --driver)")
    args = parser.parse_args()
    if args.case and not args.driver:
        parser.error("--case requires --driver")
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    package = output / "package"
    settings = {key: value for key, value in os.environ.items() if not key.startswith("EFI_AGENT_")}
    settings.update(EFI_AGENT_API_BASE="https://provider.test/v1", EFI_AGENT_API_KEY="smoke-key",
                    EFI_AGENT_MODEL="smoke-model")
    subprocess.run([str(args.launcher.resolve()), "package", str(args.efi.resolve()), str(package)],
                   check=True, env=settings)
    assert (package / "esp/EFI/BOOT/BOOTX64.EFI").read_bytes() == args.efi.read_bytes()
    assert (package / "native-esp/EFI/AGENT/AGENT.EFI").read_bytes() == args.efi.read_bytes()
    assert not (package / "native-esp/EFI/AGENT/VM.TXT").exists()
    assert (package / "native-esp/EFI/BOOT/BOOTX64.EFI").read_bytes() == (
        package / "native-esp/EFI/TOOLS/SHELLX64.EFI").read_bytes()
    decoy = output / "decoy"
    decoy.mkdir(exist_ok=True)
    (decoy / "DECOY.TXT").write_text("Not the Agent boot volume", encoding="ascii")
    (decoy / "EFI/BOOT").mkdir(parents=True, exist_ok=True)
    (decoy / "EFI/BOOT/BOOTX64.EFI").write_bytes(b"Not a loadable EFI image")
    (output / "decoy.img").unlink(missing_ok=True)
    subprocess.run([str(args.launcher.resolve()), "pack", str(decoy), str(output / "decoy.img")], check=True)
    if args.case:
        if args.case == "driver-resident":
            run_case(args, output, package, False)
        run_case(args, output, package, False, driver_case=args.case)
        return
    for second_volume in (False, True):
        run_case(args, output, package, second_volume)
    run_case(args, output, package, False, network=False)
    if args.driver:
        for driver_case in ("driver-resident", "reject-application", "reject-manifest", "missing-driver"):
            run_case(args, output, package, False, driver_case=driver_case)


if __name__ == "__main__":
    main()
