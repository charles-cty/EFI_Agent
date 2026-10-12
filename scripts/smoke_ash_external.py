# /// script
# requires-python = ">=3.11"
# ///
"""Verify real ash external dispatch and QuickJS capture in host wasmi."""

import argparse
from pathlib import Path
import shutil
import subprocess
import tempfile


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("runner", "ash", "child", "quickjs"):
        parser.add_argument(f"--{name}", type=Path, required=True)
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix="efi-ash-external-") as directory:
        tree = Path(directory)
        for name in ("ash", "quickjs"):
            shutil.copyfile(getattr(args, name), tree / f"{name}.wasm")
        shutil.copyfile(args.child, tree / "ash_child.wasm")

        def run(program, script, status):
            result = subprocess.run(
                [str(args.runner.resolve()), str(tree), program,
                 "-c" if program == "ash" else "-e", script],
                capture_output=True, text=True, encoding="utf-8", timeout=120,
            )
            assert result.returncode == (0 if status == 0 else 1), result
            if status:
                assert f'Error: "Guest exited with {status}"' in result.stderr.splitlines(), result
            assert "Guest trapped" not in result.stderr, result
            return result

        result = run("ash", """
export EXPORTED=base
LOCAL=private
TEMP=temporary EXPORTED=override ash_child inspect '' "a'b" λ
echo "status:$?"
echo "$EXPORTED:${TEMP-unset}"
ash_child exit 17
echo "continued:$?"
exec ash_child exit 23
echo wrong
""", 23)
        assert result.stdout == "ASH_EXTERNAL_ARGUMENTS_ENV_OK\nstatus:7\nbase:unset\ncontinued:17\n", result
        assert "ASH_EXTERNAL_STDERR_OK" in result.stderr.splitlines(), result
        print("PASS ash external arguments, export overlay, isolation, status, continuation and exec")

        result = run("ash", """
export EXPORTED=base
TEMP=wrong TEMP=temporary EXPORTED=wrong EXPORTED=override ash_child inspect '' "a'b" λ
echo "status:$?"
echo "$EXPORTED:${TEMP-unset}"
""", 0)
        assert result.stdout == "ASH_EXTERNAL_ARGUMENTS_ENV_OK\nstatus:7\nbase:unset\n", result
        print("PASS ash repeated temporary assignments use the last value")

        result = run("ash", "set -e; ash_child exit 17; echo wrong", 17)
        assert not result.stdout, result
        print("PASS ash external set -e")
        result = run("ash", "missing_program; echo status:$?", 0)
        assert result.stdout == "status:127\n", result
        assert "not found" in result.stderr, result
        print("PASS ash missing command status")
        result = run("ash", "ash_child() { echo function; }; ash_child; command ash_child exit 13; echo status:$?", 0)
        assert result.stdout == "function\nstatus:13\n" and not result.stderr, result
        print("PASS ash function precedence and command bypass")

        script = 'x=' + 'x' * 23000 + ';export BIG="$x$x$x";ash_child exit 17;echo status:$?'
        result = run("ash", script, 0)
        assert result.stdout == "status:126\n", result
        assert "ash_child:" in result.stderr, result
        print("PASS ash spawn failure status 126 after environment expansion")
        result = run("ash", "ash_child exit -1; echo negative:$?; ash_child exit 300; echo large:$?", 0)
        assert result.stdout == "negative:255\nlarge:44\n" and not result.stderr, result
        print("PASS ash converts direct i32 guest exits to eight-bit shell statuses")

        result = run("quickjs", r"""
const ok = await $`TEMP=temporary EXPORTED=override ash_child inspect ${['', "a'b", 'λ']}; echo status:$?`;
if (ok.stdout !== 'ASH_EXTERNAL_ARGUMENTS_ENV_OK\nstatus:7\n' ||
    ok.stderr !== 'ASH_EXTERNAL_STDERR_OK\n') throw Error('External capture');
let rejected = false;
try { await $`ash_child exit 17`; } catch (result) {
    rejected = result.exitCode === 17;
}
if (!rejected) throw Error('External status');
print('QUICKJS_ASH_EXTERNAL_OK');
""", 0)
        assert result.stdout == "QUICKJS_ASH_EXTERNAL_OK\n" and not result.stderr, result
        print("PASS QuickJS real ash external capture and rejection")


if __name__ == "__main__":
    main()
