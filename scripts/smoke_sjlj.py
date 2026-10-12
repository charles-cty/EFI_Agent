# /// script
# requires-python = ">=3.11"
# ///
"""Check transformed C nonlocal jumps against a separate host arithmetic oracle."""
import argparse
from pathlib import Path
import re
import subprocess


def digest(seed):
    state = seed
    value = 2166136261
    for _ in range(37):
        state = (state * 1664525 + 1013904223) & 0xFFFFFFFF
        result = state % 2001 - 1000
        if result == 0:
            result = 1
        value = ((value ^ (result & 0xFFFFFFFF)) * 16777619) & 0xFFFFFFFF
    return value


def run(runner, directory, arguments, seeds):
    result = subprocess.run(
        [str(runner.resolve()), str(directory.resolve()), "sjlj", *arguments],
        capture_output=True, text=True, encoding="utf-8", timeout=120,
    )
    if result.returncode != 0:
        raise RuntimeError(f"C nonlocal jump failed:\n{result.stdout}\n{result.stderr}")
    actual = sorted((int(seed), int(value)) for seed, value in
                    re.findall(r"^SJLJ_ORACLE (\d+) (\d+)$", result.stdout, re.MULTILINE))
    assert actual == sorted((seed, digest(seed)) for seed in seeds), (actual, result.stdout)
    assert result.stdout.splitlines().count("SJLJ_NESTED_REPEATED_OK") == len(seeds)
    if not arguments:
        assert "SJLJ_MULTI_INSTANCE_OK" in result.stdout.splitlines()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--runner", type=Path, required=True)
    parser.add_argument("--directory", type=Path, required=True)
    args = parser.parse_args()
    run(args.runner, args.directory, [], [11, 17, 29])
    for seed in (0, 1, 2, 999, 65535, 123456789, 0xFFFFFFFF):
        run(args.runner, args.directory, [str(seed)], [seed])
    print("PASS C nonlocal jumps: host oracle, deep/nested/repeated jumps, concurrent instances")


if __name__ == "__main__":
    main()
