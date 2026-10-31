# /// script
# requires-python = ">=3.11"
# ///
"""Verify real ash fresh shell tasks, pipes and inherited shell state."""

import argparse
from pathlib import Path
import shutil
import random
import shlex
import subprocess
import tempfile


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("runner", "ash", "child", "quickjs"):
        parser.add_argument(f"--{name}", type=Path, required=True)
    parser.add_argument("--task-input", type=Path)
    args = parser.parse_args()
    cases = [
        ("subshell state", 'x=parent; (x=child; echo "$x"); echo "$x"', "child\nparent\n"),
        ("function and parameters", 'set -- "one two" "λ"; x=state; f() { echo "$x:$1:$2"; }; (f "$1" "$2"; set -- child); echo "$1:$2"',
         "state:one two:λ\none two:λ\n"),
        ("control tree", '(for x in first "two words" last; do if false; then echo wrong; else echo "$x"; fi; done)|ash_child copy',
         "first\ntwo words\nlast\n"),
        ("pipe builtin", 'echo "one two λ" | ash_child copy', "one two λ\n"),
        ("bounded pipeline", 'ash_child produce | ash_child copy | ash_child verify', "ASH_BOUNDED_PIPELINE_OK\n"),
        ("pipeline status", 'ash_child exit 7 | ash_child exit 17; echo status:$?', "status:17\n"),
        ("pipeline function", 'x=parent; f() { x=child; echo "$x:$1"; }; f "two words" | ash_child copy; echo "$x"',
         "child:two words\nparent\n"),
        ("background wait", '(echo background; exit 17) & wait $!; echo status:$?', "background\nstatus:17\n"),
        ("nested pipeline", '(echo nested | ash_child copy) | ash_child copy', "nested\n"),
        ("command substitution", 'x=parent; value=$(x=child; echo "$x"; echo; echo); echo "<$value>:$x"',
         "<child>:parent\n"),
        ("nested substitution", 'echo "$(echo "$(echo nested)")"', "nested\n"),
        ("substitution status", 'value=$(echo result; exit 17); echo "$value:$?"', "result:17\n"),
        ("substitution pipeline", 'value=$(ash_child produce | ash_child copy | ash_child verify); echo "$value"',
         "ASH_BOUNDED_PIPELINE_OK\n"),
        ("substitution errexit", 'set -e; value=$(false; echo survived); echo "$value"', "survived\n"),
        ("alias state", "alias greet='echo alias'; (eval greet)|ash_child copy", "alias\n"),
        ("getopts state", 'set -- -a -b; getopts ab flag; (getopts ab flag; echo "$flag:$OPTIND"); echo "$flag:$OPTIND"',
         "b:3\na:2\n"),
        ("shell pid", 'original=$$; (case $$ in "$original") echo same;; *) echo wrong;; esac)|ash_child copy', "same\n"),
        ("tested subshell", 'set -e; (false; echo survived) || echo wrong; echo parent', "survived\nparent\n"),
        ("tested pipeline", 'set -e; { false; echo survived; } | ash_child copy || echo wrong; echo parent',
         "survived\nparent\n"),
        ("exported unset variable", 'export absent; (echo "${absent-unset}");:', "unset\n"),
        ("task snapshot preserves inherited input", 'printf "first\\nsecond\\n">task-input.txt;exec 0<task-input.txt;(IFS= read -r value;echo "child:$value");IFS= read -r value;echo "parent:$value"',
         "child:first\nparent:second\n"),
    ]
    with tempfile.TemporaryDirectory(prefix="efi-ash-tasks-") as directory:
        tree = Path(directory)
        for name in ("ash", "quickjs"):
            shutil.copyfile(getattr(args, name), tree / f"{name}.wasm")
        shutil.copyfile(args.child, tree / "ash_child.wasm")
        for name, script, expected in cases:
            result = subprocess.run(
                [str(args.runner.resolve()), str(tree), "ash", "-c", script],
                capture_output=True, text=True, encoding="utf-8", timeout=120,
            )
            assert result.returncode == 0 and not result.stderr, (name, result)
            assert result.stdout == expected, (name, result)
            print(f"PASS ash task {name}")

        generator = random.Random(19347)
        for iteration in range(5):
            values = ["".join(generator.choice("ab λ'$;\n") for _ in range(index * 3 + 1))
                      for index in range(12)]
            values[0] = ""
            values[-1] = "λ'$;\n"
            assignments = ";".join(f"V{index}={shlex.quote(value)}" for index, value in enumerate(values))
            words = " ".join(f'"$V{index}"' for index in range(len(values)))
            script = assignments + ';f(){ for word in ' + words + ';do echo "$word";done;};f|ash_child copy;echo "$V0"'
            expected = "".join(value + "\n" for value in values) + values[0] + "\n"
            result = subprocess.run(
                [str(args.runner.resolve()), str(tree), "ash", "-c", script],
                capture_output=True, text=True, encoding="utf-8", timeout=120,
            )
            assert result.returncode == 0 and not result.stderr and result.stdout == expected, (iteration, result)
        print("PASS structured random shell state matches independent host string oracle")

        for script, expected, diagnostic in [
            ('readonly lock=original; (lock=changed); echo "$lock:$?"', "original:2\n", "read only"),
            ('set -u; (echo "$absent"); echo status:$?', "status:2\n", "not set"),
        ]:
            result = subprocess.run(
                [str(args.runner.resolve()), str(tree), "ash", "-c", script],
                capture_output=True, text=True, encoding="utf-8", timeout=120,
            )
            assert result.returncode == 0 and result.stdout == expected, result
            assert diagnostic in result.stderr and "Guest trapped" not in result.stderr, result
        print("PASS inherited readonly variables and nounset options fail inside the child only")

        script = '(echo wrong)>missing-dir/$(TEMP=temporary EXPORTED=override ash_child inspect \'\' "a\'b" λ);echo done:$?'
        result = subprocess.run(
            [str(args.runner.resolve()), str(tree), "ash", "-c", script],
            capture_output=True, text=True, encoding="utf-8", timeout=120,
        )
        assert result.returncode == 0 and result.stdout == "done:2\n", result
        # The guest filesystem exists; the parent directory does not. The
        # substitution must still execute once, without replay in the child.
        assert result.stderr.splitlines().count("ASH_EXTERNAL_STDERR_OK") == 1, result
        assert "Guest trapped" not in result.stderr, result
        print("PASS subshell redirection expansion executes once before the file error")

        for exponent, status, expected in [(12, 0, "within\n"), (13, 2, "")]:
            # A following command prevents ash's terminal subshell optimization.
            # Build 256/512 KiB of state without exceeding the argv limit.
            script = "x=" + "x" * 64 + f";i=0;while [ $i -lt {exponent} ];do x=\"$x$x\";i=$((i+1));done;(echo within);:"
            result = subprocess.run(
                [str(args.runner.resolve()), str(tree), "ash", "-c", script],
                capture_output=True, text=True, encoding="utf-8", timeout=120,
            )
            assert result.stdout == expected and result.returncode == (0 if status == 0 else 1), (exponent, result.returncode, result.stdout, result.stderr)
            if status:
                assert f'Error: "Guest exited with {status}"' in result.stderr, (exponent, result.stderr)
                assert "transfer limit" in result.stderr and "Guest trapped" not in result.stderr, (exponent, result.stderr)
        print("PASS shell task transfer size accepts and rejects opposite sides of its limit")

        # NCMD plus the command word and values: depth = count + 2.
        for count in (254, 255):
            script = "echo " + "x " * count + "|ash_child copy"
            result = subprocess.run(
                [str(args.runner.resolve()), str(tree), "ash", "-c", script],
                capture_output=True, text=True, encoding="utf-8", timeout=120,
            )
            if count == 254:
                assert result.returncode == 0 and not result.stderr, (count, result.stderr)
                assert result.stdout == " ".join(["x"] * count) + "\n", (count, result.stdout)
            else:
                assert result.returncode == 1 and not result.stdout, (count, result.stdout, result.stderr)
                assert 'Error: "Guest exited with 2"' in result.stderr, result.stderr
                assert "depth" in result.stderr and "Guest trapped" not in result.stderr, result.stderr
        print("PASS shell task tree depth bound")

        for state in ["", "0", "-1", "g0", "524289", "999999999999999999999999", "8"]:
            result = subprocess.run(
                [str(args.runner.resolve()), str(tree), "ash", "--efi-task", state, "4"],
                capture_output=True, text=True, encoding="utf-8", timeout=120,
            )
            assert result.returncode == 1 and not result.stdout, result
            assert 'Error: "Guest exited with 2"' in result.stderr and "Guest trapped" not in result.stderr, result
        print("PASS malformed shell task state gives domain errors")

        if args.task_input:
            shutil.copyfile(args.task_input, tree / "ash_task_input.wasm")
            result = subprocess.run(
                [str(args.runner.resolve()), str(tree), "ash_task_input"],
                capture_output=True, text=True, encoding="utf-8", timeout=120,
            )
            assert result.returncode == 0, result
            assert result.stdout == "ASH_TASK_BINARY_REJECTION_OK\n", result
            diagnostics = [
                "invalid or truncated shell task input",
                "invalid or truncated shell task input",
                "invalid shell task input length",
                "invalid shell task execution flags",
                "invalid shell task state version",
                "shell task state exceeds its transfer limit or is truncated",
            ]
            assert result.stderr.splitlines() == [f"ash: {text}" for text in diagnostics], result.stderr
            print("PASS binary shell task exact length, flags, version and truncated state errors")

        result = subprocess.run(
            [str(args.runner.resolve()), str(tree), "ash", "-c",
             "i=0;while [ $i -lt 300 ];do (:);i=$((i+1));done;echo TASK_SNAPSHOT_REPEAT_OK"],
            capture_output=True, text=True, encoding="utf-8", timeout=120,
        )
        assert result.returncode == 0 and not result.stderr, result
        assert result.stdout == "TASK_SNAPSHOT_REPEAT_OK\n", result
        print("PASS 300 fresh shell tasks release snapshot descriptors")

        script = r"""
const result = await ash`ash_child produce | ash_child copy | ash_child verify`;
if (result.stdout !== 'ASH_BOUNDED_PIPELINE_OK\n' || result.stderr !== '')
    throw Error('Pipeline capture');
print('QUICKJS_ASH_PIPELINE_OK');
"""
        result = subprocess.run(
            [str(args.runner.resolve()), str(tree), "quickjs", "-e", script],
            capture_output=True, text=True, encoding="utf-8", timeout=120,
        )
        assert result.returncode == 0 and not result.stderr, result
        assert result.stdout == "QUICKJS_ASH_PIPELINE_OK\n", result
        print("PASS QuickJS real ash bounded pipeline capture")


if __name__ == "__main__":
    main()
