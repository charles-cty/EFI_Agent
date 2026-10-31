# /// script
# requires-python = ">=3.11"
# dependencies = ["cryptography>=44"]
# ///
"""Run the actual curl command guest against an independent local HTTP server."""
import argparse
import hashlib
import os
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
import shutil
import socket
import ssl
import struct
import subprocess
import tempfile
import threading
import unittest
from test_dns import read_exact, response
from wasm_tls_fixture import CURL_TLS_EXIT_CODES, tls_context
from wasm_namespace_fixture import DirectoryWorkspace

BODY = bytes((index * 53 + index // 239 + 7) % 256 for index in range(131089))
UPLOAD = bytes((index * 37 + index // 251 + 19) % 256 for index in range(196613))


def create_http_server(address="127.0.0.1"):
    """Use the same independent HTTP byte oracle for native and UEFI runs."""
    errors = []
    blocked_started = threading.Event()
    blocked_closed = threading.Event()
    requests = []
    tls_versions = []

    class Handler(BaseHTTPRequestHandler):
        protocol_version = "HTTP/1.1"

        def setup(self):
            super().setup()
            self.connection.settimeout(30)

        def log_message(self, *_):
            pass

        def handle(self):
            try:
                super().handle()
            except ConnectionResetError:
                # A client may reject the certificate after TLS negotiation,
                # before HTTP, or close between keep-alive requests. Payload
                # failures inside do_GET/do_POST still enter the error list.
                self.close_connection = True

        def do_GET(self):
            try:
                requests.append(self.path)
                if isinstance(self.connection, ssl.SSLSocket):
                    tls_versions.append(self.connection.version())
                if self.path == "/blocked":
                    # Allow actual firmware application loading and execution
                    # before the test explicitly cancels this held request.
                    self.connection.settimeout(180)
                    self.send_response(200)
                    self.send_header("Content-Length", "1")
                    self.end_headers()
                    self.wfile.flush()
                    blocked_started.set()
                    try:
                        unexpected = self.connection.recv(1)
                    except ConnectionResetError:
                        unexpected = b""
                    if unexpected:
                        raise AssertionError("Unexpected client bytes during held response")
                    blocked_closed.set()
                    self.close_connection = True
                    return
                if self.path == "/redirect":
                    self.send_response(302)
                    self.send_header("Location", "/binary")
                    self.send_header("Content-Length", "0")
                    self.end_headers()
                    return
                if self.path not in ["/binary", "/chunked"]:
                    raise AssertionError(f"Unexpected GET path: {self.path}")
                self.send_response(200)
                if self.path == "/binary":
                    self.send_header("Content-Length", str(len(BODY)))
                    self.send_header("Connection", "close")
                    self.end_headers()
                    for offset in range(0, len(BODY), 317):
                        self.wfile.write(BODY[offset:offset + 317])
                    self.close_connection = True
                else:
                    self.send_header("Transfer-Encoding", "chunked")
                    self.end_headers()
                    for offset in range(0, len(BODY), 419):
                        chunk = BODY[offset:offset + 419]
                        self.wfile.write(f"{len(chunk):x}\r\n".encode() + chunk + b"\r\n")
                    self.wfile.write(b"0\r\n\r\n")
            except BaseException as error:
                errors.append(error)
                self.close_connection = True

        def do_POST(self):
            try:
                requests.append(self.path)
                if self.path != "/upload":
                    raise AssertionError(f"Unexpected POST path: {self.path}")
                if int(self.headers["Content-Length"]) != len(UPLOAD):
                    raise AssertionError("Wrong upload length")
                received = self.rfile.read(len(UPLOAD))
                if received != UPLOAD:
                    raise AssertionError("Wrong upload bytes")
                digest = hashlib.sha256(received).hexdigest().encode()
                self.send_response(200)
                self.send_header("Content-Length", str(len(digest)))
                self.end_headers()
                self.wfile.write(digest)
            except BaseException as error:
                errors.append(error)
                self.close_connection = True

    server = ThreadingHTTPServer((address, 0), Handler)
    server.blocked_started = blocked_started
    server.blocked_closed = blocked_closed
    server.requests = requests
    server.tls_versions = tls_versions
    return server, errors


class CurlIntegration(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        # Fail before starting DNS peers if the actual module cannot link.
        with tempfile.TemporaryDirectory(prefix="efi-curl-link-") as directory:
            tree = Path(directory)
            shutil.copyfile(cls.options.guest, tree / "curl.wasm")
            result = subprocess.run([str(cls.options.runner.resolve()), str(tree), "curl", "--version"],
                                    capture_output=True, timeout=30)
            if result.returncode or b"wolfSSL/5.9.4" not in result.stdout:
                raise AssertionError(f"Actual TLS curl cannot start: {result.stdout!r} {result.stderr!r}")

    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="efi-curl-")
        self.addCleanup(temporary.cleanup)
        self.tree = Path(temporary.name)
        shutil.copyfile(self.options.guest, self.tree / "curl.wasm")
        self.server, self.errors = create_http_server()
        self.server.daemon_threads = True
        self.thread = threading.Thread(target=self.server.serve_forever, daemon=True)
        self.thread.start()
        self.addCleanup(self.stop_server)
        self.base = f"http://127.0.0.1:{self.server.server_port}"

    def stop_server(self):
        self.server.shutdown()
        self.server.server_close()
        self.thread.join(timeout=5)
        self.assertFalse(self.thread.is_alive())

    def execute(self, *arguments, environment=None, expected_exit=0):
        result = subprocess.run(
            [str(self.options.runner.resolve()), str(self.tree), "curl",
             "--silent", "--show-error", "--noproxy", "*", *arguments],
            capture_output=True, timeout=60, env=environment,
        )
        self.assertEqual(result.returncode, 0 if expected_exit == 0 else 1,
                         result.stderr.decode("utf-8", "replace"))
        if expected_exit:
            self.assertIn(f"Guest exited with {expected_exit}".encode(), result.stderr)
        else:
            self.assertEqual(result.stderr, b"")
        self.assertEqual(self.errors, [])
        return result.stdout

    def test_redirect_binary_file_and_exact_saved_bytes(self):
        self.assertEqual(self.execute("--location", "--output", "download.bin", self.base + "/redirect"), b"")
        self.assertEqual(DirectoryWorkspace(self.tree).read('download.bin'), BODY)

    def test_chunked_response_to_stdout(self):
        # WASI standard streams are character devices. Explicitly permit curl's
        # binary terminal output while still checking every byte on host stdout.
        self.assertEqual(self.execute("--output", "-", self.base + "/chunked"), BODY)

    def test_binary_upload_from_guest_file(self):
        (self.tree / "upload.bin").write_bytes(UPLOAD)
        self.assertEqual(self.execute("--data-binary", "@upload.bin", self.base + "/upload"),
                         hashlib.sha256(UPLOAD).hexdigest().encode())

    def test_actual_curl_deadline_closes_held_response(self):
        self.assertEqual(self.execute("--max-time", "1", self.base + "/blocked",
                                      expected_exit=28), b"")
        self.assertTrue(self.server.blocked_started.is_set(), "Curl did not reach HTTP server")
        self.assertTrue(self.server.blocked_closed.wait(5), "Curl timeout retained its connection")
        self.assertEqual(self.errors, [])

    def test_actual_curl_tls_and_certificate_rejection(self):
        contexts = {}
        for case in CURL_TLS_EXIT_CODES:
            directory = self.tree / case
            directory.mkdir()
            failure = None if case in ["tls12", "tls13"] else case
            contexts[case] = tls_context(directory, failure)
        for case, context in contexts.items():
            with self.subTest(case=case):
                failure = None if case in ["tls12", "tls13"] else case
                directory = self.tree / case
                if case in ["tls12", "tls13"]:
                    version = ssl.TLSVersion.TLSv1_2 if case == "tls12" else ssl.TLSVersion.TLSv1_3
                    context.minimum_version = version
                    context.maximum_version = version
                server, errors = create_http_server()
                server.socket = context.wrap_socket(server.socket, server_side=True)
                server.daemon_threads = True
                thread = threading.Thread(target=server.serve_forever, daemon=True)
                thread.start()
                try:
                    data = self.execute("--cacert", f"{directory.name}/curl-ca.pem", "--output", "-",
                                        f"https://127.0.0.1:{server.server_port}/binary",
                                        expected_exit=CURL_TLS_EXIT_CODES[case])
                    self.assertEqual(data, b"" if failure else BODY)
                    self.assertEqual(server.requests, [] if failure else ["/binary"])
                    if not failure:
                        self.assertEqual(server.tls_versions, ["TLSv1.2" if case == "tls12" else "TLSv1.3"])
                    self.assertEqual(errors, [])
                finally:
                    server.shutdown()
                    server.server_close()
                    thread.join(timeout=5)
                    self.assertFalse(thread.is_alive())

    def test_actual_curl_dns_success_and_rejected_responses(self):
        for variant in ["a", "cname", "nxdomain", "wrong_id", "pointer_cycle", "wrong_question"]:
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
                            reply = response(read_exact(peer, size), variant)
                            if variant in ["a", "cname"]:
                                reply = reply[:-4] + socket.inet_aton("127.0.0.1")
                            frame = struct.pack("!H", len(reply)) + reply
                            for byte in frame:
                                peer.sendall(bytes([byte]))
                    except BaseException as error:
                        errors.append(error)

                thread = threading.Thread(target=server, daemon=True)
                thread.start()
                environment = dict(os.environ, EFI_WASM_DNS=f"127.0.0.1:{listener.getsockname()[1]}")
                try:
                    data = self.execute("--output", "-",
                                        f"http://MiXeD.Example.:{self.server.server_port}/binary",
                                        environment=environment,
                                        expected_exit=0 if variant in ["a", "cname"] else 6)
                finally:
                    thread.join(timeout=35)
                self.assertFalse(thread.is_alive())
                self.assertEqual(errors, [])
                self.assertEqual(data, BODY if variant in ["a", "cname"] else b"")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--runner", required=True, type=Path)
    parser.add_argument("--guest", required=True, type=Path)
    CurlIntegration.options = parser.parse_args()
    result = unittest.TextTestRunner(verbosity=2).run(unittest.defaultTestLoader.loadTestsFromTestCase(CurlIntegration))
    raise SystemExit(0 if result.wasSuccessful() else 1)


if __name__ == "__main__":
    main()
