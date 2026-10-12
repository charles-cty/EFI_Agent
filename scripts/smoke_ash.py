# /// script
# requires-python = ">=3.11"
# ///
"""Run real BusyBox ash parser and builtin checks in host wasmi."""

import argparse
from pathlib import Path
import subprocess


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--runner", type=Path, required=True)
    parser.add_argument("--directory", type=Path, required=True)
    parser.add_argument("--quickjs", action="store_true")
    args = parser.parse_args()
    cases = [
        ("echo", "echo ASH_REAL_ENGINE_OK", "ASH_REAL_ENGINE_OK\n", 0),
        ("quoting", 'x="a b"; echo "$x"; echo \'$x\'; echo "${missing:-fallback}"',
         "a b\n$x\nfallback\n", 0),
        ("function", 'f() { echo "arg:$1"; return 7; }; f "two words"; echo "status:$?"',
         "arg:two words\nstatus:7\n", 0),
        ("loop", 'for x in first "two words" last; do echo "$x"; done',
         "first\ntwo words\nlast\n", 0),
        ("condition", 'if false; then echo wrong; else echo correct; fi; false || echo recovered',
         "correct\nrecovered\n", 0),
        ("exit", "echo before; exit 37; echo wrong", "before\n", 37),
        ("syntax", "if then", "", 2),
        ("subshell", 'x=parent; (x=child; echo "$x"); echo "$x"', "child\nparent\n", 0),
        ("subshell function", 'f() { echo "$x:$1"; }; x=state; (f "two words")',
         "state:two words\n", 0),
    ]
    for name, script, expected, status in cases:
        result = subprocess.run(
            [str(args.runner.resolve()), str(args.directory.resolve()), "ash", "-c", script],
            capture_output=True, text=True, encoding="utf-8", timeout=60,
        )
        assert result.stdout == expected, (name, result)
        if status == 0:
            assert result.returncode == 0 and result.stderr == "", (name, result)
        else:
            assert result.returncode == 1, (name, result)
            assert f'Error: "Guest exited with {status}"' in result.stderr.splitlines(), (name, result)
            assert "Guest trapped" not in result.stderr, (name, result)
        if name == "syntax":
            assert "syntax error" in result.stderr.lower(), result
        print(f"PASS real ash {name}")

    if args.quickjs:
        script = r"""
const results = await Promise.all([
    $`echo ${"a'b"} ${['', 'x;$(bad)', 'λ']}`,
    $`echo ${"second"}`,
]);
if (results[0].stdout !== "a'b  x;$(bad) λ\n" ||
    results[1].stdout !== 'second\n') throw Error('Shell quoting or capture');
let rejected = false;
try { await $`exit 17`; } catch (result) {
    rejected = result.exitCode === 17;
}
if (!rejected) throw Error('Shell status lost');
print('QUICKJS_REAL_ASH_OK');
"""
        result = subprocess.run(
            [str(args.runner.resolve()), str(args.directory.resolve()), "quickjs", "-e", script],
            capture_output=True, text=True, encoding="utf-8", timeout=120,
        )
        assert result.returncode == 0, result
        assert result.stdout == "QUICKJS_REAL_ASH_OK\n" and not result.stderr, result
        print("PASS real QuickJS and ash: concurrent shells, quoted substitutions, exit rejection")


if __name__ == "__main__":
    main()
