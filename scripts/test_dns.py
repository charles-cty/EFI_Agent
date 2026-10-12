# /// script
# requires-python = ">=3.11"
# ///
"""Verify actual DNS TCP framing and guest output against an independent server."""
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


def read_exact(connection, count):
    result = bytearray()
    while len(result) < count:
        chunk = connection.recv(count - len(result))
        if not chunk:
            raise AssertionError("Truncated query")
        result.extend(chunk)
    return bytes(result)


def response(query, variant):
    question = b'\x05mixed\x07example\x00\x00\x01\x00\x01'
    assert query[2:] == b'\x01\x00\x00\x01' + b'\0' * 6 + question
    header = query[:2] + struct.pack('!HHHHH', 0x8180, 1, 1, 0, 0)
    record = b'\xc0\x0c' + struct.pack('!HHIH', 1, 1, 60, 4) + bytes([203, 0, 113, 7])
    if variant == 'nxdomain':
        return query[:2] + struct.pack('!HHHHH', 0x8183, 1, 0, 0, 0) + question
    if variant == 'cname':
        alias = b'\x05alias\xc0\x12'
        cname = b'\xc0\x0c' + struct.pack('!HHIH', 5, 1, 60, len(alias)) + alias
        address = b'\x05alias\xc0\x12' + struct.pack('!HHIH', 1, 1, 60, 4) + bytes([203, 0, 113, 7])
        return query[:2] + struct.pack('!HHHHH', 0x8180, 1, 2, 0, 0) + question + cname + address
    message = header + question + record
    if variant == 'wrong_id':
        message = bytes([message[0] ^ 1]) + message[1:]
    elif variant == 'pointer_cycle':
        offset = len(header + question)
        message = header + question + struct.pack('!H', 0xc000 | offset) + record[2:]
    elif variant == 'wrong_question':
        message = message[:13] + b'X' + message[14:]
    return message


class DnsIntegration(unittest.TestCase):
    def test_actual_dns_success_cname_and_rejected_responses(self):
        for variant, errno in [('a', 0), ('cname', 0), ('nxdomain', 44),
                               ('wrong_id', 28), ('pointer_cycle', 28), ('wrong_question', 28)]:
            with self.subTest(variant=variant), tempfile.TemporaryDirectory(prefix='efi-dns-') as directory:
                tree = Path(directory)
                shutil.copyfile(self.options.fixture, tree / 'network.wasm')
                errors = []
                with socket.socket() as listener:
                    listener.bind(('127.0.0.1', 0))
                    listener.listen()
                    listener.settimeout(30)
                    def server():
                        try:
                            with listener.accept()[0] as connection:
                                connection.settimeout(30)
                                size = struct.unpack('!H', read_exact(connection, 2))[0]
                                query = read_exact(connection, size)
                                message = response(query, variant)
                                frame = struct.pack('!H', len(message)) + message
                                for byte in frame:
                                    connection.sendall(bytes([byte]))
                        except BaseException as error:
                            errors.append(error)
                    thread = threading.Thread(target=server, daemon=True)
                    thread.start()
                    environment = dict(os.environ, EFI_WASM_DNS=f'127.0.0.1:{listener.getsockname()[1]}')
                    try:
                        result = subprocess.run([str(self.options.runner.resolve()), str(tree), 'network',
                                                 'dns', 'MiXeD.Example.', str(errno)],
                                                capture_output=True, timeout=45, env=environment)
                    finally:
                        thread.join(timeout=35)
                    self.assertFalse(thread.is_alive())
                self.assertEqual(errors, [])
                self.assertEqual(result.returncode, 0, result.stderr.decode('utf-8', 'replace'))
                self.assertEqual(result.stdout, b'NETWORK_DNS_OK\n')
                self.assertEqual(result.stderr, b'')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--runner', required=True, type=Path)
    parser.add_argument('--fixture', required=True, type=Path)
    DnsIntegration.options = parser.parse_args()
    result = unittest.TextTestRunner(verbosity=2).run(unittest.defaultTestLoader.loadTestsFromTestCase(DnsIntegration))
    raise SystemExit(0 if result.wasSuccessful() else 1)


if __name__ == '__main__':
    main()
