"""Verify the Linux launcher in a real tmux PTY with QEMU/KVM.

Run with uv from a Linux-local checkout. No model provider is required.
"""
import argparse
from pathlib import Path
import shlex
import subprocess
import tempfile
import time
import uuid


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("launcher", "code", "vars", "esp", "output"):
        parser.add_argument(f"--{name}", type=Path, required=True)
    parser.add_argument("--qemu", default="qemu-system-x86_64")
    args = parser.parse_args()
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    socket = "efi-agent-" + uuid.uuid4().hex[:12]

    def tmux(*command):
        return subprocess.check_output(["tmux", "-L", socket, *command], text=True)

    with tempfile.TemporaryDirectory(prefix="efi-launcher-") as temporary:
        workspace = Path(temporary)
        command = shlex.join([
            str(args.launcher.resolve()), "vm", args.qemu, str(args.code.resolve()),
            str(args.vars.resolve()), str(args.esp.resolve()), str(workspace),
        ])
        script = workspace / "run.sh"
        script.write_text(
            "#!/usr/bin/env bash\n"
            f"stty -g > {shlex.quote(str(workspace / 'before'))}\n"
            f"{command}\nresult=$?\n"
            f"stty -g > {shlex.quote(str(workspace / 'after'))}\n"
            "printf '\\nLAUNCHER_EXIT_%s\\n' \"$result\"\n"
            "exec bash --noprofile --norc\n", encoding="utf-8",
        )
        pane = None

        def capture():
            return tmux("capture-pane", "-p", "-t", pane)

        def wait_for(marker, timeout=30):
            deadline = time.monotonic() + timeout
            screen = ""
            while time.monotonic() < deadline:
                screen = capture()
                if marker in screen:
                    return screen
                time.sleep(0.1)
            (output / "failure-screen.txt").write_text(screen, encoding="utf-8")
            raise TimeoutError(f"Terminal did not show {marker!r}")

        def send(text):
            tmux("send-keys", "-t", pane, "-l", text)
            tmux("send-keys", "-t", pane, "Enter")

        def restored():
            wait_for("LAUNCHER_EXIT_0", 10)
            if (workspace / "before").read_bytes() != (workspace / "after").read_bytes():
                raise AssertionError("Launcher did not restore the PTY termios mode")
            if tmux("display-message", "-p", "-t", pane, "#{alternate_on}").strip() != "0":
                raise AssertionError("Launcher left the alternate screen active")

        try:
            tmux("new-session", "-d", "-s", "check", "-x", "83", "-y", "23", "-c", str(workspace), f"bash {shlex.quote(str(script))}")
            pane = tmux("list-panes", "-t", "check", "-F", "#{pane_id}").strip()
            wait_for("What would you like to build?")
            initial = wait_for("Ready  •")
            if len(initial.splitlines()) != 23 or "Ready" not in initial.splitlines()[-1]:
                raise AssertionError("Initial guest dimensions did not match the PTY")
            (output / "initial.txt").write_text(initial, encoding="utf-8")
            print("PASS launcher boot and initial 83x23 dimensions", flush=True)
            tmux("resize-window", "-t", "check", "-x", "107", "-y", "31")
            deadline = time.monotonic() + 10
            while time.monotonic() < deadline:
                resized = capture()
                if len(resized.splitlines()) == 31 and "Ready" in resized.splitlines()[-1]:
                    break
                time.sleep(0.1)
            else:
                (output / "resize-failure.txt").write_text(resized, encoding="utf-8")
                raise AssertionError("Guest did not redraw at resized PTY dimensions")
            (output / "resized.txt").write_text(resized, encoding="utf-8")
            print("PASS live resize to 107x31", flush=True)
            send("/host-write unicode.txt left 中 right")
            wait_for("File saved")
            if (workspace / "unicode.txt").read_text(encoding="utf-8") != "left 中 right":
                raise AssertionError("Unicode input did not reach the actual host file")
            tmux("send-keys", "-t", pane, "-l", "/host-read unicode.txtX")
            tmux("send-keys", "-t", pane, "BSpace", "Enter")
            screen = wait_for("left 中 right")
            (output / "file.txt").write_text(screen, encoding="utf-8")
            print("PASS Unicode input, Backspace, and host file RPC", flush=True)
            send("/quit")
            restored()
            print("PASS /quit and terminal restoration", flush=True)
            send(f"bash {shlex.quote(str(script))}")
            wait_for("What would you like to build?")
            tmux("send-keys", "-t", pane, "C-c")
            restored()
            print("PASS Ctrl+C and terminal restoration", flush=True)
        finally:
            if pane is not None:
                # Let the launcher Drop handler retire its QEMU child before
                # tearing down this test's multiplexer session.
                try:
                    tmux("send-keys", "-t", pane, "C-c")
                    tmux("kill-session", "-t", "check")
                except subprocess.CalledProcessError:
                    pass


if __name__ == "__main__":
    main()
