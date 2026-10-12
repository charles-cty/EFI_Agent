# /// script
# requires-python = ">=3.11"
# ///
"""Verify actual BusyBox ash here-documents without fork or pipe writers."""
import argparse
from pathlib import Path
import shutil
import subprocess
import tempfile

from wasm_heredoc_fixture import heredoc_cases, stage_heredocs


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ["runner", "ash", "child"]:
        parser.add_argument(f"--{name}", type=Path, required=True)
    options = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix="efi-ash-heredocs-") as temporary:
        directory = Path(temporary)
        shutil.copyfile(options.ash, directory / "ash.wasm")
        shutil.copyfile(options.child, directory / "ash_child.wasm")
        stage_heredocs(directory)
        for name, _, stdout, stderr, status in heredoc_cases():
            result = subprocess.run([
                str(options.runner.resolve()), str(directory), "ash", f"heredoc-{name}.sh",
            ], capture_output=True, timeout=120)
            if status == 0:
                assert result.returncode == 0, (name, result.stderr[-512:])
                assert result.stderr == stderr, (name, result.stderr[-512:])
            else:
                assert result.returncode != 0, name
                assert f'Error: "Guest exited with {status}"\n'.encode() in result.stderr, result.stderr[-512:]
                assert b"cannot create here-document input" in result.stderr, result.stderr[-512:]
                assert b"Guest trapped" not in result.stderr, result.stderr[-512:]
            assert result.stdout == stdout, (name, len(result.stdout), len(stdout))
            print(f"PASS actual ash here-document {name}")


if __name__ == "__main__":
    main()
