# /// script
# requires-python = ">=3.11"
# ///
"""Verify BusyBox grep options, regex, recursion, streams, and exit status."""
import argparse
from pathlib import Path
import shutil
import subprocess
import tempfile


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--runner", type=Path, required=True)
    parser.add_argument("--ash", type=Path, required=True)
    args = parser.parse_args()
    cases = [
        ("basic", "grep '^alpha$' lines", b"alpha\n", 0, None),
        ("extended", "grep -E '^(Beta|gamma)[0-9]*$' lines", b"Beta\ngamma\nBeta2\n", 0, None),
        ("literal", "printf 'a.b\\naxb\\n' | grep -F 'a.b'", b"a.b\n", 0, None),
        ("ignore-case", "grep -i '^beta' lines", b"Beta\nBeta2\n", 0, None),
        ("inverse", "grep -v -e alpha -e Beta lines", b"gamma\n", 0, None),
        ("whole-word", "grep -w alpha lines", b"alpha\n", 0, None),
        ("whole-line", "grep -x Beta lines", b"Beta\n", 0, None),
        ("only-match", "printf '12 a34b 567\\n' | grep -Eo '[0-9]+'", b"12\n34\n567\n", 0, None),
        ("count", "grep -c alpha lines", b"2\n", 0, None),
        ("number-limit", "grep -n -m 1 Beta lines", b"2:Beta\n", 0, None),
        ("zero-limit", "grep -m 0 alpha lines", b"", 1, None),
        ("patterns", "grep -f patterns lines", b"alpha\ngamma\n", 0, None),
        ("empty-pattern-file", "grep -f empty lines", b"", 1, None),
        ("newline-patterns", "grep -e 'alpha\ngamma' lines", b"alpha\nalphabet\ngamma\n", 0, None),
        ("quiet-match", "grep -q alpha lines", b"", 0, None),
        ("quiet-miss", "grep -q absent lines", b"", 1, None),
        ("context", "grep -n -B 1 -A 1 '^gamma$' lines", b"3-alphabet\n4:gamma\n5-Beta2\n", 0, None),
        ("context-gaps", "grep -n -A 0 -B 0 -e '^alpha$' -e '^gamma$' lines", b"1:alpha\n4:gamma\n", 0, None),
        ("filenames", "grep -l alpha lines other", b"lines\n", 0, None),
        ("unmatched-filenames", "grep -L alpha lines other", b"other\n", 0, None),
        ("multiple-files", "grep gamma lines other", b"lines:gamma\nother:gamma\n", 0, None),
        ("recursive", "grep -r MATCH tree", b"tree/a:MATCH first\ntree/sub/b:MATCH second\n", 0, None),
        ("long-line", "grep 'END$' long-line", b"x" * 196613 + b"END\n", 0, None),
        ("empty-pattern", "grep '' lines", b"alpha\nBeta\nalphabet\ngamma\nBeta2\n", 0, None),
        ("missing-file", "grep alpha missing", b"", 2, b"missing"),
        ("suppressed-error", "grep -s alpha missing", b"", 2, None),
        ("bad-regex", "grep '[' lines", b"", 2, b"bad regex"),
        ("bad-number", "grep -m -1 alpha lines", b"", 2, b"invalid number"),
        ("space-number", "grep -m ' 1' alpha lines", b"", 2, b"invalid number"),
        ("plus-number", "grep -m +1 alpha lines", b"", 2, b"invalid number"),
        ("overflow-number", "grep -m 2147483648 alpha lines", b"", 2, b"not in 0..2147483647 range"),
        ("directory-input", "grep alpha tree", b"", 2, None),
        ("closed-output", "grep alpha lines >&-", b"", 2, None),
    ]
    with tempfile.TemporaryDirectory(prefix="efi-busybox-grep-") as temporary:
        root = Path(temporary)
        shutil.copyfile(args.ash, root / "ash.wasm")
        fixtures = {
            "lines": b"alpha\nBeta\nalphabet\ngamma\nBeta2\n",
            "other": b"gamma\n",
            "patterns": b"^alpha$\n^gamma$\n",
            "empty": b"",
            "long-line": b"x" * 196613 + b"END\n",
            "tree/a": b"MATCH first\n",
            "tree/sub/b": b"MATCH second\n",
        }
        for name, data in fixtures.items():
            target = root / name
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(data)
        for name, command, expected, status, diagnostic in cases:
            result = subprocess.run(
                [str(args.runner.resolve()), str(root), "ash", "-c", command],
                capture_output=True, timeout=120,
            )
            actual = result.stdout
            if name == "recursive":
                actual = b"".join(sorted(actual.splitlines(keepends=True)))
            assert actual == expected, (name, actual[-256:], expected[-256:], result.stderr)
            assert result.returncode == (0 if status == 0 else 1), (name, result.stderr)
            if status == 0:
                assert not result.stderr, (name, result.stderr)
            else:
                assert f"Guest exited with {status}".encode() in result.stderr, (name, result.stderr)
                if diagnostic:
                    assert diagnostic in result.stderr, (name, result.stderr)
                if name in {"quiet-miss", "zero-limit", "empty-pattern-file", "suppressed-error"}:
                    assert result.stderr == f'Error: "Guest exited with {status}"\n'.encode(), (name, result.stderr)
            print(f"PASS upstream grep {name}", flush=True)


if __name__ == "__main__":
    main()
