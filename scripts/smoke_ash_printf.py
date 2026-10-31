# /// script
# requires-python = ">=3.11"
# ///
"""Verify BusyBox printf in actual ash with byte-exact output."""

import argparse
from pathlib import Path
import random
import shlex
import shutil
import subprocess
import tempfile


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("runner", "ash", "quickjs", "child"):
        parser.add_argument(f"--{name}", type=Path, required=True)
    args = parser.parse_args()
    cases = [
        ("repeat", "printf '<%s>\\n' '' 'a b' λ", "<>\n<a b>\n<λ>\n".encode(), 0),
        ("missing", "printf '%s:%d:%c'", b":0:\0", 1),
        ("width", "printf '<%*.*s>' -6 3 abcdef", b"<abc   >", 0),
        ("integer limits", "printf '%d:%u:%x' -9223372036854775808 18446744073709551615 -2",
         b"-9223372036854775808:18446744073709551615:fffffffffffffffe", 0),
        ("integer syntax", "printf '%d:%d:%d' 010 0x11 +19", b"8:17:19", 0),
        ("float", "printf '%.2f:%.*f' 1.25 3 -2.5", b"1.25:-2.500", 0),
        ("escapes", r"printf '%b' 'a\000b\x43\n'", b"a\0bC\n", 0),
        ("stop", r"printf '%bAFTER' 'before\cafter' extra", b"before", 0),
        ("invalid number", "printf '%d' bad", b"0", 1),
        ("overflow", "printf '%d' 9223372036854775808", b"0", 1),
        ("invalid format", "printf '%'", b"", 1),
        ("usage", "printf", b"", 2),
        ("pipe", "printf '%s\\n' first second | ash_child copy", b"first\nsecond\n", 0),
        ("read NUL", r"printf '%b' 'one\000two\n' | { read -r value; printf '<%s>' \"$value\"; }".replace(r'\"', '"'),
         b"<onetwo>", 0),
    ]
    with tempfile.TemporaryDirectory(prefix="efi-ash-printf-") as directory:
        tree = Path(directory)
        for name in ("ash", "quickjs"):
            shutil.copyfile(getattr(args, name), tree / f"{name}.wasm")
        shutil.copyfile(args.child, tree / "ash_child.wasm")

        def run(script):
            return subprocess.run(
                [str(args.runner.resolve()), str(tree), "ash", "-c", script],
                capture_output=True, timeout=120,
            )

        for name, script, expected, status in cases:
            result = run(script)
            assert result.stdout == expected, (name, result)
            assert result.returncode == (0 if status == 0 else 1), (name, result)
            if status:
                assert f'Error: "Guest exited with {status}"'.encode() in result.stderr, (name, result)
            else:
                assert not result.stderr, (name, result)
            assert b"Guest trapped" not in result.stderr, (name, result)
            print(f"PASS ash printf {name}")

        generator = random.Random(719)
        numbers = [-(1 << 63), (1 << 63) - 1, 0] + [generator.randrange(-(1 << 63), 1 << 63) for _ in range(20)]
        script = "printf '%d:%x\\n' " + " ".join(shlex.quote(str(number)) + " " + shlex.quote(str(number)) for number in numbers)
        expected = "".join(f"{number}:{number & ((1 << 64) - 1):x}\n" for number in numbers).encode()
        result = run(script)
        assert result.returncode == 0 and not result.stderr and result.stdout == expected, result
        print("PASS signed and hexadecimal conversions match independent integer oracle")

        script = r"""
const result = await ash`printf '%b' 'a\\000b'`;
if (result.stdout !== 'a\0b' || result.stderr !== '') throw Error('binary capture');
print('QUICKJS_PRINTF_BINARY_OK');
"""
        result = subprocess.run(
            [str(args.runner.resolve()), str(tree), "quickjs", "-e", script],
            capture_output=True, timeout=120,
        )
        assert result.returncode == 0 and not result.stderr, result
        assert result.stdout == b"QUICKJS_PRINTF_BINARY_OK\n", result
        print("PASS QuickJS actual printf NUL capture")


if __name__ == "__main__":
    main()
