# /// script
# requires-python = ">=3.11"
# dependencies = ["asyncssh>=2.21"]
# ///
"""Measure repeated real guest lifetimes in one World with independent peers."""
import argparse
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
import shutil
import subprocess
import tempfile
import threading

from test_wolfssh import SshPeer


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--runner", type=Path, required=True)
    parser.add_argument("--guests", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--case", action="append", help="Run only named lifecycle cases")
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    closed = threading.Event()
    payload = b"resource-lifetime\n" * 16384

    class Handler(BaseHTTPRequestHandler):
        def do_GET(self):
            if self.path == "/hold":
                self.connection.settimeout(15)
                if self.connection.recv(1) == b"":
                    closed.set()
                return
            self.send_response(200)
            self.send_header("Content-Length", str(len(payload)))
            self.end_headers()
            self.wfile.write(payload)

        def log_message(self, *_):
            pass

    with tempfile.TemporaryDirectory(prefix="efi-resource-") as directory, SshPeer() as ssh:
        tree = Path(directory)
        for path in args.guests.iterdir():
            if path.is_dir():
                if path.name == "lib":
                    shutil.copytree(path, tree / path.name)
            elif path.suffix == ".wasm":
                shutil.copyfile(path, tree / path.name)
        (tree / "password.txt").write_text("secret\n", encoding="utf-8")
        server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        url = f"http://127.0.0.1:{server.server_port}"
        ssh_args = ["wolfssh", "--host", "127.0.0.1", "--port", str(ssh.port),
                    "--user", "guest", "--host-key-sha256", ssh.pin,
                    "--password-file", "password.txt", "--"]
        executed = set()

        def run(name, argv, *, cycles=6, cancel_ms=0):
            if args.case and name not in args.case:
                return
            executed.add(name)
            result = subprocess.run([str(args.runner.resolve()), str(tree), "--lifecycle",
                                     str(cycles), str(cancel_ms), *argv],
                                    capture_output=True, timeout=240)
            (args.output / f"{name}.tsv").write_bytes(result.stdout)
            (args.output / f"{name}.stderr").write_bytes(result.stderr)
            if result.returncode:
                raise AssertionError(f"{name}: {result.stdout.decode()} {result.stderr.decode()}")
            print(f"PASS {name}: {result.stdout.decode().splitlines()[-1]}")

        try:
            run("js-sessions", ["session-js"])
            run("python", ["python", "-B", "-c", "import subprocess, json; assert subprocess.check_output('printf ok',shell=True)==b'ok'"])
            run("python-cancel", ["python", "-B", "-c", "while True: pass"], cycles=3, cancel_ms=1500)
            run("ash-pipelines", ["ash", "-c", """
                i=0
                while [ "$i" -lt 30 ]; do
                    [ "$(yes x | head -n 90000 | wc -l)" = 90000 ] || exit 10
                    [ "$(printf abc | cat | cat)" = abc ] || exit 11
                    missing_pipeline_command | cat
                    printf xyz | (read -n 3 value; [ "$value" = xyz ]) || exit 12
                    i=$((i+1))
                done
            """])
            failing_pipeline = " | ".join(["sleep 60"] * 40)
            overflow = "big=x; i=0; while [ $i -lt 20 ]; do big=$big$big; i=$((i+1)); done; printf x | cat"
            run("ash-serialization-error", ["ash", "-c", f"""
                message=$(ash -c '{overflow}' 2>&1); status=$?
                [ "$status" -ne 0 ] || exit 13
                case "$message" in *'shell task state exceeds'*) :;; *) exit 14;; esac
            """])
            run("ash-partial-pipeline", ["ash", "-c", f"""
                message=$(ash -c '{failing_pipeline}' 2>&1); status=$?
                [ "$status" -ne 0 ] || exit 15
                case "$message" in *'cannot start pipeline task: -11'*) :;; *) exit 16;; esac
            """], cycles=6)
            run("ash-cancel", ["ash", "-c", "yes x | cat | cat > /dev/null"], cycles=3, cancel_ms=500)
            run("curl", ["ash", "-c", f'[ "$(curl -s {url}/ | wc -c)" = {len(payload)} ]'])
            run("curl-error", ["ash", "-c", "curl -s http://127.0.0.1:1/; [ $? -ne 0 ]"])
            run("curl-cancel", ["curl", "-s", url + "/hold"], cycles=3, cancel_ms=500)
            if not args.case or "curl-cancel" in args.case:
                assert closed.wait(5), "Cancelled curl did not close its connection"
            run("ssh", [*ssh_args, "large"])
            run("ssh-cancel", [*ssh_args, "blocked"], cycles=3, cancel_ms=1500)
            if not args.case or "ssh-cancel" in args.case:
                assert ssh.blocked_closed.wait(5), "Cancelled SSH did not close its connection"
            assert not ssh.errors, ssh.errors
            if args.case:
                assert set(args.case) <= executed, f"Unknown lifecycle cases: {set(args.case) - executed}"
        finally:
            server.shutdown()
            server.server_close()
            thread.join(5)


if __name__ == "__main__":
    main()
