# /// script
# requires-python = ">=3.11"
# dependencies = ["asyncssh>=2.21"]
# ///
"""Verify the actual WolfSSH WASM client against an independent SSH server."""
import argparse
import asyncio
import base64
import hashlib
from pathlib import Path
import shutil
import subprocess
import tempfile
import threading
import unittest

import asyncssh

STDOUT = bytes((index * 53 + index // 239 + 7) % 256 for index in range(262181))
STDERR = bytes((index * 37 + index // 251 + 19) % 256 for index in range(196613))


class SshPeer:
    """Independent protocol, authentication, output, and exit-status oracle."""

    def __init__(self, host="127.0.0.1"):
        self.host = host
        self.key = asyncssh.generate_private_key("ecdsa-sha2-nistp256")
        blob = base64.b64decode(self.key.export_public_key().split()[1])
        self.pin = hashlib.sha256(blob).hexdigest()
        self.password = "secret"
        self.auth_attempts = []
        self.commands = []
        self.errors = []
        self.blocked_started = threading.Event()
        self.blocked_closed = threading.Event()
        self.loop = asyncio.new_event_loop()
        self.thread = threading.Thread(target=self.loop.run_forever, daemon=True)

    async def start(self):
        peer = self

        class Server(asyncssh.SSHServer):
            def begin_auth(self, username):
                return True

            def password_auth_supported(self):
                return True

            def validate_password(self, username, password):
                peer.auth_attempts.append((username, password))
                return username == "guest" and password == peer.password

        self.listener = await asyncssh.create_server(
            Server, self.host, 0, server_host_keys=[self.key],
            process_factory=self.process, encoding=None,
            kex_algs=["ecdh-sha2-nistp256"],
            encryption_algs=["aes128-ctr"], mac_algs=["hmac-sha2-256"],
        )
        self.port = self.listener.get_port()

    async def process(self, process):
        command = process.command
        self.commands.append(command)
        try:
            if command == "large":
                # Each output exceeds the guest channel window. Both streams
                # must replenish credit, including when only stderr is pending.
                for offset in range(0, max(len(STDOUT), len(STDERR)), 997):
                    process.stdout.write(STDOUT[offset:offset + 997])
                    process.stderr.write(STDERR[offset:offset + 997])
                    await process.stdout.drain()
                process.exit(0)
            elif command == "stderr-only":
                process.stderr.write(STDERR)
                await process.stderr.drain()
                process.exit(0)
            elif command == "late-status":
                process.stdout.write(b"before-eof\n")
                process.stdout.write_eof()
                await asyncio.sleep(0.05)
                process.exit(37)
            elif command == "nonzero":
                process.stdout.write(b"remote-output\n")
                process.stderr.write(b"remote-error\n")
                process.exit(23)
            elif command == "no-status":
                process.stdout.write(b"without-status\n")
                process.close()
            elif command == "blocked":
                self.blocked_started.set()
                await process.wait_closed()
                self.blocked_closed.set()
            else:
                raise AssertionError(f"Unexpected SSH command: {command!r}")
        except asyncio.CancelledError:
            raise
        except BaseException as error:
            self.errors.append(error)
            process.close()

    def __enter__(self):
        self.thread.start()
        asyncio.run_coroutine_threadsafe(self.start(), self.loop).result(10)
        return self

    def __exit__(self, *_):
        async def stop():
            self.listener.close()
            await self.listener.wait_closed()
            pending = [task for task in asyncio.all_tasks() if task is not asyncio.current_task()]
            for task in pending:
                task.cancel()
            await asyncio.gather(*pending, return_exceptions=True)

        asyncio.run_coroutine_threadsafe(stop(), self.loop).result(10)
        self.loop.call_soon_threadsafe(self.loop.stop)
        self.thread.join(10)
        self.loop.close()


class WolfSsh(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="efi-wolfssh-")
        self.addCleanup(temporary.cleanup)
        self.tree = Path(temporary.name)
        shutil.copyfile(self.options.guest, self.tree / "wolfssh.wasm")
        (self.tree / "password.txt").write_bytes(b"secret\r\n")
        self.peer = self.enterContext(SshPeer())

    def execute(self, command, *, pin=None, expected_exit=0):
        result = subprocess.run([
            str(self.options.runner.resolve()), str(self.tree), "wolfssh",
            "--host", "127.0.0.1", "--port", str(self.peer.port),
            "--user", "guest", "--host-key-sha256", pin or self.peer.pin,
            "--password-file", "password.txt", "--", command,
        ], capture_output=True, timeout=45)
        if expected_exit == 0:
            self.assertEqual(result.returncode, 0, result.stderr[-512:].decode("utf-8", "replace"))
            diagnostics = b""
        else:
            self.assertNotEqual(result.returncode, 0)
            # The example runner reports the actual WASI status on stderr.
            diagnostics = f'Error: "Guest exited with {expected_exit}"\n'.encode()
            self.assertTrue(result.stderr.endswith(diagnostics), result.stderr[-512:])
        self.assertEqual(self.peer.errors, [])
        return result.stdout, result.stderr.removesuffix(diagnostics)

    def test_large_interleaved_binary_streams(self):
        self.assertEqual(self.execute("large"), (STDOUT, STDERR))
        self.assertEqual(self.peer.commands, ["large"])

    def test_stderr_only_replenishes_channel_window(self):
        self.assertEqual(self.execute("stderr-only"), (b"", STDERR))

    def test_remote_exit_and_stderr(self):
        self.assertEqual(self.execute("nonzero", expected_exit=23),
                         (b"remote-output\n", b"remote-error\n"))

    def test_exit_status_after_eof(self):
        self.assertEqual(self.execute("late-status", expected_exit=37),
                         (b"before-eof\n", b""))

    def test_missing_exit_status_is_failure(self):
        output, error = self.execute("no-status", expected_exit=255)
        self.assertEqual(output, b"without-status\n")
        self.assertIn(b"missing or out-of-range remote exit status", error)

    def test_wrong_host_key_rejected_before_authentication(self):
        output, error = self.execute("large", pin="00" * 32, expected_exit=255)
        self.assertEqual(output, b"")
        self.assertIn(b"host key does not match SHA256 pin", error)
        self.assertEqual(self.peer.auth_attempts, [])
        self.assertEqual(self.peer.commands, [])

    def test_wrong_password_prevents_command(self):
        (self.tree / "password.txt").write_bytes(b"incorrect")
        output, error = self.execute("large", expected_exit=255)
        self.assertEqual(output, b"")
        self.assertIn(b"handshake failed", error)
        self.assertEqual(self.peer.auth_attempts, [("guest", "incorrect")])
        self.assertEqual(self.peer.commands, [])

    def test_password_file_exact_limit_and_overflow(self):
        self.peer.password = "p" * 4096
        (self.tree / "password.txt").write_bytes(self.peer.password.encode())
        self.assertEqual(self.execute("nonzero", expected_exit=23),
                         (b"remote-output\n", b"remote-error\n"))
        attempts = list(self.peer.auth_attempts)
        commands = list(self.peer.commands)
        for password in [b"p" * 4097, b"secret\x00"]:
            with self.subTest(size=len(password)):
                (self.tree / "password.txt").write_bytes(password)
                output, error = self.execute("large", expected_exit=255)
                self.assertEqual(output, b"")
                self.assertIn(b"cannot read bounded password file", error)
                self.assertEqual(self.peer.auth_attempts, attempts)
                self.assertEqual(self.peer.commands, commands)

    def test_malformed_host_pins_fail_before_network(self):
        for pin in ["a" * 63, "a" * 65, "z" * 64]:
            with self.subTest(pin=pin):
                output, error = self.execute("large", pin=pin, expected_exit=2)
                self.assertEqual(output, b"")
                self.assertIn(b"Usage:", error)
                self.assertEqual(self.peer.auth_attempts, [])
                self.assertEqual(self.peer.commands, [])


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--runner", required=True, type=Path)
    parser.add_argument("--guest", required=True, type=Path)
    WolfSsh.options = parser.parse_args()
    result = unittest.TextTestRunner(verbosity=2).run(unittest.defaultTestLoader.loadTestsFromTestCase(WolfSsh))
    raise SystemExit(0 if result.wasSuccessful() else 1)


if __name__ == "__main__":
    main()
