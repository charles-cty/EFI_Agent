# /// script
# requires-python = ">=3.11"
# ///
"""Verify path-oriented BusyBox applets through the host WASM runner."""
import argparse
from pathlib import Path
import subprocess
import tempfile


def run(runner: Path, workspace: Path, *args: str) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [str(runner.resolve()), str(workspace.resolve()), *args],
        capture_output=True, text=True, encoding="utf-8", timeout=60,
    )


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--runner", type=Path, required=True)
    parser.add_argument("--ash", type=Path, required=True)
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix="efi-path-applets-") as temporary:
        workspace = Path(temporary)
        (workspace / "input.txt").write_text("path\n", encoding="utf-8")
        (workspace / "binary.bin").write_bytes(bytes(range(16)))
        (workspace / "ash.wasm").write_bytes(args.ash.read_bytes())
        direct = run(args.runner, workspace, "realpath", "input.txt")
        assert direct.returncode == 0, direct
        assert direct.stdout == "/input.txt\n", direct
        assert direct.stderr == "", direct
        missing = run(args.runner, workspace, "realpath", "missing.txt")
        assert missing.returncode != 0, missing
        shell = run(args.runner, workspace, "ash", "-c", "realpath input.txt")
        assert shell.returncode == 0, shell
        assert shell.stdout == "/input.txt\n", shell
        assert shell.stderr == "", shell
        octal = run(args.runner, workspace, "od", "binary.bin")
        assert octal.returncode == 0, octal
        assert octal.stdout == (
            "0000000 300400400 501001402 701402404 1102003406 "
            "1302404410 1503005412 1703406414 007416\n0000020\n"
        ), octal
        invalid = run(args.runner, workspace, "od", "--invalid", "binary.bin")
        assert invalid.returncode != 0, invalid
        encoded = run(args.runner, workspace, "base64", "binary.bin")
        assert encoded.returncode == 0, encoded
        assert encoded.stdout == "AAECAwQFBgcICQoLDA0ODw==\n", encoded
        decoded = subprocess.run(
            [str(args.runner.resolve()), str(workspace.resolve()), "base64", "-d"],
            input=encoded.stdout, capture_output=True, text=False, timeout=60,
        )
        assert decoded.returncode == 0 and decoded.stdout == bytes(range(16)), decoded
        wrapped = run(args.runner, workspace, "ash", "-c", "printf hello | base64 -w 4")
        assert wrapped.returncode == 0 and wrapped.stdout == "aGVs\nbG8=\n", wrapped
        invalid_base64 = subprocess.run(
            [str(args.runner.resolve()), str(workspace.resolve()), "base64", "-d"],
            input=b"!invalid!\n", capture_output=True, timeout=60,
        )
        assert invalid_base64.returncode != 0, invalid_base64
        print("PASS realpath, od and base64 direct/ash/error cases")


if __name__ == "__main__":
    main()
