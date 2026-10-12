# /// script
# requires-python = ">=3.11"
# ///
"""Verify real BusyBox read in cooperative ash pipelines."""

import argparse
from pathlib import Path
import random
import re
import shlex
import shutil
import subprocess
import tempfile


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("runner", "ash", "child", "quickjs"):
        parser.add_argument(f"--{name}", type=Path, required=True)
    args = parser.parse_args()
    cases = [
        ("whitespace", "  first \t second  third  \n", 'read a b; echo "<$a><$b>:$?"',
         "<first><second  third>:0\n"),
        ("colon remainder", "X:Y:Z:\n", 'IFS=: read a b; echo "<$a><$b>:$?"', "<X><Y:Z:>:0\n"),
        ("colon final delimiter", "X:Y:\n", 'IFS=: read a b; echo "<$a><$b>:$?"', "<X><Y>:0\n"),
        ("colon empty fields", ":a::b\n", 'IFS=: read a b; echo "<$a><$b>:$?"', "<><a::b>:0\n"),
        ("fewer fields", "one\n", 'read a b c; echo "<$a><$b><$c>:$?"', "<one><><>:0\n"),
        ("raw", "a\\ b\\c\n", 'read -r a b; echo "<$a><$b>:$?"', "<a\\><b\\c>:0\n"),
        ("escaped", "a\\ b\\c\n", 'read a b; echo "<$a><$b>:$?"', "<a bc><>:0\n"),
        ("continuation", "first\\\nsecond\n", 'read a; echo "<$a>:$?"', "<firstsecond>:0\n"),
        ("empty IFS", "  one two  \n", 'IFS= read -r a; echo "<$a>:$?"', "<  one two  >:0\n"),
        ("EOF partial", "last partial", 'read a b; echo "<$a><$b>:$?"', "<last><partial>:1\n"),
        ("EOF empty", "", 'a=old; read a; echo "<$a>:$?"', "<>:1\n"),
        ("count boundary", "abcde\n", 'read -n 2 a; read b; echo "<$a><$b>:$?"', "<ab><cde>:0\n"),
        ("delimiter", "one:two\n", 'read -d : a; read b; echo "<$a><$b>:$?"', "<one><two>:0\n"),
        ("line loop", "first\n\nλ\n", 'while IFS= read -r a; do echo "<$a>"; done', "<first>\n<>\n<λ>\n"),
        ("timed input", "ready\n", 'read -t 1 a; echo "<$a>:$?"', "<ready>:0\n"),
        ("bad descriptor", "ready\n", 'read -u 99 a; echo status:$?', "status:1\n"),
        ("bad readiness descriptor", "ready\n", 'read -t 0 -u 99 a; echo status:$?', "status:1\n"),
    ]
    with tempfile.TemporaryDirectory(prefix="efi-ash-read-") as directory:
        tree = Path(directory)
        for name in ("ash", "quickjs"):
            shutil.copyfile(getattr(args, name), tree / f"{name}.wasm")
        shutil.copyfile(args.child, tree / "ash_child.wasm")

        def run(data, consumer, delayed=False):
            mode = "delay_emit" if delayed else "emit"
            script = f"ash_child {mode} {shlex.quote(data)} | {{ {consumer}; }}"
            result = subprocess.run(
                [str(args.runner.resolve()), str(tree), "ash", "-c", script],
                capture_output=True, text=True, encoding="utf-8", timeout=120,
            )
            assert "Guest trapped" not in result.stderr, result
            return result

        for name, data, consumer, expected in cases:
            result = run(data, consumer)
            assert result.returncode == 0 and result.stdout == expected and not result.stderr, (name, result)
            print(f"PASS ash read {name}")

        generator = random.Random(8071)
        for iteration in range(10):
            words = ["".join(generator.choice("abλ'\\$:") for _ in range(index + 1))
                     for index in range(generator.randrange(2, 7))]
            data = " \t" + "".join(word + generator.choice([" ", "  ", "\t "]) for word in words) + "\n"
            matches = list(re.finditer(r"[^ \t\n]+", data))
            values = [matches[0].group(), matches[1].group(), ""]
            if len(matches) > 2:
                values[2] = data[matches[2].start():].rstrip(" \t\n")
            result = run(data, 'read -r a b c; echo "$a|$b|$c:$?"')
            assert result.returncode == 0 and not result.stderr, (iteration, result)
            assert result.stdout == "|".join(values) + ":0\n", (iteration, result)
        print("PASS structured raw IFS reads match independent host field oracle")

        result = run("later\n", 'a=old; read -t 1 a; status=$?; read b; echo "$a:$status:$b"', delayed=True)
        assert result.returncode == 0 and result.stdout == "old:1:later\n" and not result.stderr, result
        print("PASS ash timeout while writer stays open, followed by successful blocking read")

        for consumer, diagnostic in [
            ("read -n -1 a", "invalid count"),
            ("read -s a", "terminal echo control is unavailable"),
            ("read invalid-name", "bad variable name"),
        ]:
            result = run("line\n", consumer)
            assert result.returncode == 1 and diagnostic in result.stderr, result
            assert not result.stdout, result
        print("PASS ash read invalid options and unsupported terminal control")

        script = r"""
const result = await $`echo ${'one two λ'} | { read -r a b; echo "$a:$b"; }`;
if (result.stdout !== 'one:two λ\n' || result.stderr !== '') throw Error('read capture');
print('QUICKJS_ASH_READ_OK');
"""
        result = subprocess.run(
            [str(args.runner.resolve()), str(tree), "quickjs", "-e", script],
            capture_output=True, text=True, encoding="utf-8", timeout=120,
        )
        assert result.returncode == 0 and not result.stderr, result
        assert result.stdout == "QUICKJS_ASH_READ_OK\n", result
        print("PASS QuickJS actual ash read pipeline")


if __name__ == "__main__":
    main()
