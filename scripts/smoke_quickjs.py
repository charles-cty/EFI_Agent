# /// script
# requires-python = ">=3.11"
# ///
"""Run the real QuickJS engine against a command ABI probe, not a shell."""
import argparse
from pathlib import Path
import shutil
import subprocess
import tempfile


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("runner", "quickjs", "probe"):
        parser.add_argument(f"--{name}", type=Path, required=True)
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix="efi-quickjs-") as directory:
        tree = Path(directory)
        shutil.copyfile(args.quickjs, tree / "quickjs.wasm")
        shutil.copyfile(args.probe, tree / "ash.wasm")
        script = r"""
if (typeof std !== 'undefined' || typeof os !== 'undefined' ||
    typeof process !== 'undefined' || typeof require !== 'undefined') {
    throw Error('Unexpected host interface');
}
const results = await Promise.all([
    $`inspect ${"a'b"} ${['', 'x;$(bad)', 'λ']}`,
    $`inspect ${"a'b"} ${['', 'x;$(bad)', 'λ']}`,
]);
for (const result of results) {
    if (result.exitCode !== 0 || result.stdout !== 'probe-output' ||
        result.stderr !== 'E'.repeat(70003) || result.captureError !== 0) {
        throw Error('Incorrect capture');
    }
}
let rejected = false;
try { await $`fail`; } catch (result) {
    rejected = result.exitCode === 17 && result.stderr === 'probe-error';
}
if (!rejected) throw Error('Missing rejection');
let nulRejected = false;
try { await $`inspect ${'x\0y'}`; } catch (error) {
    nulRejected = error instanceof TypeError;
}
if (!nulRejected) throw Error('NUL accepted');
if (await Promise.resolve(6 * 7) !== 42) throw Error('Promise failure');
print('QUICKJS_PROCESS_ABI_OK');
"""
        result = subprocess.run(
            [str(args.runner.resolve()), str(tree), "quickjs", "-e", script],
            capture_output=True, text=True, encoding="utf-8", timeout=120,
        )
        if result.returncode != 0:
            raise RuntimeError(f"QuickJS failed:\n{result.stdout}\n{result.stderr}")
        assert "QUICKJS_PROCESS_ABI_OK" in result.stdout, result.stdout
        print("PASS real QuickJS: quoted templates, concurrent capture, rejection, restricted host API")


if __name__ == "__main__":
    main()
