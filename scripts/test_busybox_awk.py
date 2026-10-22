# /// script
# requires-python = ">=3.11"
# ///
"""Exercise upstream BusyBox awk through real WASM ash processes."""
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
    # Expected values are independent arithmetic/text oracles, not guest output.
    cases = [
        ("loop", "awk 'BEGIN { for (i=1;i<=100;i++) s+=i*i; print s }'", b"338350\n", 0),
        ("fields", "awk -F: -v offset=3 '{print $2, $1+offset, NF}' fields", b"red 10 3\nblue 14 3\n", 0),
        ("arrays", "awk 'BEGIN { a[\"x\"]=7; a[\"y\"]=11; for (k in a) s+=a[k]; delete a[\"x\"]; print s, (\"x\" in a), a[\"y\"] }'", b"18 0 11\n", 0),
        ("regex", "awk '/^[0-9]+:red:/ {gsub(/:/, \"-\"); print}' fields", b"7-red-A\n", 0),
        ("stdin", "printf '3 5\\n8 13\\n' | awk '{a+=$1; b+=$2} END {print NR,a,b}'", b"2 11 18\n", 0),
        ("program-file", "awk -f program.awk fields", b"1=red\n2=blue\n", 0),
        ("math", "awk 'BEGIN {printf \"%.2f %.0f %.0f\\n\", sqrt(2), cos(0), 2^10}'", b"1.41 1 1024\n", 0),
        ("system", "awk 'BEGIN {print system(\"exit 7\"); print system(\"printf shell\")}'", b"7\nshell0\n", 0),
        ("read-pipe", "awk 'BEGIN {c=\"printf line\"; c | getline x; print x; print close(c)}'", b"line\n0\n", 0),
        ("write-pipe", "awk 'BEGIN {c=\"cat > result\"; print \"first\" | c; print \"second\" | c; print close(c)}'; cat result", b"0\nfirst\nsecond\n", 0),
        ("pipe-status", "awk 'BEGIN {c=\"exit 9\"; c | getline x; print close(c)}'", b"2304\n", 0),
        ("large-read-pipe", "awk 'BEGIN {c=\"cat payload\"; while ((c | getline x)>0) {n++; s+=length(x)}; print n,s,close(c)}'", b"3000 192000 0\n", 0),
        ("large-write-pipe", "awk 'BEGIN {c=\"cat > large-result\"; for(i=0;i<3000;i++) print \"xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx\" | c; print close(c)}'", b"0\n", 0),
        ("independent-pipes", "awk 'BEGIN {a=\"printf A\"; b=\"printf B\"; a | getline x; b | getline y; print x y,close(a),close(b)}'", b"AB 0 0\n", 0),
        ("independent-writers", "awk 'BEGIN {a=\"cat > writer-a\"; b=\"cat > writer-b\"; print \"A\" | a; print \"B\" | b; print close(a),close(b)}'; cat writer-a writer-b", b"0 0\nA\nB\n", 0),
        ("closed-stdin", "awk 'BEGIN {c=\"printf data\"; c | getline x; print x,close(c)}' <&-", b"data 0\n", 0),
        ("exit-status", "awk 'BEGIN {exit 17}'", b"", 17),
        ("bad-regex", "awk 'BEGIN {print match(\"abc\", \"[\")}'", b"", 2),
        ("bad-syntax", "awk 'BEGIN {'", b"", 2),
        ("missing-input", "awk '{print}' missing-input", b"", 2),
    ]
    with tempfile.TemporaryDirectory(prefix="efi-busybox-awk-") as temporary:
        root = Path(temporary)
        shutil.copyfile(args.ash, root / "ash.wasm")
        (root / "fields").write_bytes(b"7:red:A\n11:blue:B\n")
        (root / "program.awk").write_bytes(b'BEGIN { FS=":" } {print NR "=" $2}\n')
        payload = (b"x" * 64 + b"\n") * 3000
        (root / "payload").write_bytes(payload)
        for name, command, expected, status in cases:
            result = subprocess.run(
                [str(args.runner.resolve()), str(root), "ash", "-c", command],
                capture_output=True, timeout=120,
            )
            assert result.stdout == expected, (name, result.stdout[-1000:], expected, result.stderr)
            assert result.returncode == (0 if status == 0 else 1), (name, result.stderr)
            if status == 0:
                assert not result.stderr, (name, result.stderr)
            else:
                assert f'Guest exited with {status}'.encode() in result.stderr, (name, result.stderr)
                diagnostic = {
                    "bad-regex": b"bad regex",
                    "bad-syntax": b"Unexpected token",
                    "missing-input": b"missing-input",
                }.get(name)
                if diagnostic:
                    assert diagnostic in result.stderr, (name, result.stderr)
            assert b"Guest trapped" not in result.stderr, (name, result.stderr)
            print(f"PASS upstream awk {name}", flush=True)
        # The native runner stores logical files in its namespace. Read back
        # through an independently selected command, not raw backing names.
        result = subprocess.run(
            [str(args.runner.resolve()), str(root), "ash", "-c", "cat large-result"],
            capture_output=True, timeout=120,
        )
        assert result.returncode == 0 and not result.stderr, result.stderr
        assert result.stdout == payload
        print("PASS upstream awk independent pipe byte oracle", flush=True)


if __name__ == "__main__":
    main()
