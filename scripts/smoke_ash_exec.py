# /// script
# requires-python = ">=3.11"
# ///
"""Verify the ash fresh-instance exec bridge, without claiming full ash execution."""
import argparse
from pathlib import Path
import subprocess


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--runner", type=Path, required=True)
    parser.add_argument("--directory", type=Path, required=True)
    args = parser.parse_args()
    result = subprocess.run(
        [str(args.runner.resolve()), str(args.directory.resolve()), "ash_exec"],
        capture_output=True, text=True, encoding="utf-8", timeout=60,
    )
    # The runner reports guest nonzero exits as a host error. Check its exact
    # guest status as well as the marker; a trap must not satisfy this test.
    assert result.returncode == 1, result
    assert "ASH_EXEC_FRESH_INSTANCE_OK" in result.stdout.splitlines(), result.stdout
    assert 'Error: "Guest exited with 37"' in result.stderr.splitlines(), result.stderr
    print("PASS ash exec bridge: fresh child, explicit environment, failure errno, status propagation")


if __name__ == "__main__":
    main()
