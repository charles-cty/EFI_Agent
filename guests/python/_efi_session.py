"""Private host code-session driver; ordinary command stdin stays finite."""
import os
import struct
import sys
import traceback


def run():
    channel = os.open('/.efi-session', os.O_RDWR)
    read, write = os.read, os.write
    pack, unpack = struct.pack, struct.unpack
    namespace = sys.modules['__main__'].__dict__
    namespace.pop('_efi_session', None)

    def receive(size):
        result = bytearray()
        while len(result) < size:
            chunk = read(channel, size - len(result))
            if not chunk:
                raise RuntimeError('Code session channel closed')
            result.extend(chunk)
        return result

    while True:
        sequence, size = unpack('<II', receive(8))
        if size > 60 * 1024:
            raise RuntimeError('Code session frame exceeds 60 KiB')
        source = receive(size).decode('utf-8')
        status = 0
        try:
            exec(compile(source, '<python-tool>', 'exec'), namespace, namespace)
        except Exception:
            traceback.print_exc()
            status = 1
        for stream in (sys.stdout, sys.stderr):
            if stream is not None:
                stream.flush()
        if write(channel, pack('<Ii', sequence, status)) != 8:
            raise RuntimeError('Incomplete code session reply')
