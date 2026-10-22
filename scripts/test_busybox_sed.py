# /// script
# requires-python = ">=3.11"
# ///
"""Verify upstream BusyBox sed with real WASM ash and persistent file bytes."""
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
        ("regex-global", "sed 's/[a-z][a-z]*/X/g' words", b"X 11 X\nX 22 X\nX 33 X\n", 0, None),
        ("backreference", r"sed -E 's/([a-z]+) ([0-9]+)/\2=\1/' words", b"11=red red\n22=blue blue\n33=green red\n", 0, None),
        ("addresses", "sed -n '2,3p' words", b"blue 22 blue\ngreen 33 red\n", 0, None),
        ("last-line", "sed -n '$p' words", b"green 33 red\n", 0, None),
        ("regex-range", "sed -n '/blue/,$p' words", b"blue 22 blue\ngreen 33 red\n", 0, None),
        ("multiple-expressions", "sed -e 's/red/R/g' -e 's/blue/B/g' words", b"R 11 R\nB 22 B\ngreen 33 R\n", 0, None),
        ("script-file", "sed -f script.sed words", b"red 11 red\nblue 22 blue\n", 0, None),
        ("hold-space", "printf 'one\\ntwo\\n' | sed -n '1h;2{G;p;}'", b"two\none\n", 0, None),
        ("branch", "printf 'aaab\\n' | sed ':again;s/aa/a/;t again'", b"ab\n", 0, None),
        ("transliterate", "printf 'red blue\\n' | sed 'y/ed/ED/'", b"rED bluE\n", 0, None),
        ("multiline", "printf 'one\\ntwo\\nthree\\nfour\\n' | sed 'N;s/\\n/:/'", b"one:two\nthree:four\n", 0, None),
        ("read-file", "printf 'top\\n' | sed 'r extra'", b"top\nextra\n", 0, None),
        ("write-file", "sed -n '2w written' words; cat written", b"blue 22 blue\n", 0, None),
        ("no-final-newline", "sed 's/tail/TAIL/' unterminated", b"TAIL", 0, None),
        ("binary-nul", "sed 's/b/B/' binary", b"a\x00B\n", 0, None),
        ("long-line", "sed 's/END$/done/' long-line", b"x" * 196613 + b"done\n", 0, None),
        ("in-place", "sed -i 's/old/new/g' inplace; cat inplace", b"new new\n", 0, None),
        ("backup", "sed -i.bak 's/new/next/g' inplace; cat inplace inplace.bak", b"next next\nnew new\n", 0, None),
        ("long-options", "sed --quiet --regexp-extended --expression 's/([0-9]+)/[\\1]/p' words", b"red [11] red\nblue [22] blue\ngreen [33] red\n", 0, None),
        ("missing-file", "sed 's/a/b/' missing", b"", 1, b"missing"),
        ("bad-regex", "sed -E 's/(a/b/' words", b"", 2, b"bad regex"),
        ("bad-expression", "sed 's/[a/b/' words", b"", 2, b"unmatched"),
        ("bad-option", "sed -Q words", b"", 2, b"invalid applet arguments"),
    ]
    with tempfile.TemporaryDirectory(prefix="efi-busybox-sed-") as temporary:
        root = Path(temporary)
        shutil.copyfile(args.ash, root / "ash.wasm")
        fixtures = {
            "words": b"red 11 red\nblue 22 blue\ngreen 33 red\n",
            "script.sed": b"3d\n",
            "extra": b"extra\n",
            "unterminated": b"tail",
            "binary": b"a\x00b\n",
            "long-line": b"x" * 196613 + b"END\n",
            "inplace": b"old old\n",
        }
        for name, data in fixtures.items():
            (root / name).write_bytes(data)
        for name, command, expected, status, diagnostic in cases:
            result = subprocess.run(
                [str(args.runner.resolve()), str(root), "ash", "-c", command],
                capture_output=True, timeout=120,
            )
            assert result.returncode == (0 if status == 0 else 1), (name, result.stderr)
            assert result.stdout == expected, (name, result.stdout[-256:], expected[-256:], result.stderr)
            if status == 0:
                assert not result.stderr, (name, result.stderr)
            else:
                assert f"Guest exited with {status}".encode() in result.stderr, (name, result.stderr)
                assert diagnostic in result.stderr, (name, result.stderr)
            print(f"PASS upstream sed {name}", flush=True)
        # In-place edits must leave no temporary names, including on a second run.
        result = subprocess.run(
            [str(args.runner.resolve()), str(root), "ash", "-c", "printf '%s\\n' inplace*"],
            capture_output=True, timeout=120,
        )
        assert result.returncode == 0 and not result.stderr, result.stderr
        assert result.stdout == b"inplace\ninplace.bak\n", result.stdout
        print("PASS upstream sed temporary-file cleanup", flush=True)


if __name__ == "__main__":
    main()
