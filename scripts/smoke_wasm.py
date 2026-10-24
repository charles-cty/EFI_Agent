# /// script
# dependencies = ["pyte>=0.8.2,<0.9", "dissect.fat>=3,<4", "cryptography>=44", "asyncssh>=2.21"]
# ///
"""Verify the UEFI wasmi dispatcher and applications with Windows WHPX."""
import argparse
import hashlib
import json
import re
from pathlib import Path
import shutil
import socket
import ssl
import subprocess
import time
import threading

import pyte
from dissect.fat import FATFS
from dissect.util.stream import RangeStream
from wasm_directory_fixture import expected_glob, stage_directory, verify_directory_artifacts
from wasm_file_fixture import resize_contents
from test_curl import BODY as CURL_BODY, UPLOAD as CURL_UPLOAD, create_http_server
from wasm_tls_fixture import tls_context
from test_wolfssh import SshPeer, STDOUT as SSH_STDOUT, STDERR as SSH_STDERR
from wasm_heredoc_fixture import heredoc_cases, stage_heredocs
from wasm_math_fixture import math_cases, stage_math
from wasm_descriptor_fixture import descriptor_cases, stage_descriptors
from wasm_namespace_fixture import FatWorkspace

PROGRAMS = ("child", "parent", "loop", "trap", "producer", "copy", "pipeline", "capture", "poll", "clock", "dual", "shell_capture", "environment")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("launcher", "efi", "code", "vars", "fixtures", "output"):
        parser.add_argument(f"--{name}", type=Path, required=True)
    parser.add_argument("--qemu", default="qemu-system-x86_64.exe")
    parser.add_argument("--memory", type=int, default=256, help="QEMU RAM in MiB.")
    parser.add_argument("--command-timeout", type=float, default=45,
                        help="Maximum seconds for each firmware command.")
    parser.add_argument("--quickjs", type=Path)
    parser.add_argument("--python", type=Path)
    parser.add_argument("--output-panel-test", action="store_true",
                        help="Verify one collapsed output disclosure per WASM run.")
    parser.add_argument("--curl", type=Path)
    parser.add_argument("--wolfssh", type=Path)
    parser.add_argument("--curl-tls", action="store_true")
    parser.add_argument("--sjlj", type=Path)
    parser.add_argument("--ash-exec", type=Path)
    parser.add_argument("--ash", type=Path)
    parser.add_argument("--ash-child", type=Path)
    parser.add_argument("--ash-tasks", action="store_true")
    parser.add_argument("--ash-heredocs", action="store_true")
    parser.add_argument("--ash-math", action="store_true")
    parser.add_argument("--ash-task-input", action="store_true")
    parser.add_argument("--ash-descriptors", action="store_true")
    parser.add_argument("--descriptor-mapping", action="store_true")
    parser.add_argument("--ash-read", action="store_true")
    parser.add_argument("--ash-printf", action="store_true")
    parser.add_argument("--filesystem", action="store_true")
    parser.add_argument("--directories", action="store_true")
    parser.add_argument("--random", action="store_true")
    parser.add_argument("--network", action="store_true")
    parser.add_argument("--dns", action="store_true")
    parser.add_argument("--posix-network", action="store_true")
    parser.add_argument("--posix-connect-error", choices=["timeout", "refused", "reset"], default="reset",
                        help="Expected error for the non-listening host port; Windows normally sends TCP RST.")
    parser.add_argument("--no-rdrand", action="store_true")
    args = parser.parse_args()
    if args.dns and not args.network:
        parser.error("--dns requires --network")
    if args.posix_network and not args.dns:
        parser.error("--posix-network requires --dns")
    if args.curl and not args.network:
        parser.error("--curl requires --network")
    if args.curl_tls and not args.curl:
        parser.error("--curl-tls requires --curl")
    if args.wolfssh and not (args.network and args.ash):
        parser.error("--wolfssh requires --network and --ash")
    refused_listener = None
    dns_listener = None
    dns_thread = None
    dns_errors = []
    if args.dns:
        dns_listener = socket.socket()
        dns_listener.bind(("0.0.0.0", 0))
        dns_listener.listen()
        dns_listener.settimeout(45)
    if args.memory <= 0:
        parser.error("--memory must be positive")
    if args.command_timeout <= 0:
        parser.error("--command-timeout must be positive")
    if args.python and not args.ash:
        parser.error("--python requires --ash")
    if args.no_rdrand and not args.random:
        parser.error("--no-rdrand requires --random")
    if args.ash_child and not args.ash:
        parser.error("--ash-child requires --ash")
    if args.ash_tasks and not args.ash_child:
        parser.error("--ash-tasks requires --ash-child")
    if args.ash_heredocs and not args.ash_child:
        parser.error("--ash-heredocs requires --ash-child")
    if args.ash_math and not args.ash_child:
        parser.error("--ash-math requires --ash-child")
    if args.ash_task_input and not args.ash:
        parser.error("--ash-task-input requires --ash")
    if args.ash_descriptors and not args.ash_child:
        parser.error("--ash-descriptors requires --ash-child")
    if args.ash_read and not args.ash_child:
        parser.error("--ash-read requires --ash-child")
    if args.ash_printf and not args.ash_read:
        parser.error("--ash-printf requires --ash-read")
    if args.directories and not args.filesystem:
        parser.error("--directories requires --filesystem")
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    tree = output / "esp"
    if tree.exists():
        shutil.rmtree(tree)
    (tree / "EFI/BOOT").mkdir(parents=True)
    (tree / "EFI/AGENT").mkdir()
    (tree / "work").mkdir()
    if args.ash_descriptors:
        stage_descriptors(tree / "work")
    shutil.copyfile(args.efi, tree / "EFI/BOOT/BOOTX64.EFI")
    if args.descriptor_mapping:
        shutil.copyfile(args.fixtures / "descriptor_mapping.wasm", tree / "work/descriptor_mapping.wasm")
    if args.ash_task_input:
        shutil.copyfile(args.fixtures / "ash_task_input.wasm", tree / "work/ash_task_input.wasm")
    if args.random:
        shutil.copyfile(args.fixtures / "random.wasm", tree / "work/random.wasm")
    if args.network:
        shutil.copyfile(args.fixtures / "network.wasm", tree / "work/network.wasm")
        if args.posix_network:
            shutil.copyfile(args.fixtures / "posix_network.wasm", tree / "work/posix_network.wasm")
            refused_listener = socket.socket()
            refused_listener.bind(("0.0.0.0", 0))
        config = {"api_base": "http://10.0.2.2:1/v1", "api_key": "local-test",
                  "model": "local-test", "workspace": "\\work",
                  "ipv4": {"address": [10, 0, 2, 15], "subnet_mask": [255, 255, 255, 0], "gateway": [10, 0, 2, 2]}}
        if args.dns:
            config["dns_address"] = [10, 0, 2, 2]
            config["dns_port"] = dns_listener.getsockname()[1]
        (tree / "EFI/AGENT/CONFIG.JSON").write_text(json.dumps(config), encoding="utf-8")
    if args.python:
        shutil.copyfile(args.python, tree / "work/python.wasm")
        shutil.copytree(args.python.parent / "lib", tree / "work/lib", dirs_exist_ok=True)
        (tree / "work/panel-demo.py").write_text(
            "import subprocess\n"
            "r = subprocess.run('echo PANEL_STDOUT;echo PANEL_STDERR >&2',shell=True,capture_output=True,text=True,check=True)\n"
            "print(r.stdout,end='')\nprint(r.stderr,end='')\n"
            "subprocess.run('echo PANEL_DIRECT;echo PANEL_ERROR_DIRECT >&2',shell=True,check=True)\n",
            encoding="utf-8")
        (tree / "work/python-duplex-input.bin").write_bytes(
            bytes((i * 37 + i // 251 + 19) % 256 for i in range(196613)))
        (tree / "work/port_probe.py").write_text("value = 731\n", encoding="utf-8")
        (tree / "work/python-standard.py").write_text(r'''
import subprocess, port_probe
assert port_probe.value == 731
with open('python-duplex-input.bin', 'rb') as source:
    data = source.read()
code = "import sys;data=sys.stdin.buffer.read();sys.stderr.buffer.write(data[::-1]);sys.stdout.buffer.write(data)"
result = subprocess.run(['python', '-c', code], input=data, capture_output=True, check=True)
assert result.stdout == data and result.stderr == data[::-1]
with open('python-standard.bin', 'wb') as target:
    target.write(result.stdout)
print('UEFI_PYTHON_STANDARD_SCRIPT_OK')
''', encoding="utf-8")
    if args.wolfssh:
        shutil.copyfile(args.wolfssh, tree / "work/wolfssh.wasm")
        (tree / "work/ssh-password.txt").write_bytes(b"secret\r\n")
        (tree / "work/ssh-wrong-password.txt").write_bytes(b"incorrect")
    if args.curl:
        shutil.copyfile(args.curl, tree / "work/curl.wasm")
        (tree / "work/curl-upload.bin").write_bytes(CURL_UPLOAD)
    tls_cases = []
    if args.curl_tls:
        for name in ["tls12", "tls13", "untrusted", "no_signer", "hostname", "expired"]:
            failure = None if name in ["tls12", "tls13"] else name
            directory = output / "tls" / name
            directory.mkdir(parents=True, exist_ok=True)
            context = tls_context(directory, failure)
            if name in ["tls12", "tls13"]:
                version = ssl.TLSVersion.TLSv1_2 if name == "tls12" else ssl.TLSVersion.TLSv1_3
                context.minimum_version = version
                context.maximum_version = version
            shutil.copyfile(directory / "curl-ca.pem", tree / "work" / f"curl-{name}-ca.pem")
            server, errors = create_http_server("0.0.0.0")
            server.socket = context.wrap_socket(server.socket, server_side=True)
            server.daemon_threads = True
            thread = threading.Thread(target=server.serve_forever, daemon=True)
            tls_cases.append((name, server, errors, thread))
    (tree / "EFI/AGENT/VM.TXT").write_text("serial\n", encoding="utf-8")
    for name in PROGRAMS:
        shutil.copyfile(args.fixtures / f"{name}.wasm", tree / "work" / f"{name}.wasm")
    shutil.copyfile(args.fixtures / "child.wasm", tree / "work/child.variant.WASM")
    shutil.copyfile(args.fixtures / "child.wasm", tree / "work/extensionless")
    if args.filesystem:
        shutil.copyfile(args.fixtures / "filesystem.wasm", tree / "work/filesystem.wasm")
        (tree / "work/subdir").mkdir()
        (tree / "work/subdir/λ.txt").write_bytes(b"xyz")
    if args.directories:
        shutil.copyfile(args.fixtures / "directory.wasm", tree / "work/directory.wasm")
        directory_entries = stage_directory(tree / "work")
    if args.quickjs:
        shutil.copyfile(args.quickjs, tree / "work/quickjs.wasm")
    if args.sjlj:
        shutil.copyfile(args.sjlj, tree / "work/sjlj.wasm")
    if args.ash_exec:
        shutil.copyfile(args.ash_exec, tree / "work/ash_exec.wasm")
    if args.ash:
        shutil.copyfile(args.ash, tree / "work/ash.wasm")
        shutil.copytree(args.ash.parent / "share", tree / "work/share", dirs_exist_ok=True)
    if args.ash_child:
        shutil.copyfile(args.ash_child, tree / "work/ash_child.wasm")
    if args.ash_heredocs:
        stage_heredocs(tree / "work")
    if args.ash_math:
        stage_math(tree / "work")
    image = output / "disk.img"
    image.unlink(missing_ok=True)
    subprocess.run([str(args.launcher.resolve()), "pack", str(tree), str(image)], check=True)
    console = socket.socket()
    console.bind(("127.0.0.1", 0))
    console.listen()
    console.settimeout(45)
    command = [args.qemu, "-machine", "q35,accel=whpx", "-cpu", "qemu64,-rdrand" if args.no_rdrand else "qemu64,+rdrand", "-m", str(args.memory),
               "-display", "none", "-serial", "none", "-monitor", "none", "-no-reboot",
               "-net", "none",
               "-drive", f"if=pflash,format=raw,readonly=on,file={args.code.resolve()}",
               "-drive", f"if=pflash,format=raw,snapshot=on,file={args.vars.resolve()}",
               "-drive", f"if=none,id=esp,format=raw,file={image}", "-device", "virtio-blk-pci,drive=esp",
               "-device", "virtio-serial-pci",
               "-chardev", f"socket,id=terminal,host=127.0.0.1,port={console.getsockname()[1]}",
               "-device", "virtconsole,chardev=terminal"]
    network_listener = None
    network_thread = None
    network_errors = []
    network_progress = threading.Event()
    network_cancelled = threading.Event()
    network_observations = {"accepted": False, "request_bytes": 0, "child_progress": False, "response_bytes": 0, "cancel_closed": False, "graceful_bytes": 0, "graceful_eof": False}
    if args.network:
        network_option = command.index("-net")
        del command[network_option:network_option + 2]
        command.extend(["-netdev", "user,id=net", "-device", "e1000,netdev=net"])
        if args.posix_network:
            command.extend(["-object", f"filter-dump,id=packet-record,netdev=net,file={output / 'network.pcap'}"])
        network_listener = socket.socket()
        network_listener.bind(("0.0.0.0", 0))
        network_listener.listen()
        network_listener.settimeout(45)
        network_port = network_listener.getsockname()[1]
        def network_server():
            try:
                with network_listener.accept()[0] as peer:
                    network_observations["accepted"] = True
                    peer.settimeout(45)
                    request = bytearray()
                    while len(request) < 196613:
                        chunk = peer.recv(min(613, 196613 - len(request)))
                        assert chunk, "Early request EOF"
                        request.extend(chunk)
                        network_observations["request_bytes"] = len(request)
                    assert request == bytes((i * 37 + i // 251 + 19) % 256 for i in range(196613))
                    assert network_progress.wait(45), "Guest child blocked by TCP wait"
                    network_observations["child_progress"] = True
                    response = bytes((i * 53 + i // 239 + 7) % 256 for i in range(131089))
                    for offset in range(0, len(response), 317):
                        peer.sendall(response[offset:offset + 317])
                        network_observations["response_bytes"] += len(response[offset:offset + 317])
                    peer.shutdown(socket.SHUT_WR)
                with network_listener.accept()[0] as cancelled:
                    cancelled.settimeout(15)
                    assert network_cancelled.wait(45), "No cancellation requested"
                    try:
                        assert cancelled.recv(1) == b"", "Unexpected cancelled socket payload"
                    except ConnectionResetError:
                        pass
                    network_observations["cancel_closed"] = True
                with network_listener.accept()[0] as graceful:
                    graceful.settimeout(30)
                    received = bytearray()
                    while True:
                        chunk = graceful.recv(419)
                        if not chunk:
                            break
                        received.extend(chunk)
                        network_observations["graceful_bytes"] = len(received)
                    assert received == bytes((i * 37 + i // 251 + 19) % 256 for i in range(196613))
                    network_observations["graceful_eof"] = True
                with network_listener.accept()[0] as nonblocking:
                    nonblocking.settimeout(30)
                    assert nonblocking.recv(1) == b"\xa7", "Nonblocking handshake"
                    nonblocking.sendall(bytes([19, 73, 211]))
                    nonblocking.shutdown(socket.SHUT_WR)
                    assert nonblocking.recv(1) == b"", "Nonblocking descriptor closure"
                    network_observations["nonblocking_eof"] = True
                with network_listener.accept()[0] as asynchronous:
                    asynchronous.settimeout(30)
                    assert asynchronous.recv(1) == b"\xa7", "Async handshake"
                    asynchronous.sendall(bytes([19, 73, 211]))
                    asynchronous.shutdown(socket.SHUT_WR)
                    assert asynchronous.recv(1) == b"", "Async descriptor closure"
                    network_observations["async_eof"] = True
                if args.posix_network:
                    with network_listener.accept()[0] as posix:
                        posix.settimeout(30)
                        assert posix.recv(1) == b"\xa7", "POSIX handshake"
                        for byte in [19, 73, 211]:
                            posix.sendall(bytes([byte]))
                        posix.shutdown(socket.SHUT_WR)
                        assert posix.recv(1) == b"", "POSIX descriptor closure"
                        network_observations["posix_eof"] = True
            except BaseException as failure:
                network_errors.append(failure)
        network_thread = threading.Thread(target=network_server, daemon=True)
    wire = bytearray()
    screen = pyte.Screen(110, 40)
    stream = pyte.ByteStream(screen)
    frame_bytes = bytearray()
    view_prefix = b"\x1b]778;efi-agent;view;"
    latest_view = {}
    terminal_pids = set()
    reaped_pids = set()

    def feed_frames(data):
        nonlocal latest_view
        frame_bytes.extend(data)
        while True:
            start = frame_bytes.find(view_prefix)
            if start < 0:
                return
            payload = start + len(view_prefix)
            end = frame_bytes.find(b"\x07", payload)
            if end < 0:
                return
            view = json.loads(frame_bytes[payload:end])
            latest_view = view
            width, height = view["dimensions"]
            if (screen.columns, screen.lines) != (width, height):
                screen.resize(lines=height, columns=width)
            stream.feed(bytes(frame_bytes[:start]))
            del frame_bytes[:end + 1]
            visible = "\n".join(screen.display)
            terminal_pids.update(int(pid) for pid in
                                 re.findall(r'PID (\d+): (?:Completed|Failed|Cancelled)', visible)
                                 if int(pid) not in reaped_pids)
            if "NETWORK_PEER_PROGRESS" in visible:
                network_progress.set()

    connection = None
    curl_server = None
    curl_thread = None
    ssh_peer = None
    if args.curl:
        curl_server, curl_errors = create_http_server("0.0.0.0")
        curl_server.daemon_threads = True
        curl_thread = threading.Thread(target=curl_server.serve_forever, daemon=True)
        curl_thread.start()
    if args.wolfssh:
        ssh_peer = SshPeer("0.0.0.0").__enter__()
    for _, _, _, thread in tls_cases:
        thread.start()
    with (output / "qemu.log").open("wb") as log:
        qemu = subprocess.Popen(command, stdout=subprocess.DEVNULL, stderr=log)
        try:
            connection, _ = console.accept()
            connection.settimeout(0.1)
            ready = False

            def wait(marker, complete_line=False, panel_pid=None):
                nonlocal ready
                deadline = time.monotonic() + args.command_timeout
                opening = None
                scrolling = False
                while time.monotonic() < deadline:
                    try:
                        data = connection.recv(65536)
                        if not data:
                            raise EOFError("Guest closed console")
                        wire.extend(data)
                        feed_frames(data)
                        if not ready and b"\x1b]777;efi-agent;ready\x07" in wire:
                            connection.sendall(b"\x1b[8;40;110t")
                            ready = True
                    except socket.timeout:
                        pass
                    text = "\n".join(screen.display)
                    input_empty = all(first == end for _, first, end in latest_view.get("input_rows", []))
                    if marker == "Ready":
                        if input_empty and screen.display[-1].lstrip().startswith(("Ready", "Conversation cleared")):
                            return text
                        continue
                    # Do not send mouse/scroll input while the command is being
                    # entered: those events can move its editor cursor.
                    if not input_empty:
                        continue
                    if "Cancelled (exit code" in marker or marker == "exit code 130)":
                        # Do not interleave a mouse escape sequence with Esc cancellation.
                        if marker in text:
                            return text
                        continue
                    # A control reply can appear below an older collapsed panel.
                    # Accept its specific response before opening historical output.
                    if marker.startswith(("Reaped ", "Loaded ", "Cancelled ", "Conversation cleared")) and marker in text:
                        return text
                    if not args.output_panel_test:
                        latest_panel = next(((row, line) for row, line in reversed(list(enumerate(screen.display, 1)))
                                             if "WASM:" in line and ("[+]" in line or "[-]" in line)
                                             and (panel_pid is None or re.search(rf'PID {panel_pid}:', line))), None)
                        collapsed = latest_panel if latest_panel and "[+]" in latest_panel[1] else None
                        if collapsed is not None:
                            row, line = collapsed
                            identity = re.search(r'PID (\d+)', line)
                            identity = identity[1] if identity else line
                            if opening != identity:
                                connection.sendall(f"\x1b[<0;3;{row}M\x1b[<0;3;{row}m".encode())
                                opening = identity
                            continue
                        if opening is not None:
                            connection.sendall(b"\x1b[B" * 40)
                            opening = None
                            scrolling = True
                            continue
                        if panel_pid is not None and latest_panel is None:
                            if latest_view.get("offset", 0):
                                connection.sendall(b"\x1b[A" * 20)
                            continue
                        if panel_pid is None and latest_view.get("offset", 0) != latest_view.get("max_offset", 0):
                            if not scrolling:
                                connection.sendall(b"\x1b[B" * 40)
                                scrolling = True
                            continue
                        scrolling = False
                    if (any(line.strip() == marker for line in text.splitlines())
                            if complete_line else marker in text):
                        return text
                    if qemu.poll() is not None:
                        raise RuntimeError(f"QEMU exited before {marker}")
                raise TimeoutError(f"Missing {marker}:\n{text}")

            def send(text, marker, complete_line=False):
                wait("Ready")
                print(f"RUN {text[:120]}", flush=True)
                connection.sendall(text.encode() + b"\r")
                visible = wait(marker, complete_line)
                wait("Ready")
                if text.startswith('/wasm reap '):
                    pid = int(text.split()[-1])
                    if f'Reaped {pid}:' in visible:
                        reaped_pids.add(pid)
                        terminal_pids.discard(pid)
                return visible

            def wait_event(event, message):
                # Keep draining the terminal while the guest handshakes.
                # Blocking the test thread on Event.wait can fill the virtio
                # console and stop guest execution before it reaches the peer.
                deadline = time.monotonic() + 45
                while not event.is_set() and time.monotonic() < deadline:
                    try:
                        data = connection.recv(65536)
                        if not data:
                            raise EOFError("Guest closed console")
                        wire.extend(data)
                        feed_frames(data)
                    except socket.timeout:
                        pass
                    if qemu.poll() is not None:
                        raise RuntimeError(f"QEMU exited: {message}")
                assert event.is_set(), message + "\n" + "\n".join(screen.display)

            wait("What would you like to build?")
            if args.output_panel_test:
                assert args.python and args.ash, "Output panel test needs --python and --ash"
                for name in ("ash", "python", "loop", "child", "trap"):
                    send(f"/wasm load {name}.wasm", f"Loaded {name}")
                send("/clear", "Conversation cleared")
                text = send("/wasm run python panel-demo.py", "Completed (exit code 0)")
                assert text.count("[+] WASM:") == 1, text
                assert "PANEL_STDOUT" not in text and "PANEL_DIRECT" not in text, text
                assert "Process " not in text and "wasm stderr" not in text, text
                connection.sendall(b"\t\r")
                text = wait("PANEL_DIRECT")
                assert "PANEL_STDOUT" in text and "PANEL_STDERR" in text, text
                assert "Standard output:" in text, text
                assert "Standard error:" in text and "PANEL_ERROR_DIRECT" in text, text
                assert text.count("[-] WASM:") == 1, text
                connection.sendall(b"\r")
                text = wait("[+] WASM:")
                assert "PANEL_STDOUT" not in text and "PANEL_DIRECT" not in text, text
                send("/clear", "Conversation cleared")
                text = send("/wasm run ash -c exit${IFS}130", "Failed (exit code 130)")
                assert "Cancelled" not in text, text
                for name, status in [("child", "Failed (exit code 37)"), ("trap", "Failed:")]:
                    send("/clear", "Conversation cleared")
                    text = send(f"/wasm run {name}", status)
                    assert text.count("[+] WASM:") == 1, text
                    if name == "child":
                        assert "WASM_CHILD_731" not in text, text
                        connection.sendall(b"\t\r")
                        wait("WASM_CHILD_731")
                send("/clear", "Conversation cleared")
                send("/wasm run loop", "WASM: loop")
                connection.sendall(b"\x1b")
                text = wait("Cancelled (exit code 130)")
                assert text.count("[+] WASM:") == 1, text
                assert "Process " not in text, text
                send("/clear", "Conversation cleared")
                text = send("/wasm start loop", ": Running")
                background = re.search(r'PID (\d+): Running', text)[1]
                text = send("/wasm run child", "Failed (exit code 37)")
                assert text.count("[+] WASM:") == 2, text
                assert f"PID {background}: Running" in text, text
                send("/clear", "Conversation cleared")
                send(f"/wasm kill {background}", f"Cancelled {background}")
                text = wait("Cancelled (exit code 130)")
                assert text.count("[+] WASM:") == 1, text
                connection.sendall(b"/exit\r")
                qemu.wait(timeout=15)
                assert qemu.returncode == 0, qemu.returncode
                print("PASS actual UEFI WASM output: one collapsed panel, plain text, stdout/capture/direct output, child grouping, click-free keyboard disclosure, failure, trap and cancellation")
                return
            module_paths = [
                "child.wasm", "./child.wasm", ".\\child.wasm",
                "/child.wasm", "\\child.wasm", "/./child.wasm",
            ]
            for path in module_paths:
                send("/clear", "Conversation cleared")
                send(f"/wasm load {path}", "Loaded child")
            send("/wasm load child_alias child.wasm", "Loaded child_alias")
            send("/wasm load child.variant.WASM", "Loaded child.variant")
            send("/wasm load extensionless", "Loaded extensionless")
            send("/wasm load missing-module.wasm",
                 "Open 'missing-module.wasm': NotFound")
            send("/wasm load fs0:child.wasm", "Invalid workspace path:")
            send("/wasm load ../child.wasm", "Invalid workspace path:")
            send("/wasm load .wasm", "Module path must include a program name")
            send("/clear", "Conversation cleared")
            send("/wasm load /", "Module path must include a program name")
            send("/wasm load", "Usage: /wasm load PATH")
            send("/clear", "Conversation cleared")
            send("/wasm load a b c", "Usage: /wasm load PATH")
            print("PASS UEFI module loading: inferred names, explicit aliases, both separators, absolute paths and rejected invalid paths/arguments")
            for name in PROGRAMS:
                send(f"/wasm load {name}.wasm", f"Loaded {name}")
            send("/wasm start loop", "PID 1: Running")
            text = send("/wasm run parent", "PID 2: Failed (exit code 37)")
            assert "WASM_CHILD_731" in text
            send("/wasm kill 1", "Cancelled 1")
            send("/wasm reap 1", "Reaped 1: Exited(130)")
            send("/wasm reap 2", "Reaped 2: Exited(37)")
            send("/wasm run trap", "PID 4: Failed:")
            send("/wasm run child", "PID 5: Failed (exit code 37)")
            connection.sendall(b"/wasm run loop\r")
            wait("/wasm run loop")
            connection.sendall(b"\x1b")
            wait("PID 6: Cancelled (exit code 130)")
            send("/wasm run child", "PID 7: Failed (exit code 37)")
            text = send("/wasm run pipeline", "PID 8: Completed (exit code 0)")
            assert "PIPELINE_BYTES_OK_196608" in text
            send("/wasm reap 9", "WASM: Unknown WASM process: 9")
            send("/wasm reap 10", "WASM: Unknown WASM process: 10")
            text = send("/wasm run capture", "PID 11: Completed (exit code 0)")
            assert "C_SPAWN_CAPTURE_OK" in text
            send("/wasm reap 12", "WASM: Unknown WASM process: 12")
            text = send("/wasm run poll", "PID 13: Completed (exit code 0)")
            assert "POLL_READY_OK" in text
            send("/wasm reap 14", "WASM: Unknown WASM process: 14")
            text = send("/wasm run clock", "PID 15: Completed (exit code 0)")
            assert "FIRMWARE_CLOCK_WAIT_OK" in text
            text = send("/wasm run shell_capture", "PID 16: Completed (exit code 0)")
            assert "MULTI_CHILD_SHELL_CAPTURE_OK" in text
            for pid in (17, 18, 19):
                send(f"/wasm reap {pid}", f"WASM: Unknown WASM process: {pid}")
            text = send("/wasm run environment", "PID 20: Completed (exit code 0)")
            assert any(line.strip() == "PROCESS_ENVIRONMENT_OK" for line in text.splitlines())
            for child in (21, 22, 23, 24):
                send(f"/wasm reap {child}", f"WASM: Unknown WASM process: {child}")
            if args.quickjs:
                send("/wasm load quickjs.wasm", "Loaded quickjs")
                script = ("if(typeof(std)!=='undefined'||typeof(os)!=='undefined')throw(Error('host'));"
                          "if(await(Promise.resolve(42))!==42)throw(Error('promise'));"
                          "if(Date.now()<1700000000000)throw(Error('clock'));"
                          "print('QUICKJS_ENGINE_OK')")
                text = send(f"/wasm run quickjs -e {script}", "PID 25: Completed (exit code 0)")
                assert any(line.strip() == "QUICKJS_ENGINE_OK" for line in text.splitlines())
            if args.sjlj:
                pid = 26 if args.quickjs else 25
                send("/wasm load sjlj \\sjlj.wasm", "Loaded sjlj")
                text = send("/wasm run sjlj", f"PID {pid}: Completed (exit code 0)")
                assert any(line.strip() == "SJLJ_MULTI_INSTANCE_OK" for line in text.splitlines())
                for child in (pid + 1, pid + 2):
                    send(f"/wasm reap {child}", f"WASM: Unknown WASM process: {child}")
            if args.ash_exec:
                pid = 25 + int(bool(args.quickjs)) + 3 * int(bool(args.sjlj))
                send("/wasm load ash_exec \\ash_exec.wasm", "Loaded ash_exec")
                text = send("/wasm run ash_exec", f"PID {pid}: Failed (exit code 37)")
                assert any(line.strip() == "ASH_EXEC_FRESH_INSTANCE_OK" for line in text.splitlines())
                child = pid + 1
                send(f"/wasm reap {child}", f"WASM: Unknown WASM process: {child}")
            if args.ash:
                pid = 25 + int(bool(args.quickjs)) + 3 * int(bool(args.sjlj)) + 2 * int(bool(args.ash_exec))
                send("/wasm load ash.wasm", "Loaded ash")
                text = send("/wasm run ash -c echo${IFS}ASH_REAL_ENGINE_OK", f"PID {pid}: Completed (exit code 0)")
                assert any(line.strip() == "ASH_REAL_ENGINE_OK" for line in text.splitlines())
                if args.quickjs:
                    script = (r'''Promise.all([$`echo\x20${"a'b"}`,$`echo\x20second`])'''
                              r'''.then(r=>{if(r[0].stdout!=="a'b\n"||r[1].stdout!=='second\n')throw(Error('capture'));'''
                              r"print('QUICKJS_REAL_ASH_OK')})")
                    text = send(f"/wasm run quickjs -e {script}", f"PID {pid + 1}: Completed (exit code 0)")
                    assert any(line.strip() == "QUICKJS_REAL_ASH_OK" for line in text.splitlines())
                    for child in (pid + 2, pid + 3):
                        send(f"/wasm reap {child}", f"WASM: Unknown WASM process: {child}")
                if args.ash_child:
                    pid += 4 if args.quickjs else 1
                    send("/wasm load ash_child \\ash_child.wasm", "Loaded ash_child")
                    script = "ash_child${IFS}exit${IFS}17;echo${IFS}ASH_EXTERNAL_STATUS:$?"
                    text = send(f"/wasm run ash -c {script}", f"PID {pid}: Completed (exit code 0)")
                    assert any(line.strip() == "ASH_EXTERNAL_STATUS:17" for line in text.splitlines())
                    send(f"/wasm reap {pid + 1}", f"WASM: Unknown WASM process: {pid + 1}")
                    if args.quickjs:
                        pid += 2
                        script = (r"$`ash_child\x20exit\x2017`.then(()=>{throw(Error('status'))},"
                                  r"r=>{if(r.exitCode!==17)throw(Error('status'));print('QUICKJS_ASH_EXTERNAL_OK')})")
                        text = send(f"/wasm run quickjs -e {script}", f"PID {pid}: Completed (exit code 0)")
                        assert any(line.strip() == "QUICKJS_ASH_EXTERNAL_OK" for line in text.splitlines())
                        for child in (pid + 1, pid + 2):
                            send(f"/wasm reap {child}", f"WASM: Unknown WASM process: {child}")
                    if args.ash_tasks:
                        pid += 3 if args.quickjs else 2
                        script = "ash_child${IFS}produce|ash_child${IFS}copy|ash_child${IFS}verify"
                        text = send(f"/wasm run ash -c {script}", f"PID {pid}: Completed (exit code 0)")
                        assert any(line.strip() == "ASH_BOUNDED_PIPELINE_OK" for line in text.splitlines())
                        for child in range(pid + 1, pid + 7):
                            send(f"/wasm reap {child}", f"WASM: Unknown WASM process: {child}")
                        pid += 7
                        script = "x=parent;value=$(x=child;echo${IFS}$x);echo${IFS}$value:$x"
                        text = send(f"/wasm run ash -c {script}", f"PID {pid}: Completed (exit code 0)")
                        assert any(line.strip() == "child:parent" for line in text.splitlines())
                        send(f"/wasm reap {pid + 1}", f"WASM: Unknown WASM process: {pid + 1}")
                        if args.quickjs:
                            pid += 2
                            script = (r"if((await($`value=$(ash_child\x20produce|ash_child\x20copy|ash_child\x20verify);"
                                      r"echo\x20$value`)).stdout!=='ASH_BOUNDED_PIPELINE_OK\n')throw(Error('pipeline'));"
                                      r"print('UEFI_ASH_TASKS_OK')")
                            text = send(f"/wasm run quickjs -e {script}", f"PID {pid}: Completed (exit code 0)")
                            assert any(line.strip() == "UEFI_ASH_TASKS_OK" for line in text.splitlines())
                            for child in range(pid + 1, pid + 9):
                                send(f"/wasm reap {child}", f"WASM: Unknown WASM process: {child}")
                    if args.ash_read:
                        if args.ash_tasks:
                            pid += 9 if args.quickjs else 2
                        else:
                            pid += 3 if args.quickjs else 2
                        script = "echo${IFS}UEFI_READ_INPUT|(read${IFS}-r${IFS}value;echo${IFS}$value)"
                        text = send(f"/wasm run ash -c {script}", f"PID {pid}: Completed (exit code 0)")
                        assert any(line.strip() == "UEFI_READ_INPUT" for line in text.splitlines())
                        for child in (pid + 1, pid + 2):
                            send(f"/wasm reap {child}", f"WASM: Unknown WASM process: {child}")
                        pid += 3
                        script = "echo${IFS}ready|(read${IFS}-t${IFS}1${IFS}value)"
                        text = send(f"/wasm run ash -c {script}", f"PID {pid}: Completed (exit code 0)")
                        assert "monotonic timeout clock is unavailable" not in text
                        for child in (pid + 1, pid + 2):
                            send(f"/wasm reap {child}", f"WASM: Unknown WASM process: {child}")
                        if args.quickjs:
                            pid += 3
                            script = (r'''if((await($`echo\x20${'one\x20two\x20位'}|'''
                                      r'''(read\x20-r\x20a\x20b;echo\x20"$a:$b")`)).stdout!=='''
                                      r"'one:two\x20位\n')throw(Error('read'));print('UEFI_QUICKJS_READ_OK')")
                            text = send(f"/wasm run quickjs -e {script}", f"PID {pid}: Completed (exit code 0)")
                            assert any(line.strip() == "UEFI_QUICKJS_READ_OK" for line in text.splitlines())
                            for child in range(pid + 1, pid + 4):
                                send(f"/wasm reap {child}", f"WASM: Unknown WASM process: {child}")
                        if args.ash_printf:
                            pid += 4 if args.quickjs else 3
                            script = "printf${IFS}'%d:%x'${IFS}-9223372036854775808${IFS}-2"
                            text = send(f"/wasm run ash -c {script}", f"PID {pid}: Completed (exit code 0)")
                            assert "-9223372036854775808:fffffffffffffffe" in text
                            if args.quickjs:
                                pid += 1
                                script = (r"const/**/r=await($`printf\x20'%b'\x20'a\\000b'`);"
                                          r"if(r.stdout!=='a\0b'||r.stderr!=='')throw(Error('binary'));"
                                          r"print('UEFI_QUICKJS_PRINTF_BINARY_OK')")
                                text = send(f"/wasm run quickjs -e {script}", f"PID {pid}: Completed (exit code 0)")
                                assert any(line.strip() == "UEFI_QUICKJS_PRINTF_BINARY_OK" for line in text.splitlines())
                                send(f"/wasm reap {pid + 1}", f"WASM: Unknown WASM process: {pid + 1}")
            if args.filesystem:
                send("/wasm load filesystem \\filesystem.wasm", "Loaded filesystem")
                send("/wasm run filesystem", "WASM_FILESYSTEM_OK", complete_line=True)
                if args.ash:
                    script = ("printf${IFS}'%s\\n'${IFS}first>shell.txt;"
                              "printf${IFS}'%s\\n'${IFS}second>>shell.txt;"
                              "(read${IFS}-r${IFS}line;printf${IFS}'<%s>'${IFS}\"$line\";"
                              "read${IFS}-r${IFS}line;printf${IFS}'<%s>\\n'${IFS}\"$line\")<shell.txt;"
                              "echo${IFS}UEFI_ASH_FILES_OK")
                    text = send(f"/wasm run ash -c {script}", "UEFI_ASH_FILES_OK", complete_line=True)
                    assert "<first>" in text and "<second>" in text
                    if args.quickjs:
                        script = (r"if((await($`read\x20-r\x20line<shell.txt;printf\x20'%s'\x20\"$line\"`)).stdout!=="
                                  r"'first')throw(Error('file'));print('UEFI_QUICKJS_FILES_OK')")
                        send(f"/wasm run quickjs -e {script}", "UEFI_QUICKJS_FILES_OK", complete_line=True)
            if args.directories:
                send("/wasm load directory \\directory.wasm", "Loaded directory")
                send("/wasm run directory", "WASM_DIRECTORY_OK", complete_line=True)
                if args.ash:
                    script = ("printf${IFS}'<%s>\\n'${IFS}directory-fixture/*.txt>glob-list.txt;"
                              "echo${IFS}UEFI_ASH_DIRECTORY_OK")
                    send(f"/wasm run ash -c {script}", "UEFI_ASH_DIRECTORY_OK", complete_line=True)
                    if args.quickjs:
                        expected = json.dumps(expected_glob(directory_entries).decode("utf-8")).replace(" ", r"\x20")
                        script = (r"const/**/r=await($`printf\x20'<%s>\\n'\x20directory-fixture/*.txt`);"
                                  f"if(r.stdout!=={expected}||r.stderr!=='')throw(Error('glob'));"
                                  "print('UEFI_QUICKJS_DIRECTORY_OK')")
                        send(f"/wasm run quickjs -e {script}", "UEFI_QUICKJS_DIRECTORY_OK", complete_line=True)
            # The initial ABI cases keep roots for explicit status/reap checks.
            # Release their terminal records before concurrent application tests.
            for pid in sorted(terminal_pids):
                send(f'/wasm reap {pid}', f'Reaped {pid}:')
            if args.network:
                network_thread.start()
                send("/wasm load network \\network.wasm", "Loaded network")
                send(f"/wasm run network 10.0.2.2 {network_port}", "NETWORK_EXCHANGE_OK", complete_line=True)
                connection.sendall(f"/wasm run network 10.0.2.2 {network_port} blocked\r".encode())
                wait("NETWORK_BLOCKED_READ", complete_line=True)
                connection.sendall(b"\x1b")
                wait("exit code 130)")
                network_cancelled.set()
                send(f"/wasm run network 10.0.2.2 {network_port} graceful", "NETWORK_GRACEFUL_CLOSE_OK", complete_line=True)
                send(f"/wasm run network 10.0.2.2 {network_port} nonblocking", "NETWORK_NONBLOCKING_OK", complete_line=True)
                send(f"/wasm run network 10.0.2.2 {network_port} async", "NETWORK_ASYNC_OK", complete_line=True)
                if args.posix_network:
                    send("/wasm load posix_network \\posix_network.wasm", "Loaded posix_network")
                    send(f"/wasm run posix_network 10.0.2.2 {network_port}", "POSIX_NETWORK_OK", complete_line=True)
                    failure_marker = f"POSIX_{args.posix_connect_error.upper()}_OK"
                    send(f"/wasm run posix_network 10.0.2.2 {refused_listener.getsockname()[1]} {args.posix_connect_error}", failure_marker, complete_line=True)
                network_thread.join(timeout=5)
                assert not network_thread.is_alive(), "Network server did not finish"
                assert not network_errors, network_errors
                assert network_observations["cancel_closed"]
                assert network_observations["graceful_eof"]
                assert network_observations["nonblocking_eof"]
                assert network_observations["async_eof"]
                if args.posix_network:
                    assert network_observations["posix_eof"]
            if args.dns:
                from test_dns import read_exact, response
                import struct
                variants = [('a', 0), ('cname', 0), ('nxdomain', 44), ('wrong_id', 28), ('pointer_cycle', 28), ('wrong_question', 28)]
                cases = [('network', variant, errno, 'NETWORK_DNS_OK') for variant, errno in variants]
                if args.posix_network:
                    cases += [('posix_network', variant, 0 if errno == 0 else -2 if errno == 44 else -4, 'POSIX_DNS_OK') for variant, errno in variants]
                if args.curl:
                    send("/wasm load curl \\curl.wasm", "Loaded curl")
                    cases += [('curl', variant, 0 if errno == 0 else 6, 'exit code ') for variant, errno in variants]
                def dns_server():
                    try:
                        for program, variant, _, _ in cases:
                            with dns_listener.accept()[0] as peer:
                                peer.settimeout(30)
                                size = struct.unpack('!H', read_exact(peer, 2))[0]
                                reply = response(read_exact(peer, size), variant)
                                if program == 'curl' and variant in ['a', 'cname']:
                                    reply = reply[:-4] + bytes([10, 0, 2, 2])
                                frame = struct.pack('!H', len(reply)) + reply
                                for byte in frame:
                                    peer.sendall(bytes([byte]))
                    except BaseException as failure:
                        dns_errors.append(failure)
                dns_thread = threading.Thread(target=dns_server, daemon=True)
                dns_thread.start()
                for index, (program, _, errno, marker) in enumerate(cases):
                    # Clear the prior result marker so each query must finish.
                    if index:
                        send('/clear', 'Conversation cleared')
                    if program == 'curl':
                        text = send(f'/wasm run curl --silent --show-error --noproxy * --output curl-dns-{index}.bin http://MiXeD.Example.:{curl_server.server_port}/binary', marker)
                        assert f'exit code {errno})' in text, text
                    else:
                        send(f'/wasm run {program} dns MiXeD.Example. {errno}', marker, complete_line=True)
                        text = wait('exit code ')
                    match = re.search(r'PID (\d+): (?:Completed|Failed|Cancelled) \(exit code (\d+)\)', text)
                    assert match, text
                    send(f'/wasm reap {match[1]}', f'Reaped {match[1]}: Exited({match[2]})')
                dns_thread.join(timeout=5)
                assert not dns_thread.is_alive() and not dns_errors, dns_errors
            if args.curl:
                send("/wasm load curl \\curl.wasm", "Loaded curl")
                base = f"http://10.0.2.2:{curl_server.server_port}"
                send("/clear", "Conversation cleared")
                text = send(f"/wasm start curl --silent --show-error --noproxy * {base}/blocked", ": Running")
                match = re.search(r'PID (\d+): Running', text)
                assert match, text
                held_pid = match[1]
                assert curl_server.blocked_started.wait(15), "Curl did not reach held HTTP response"
                assert not curl_server.blocked_closed.is_set(), "Held response closed before cancellation"
                send("/clear", "Conversation cleared")
                text = send("/wasm run child", "exit code 37)")
                assert "WASM_CHILD_731" in text, text
                peer = re.search(r'PID (\d+): (?:Completed|Failed|Cancelled) \(exit code 37\)', text)
                assert peer, text
                assert not curl_server.blocked_closed.is_set(), "Peer progress ended held curl connection"
                send(f'/wasm reap {peer[1]}', f'Reaped {peer[1]}: Exited(37)')
                digest = hashlib.sha256(CURL_UPLOAD).hexdigest()
                if args.quickjs and args.ash:
                    script = (r"Promise.all([$`curl\x20--silent\x20--show-error\x20--noproxy\x20'*'\x20--data-binary\x20@curl-upload.bin\x20"
                              + base + r"/upload`,$`echo\x20SHELL_PEER`]).then(r=>{if(r[0].stdout!=='"
                              + digest + r"'||r[1].stdout!=='SHELL_PEER\n')throw(Error('curl'));print('QUICKJS_ASH_CURL_OK')})")
                    send("/clear", "Conversation cleared")
                    text = send(f"/wasm run quickjs -e {script}", "QUICKJS_ASH_CURL_OK", complete_line=True)
                    # /wasm run reports child exits too. Wait for the command
                    # footer to become Ready before selecting the root status.
                    text = wait("Ready")
                    roots = re.findall(r'PID (\d+): (?:Completed|Failed|Cancelled) \(exit code 0\)', text)
                    assert roots, text
                    root = min(map(int, roots))
                    send(f'/wasm reap {root}', f'Reaped {root}: Exited(0)')
                if args.python:
                    send("/wasm load python.wasm", "Loaded python")
                    code = ("import subprocess\n"
                            f"r=subprocess.run(['curl','--silent','--show-error','--noproxy','*','--data-binary','@curl-upload.bin','{base}/upload'],capture_output=True,text=True,check=True)\n"
                            f"assert r.stdout=='{digest}' and r.stderr==''\n"
                            "print('PYTHON_ASH_CURL_OK')\n")
                    send("/clear", "Conversation cleared")
                    text = send(f"/wasm run python -c exec(bytes.fromhex('{code.encode().hex()}').decode())", "PYTHON_ASH_CURL_OK", complete_line=True)
                    text = wait("Ready")
                    roots = re.findall(r'PID (\d+): (?:Completed|Failed|Cancelled) \(exit code 0\)', text)
                    assert roots, text
                    root = min(map(int, roots))
                    send(f'/wasm reap {root}', f'Reaped {root}: Exited(0)')
                assert not curl_server.blocked_closed.is_set(), "Application chain ended held curl connection"
                send(f'/wasm kill {held_pid}', f'Cancelled {held_pid}')
                assert curl_server.blocked_closed.wait(5), "Cancelled curl retained TCP4 connection"
                send(f'/wasm reap {held_pid}', f'Reaped {held_pid}: Exited(130)')
                for arguments in [
                    f"--location --output curl-download.bin {base}/redirect",
                    f"--output curl-chunked.bin {base}/chunked",
                    f"--data-binary @curl-upload.bin --output curl-digest.txt {base}/upload",
                ]:
                    send("/clear", "Conversation cleared")
                    text = send(f"/wasm run curl --silent --show-error --noproxy * {arguments}", "exit code ")
                    assert "exit code 0)" in text, text
                    match = re.search(r'PID (\d+): (?:Completed|Failed|Cancelled) \(exit code 0\)', text)
                    assert match, text
                    send(f'/wasm reap {match[1]}', f'Reaped {match[1]}: Exited(0)')
                assert not curl_errors, curl_errors
                for name, server, errors, _ in tls_cases:
                    send("/clear", "Conversation cleared")
                    text = send(f"/wasm run curl --silent --show-error --noproxy * --cacert curl-{name}-ca.pem --output curl-{name}.bin https://10.0.2.2:{server.server_port}/binary", "exit code ")
                    expected = 35 if args.no_rdrand else 0 if name in ["tls12", "tls13"] else 60
                    assert f"exit code {expected})" in text, text
                    root = re.search(rf'PID (\d+): (?:Completed|Failed|Cancelled) \(exit code {expected}\)', text)
                    assert root, text
                    send(f'/wasm reap {root[1]}', f'Reaped {root[1]}: Exited({expected})')
                    assert server.requests == (["/binary"] if expected == 0 else []), server.requests
                    if expected == 0:
                        assert server.tls_versions == ["TLSv1.2" if name == "tls12" else "TLSv1.3"], server.tls_versions
                    assert not errors, errors
            if args.wolfssh:
                send("/wasm load wolfssh \\wolfssh.wasm", "Loaded wolfssh")
                ssh_arguments = (f"--host 10.0.2.2 --port {ssh_peer.port} --user guest "
                                 f"--host-key-sha256 {ssh_peer.pin} --password-file ssh-password.txt --")

                def run_ssh_shell(arguments, name, expected):
                    send("/clear", "Conversation cleared")
                    # /wasm splits argv on whitespace. Keep redirect targets
                    # separate in shell syntax; IFS inside a redirect target
                    # expands into its filename instead of creating a token.
                    command = f"wolfssh {arguments}".replace(" ", "${IFS}")
                    script = f"({command}>ssh-{name}.out)2>ssh-{name}.err"
                    send(f"/wasm run ash -c {script}", "exit code ")
                    text = wait("Ready")
                    roots = re.findall(rf'PID (\d+): (?:Completed|Failed|Cancelled) \(exit code {expected}\)', text)
                    assert roots, text
                    root = min(map(int, roots))
                    send(f'/wasm reap {root}', f'Reaped {root}: Exited({expected})')

                if args.no_rdrand:
                    run_ssh_shell(ssh_arguments + " large", "entropy", 255)
                    assert ssh_peer.commands == [] and ssh_peer.auth_attempts == []
                else:
                    send("/clear", "Conversation cleared")
                    text = send(f"/wasm start wolfssh {ssh_arguments} blocked", ": Running")
                    held = re.search(r'PID (\d+): Running', text)
                    assert held, text
                    wait_event(ssh_peer.blocked_started, "WolfSSH did not reach exec server")
                    run_ssh_shell(ssh_arguments + " large", "large", 0)
                    run_ssh_shell(ssh_arguments + " stderr-only", "stderr", 0)
                    run_ssh_shell(ssh_arguments + " late-status", "late", 37)
                    run_ssh_shell(ssh_arguments + " no-status", "missing", 255)
                    attempts = list(ssh_peer.auth_attempts)
                    commands = list(ssh_peer.commands)
                    wrong_pin = ssh_arguments.replace(ssh_peer.pin, "00" * 32)
                    run_ssh_shell(wrong_pin + " large", "pin", 255)
                    assert ssh_peer.auth_attempts == attempts and ssh_peer.commands == commands
                    wrong_password = ssh_arguments.replace("ssh-password.txt", "ssh-wrong-password.txt")
                    run_ssh_shell(wrong_password + " large", "password", 255)
                    assert ssh_peer.commands == commands
                    assert ssh_peer.auth_attempts == attempts + [("guest", "incorrect")]
                    if args.quickjs:
                        script = (r"Promise.all([$`wolfssh\x20" + ssh_arguments.replace(" ", r"\x20")
                                  + r"\x20nonzero`.then(()=>{throw(Error('status'))},r=>{if(r.exitCode!==23||r.stdout!=='remote-output\n'||r.stderr!=='remote-error\n')throw(Error('capture'))}),"
                                  + r"$`echo\x20SSH_PEER`]).then(r=>{if(r[1].stdout!=='SSH_PEER\n')throw(Error('peer'));print('QUICKJS_ASH_WOLFSSH_OK')})")
                        send("/clear", "Conversation cleared")
                        send(f"/wasm run quickjs -e {script}", "QUICKJS_ASH_WOLFSSH_OK", complete_line=True)
                        text = wait("Ready")
                        root = min(map(int, re.findall(r'PID (\d+): (?:Completed|Failed|Cancelled) \(exit code 0\)', text)))
                        send(f'/wasm reap {root}', f'Reaped {root}: Exited(0)')
                    if args.python:
                        send("/wasm load \\python.wasm", "Loaded python")
                        code = ("import subprocess\n"
                                f"r=subprocess.run('wolfssh {ssh_arguments} nonzero',shell=True,capture_output=True)\n"
                                "assert r.returncode==23 and r.stdout==b'remote-output\\n' and r.stderr==b'remote-error\\n'\n"
                                "print('PYTHON_ASH_WOLFSSH_OK')")
                        send("/clear", "Conversation cleared")
                        send(f"/wasm run python -c exec(bytes.fromhex('{code.encode().hex()}').decode())", "PYTHON_ASH_WOLFSSH_OK", complete_line=True)
                        text = wait("Ready")
                        root = min(map(int, re.findall(r'PID (\d+): (?:Completed|Failed|Cancelled) \(exit code 0\)', text)))
                        send(f'/wasm reap {root}', f'Reaped {root}: Exited(0)')
                    assert not ssh_peer.blocked_closed.is_set(), "Peer progress closed held SSH"
                    send(f"/wasm kill {held[1]}", f"Cancelled {held[1]}")
                    assert ssh_peer.blocked_closed.wait(5), "Cancelled WolfSSH retained TCP4"
                    send(f"/wasm reap {held[1]}", f"Reaped {held[1]}: Exited(130)")
                assert ssh_peer.errors == [], ssh_peer.errors
            if args.python:
                send("/wasm load \\python.wasm", "Loaded python")
                send("/wasm run python python-standard.py", "UEFI_PYTHON_STANDARD_SCRIPT_OK", complete_line=True)
                code = r"""
import sys, math, os, subprocess
assert sys.version_info[:3] == (3, 14, 7)
assert sys.version_info.releaselevel == 'final'
assert sum(x*x for x in range(11)) == 385 and math.gcd(84, 30) == 6
word = "quote'$();浣犲ソ"
assert subprocess.check_output(['ash', '-c', 'printf "%s" "$1"', 'ash', word],text=True) == word
r = subprocess.run(r"printf 'a\000\377'; printf error >&2",shell=True,capture_output=True)
assert r.stdout == b'a\0\xff' and r.stderr == b'error'
try:
    subprocess.run('exit 17',shell=True,check=True)
except subprocess.CalledProcessError as error:
    assert error.returncode == 17
else:
    raise AssertionError('Nonzero exit accepted')
for index in range(7):
    assert subprocess.check_output(['ash','-c','printf "%s" "$1"','ash',str(index)],text=True) == str(index)
with open('python-shell.txt','w') as target:
    target.write('PYTHON_SHELL_FILE')
with open('python-shell.txt') as source:
    assert source.read() == 'PYTHON_SHELL_FILE'
assert os.waitstatus_to_exitcode(os.system('exit 23')) == 23
print('UEFI_CPYTHON_SHELL_OK')
"""
                # The current /wasm command splits on whitespace. Pass one
                # source word and decode the real multiline UTF-8 program.
                encoded = code.encode().hex()
                send(f"/wasm run python -c exec(bytes.fromhex('{encoded}').decode())",
                     "UEFI_CPYTHON_SHELL_OK", complete_line=True)
                # Both roots must make progress: alpha waits for beta's file,
                # and beta waits for alpha's file before writing its own.
                alpha = "import subprocess;subprocess.run('printf alpha>python-alpha.txt;while [ ! -f python-beta.txt ];do :;done',shell=True,check=True);print('UEFI_PYTHON_ALPHA_OK')"
                beta = "import subprocess;subprocess.run('while [ ! -f python-alpha.txt ];do :;done;printf beta>python-beta.txt',shell=True,check=True);print('UEFI_PYTHON_BETA_OK')"
                send('/clear', 'Conversation cleared')
                started = send(f"/wasm start python -c exec(bytes.fromhex('{alpha.encode().hex()}').decode())",
                               ": Running")
                alpha_root = re.search(r'PID (\d+): Running', started)
                assert alpha_root, started
                send(f"/wasm run python -c exec(bytes.fromhex('{beta.encode().hex()}').decode())",
                     "UEFI_PYTHON_BETA_OK", complete_line=True)
                wait("UEFI_PYTHON_ALPHA_OK", complete_line=True, panel_pid=int(alpha_root[1]))
            if args.ash_math:
                for name, _, _, _, status in math_cases():
                    send("/clear", "Conversation cleared")
                    script = f"(ash${{IFS}}math-{name}.sh>math-{name}.out)2>math-{name}.err"
                    send(f"/wasm run ash -c {script}", "exit code ")
                    text = wait("Ready")
                    roots = re.findall(r'PID (\d+): (?:Completed|Failed|Cancelled) \(exit code (\d+)\)', text)
                    assert roots, text
                    root, actual_status = min((int(pid), int(code)) for pid, code in roots)
                    assert actual_status == status, (name, actual_status, status, text)
                    send(f'/wasm reap {root}', f'Reaped {root}: Exited({status})')
                if args.quickjs:
                    script = (r"await(Promise.all([$`i=0;sum=0;while\x20[\x20$i\x20-lt\x2021\x20];do\x20sum=$((sum+i*i));i=$((i+1));done;echo\x20CALC:$sum`,"
                              r"$`test\x20invalid\x20-eq\x201`.then(()=>{throw(Error('test'))},r=>{if(r.exitCode!==2||r.stderr!=='invalid:\x20out\x20of\x20range\n')throw(Error('status'))})])"
                              r".then(r=>{if(r[0].stdout!=='CALC:2870\n')throw(Error('math'));print('QUICKJS_ASH_MATH_OK')}))")
                    send("/clear", "Conversation cleared")
                    send(f"/wasm run quickjs -e {script}", "QUICKJS_ASH_MATH_OK", complete_line=True)
                    text = wait("Ready")
                    root = min(map(int, re.findall(r'PID (\d+): (?:Completed|Failed|Cancelled) \(exit code 0\)', text)))
                    send(f'/wasm reap {root}', f'Reaped {root}: Exited(0)')
                if args.python:
                    code = ("import subprocess\n"
                            "r=subprocess.run('i=0;sum=0;while [ $i -lt 21 ];do sum=$((sum+i*i));i=$((i+1));done;echo CALC:$sum',shell=True,capture_output=True,check=True)\n"
                            "assert r.stdout==b'CALC:2870\\n' and r.stderr==b''\n"
                            "r=subprocess.run('test invalid -eq 1',shell=True,capture_output=True)\n"
                            "assert r.returncode==2 and r.stderr==b'invalid: out of range\\n'\n"
                            "print('PYTHON_ASH_MATH_OK')")
                    send("/clear", "Conversation cleared")
                    send(f"/wasm run python -c exec(bytes.fromhex('{code.encode().hex()}').decode())", "PYTHON_ASH_MATH_OK", complete_line=True)
                    text = wait("Ready")
                    root = min(map(int, re.findall(r'PID (\d+): (?:Completed|Failed|Cancelled) \(exit code 0\)', text)))
                    send(f'/wasm reap {root}', f'Reaped {root}: Exited(0)')
            if args.ash_descriptors:
                for name, _, _ in descriptor_cases():
                    send("/clear", "Conversation cleared")
                    script = f"(ash${{IFS}}fd-{name}.sh>fd-{name}.out)2>fd-{name}.err"
                    send(f"/wasm run ash -c {script}", "exit code ")
                    text = wait("Ready")
                    roots = re.findall(r'PID (\d+): (?:Completed|Failed|Cancelled) \(exit code (\d+)\)', text)
                    assert roots, text
                    root, status = min((int(pid), int(code)) for pid, code in roots)
                    assert status == 0, (name, text)
                    send(f'/wasm reap {root}', f'Reaped {root}: Exited(0)')
            if args.ash_heredocs:
                for name, _, _, _, status in heredoc_cases():
                    send("/clear", "Conversation cleared")
                    script = f"(ash${{IFS}}heredoc-{name}.sh>heredoc-{name}.out)2>heredoc-{name}.err"
                    send(f"/wasm run ash -c {script}", "exit code ")
                    text = wait("Ready")
                    roots = re.findall(r'PID (\d+): (?:Completed|Failed|Cancelled) \(exit code (\d+)\)', text)
                    assert roots, text
                    root, actual_status = min((int(pid), int(code)) for pid, code in roots)
                    assert actual_status == status, (name, actual_status, status, text)
                    send(f'/wasm reap {root}', f'Reaped {root}: Exited({status})')
            if args.ash_task_input:
                send("/clear", "Conversation cleared")
                send("/wasm load ash_task_input \\ash_task_input.wasm", "Loaded ash_task_input")
                send("/wasm run ash_task_input", "ASH_TASK_BINARY_REJECTION_OK", complete_line=True)
                text = wait("Ready")
                root = min(map(int, re.findall(r'PID (\d+): (?:Completed|Failed|Cancelled) \(exit code 0\)', text)))
                send(f'/wasm reap {root}', f'Reaped {root}: Exited(0)')
            if args.descriptor_mapping:
                send("/clear", "Conversation cleared")
                send("/wasm load descriptor_mapping \\descriptor_mapping.wasm", "Loaded descriptor_mapping")
                send("/wasm run descriptor_mapping", "UEFI_DESCRIPTOR_MAPPING_OK", complete_line=True)
                text = wait("Ready")
                root = min(map(int, re.findall(r'PID (\d+): (?:Completed|Failed|Cancelled) \(exit code 0\)', text)))
                send(f'/wasm reap {root}', f'Reaped {root}: Exited(0)')
            if args.random:
                send("/wasm load random \\random.wasm", "Loaded random")
                if args.no_rdrand:
                    send("/wasm run random unavailable", "WASM_RANDOM_UNAVAILABLE_OK", complete_line=True)
                else:
                    send("/wasm run random", "WASM_RANDOM_OK", complete_line=True)
            connection.sendall(b"/exit\r")
            qemu.wait(timeout=15)
            assert qemu.returncode == 0, qemu.returncode
            if args.dns:
                print('PASS actual UEFI DNS A/CNAME, NXDOMAIN and malformed-response rejection over TCP')
            if args.posix_network:
                print("PASS actual UEFI POSIX socket/connect/poll/fcntl/send/recv, SO_ERROR and getaddrinfo")
            if args.network:
                with image.open("rb") as disk:
                    filesystem = FATFS(RangeStream(disk, 1048576, image.stat().st_size - 1048576))
                    workspace = FatWorkspace(filesystem)
                    assert workspace.read("network-response.bin") == bytes((i * 53 + i // 239 + 7) % 256 for i in range(131089))
                    assert workspace.read("network-peer.txt") == b"PEER_PROGRESS"
                print("PASS actual UEFI TCP4 WASM transfer, child handshake, EOF, cancellation, graceful close, nonblocking flags and saved-byte oracle")
            if args.wolfssh:
                with image.open("rb") as disk:
                    filesystem = FATFS(RangeStream(disk, 1048576, image.stat().st_size - 1048576))
                    workspace = FatWorkspace(filesystem)
                    def ssh_file(name, suffix):
                        return workspace.read(f"ssh-{name}.{suffix}")
                    if args.no_rdrand:
                        assert ssh_file("entropy", "out") == b""
                    else:
                        assert ssh_file("large", "out") == SSH_STDOUT
                        assert ssh_file("large", "err") == SSH_STDERR
                        assert ssh_file("stderr", "out") == b""
                        assert ssh_file("stderr", "err") == SSH_STDERR
                        assert ssh_file("late", "out") == b"before-eof\n"
                        assert ssh_file("late", "err") == b""
                        assert ssh_file("missing", "out") == b"without-status\n"
                        assert b"missing or out-of-range remote exit status" in ssh_file("missing", "err")
                        assert ssh_file("pin", "out") == b""
                        assert b"host key does not match SHA256 pin" in ssh_file("pin", "err")
                        assert ssh_file("password", "out") == b""
                        assert b"handshake failed" in ssh_file("password", "err")
                assert ssh_peer.errors == [], ssh_peer.errors
                print("PASS actual UEFI WolfSSH entropy rejection" if args.no_rdrand else
                      "PASS actual UEFI upstream WolfSSH pinned host key, password authentication, binary streams, late/missing exit status, cooperative peers and cancellation")
                if not args.no_rdrand and args.quickjs:
                    print("PASS actual UEFI QuickJS zx-like API to ash to WolfSSH with exact remote status and streams")
                if not args.no_rdrand and args.python:
                    print("PASS actual UEFI CPython standard subprocess to ash to WolfSSH while another SSH waits")
            if args.curl:
                with image.open("rb") as disk:
                    filesystem = FATFS(RangeStream(disk, 1048576, image.stat().st_size - 1048576))
                    workspace = FatWorkspace(filesystem)
                    assert workspace.read("curl-download.bin") == CURL_BODY
                    assert workspace.read("curl-chunked.bin") == CURL_BODY
                    assert workspace.read("curl-digest.txt") == hashlib.sha256(CURL_UPLOAD).hexdigest().encode()
                    if args.curl_tls and not args.no_rdrand:
                        for name in ["tls12", "tls13"]:
                            assert workspace.read(f"curl-{name}.bin") == CURL_BODY
                    if args.dns:
                        offset = 12 if args.posix_network else 6
                        for index in [offset, offset + 1]:
                            assert workspace.read(f"curl-dns-{index}.bin") == CURL_BODY
                assert not curl_errors, curl_errors
                print("PASS actual UEFI upstream curl HTTP redirect, chunked transfer and binary upload with exact FAT bytes")
                assert curl_server.blocked_started.is_set() and curl_server.blocked_closed.is_set()
                print("PASS actual UEFI curl held response, concurrent WASM peer progress and cancellation with observed TCP closure")
                if args.quickjs and args.ash:
                    print("PASS actual UEFI QuickJS zx-like API to ash to curl, concurrent shell capture and exact HTTP upload digest")
                if args.python:
                    print("PASS actual UEFI CPython standard subprocess to ash to curl while another curl waits")
                if args.curl_tls:
                    if args.no_rdrand:
                        print("PASS actual UEFI curl TLS fails before HTTP when cryptographic entropy is unavailable")
                    else:
                        print("PASS actual UEFI curl wolfSSL TLS 1.2/1.3 bytes and rejection of untrusted, wrong-IP and expired certificates")
            if args.python:
                with image.open("rb") as disk:
                    filesystem = FATFS(RangeStream(disk, 1048576, image.stat().st_size - 1048576))
                    workspace = FatWorkspace(filesystem)
                    assert workspace.read("python-shell.txt") == b"PYTHON_SHELL_FILE"
                    assert workspace.read("python-alpha.txt") == b"alpha"
                    assert workspace.read("python-beta.txt") == b"beta"
                    expected = bytes((i * 37 + i // 251 + 19) % 256 for i in range(196613))
                    assert workspace.read("python-standard.bin") == expected
                print("PASS actual UEFI CPython standard CLI, script imports, file interfaces, duplex subprocess capture, concurrent Python roots and persisted bytes")
            if args.random and not args.no_rdrand:
                with image.open("rb") as disk:
                    filesystem = FATFS(RangeStream(disk, 1048576, image.stat().st_size - 1048576))
                    workspace = FatWorkspace(filesystem)
                    records = workspace.read("wasm-random.bin")
                    assert len(records) == 768
                    assert len({records[index:index + 256] for index in range(0, 768, 256)}) == 3
                print("PASS UEFI actual libc getentropy, guarded output and persisted distinct records")
            if args.no_rdrand:
                print("PASS UEFI missing RDRAND rejects guest entropy without changing output")
            if args.directories:
                with image.open("rb") as disk:
                    filesystem = FATFS(RangeStream(disk, 1048576, image.stat().st_size - 1048576))
                    workspace = FatWorkspace(filesystem)
                    read_file = lambda name: workspace.read(f"{name}")
                    verify_directory_artifacts(read_file, directory_entries)
                    if args.ash:
                        assert read_file("glob-list.txt") == expected_glob(directory_entries)
                print("PASS UEFI actual directory cookies, libc enumeration and persisted name/type oracle")
            if args.filesystem:
                with image.open("rb") as disk:
                    filesystem = FATFS(RangeStream(disk, 1048576, image.stat().st_size - 1048576))
                    workspace = FatWorkspace(filesystem)
                    assert workspace.read("wasm-file.bin") == b"a\0b\xffz!"
                    expected = bytearray(257)
                    for index in range(97):
                        expected[(index * 73 + 11) % 257] = (index * 19 + 7) % 256
                    assert workspace.read("wasm-cursor.bin") == expected
                    assert workspace.read("wasm-resize.bin") == b"\0\0\0\0!"
                    assert workspace.read("wasm-resize-oracle.bin") == resize_contents()
                    if args.ash:
                        assert workspace.read("shell.txt") == b"first\nsecond\n"
                print("PASS UEFI actual WASI files, shared/independent cursors, rights, path errors and persisted FAT bytes")
            print("PASS UEFI wasmi: processes, pipes, concurrent C capture, readiness, and firmware timed wait")
            print("PASS UEFI process environments: explicit, inherited, cleared, isolated, rejected invalid entries")
            if args.descriptor_mapping:
                print("PASS actual UEFI explicit descriptor mapping: shared binary input, parent lifetime, fd 127, full map and workspace preopen")
            if args.ash_task_input:
                print("PASS actual UEFI ash binary task length, flags, version and truncated-state rejection")
            if args.quickjs:
                print("PASS real QuickJS in UEFI: engine, top-level await, restricted globals, firmware Date.now")
            if args.sjlj:
                print("PASS UEFI nonlocal jumps: nested, repeated, stack locals, concurrent instances")
            if args.ash_exec:
                print("PASS UEFI ash exec bridge: fresh instance, environment, error errno, exit propagation")
            if args.ash:
                print("PASS real ash in UEFI; real QuickJS shell capture when enabled")
            if args.ash_child:
                print("PASS UEFI ash external dispatch, status propagation and child reaping")
            if args.ash_tasks:
                print("PASS UEFI real ash bounded pipelines, command substitution and shell state isolation")
            if args.ash_heredocs:
                with image.open("rb") as disk:
                    filesystem = FATFS(RangeStream(disk, 1048576, image.stat().st_size - 1048576))
                    workspace = FatWorkspace(filesystem)
                    for name, _, stdout, stderr, status in heredoc_cases():
                        actual_out = workspace.read(f"heredoc-{name}.out")
                        actual_err = workspace.read(f"heredoc-{name}.err")
                        assert actual_out == stdout, (name, len(actual_out), len(stdout))
                        if status:
                            assert b"cannot create here-document input" in actual_err, (name, actual_err)
                            assert b"Guest trapped" not in actual_err, (name, actual_err)
                        else:
                            assert actual_err == stderr, (name, actual_err)
                print("PASS actual UEFI ash here-documents: pipe-capacity boundaries, expansion, quoting, tab stripping, multiple inputs, pipeline, repeated cleanup and size rejection")
            if args.ash_descriptors:
                with image.open("rb") as disk:
                    filesystem = FATFS(RangeStream(disk, 1048576, image.stat().st_size - 1048576))
                    workspace = FatWorkspace(filesystem)
                    for name, _, expected in descriptor_cases():
                        actual = workspace.read(f"fd-{name}.out")
                        errors = workspace.read(f"fd-{name}.err")
                        assert actual == expected and errors == b"", (name, actual, errors)
                print("PASS actual UEFI ash user descriptors: subshell, substitution, pipeline, external command, fd 4, close isolation, output, closed stdin and repeated cleanup")
            if args.ash_math:
                with image.open("rb") as disk:
                    filesystem = FATFS(RangeStream(disk, 1048576, image.stat().st_size - 1048576))
                    workspace = FatWorkspace(filesystem)
                    for name, _, stdout, diagnostic, _ in math_cases():
                        actual_out = workspace.read(f"math-{name}.out")
                        actual_err = workspace.read(f"math-{name}.err")
                        assert actual_out == stdout, (name, actual_out[-512:], stdout[-512:])
                        if diagnostic:
                            assert diagnostic in actual_err, (name, actual_err)
                        else:
                            assert actual_err == b"", (name, actual_err)
                        assert b"Guest trapped" not in actual_err, (name, actual_err)
                print("PASS actual UEFI ash 64-bit math/test: precedence, assignment, short-circuit, wrapping, filesystem checks, task isolation, error recovery and independent randomized oracles")
                if args.quickjs:
                    print("PASS actual UEFI QuickJS zx-like ash arithmetic and test error capture")
                if args.python:
                    print("PASS actual UEFI CPython standard subprocess ash arithmetic and test error capture")
            if args.ash_read:
                print("PASS UEFI actual ash read pipeline, monotonic timed read and QuickJS capture when enabled")
            if args.ash_printf:
                print("PASS UEFI actual ash printf integer boundaries and QuickJS NUL capture when enabled")
        finally:
            if qemu.poll() is None:
                qemu.terminate()
            qemu.wait(timeout=15)
            (output / "serial.bin").write_bytes(wire)
            (output / "screen.txt").write_text("\n".join(screen.display), encoding="utf-8")
            if connection is not None:
                connection.close()
            console.close()
            if ssh_peer is not None:
                ssh_peer.__exit__(None, None, None)
            if curl_server is not None:
                curl_server.shutdown()
                curl_server.server_close()
                curl_thread.join(timeout=5)
                assert not curl_thread.is_alive(), "HTTP server did not stop"
            for _, server, _, thread in tls_cases:
                server.shutdown()
                server.server_close()
                thread.join(timeout=5)
                assert not thread.is_alive(), "HTTPS server did not stop"
            if network_listener is not None:
                network_listener.close()
            if network_thread is not None and network_thread.is_alive():
                network_progress.set()
                network_cancelled.set()
                network_thread.join(timeout=5)
            if args.network:
                (output / "network-server.json").write_text(json.dumps(network_observations), encoding="utf-8")
            if dns_listener is not None:
                dns_listener.close()
            if refused_listener is not None:
                refused_listener.close()
            if dns_thread is not None and dns_thread.is_alive():
                dns_thread.join(timeout=5)


if __name__ == "__main__":
    main()
