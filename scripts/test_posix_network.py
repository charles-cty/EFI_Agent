# /// script
# requires-python = ">=3.11"
# ///
"""Actual guest POSIX socket and resolver tests against independent TCP peers."""
import argparse
import os
from pathlib import Path
import shutil
import socket
import struct
import subprocess
import tempfile
import threading
import unittest

from test_dns import read_exact, response


class PosixNetwork(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="efi-posix-net-")
        self.addCleanup(temporary.cleanup)
        self.tree = Path(temporary.name)
        shutil.copyfile(self.options.fixture, self.tree / "network.wasm")

    def execute(self, *arguments, environment=None):
        return subprocess.run(
            [str(self.options.runner.resolve()), str(self.tree), "network", *arguments],
            capture_output=True, timeout=45, env=environment,
        )

    def check_output(self, result, expected):
        self.assertEqual(result.returncode, 0, result.stderr.decode("utf-8", "replace"))
        self.assertEqual(result.stdout, expected)
        self.assertEqual(result.stderr, b"")

    def test_numeric_resolver_boundaries_without_network(self):
        self.check_output(self.execute("check"), b"POSIX_CHECK_OK\n")

    def test_connection_refused_and_so_error(self):
        with socket.socket() as refused:
            refused.bind(("127.0.0.1", 0))
            result = self.execute("127.0.0.1", str(refused.getsockname()[1]), "refused")
        self.check_output(result, b"POSIX_REFUSED_OK\n")

    def test_socket_connect_fcntl_poll_send_recv_and_eof(self):
        errors = []
        with socket.socket() as listener:
            listener.bind(("127.0.0.1", 0))
            listener.listen()
            listener.settimeout(30)

            def server():
                try:
                    with listener.accept()[0] as peer:
                        peer.settimeout(30)
                        self.assertEqual(peer.recv(1), b"\xa7")
                        for byte in [19, 73, 211]:
                            peer.sendall(bytes([byte]))
                        peer.shutdown(socket.SHUT_WR)
                        self.assertEqual(peer.recv(1), b"")
                except BaseException as error:
                    errors.append(error)

            thread = threading.Thread(target=server, daemon=True)
            thread.start()
            try:
                result = self.execute("127.0.0.1", str(listener.getsockname()[1]))
            finally:
                thread.join(timeout=35)
            self.assertFalse(thread.is_alive())
        self.assertEqual(errors, [])
        self.check_output(result, b"POSIX_NETWORK_OK\n")

    def test_getaddrinfo_over_real_dns_tcp(self):
        for variant, expected in [("a", 0), ("cname", 0), ("nxdomain", -2),
                                  ("wrong_id", -4), ("pointer_cycle", -4), ("wrong_question", -4)]:
            with self.subTest(variant=variant), socket.socket() as listener:
                listener.bind(("127.0.0.1", 0))
                listener.listen()
                listener.settimeout(30)
                errors = []

                def server():
                    try:
                        with listener.accept()[0] as peer:
                            peer.settimeout(30)
                            size = struct.unpack("!H", read_exact(peer, 2))[0]
                            message = response(read_exact(peer, size), variant)
                            frame = struct.pack("!H", len(message)) + message
                            for byte in frame:
                                peer.sendall(bytes([byte]))
                    except BaseException as error:
                        errors.append(error)

                thread = threading.Thread(target=server, daemon=True)
                thread.start()
                environment = dict(os.environ, EFI_WASM_DNS=f"127.0.0.1:{listener.getsockname()[1]}")
                try:
                    result = self.execute("dns", "MiXeD.Example.", str(expected), environment=environment)
                finally:
                    thread.join(timeout=35)
                self.assertFalse(thread.is_alive())
                self.assertEqual(errors, [])
                self.check_output(result, b"POSIX_DNS_OK\n")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--runner", required=True, type=Path)
    parser.add_argument("--fixture", required=True, type=Path)
    PosixNetwork.options = parser.parse_args()
    result = unittest.TextTestRunner(verbosity=2).run(unittest.defaultTestLoader.loadTestsFromTestCase(PosixNetwork))
    raise SystemExit(0 if result.wasSuccessful() else 1)


if __name__ == "__main__":
    main()
