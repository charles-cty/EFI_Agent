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
        digest = run(args.runner, workspace, "sha256sum", "binary.bin")
        assert digest.returncode == 0, digest
        assert digest.stdout == "be45cb2605bf36bebde684841a28f0fd43c69850a3dce5fedba69928ee3a8991  binary.bin\n", digest
        link = run(args.runner, workspace, "ln", "-s", "binary.bin", "link.bin")
        assert link.returncode == 0, link
        target = run(args.runner, workspace, "realpath", "link.bin")
        assert target.returncode == 0 and target.stdout == "/binary.bin\n", target
        replaced = run(args.runner, workspace, "ln", "-sf", "input.txt", "link.bin")
        assert replaced.returncode == 0, replaced
        target = run(args.runner, workspace, "realpath", "link.bin")
        assert target.returncode == 0 and target.stdout == "/input.txt\n", target
        hard = run(args.runner, workspace, "ln", "binary.bin", "hard.bin")
        assert hard.returncode == 0, hard
        missing_link = run(args.runner, workspace, "ln", "-s", "missing.bin", "dangling")
        assert missing_link.returncode == 0, missing_link
        hexed = run(args.runner, workspace, "hexdump", "-C", "binary.bin")
        assert hexed.returncode == 0, hexed
        assert hexed.stdout == (
            "00000000  00 01 02 03 04 05 06 07  08 09 0a 0b 0c 0d 0e 0f  |................|\n"
            "00000010\n"
        ), hexed
        hex_stdin = subprocess.run(
            [str(args.runner.resolve()), str(workspace.resolve()), "hexdump", "-C"],
            input=b"AB", capture_output=True, text=True, timeout=60,
        )
        assert hex_stdin.returncode == 0 and "41 42" in hex_stdin.stdout, hex_stdin
        invalid_hex = run(args.runner, workspace, "hexdump", "-Z", "binary.bin")
        assert invalid_hex.returncode != 0, invalid_hex
        print("PASS realpath, od, base64, sha256sum and ln direct/ash/error cases")


if __name__ == "__main__":
    main()
