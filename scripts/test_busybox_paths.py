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
        print("PASS realpath direct, missing-path error and ash dispatch")


if __name__ == "__main__":
    main()
