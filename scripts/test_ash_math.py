# /// script
# requires-python = ">=3.11"
# ///
"""Verify upstream ash math/test implementations and nonlocal error recovery."""
import argparse
from pathlib import Path
import shutil
import subprocess
import tempfile

from wasm_math_fixture import math_cases, stage_math


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ["runner", "ash", "child"]:
        parser.add_argument(f"--{name}", type=Path, required=True)
    options = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix="efi-ash-math-") as temporary:
        directory = Path(temporary)
        shutil.copyfile(options.ash, directory / "ash.wasm")
        shutil.copyfile(options.child, directory / "ash_child.wasm")
        stage_math(directory)
        for name, _, stdout, diagnostic, status in math_cases():
            result = subprocess.run([
                str(options.runner.resolve()), str(directory), "ash", f"math-{name}.sh",
            ], capture_output=True, timeout=120)
            assert result.returncode == (0 if status == 0 else 1), (name, result.stderr[-512:])
            assert result.stdout == stdout, (name, result.stdout[-512:], stdout[-512:])
            if diagnostic:
                assert diagnostic in result.stderr, (name, result.stderr[-512:])
            elif status == 0:
                assert result.stderr == b"", (name, result.stderr[-512:])
            if status:
                assert f'Error: "Guest exited with {status}"'.encode() in result.stderr, (name, result.stderr[-512:])
            assert b"Guest trapped" not in result.stderr, (name, result.stderr[-512:])
            print(f"PASS actual ash math/test {name}")


if __name__ == "__main__":
    main()
