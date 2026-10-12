# /// script
# requires-python = ">=3.11"
# ///
"""Run actual WASI libc and ash against the host file provider."""
import argparse
import json
from pathlib import Path
import shutil
import subprocess
import tempfile
from wasm_directory_fixture import expected_glob, stage_directory, verify_directory_artifacts
from wasm_file_fixture import resize_contents


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("runner", "fixture", "ash", "quickjs"):
        parser.add_argument(f"--{name}", type=Path, required=True)
    parser.add_argument("--directory-fixture", type=Path, required=True)
    parser.add_argument("--random-fixture", type=Path)
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix="efi-wasm-files-") as directory:
        tree = Path(directory)
        (tree / "subdir").mkdir()
        (tree / "subdir/λ.txt").write_bytes(b"xyz")
        entries = stage_directory(tree)
        for name, source in (("filesystem", args.fixture), ("directory", args.directory_fixture), ("ash", args.ash), ("quickjs", args.quickjs)):
            shutil.copyfile(source, tree / f"{name}.wasm")
        if args.random_fixture:
            shutil.copyfile(args.random_fixture, tree / "random.wasm")

        def run(program, *arguments):
            result = subprocess.run([str(args.runner.resolve()), str(tree), program, *arguments],
                                    capture_output=True, timeout=120)
            assert result.returncode == 0 and not result.stderr, result
            return result.stdout

        if args.random_fixture:
            assert run("random") == b"WASM_RANDOM_OK\n"
            records = (tree / "wasm-random.bin").read_bytes()
            assert len(records) == 768
            assert len({records[index:index + 256] for index in range(0, 768, 256)}) == 3
            print("PASS actual libc getentropy, guarded output and persisted distinct records")
        assert run("filesystem") == b"WASM_FILESYSTEM_OK\n"
        assert (tree / "wasm-file.bin").read_bytes() == b"a\0b\xffz!"
        expected = bytearray(257)
        for index in range(97):
            expected[(index * 73 + 11) % 257] = (index * 19 + 7) % 256
        assert (tree / "wasm-cursor.bin").read_bytes() == expected
        assert (tree / "wasm-resize.bin").read_bytes() == b"\0\0\0\0!"
        assert (tree / "wasm-resize-oracle.bin").read_bytes() == resize_contents()
        print("PASS actual WASI libc file cursors, dup, append, metadata and errors")
        assert run("directory") == b"WASM_DIRECTORY_OK\n"
        verify_directory_artifacts(lambda name: (tree / name).read_bytes(), entries)
        print("PASS raw directory cookies, partial records, rewind, rights and libc enumeration")
        assert run("ash", "-c", "printf '<%s>\\n' directory-fixture/*.txt") == expected_glob(entries)
        print("PASS actual ash Unicode, spaces, hidden-name exclusion and directory glob matches")
        script = ("const result=await $`printf '<%s>\\n' directory-fixture/*.txt`;"
                  f"if(result.stdout!=={json.dumps(expected_glob(entries).decode('utf-8'))}"
                  "||result.stderr!=='')throw Error('glob');print('QUICKJS_DIRECTORY_OK')")
        assert run("quickjs", "-e", script) == b"QUICKJS_DIRECTORY_OK\n"
        print("PASS QuickJS actual ash directory glob capture")
        script = "printf '%s\\n' first > shell.txt; printf '%s\\n' second >> shell.txt; while read -r line; do printf '<%s>' \"$line\"; done < shell.txt"
        assert run("ash", "-c", script) == b"<first><second>"
        assert (tree / "shell.txt").read_bytes() == b"first\nsecond\n"
        print("PASS actual ash write/append/read redirections")
        script = "value=$(printf '%s\\n' nested > task.txt; read -r value < task.txt; printf '%s' \"$value\"); printf '<%s>' \"$value\""
        assert run("ash", "-c", script) == b"<nested>"
        assert (tree / "task.txt").read_bytes() == b"nested\n"
        print("PASS fresh ash task sees the same filesystem")
        script = "const result=await $`read -r line < shell.txt; printf '%s' \"$line\"`; if(result.stdout!=='first')throw Error('file'); print('QUICKJS_FILE_OK')"
        assert run("quickjs", "-e", script) == b"QUICKJS_FILE_OK\n"
        print("PASS QuickJS actual ash file capture")


if __name__ == "__main__":
    main()
