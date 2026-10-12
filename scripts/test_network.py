# /// script
# requires-python = ">=3.11"
# ///
"""Actual native TCP, WASI stream and cooperative child integration tests."""
import argparse
from pathlib import Path
import shutil
import socket
import subprocess
import tempfile
import threading
import time
import unittest


REQUEST = bytes((index * 37 + index // 251 + 19) % 256 for index in range(196613))
RESPONSE = bytes((index * 53 + index // 239 + 7) % 256 for index in range(131089))


class NetworkIntegration(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="efi-network-")
        self.addCleanup(self.temporary.cleanup)
        self.tree = Path(self.temporary.name)
        shutil.copyfile(self.options.fixture, self.tree / "network.wasm")

    def execute(self, port, *arguments):
        return subprocess.run(
            [str(self.options.runner.resolve()), str(self.tree), "network", "127.0.0.1", str(port), *arguments],
            capture_output=True, timeout=60,
        )

    def test_real_tcp_fragmentation_eof_and_child_progress(self):
        errors = []
        with socket.socket() as listener:
            listener.bind(("127.0.0.1", 0))
            listener.listen()
            listener.settimeout(30)

            def server():
                try:
                    with listener.accept()[0] as connection:
                        connection.settimeout(30)
                        received = bytearray()
                        while len(received) < len(REQUEST):
                            chunk = connection.recv(min(613, len(REQUEST) - len(received)))
                            self.assertTrue(chunk, "EOF before the request completed")
                            received.extend(chunk)
                        self.assertEqual(received, REQUEST)
                        # A guest child must run while its parent waits for TCP.
                        # This handshake has no fixed scheduling-delay assumption.
                        deadline = time.monotonic() + 30
                        marker = self.tree / "network-peer.txt"
                        while time.monotonic() < deadline:
                            if marker.exists() and marker.read_bytes() == b"PEER_PROGRESS":
                                break
                            time.sleep(0.005)
                        else:
                            raise AssertionError("Guest child did not progress during network wait")
                        for offset in range(0, len(RESPONSE), 317):
                            connection.sendall(RESPONSE[offset:offset + 317])
                        connection.shutdown(socket.SHUT_WR)
                        self.assertEqual(connection.recv(1), b"", "Last guest descriptor did not close")
                except BaseException as error:
                    errors.append(error)

            thread = threading.Thread(target=server, daemon=True)
            thread.start()
            try:
                result = self.execute(listener.getsockname()[1])
            finally:
                thread.join(timeout=35)
            self.assertFalse(thread.is_alive(), "Server did not finish")
        self.assertEqual(errors, [])
        self.assertEqual(result.returncode, 0, result.stderr.decode("utf-8", "replace"))
        self.assertEqual(result.stdout, b"NETWORK_PEER_PROGRESS\nNETWORK_EXCHANGE_OK\n")
        self.assertEqual(result.stderr, b"")
        self.assertEqual((self.tree / "network-response.bin").read_bytes(), RESPONSE)
        self.assertEqual((self.tree / "network-peer.txt").read_bytes(), b"PEER_PROGRESS")

    def test_connection_refused_preserves_result(self):
        # Keep the port bound without listening, avoiding a port-reuse race.
        with socket.socket() as refused:
            refused.bind(("127.0.0.1", 0))
            result = self.execute(refused.getsockname()[1], "refused")
        self.assertEqual(result.returncode, 0, result.stderr.decode("utf-8", "replace"))
        self.assertEqual(result.stdout, b"NETWORK_REFUSED_OK\n")
        self.assertEqual(result.stderr, b"")

    def test_nonblocking_read_shared_flags_and_eof(self):
        self.check_nonblocking("nonblocking", b"NETWORK_NONBLOCKING_OK\n")

    def test_async_connect_readiness_and_error(self):
        self.check_nonblocking("async", b"NETWORK_ASYNC_OK\n")

    def test_async_refusal_preserves_socket_and_clears_error(self):
        with socket.socket() as refused:
            refused.bind(("127.0.0.1", 0))
            result = self.execute(refused.getsockname()[1], "async-refused")
        self.assertEqual(result.returncode, 0, result.stderr.decode("utf-8", "replace"))
        self.assertEqual(result.stdout, b"NETWORK_ASYNC_REFUSED_OK\n")
        self.assertEqual(result.stderr, b"")

    def check_nonblocking(self, mode, expected_output):
        errors = []
        with socket.socket() as listener:
            listener.bind(("127.0.0.1", 0))
            listener.listen()
            listener.settimeout(30)
            def server():
                try:
                    with listener.accept()[0] as connection:
                        connection.settimeout(30)
                        self.assertEqual(connection.recv(1), b"\xa7")
                        connection.sendall(bytes([19, 73, 211]))
                        connection.shutdown(socket.SHUT_WR)
                        self.assertEqual(connection.recv(1), b"")
                except BaseException as error:
                    errors.append(error)
            thread = threading.Thread(target=server, daemon=True)
            thread.start()
            try:
                result = self.execute(listener.getsockname()[1], mode)
            finally:
                thread.join(timeout=35)
            self.assertFalse(thread.is_alive())
        self.assertEqual(errors, [])
        self.assertEqual(result.returncode, 0, result.stderr.decode("utf-8", "replace"))
        self.assertEqual(result.stdout, expected_output)
        self.assertEqual(result.stderr, b"")

    def test_graceful_close_preserves_all_accepted_bytes_and_closes_duplicates(self):
        errors = []
        with socket.socket() as listener:
            listener.bind(("127.0.0.1", 0))
            listener.listen()
            listener.settimeout(30)
            def server():
                try:
                    with listener.accept()[0] as connection:
                        connection.settimeout(30)
                        received = bytearray()
                        while True:
                            chunk = connection.recv(419)
                            if not chunk:
                                break
                            received.extend(chunk)
                        self.assertEqual(received, REQUEST)
                except BaseException as error:
                    errors.append(error)
            thread = threading.Thread(target=server, daemon=True)
            thread.start()
            try:
                result = self.execute(listener.getsockname()[1], "graceful")
            finally:
                thread.join(timeout=35)
            self.assertFalse(thread.is_alive())
        self.assertEqual(errors, [])
        self.assertEqual(result.returncode, 0, result.stderr.decode("utf-8", "replace"))
        self.assertEqual(result.stdout, b"NETWORK_GRACEFUL_CLOSE_OK\n")
        self.assertEqual(result.stderr, b"")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--runner", required=True, type=Path)
    parser.add_argument("--fixture", required=True, type=Path)
    NetworkIntegration.options = parser.parse_args()
    suite = unittest.defaultTestLoader.loadTestsFromTestCase(NetworkIntegration)
    result = unittest.TextTestRunner(verbosity=2).run(suite)
    raise SystemExit(0 if result.wasSuccessful() else 1)


if __name__ == "__main__":
    main()
