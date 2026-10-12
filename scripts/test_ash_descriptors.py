# /// script
# requires-python = ">=3.11"
# ///
"""Verify real ash user descriptor inheritance without retaining internal pipes."""
import argparse
from pathlib import Path
import shutil
import subprocess
import tempfile

from wasm_descriptor_fixture import descriptor_cases, stage_descriptors


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ["runner", "ash", "child"]:
        parser.add_argument(f"--{name}", type=Path, required=True)
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix="efi-ash-descriptors-") as temporary:
        directory = Path(temporary)
        shutil.copyfile(args.ash, directory / "ash.wasm")
        shutil.copyfile(args.child, directory / "ash_child.wasm")
        stage_descriptors(directory)
        for name, _, expected in descriptor_cases():
            result = subprocess.run([
                str(args.runner.resolve()), str(directory), "ash", f"fd-{name}.sh",
            ], capture_output=True, timeout=90)
            assert result.returncode == 0 and result.stderr == b"", (name, result)
            assert result.stdout == expected, (name, result.stdout, expected)
            print(f"PASS actual ash descriptor inheritance {name}")


if __name__ == "__main__":
    main()
