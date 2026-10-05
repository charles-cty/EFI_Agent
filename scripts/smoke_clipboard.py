# /// script
# dependencies = ["pywinpty>=2", "pyte>=0.8.2", "pyperclip>=1.9"]
# ///
"""Check native Windows ConPTY mouse selection and system clipboard. Run with uv."""

import argparse
import os
from pathlib import Path
import tempfile
import time
import subprocess
import sys

import pyperclip
import pyte
from winpty import PtyProcess


MOUSE_INPUT = r"""
import ctypes as c
from ctypes import wintypes as w
import sys
class Coord(c.Structure):
    _fields_ = [('x', c.c_short), ('y', c.c_short)]
class Mouse(c.Structure):
    _fields_ = [('position', Coord), ('buttons', w.DWORD), ('control', w.DWORD), ('flags', w.DWORD)]
class Data(c.Union):
    _fields_ = [('mouse', Mouse), ('padding', c.c_byte * 16)]
class Record(c.Structure):
    _fields_ = [('kind', w.WORD), ('data', Data)]
k = c.WinDLL('kernel32', use_last_error=True)
k.FreeConsole()
if not k.AttachConsole(int(sys.argv[1])):
    raise c.WinError(c.get_last_error())
k.CreateFileW.restype = w.HANDLE
k.CreateFileW.argtypes = [w.LPCWSTR, w.DWORD, w.DWORD, c.c_void_p, w.DWORD, w.DWORD, w.HANDLE]
handle = k.CreateFileW('CONIN$', 0x40000000, 3, None, 3, 0, None)
k.WriteConsoleInputW.argtypes = [w.HANDLE, c.POINTER(Record), w.DWORD, c.POINTER(w.DWORD)]
x, y, end = map(int, sys.argv[2:5])
mode = sys.argv[5] if len(sys.argv) > 5 else 'select'
end_y = int(sys.argv[6]) if len(sys.argv) > 6 else y
if mode == 'right':
    events = [(x, y, 2, 0), (x, y, 0, 0)]
elif mode == 'drag':
    events = [(x, y, 1, 0), (end, end_y, 1, 1)]
elif mode == 'up':
    events = [(end, end_y, 0, 0)]
else:
    events = [(x, y, 1, 0), (end, y, 1, 1), (end, y, 0, 0)]
records = (Record * len(events))()
for record, (col, row, buttons, flags) in zip(records, events):
    record.kind = 2
    record.data.mouse = Mouse(Coord(col, row), buttons, 0, flags)
written = w.DWORD()
if not k.WriteConsoleInputW(handle, records, len(events), c.byref(written)) or written.value != len(events):
    raise c.WinError(c.get_last_error())
k.CloseHandle.argtypes = [w.HANDLE]
k.CloseHandle(handle)
k.FreeConsole()
"""


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--launcher", required=True, type=Path)
    parser.add_argument("--qemu", required=True)
    parser.add_argument("--code", required=True, type=Path)
    parser.add_argument("--vars", required=True, type=Path)
    parser.add_argument("--esp", required=True, type=Path)
    args = parser.parse_args()
    original = pyperclip.paste()
    screen = pyte.Screen(120, 40)
    stream = pyte.Stream(screen)
    env = dict(os.environ, EFI_AGENT_API_BASE="http://127.0.0.1:1/v1",
               EFI_AGENT_API_KEY="test", EFI_AGENT_MODEL="test")
    with tempfile.TemporaryDirectory(prefix="efi-clipboard-") as workspace:
        process = PtyProcess.spawn(
            [str(args.launcher.resolve()), "vm", args.qemu, str(args.code.resolve()),
             str(args.vars.resolve()), str(args.esp.resolve()), workspace],
            dimensions=(40, 120), cwd=str(Path.cwd()), env=env,
        )
        process.fileobj.settimeout(0.1)

        def pump_until(predicate, seconds=60):
            deadline = time.monotonic() + seconds
            while time.monotonic() < deadline:
                try:
                    stream.feed(process.read(65536))
                except TimeoutError:
                    pass
                except EOFError:
                    if predicate():
                        return
                    raise RuntimeError("Launcher exited early")
                if predicate():
                    return
                time.sleep(0.02)
            raise RuntimeError("Timed out:\n" + "\n".join(screen.display))

        try:
            marker = "What would you like to build?"
            pump_until(lambda: any(marker in line for line in screen.display)
                       and any("Click/Tab+Enter details" in line for line in screen.display))
            # Drain the boot/initial-size redraw before starting a selection.
            settled = time.monotonic() + 1
            pump_until(lambda: time.monotonic() >= settled, 5)
            # Interface text must not create a selection or alter the clipboard.
            pyperclip.copy("CHROME_SENTINEL")
            subprocess.run([sys.executable, "-c", MOUSE_INPUT, str(process.pid),
                            "2", "0", "25"], check=True)
            settled = time.monotonic() + 0.3
            pump_until(lambda: time.monotonic() >= settled, 5)
            assert not any(cell.reverse for cell in screen.buffer[0].values())
            assert pyperclip.paste() == "CHROME_SENTINEL"
            process.write("/help\r")
            marker = "Show commands"
            pump_until(lambda: any(marker in line for line in screen.display))
            y, line = next((y, line) for y, line in enumerate(screen.display) if marker in line)
            x = line.index(marker)
            pyperclip.copy("BLANK_START_SENTINEL")
            subprocess.run([sys.executable, "-c", MOUSE_INPUT, str(process.pid),
                            "119", str(y), str(x)], check=True)
            pump_until(lambda: screen.buffer[y][x].reverse, 10)
            process.write("\x03")
            pump_until(lambda: pyperclip.paste() == marker, 10)
            process.write("\x1b")
            pump_until(lambda: not screen.buffer[y][x].reverse, 10)
            pyperclip.copy("LEFT_BLANK_SENTINEL")
            subprocess.run([sys.executable, "-c", MOUSE_INPUT, str(process.pid),
                            "0", str(y), str(x + len(marker) - 1)], check=True)
            pump_until(lambda: screen.buffer[y][x].reverse, 10)
            process.write("\x03")
            pump_until(lambda: pyperclip.paste() == line.strip(), 10)
            process.write("\x1b")
            pump_until(lambda: not screen.buffer[y][x].reverse, 10)
            print("PASS left and right blank-area selection anchors without copied padding")
            # Exercise the Windows INPUT_RECORD backend used by Crossterm.
            subprocess.run([sys.executable, "-c", MOUSE_INPUT, str(process.pid),
                            str(x), str(y), str(x + len(marker) - 1)], check=True)
            pump_until(lambda: screen.buffer[y][x].reverse, 10)
            pyperclip.copy("INITIAL_COPY_SENTINEL")
            process.write("\x03")
            pump_until(lambda: pyperclip.paste() == marker
                       and any("Copied selection" in line for line in screen.display), 10)
            assert process.isalive(), "Copy interrupted the launcher"
            print("PASS native ConPTY drag selection and Ctrl+C system clipboard copy")
            pyperclip.copy("COPY_SENTINEL")
            subprocess.run([sys.executable, "-c", MOUSE_INPUT, str(process.pid),
                            str(x), str(y), str(x), "right"], check=True)
            pump_until(lambda: pyperclip.paste() == marker, 10)
            print("PASS right-click copies the active selection")
            process.write("\x1b")
            text = "/unsupported clipboard 中\r\nsecond line"
            pyperclip.copy(text)
            process.write("\x16")
            pump_until(lambda: any("second line" in line for line in screen.display), 10)
            continuation = next(line for line in screen.display if "second line" in line)
            assert continuation.startswith("second line"), continuation
            assert sum(line.startswith("›") for line in screen.display) == 1
            assert not any("Unknown command" in line for line in screen.display)
            first_y = next(y for y, line in enumerate(screen.display) if line.startswith("› /unsupported clipboard"))
            last_y = next(y for y, line in enumerate(screen.display) if line.startswith("second line"))
            subprocess.run([sys.executable, "-c", MOUSE_INPUT, str(process.pid),
                            "0", str(first_y), "119", "drag", str(last_y)], check=True)
            pump_until(lambda: screen.buffer[first_y][2].reverse, 10)
            subprocess.run([sys.executable, "-c", MOUSE_INPUT, str(process.pid),
                            "119", str(last_y), "119", "up", str(last_y)], check=True)
            pyperclip.copy("INPUT_COPY_SENTINEL")
            process.write("\x03")
            pump_until(lambda: pyperclip.paste() == text.replace("\r\n", "\n"), 10)
            assert process.isalive()
            process.write("\x1b")
            pump_until(lambda: not screen.buffer[first_y][2].reverse, 10)
            print("PASS multiline input selection and exact copy without prompt marker or cursor")
            process.write("\r")
            pump_until(lambda: any("Unknown command" in line for line in screen.display), 10)
            print("PASS native ConPTY Ctrl+V Unicode multiline paste, explicit submission")
            pyperclip.copy("/unsupported right-click")
            subprocess.run([sys.executable, "-c", MOUSE_INPUT, str(process.pid),
                            str(x), str(y), str(x), "right"], check=True)
            pump_until(lambda: any("› /unsupported right-click" in line for line in screen.display), 10)
            print("PASS right-click pastes without a selection")
            process.write("\r")
            pump_until(lambda: not any("› /unsupported right-click" in line for line in screen.display), 10)
            settled = time.monotonic() + 0.4
            pump_until(lambda: time.monotonic() >= settled, 5)
            injected = "/unsupported injected 中\nsecond injected line\n"
            pyperclip.copy(injected)
            # Emulate a host that handles Ctrl+V itself and injects plain keys,
            # with CR for every newline and no bracketed-paste markers.
            previous_errors = sum("Unknown command" in line for line in screen.display)
            process.write(injected.replace("\n", "\r"))
            pump_until(lambda: any(line.startswith("› /unsupported injected") for line in screen.display)
                       and any(line.startswith("second injected line") for line in screen.display), 10)
            settled = time.monotonic() + 0.3
            pump_until(lambda: time.monotonic() >= settled, 5)
            assert any(line.startswith("› /unsupported injected") for line in screen.display)
            assert sum("Unknown command" in line for line in screen.display) <= previous_errors
            assert sum("/unsupported injected" in line for line in screen.display) == 1
            process.write("\r")
            pump_until(lambda: not any(line.startswith("› /unsupported injected") for line in screen.display), 10)
            print("PASS terminal-handled paste with plain Enter records and trailing newline does not submit")
            # A transcript taller than the viewport tests both stationary edge
            # scrolling directions and selection across offscreen rows.
            long_text = "/unsupported scroll\n" + "\n".join(f"ROW{i:03d} payload 中" for i in range(1, 91))
            pyperclip.copy(long_text)
            process.write("\x16")
            pump_until(lambda: any("ROW090" in line for line in screen.display), 10)
            process.write("\r")
            pump_until(lambda: any("ROW090" in line for line in screen.display)
                       and any(line.strip() == "› ▏" for line in screen.display), 10)
            settled = time.monotonic() + 0.3
            pump_until(lambda: time.monotonic() >= settled, 5)
            y, line = next((y, line) for y, line in enumerate(screen.display) if "ROW090" in line)
            # The row ends in a wide glyph; select either of its two cells.
            end_x = len(line.rstrip())
            subprocess.run([sys.executable, "-c", MOUSE_INPUT, str(process.pid),
                            str(end_x), str(y), "2", "drag", "1"], check=True)
            pump_until(lambda: any("ROW001" in line for line in screen.display), 15)
            subprocess.run([sys.executable, "-c", MOUSE_INPUT, str(process.pid),
                            "2", "1", "2", "up", "1"], check=True)
            pyperclip.copy("SCROLL_SENTINEL")
            process.write("\x03")
            pump_until(lambda: "ROW001" in pyperclip.paste() and "ROW090" in pyperclip.paste(), 10)
            copied = pyperclip.paste()
            for i in range(1, 91):
                assert f"ROW{i:03d} payload 中" in copied, (i, copied)
            assert "EFI AGENT" not in copied and "Enter send" not in copied
            print("PASS stationary top-edge autoscroll, continuous Unicode selection and copy")
            process.write("\x1b")
            pump_until(lambda: not any(cell.reverse for row in screen.buffer.values() for cell in row.values()), 10)
            settled = time.monotonic() + 0.5
            pump_until(lambda: time.monotonic() >= settled, 5)
            y, line = next((y, line) for y, line in enumerate(screen.display) if "ROW001" in line)
            x = line.index("ROW001")
            subprocess.run([sys.executable, "-c", MOUSE_INPUT, str(process.pid),
                            str(x), str(y), "119", "drag", "39"], check=True)
            pump_until(lambda: any("ROW090" in line for line in screen.display), 15)
            subprocess.run([sys.executable, "-c", MOUSE_INPUT, str(process.pid),
                            "119", "39", "119", "up", "39"], check=True)
            pyperclip.copy("BOTTOM_SENTINEL")
            process.write("\x03")
            pump_until(lambda: "ROW090 payload 中" in pyperclip.paste(), 10)
            for i in range(1, 91):
                assert f"ROW{i:03d} payload 中" in pyperclip.paste()
            print("PASS stationary bottom-edge autoscroll and selection without chrome")
            process.write("\x1b")
            process.write("\x03")
            pump_until(lambda: any("Press Ctrl+C again" in line for line in screen.display), 10)
            assert process.isalive(), "One Ctrl+C exited the launcher"
            settled = time.monotonic() + 1.2
            pump_until(lambda: time.monotonic() >= settled, 5)
            process.write("\x03")
            settled = time.monotonic() + 0.15
            pump_until(lambda: time.monotonic() >= settled, 5)
            assert process.isalive(), "Expired confirmation still exited"
            process.write("\x03")
            pump_until(lambda: not process.isalive(), 15)
            print("PASS single Ctrl+C hint, expired confirmation, and double Ctrl+C exit")
        finally:
            pyperclip.copy(original)
            if process.isalive():
                process.write("\x03")
                time.sleep(0.15)
                process.write("\x03")
                deadline = time.monotonic() + 10
                while process.isalive() and time.monotonic() < deadline:
                    time.sleep(0.02)
                if process.isalive():
                    process.terminate(force=True)


if __name__ == "__main__":
    main()
