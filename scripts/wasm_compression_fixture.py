"""Independent gzip vectors and guest checks for upstream compression applets."""
import base64
import gzip
from textwrap import dedent
import zlib


ORACLE_DATA = bytes(range(256)) * 3 + b'\x00binary\xff\n' * 17


def vectors():
    result = {}
    for level in (0, 1, 9):
        result['level-' + str(level)] = gzip.compress(ORACLE_DATA, compresslevel=level, mtime=0)
    fixed = zlib.compressobj(6, zlib.DEFLATED, 31, strategy=zlib.Z_FIXED)
    result['fixed'] = fixed.compress(ORACLE_DATA) + fixed.flush()
    result['dynamic'] = gzip.compress(ORACLE_DATA * 100, mtime=0)
    assert (result['dynamic'][10] >> 1) & 3 == 2
    result['members'] = gzip.compress(ORACLE_DATA[:300], mtime=0) + gzip.compress(ORACLE_DATA[300:], mtime=0)
    return {name: base64.b64encode(data).decode('ascii') for name, data in result.items()}


COMPRESSION_CHECK = 'vectors = ' + repr(vectors()) + '\n' + dedent(r'''
    import base64, binascii, os, stat, struct, subprocess

    def run(arguments, data=None, output=b'', status=0, diagnostic=None):
        result = subprocess.run(arguments, input=data, capture_output=True)
        assert (result.returncode, result.stdout) == (status, output), (arguments, result)
        if diagnostic is None:
            assert not result.stderr, (arguments, result)
        else:
            assert result.stderr and diagnostic in result.stderr, (arguments, result)
        print('PASS upstream compression', repr(arguments), flush=True)
        return result

    def write(path, data, mode=0o640):
        with open(path, 'wb') as output:
            output.write(data)
        os.chmod(path, mode)

    def read(path):
        with open(path, 'rb') as source:
            return source.read()

    os.mkdir('compression')
    os.chdir('compression')
    oracle = bytes(range(256)) * 3 + b'\x00binary\xff\n' * 17
    for name, encoded in vectors.items():
        packed = base64.b64decode(encoded)
        expected = oracle * 100 if name == 'dynamic' else oracle
        for command in (['gunzip', '-c'], ['gzip', '-d', '-c'], ['zcat']):
            run(command, packed, expected)
        run(['gunzip', '-t'], packed)
        write(name + '.gz', packed)
    # Optional fields, full 16-bit extra length, header CRC, name and comment.
    member = base64.b64decode(vectors['level-9'])
    for count in (0, 16383, 16384, 16385, 65535):
        header = member[:3] + bytes([0x1e]) + struct.pack('<I', 1600000000) + member[8:10]
        header += struct.pack('<H', count) + b'x' * count + b'original.bin\0comment\0'
        header += struct.pack('<H', binascii.crc32(header) & 65535)
        decorated = header + member[10:]
        run(['gunzip', '-c'], decorated, oracle)
        if count == 65535:
            write('optional.gz', decorated)
        broken = bytearray(decorated)
        broken[len(header) - 1] ^= 1
        run(['gunzip', '-t'], bytes(broken), status=1, diagnostic=b'corrupted data')
    write('timestamp.gz', decorated)
    run(['gunzip', '-k', 'timestamp.gz'])
    assert read('timestamp') == oracle and os.stat('timestamp').st_mtime_ns == 1600000000000000000
    for count in (0, 1, 32767, 32768, 32769, 65535, 65536, 65537, 524287, 524288, 524289, 1048576):
        payload = (bytes(range(251)) * (count // 251 + 1))[:count]
        packed = subprocess.run(['gzip', '-c'], input=payload, capture_output=True)
        assert packed.returncode == 0 and not packed.stderr and packed.stdout[:2] == b'\x1f\x8b', (count, packed)
        write('roundtrip-' + str(count) + '.gz', packed.stdout)
        run(['zcat'], packed.stdout, payload)
    for level in ('-1', '-4', '-6', '-9', '--fast', '--best'):
        packed = subprocess.run(['gzip', level, '-c'], input=oracle, capture_output=True)
        assert packed.returncode == 0 and not packed.stderr, packed
        run(['gunzip', '-c'], packed.stdout, oracle)
        write('compressed-' + level.strip('-') + '.gz', packed.stdout)
    run(['gzip', '--test'], member)
    run(['gunzip', '--stdout'], member, oracle)
    run(['zcat', 'level-0.gz', 'level-9.gz'], output=oracle * 2)
    write('plain', oracle, 0o640)
    run(['gzip', '-k', 'plain'])
    assert read('plain') == oracle and stat.S_IMODE(os.stat('plain.gz').st_mode) == 0o640
    run(['gzip', 'plain'], status=1, diagnostic=b'File exists')
    assert read('plain') == oracle
    run(['gzip', '-f', 'plain'])
    assert not os.path.exists('plain')
    run(['gunzip', '-k', 'plain.gz'])
    assert read('plain') == oracle and os.path.exists('plain.gz')
    run(['gunzip', 'plain.gz'], status=1, diagnostic=b'File exists')
    run(['gunzip', '-f', 'plain.gz'])
    assert read('plain') == oracle and not os.path.exists('plain.gz')
    write('archive.tgz', member)
    run(['gunzip', 'archive.tgz'])
    assert read('archive.tar') == oracle and not os.path.exists('archive.tgz')
    write('unknown', member)
    run(['gunzip', 'unknown'], status=1, diagnostic=b'unknown suffix')
    assert read('unknown') == member
    for command in ('gzip', 'gunzip', 'zcat'):
        run([command, 'missing'], status=1, diagnostic=b'No such file')
        run([command, '.'], status=1, diagnostic=b'Is a directory')
        run([command, '-Q'], status=1, diagnostic=b'invalid applet arguments')
    run(['gunzip', '-t'], b'', status=1, diagnostic=b'invalid magic')
    run(['gunzip', '-t'], b'not gzip', status=1, diagnostic=b'invalid magic')
    for end in (2, 7, 10, len(member) // 2, len(member) - 8, len(member) - 1):
        run(['gunzip', '-t'], member[:end], status=1, diagnostic=b'')
    for offset, message in ((-8, b'crc error'), (-4, b'incorrect length')):
        broken = bytearray(member)
        broken[offset] ^= 1
        run(['gunzip', '-t'], bytes(broken), status=1, diagnostic=message)
        # A following valid member must not hide an earlier integrity error.
        run(['gunzip', '-t'], bytes(broken) + member, status=1, diagnostic=message)
        write('broken.gz', bytes(broken))
        run(['gunzip', 'broken.gz'], status=1, diagnostic=message)
        assert read('broken.gz') == bytes(broken) and not os.path.exists('broken')
    for offset, value in ((2, 7), (3, 0x20), (10, 7)):
        broken = bytearray(member)
        broken[offset] = value
        run(['gunzip', '-t'], bytes(broken), status=1, diagnostic=b'')
    # Stdout and stream-input failures must not return success.
    for command in ('gzip -c < .', 'gunzip -c < .', 'gzip -c plain >&-', 'zcat level-9.gz >&-'):
        result = subprocess.run(command, shell=True, capture_output=True)
        assert result.returncode != 0 and result.stderr, (command, result)
        print('PASS upstream compression failure', command, flush=True)
    with open('/compression-result', 'w') as output:
        output.write('UPSTREAM_COMPRESSION_OK\n')
    print('UPSTREAM_COMPRESSION_OK', flush=True)
''')
