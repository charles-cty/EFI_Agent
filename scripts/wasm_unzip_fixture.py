"""Host-generated ZIP inputs for real upstream unzip integration."""
import base64
import io
import stat
from textwrap import dedent
import zipfile


ZIP_TIME = (2040, 2, 29, 12, 34, 56)
ZIP_DATA = bytes(range(251)) * 300 + b'\x00ZIP\xff\n'


def archive(entries):
    stream = io.BytesIO()
    with zipfile.ZipFile(stream, 'w') as output:
        for name, data, mode, method in entries:
            info = zipfile.ZipInfo(name, ZIP_TIME)
            info.create_system = 3
            info.external_attr = mode << 16
            if name.endswith('/'):
                info.external_attr |= 0x10
            info.compress_type = method
            output.writestr(info, data)
    return stream.getvalue()


def vectors():
    stored, deflated = zipfile.ZIP_STORED, zipfile.ZIP_DEFLATED
    regular = stat.S_IFREG | 0o640
    result = {
        'main': archive([
            ('deep/', b'', stat.S_IFDIR | 0o500, stored),
            ('deep/inner/', b'', stat.S_IFDIR | 0o700, stored),
            ('deep/inner/data.bin', ZIP_DATA, regular, deflated),
            ('script.sh', b'#!/bin/ash\necho zip\n', stat.S_IFREG | 0o751, stored),
            ('space name', b'with spaces\n', regular, stored),
            ('\u540d.txt', b'utf8\n', regular, deflated),
        ]),
        'stored': archive([('stored.bin', b'CRC_SENTINEL\0\xff\n', regular, stored)]),
        'deflated': archive([('deflated.bin', ZIP_DATA, regular, deflated)]),
        'links': archive([
            ('target', b'link data\n', regular, stored),
            ('stored-link', b'target', stat.S_IFLNK | 0o777, stored),
            ('deflated-link', b'target', stat.S_IFLNK | 0o777, deflated),
        ]),
        'traversal': archive([
            ('../escape', b'contained', regular, stored),
            ('/absolute', b'absolute contained', regular, stored),
        ]),
        'pivot': archive([('pivot/new/data', b'escape attempt', regular, stored)]),
        'leaf': archive([('leaf', b'overwrite attempt', regular, stored)]),
        'late-link': archive([
            ('pivot', b'../outside', stat.S_IFLNK | 0o777, stored),
            ('pivot/new/data', b'escape attempt', regular, stored),
        ]),
        'local-link': archive([
            ('real/', b'', stat.S_IFDIR | 0o755, stored),
            ('pivot', b'real', stat.S_IFLNK | 0o777, stored),
            ('pivot/data', b'inside', regular, stored),
        ]),
        'empty': archive([]),
    }
    return {name: base64.b64encode(data).decode('ascii') for name, data in result.items()}


UNZIP_CHECK = 'vectors = ' + repr(vectors()) + '\n' + dedent(r'''
    import base64, datetime, io, os, stat, struct, subprocess, zipfile

    def run(arguments, status=0, output=b'', diagnostic=None, data=None):
        result = subprocess.run(['unzip'] + arguments, input=data, capture_output=True)
        assert result.returncode == status, (arguments, result)
        if output is not None:
            assert result.stdout == output, (arguments, result)
        if diagnostic is None:
            assert not result.stderr, (arguments, result)
        else:
            assert result.stderr and diagnostic in result.stderr, (arguments, result)
        print('PASS upstream unzip', repr(arguments), flush=True)
        return result

    def write(path, data):
        with open(path, 'wb') as output:
            output.write(data)

    def read(path):
        with open(path, 'rb') as source:
            return source.read()

    def mode(path):
        return stat.S_IMODE(os.stat(path).st_mode)

    os.mkdir('unzip-work')
    os.chdir('unzip-work')
    for name, encoded in vectors.items():
        write(name + '.zip', base64.b64decode(encoded))
    payload = bytes(range(251)) * 300 + b'\x00ZIP\xff\n'
    expected_time = int(datetime.datetime(2040, 2, 29, 12, 34, 56, tzinfo=datetime.timezone.utc).timestamp())
    run(['-q', 'main.zip', '-d', 'out'])
    assert read('out/deep/inner/data.bin') == payload
    assert read('out/script.sh') == b'#!/bin/ash\necho zip\n'
    assert read('out/space name') == b'with spaces\n' and read('out/\u540d.txt') == b'utf8\n'
    assert mode('out/deep') == 0o500 and mode('out/deep/inner') == 0o700
    assert mode('out/deep/inner/data.bin') == 0o640 and mode('out/script.sh') == 0o751
    for path in ('out/deep', 'out/deep/inner', 'out/deep/inner/data.bin', 'out/script.sh'):
        assert os.stat(path).st_mtime_ns == expected_time * 1000000000, path
    listing = run(['-l', 'main.zip'], output=None).stdout
    assert b'deep/inner/data.bin' in listing and b'script.sh' in listing and b'6 files' in listing, listing
    verbose = run(['-v', 'main.zip'], output=None).stdout
    assert b'Defl' in verbose and b'CRC-32' in verbose, verbose
    run(['-t', 'main.zip'])
    run(['-p', 'main.zip', 'deep/inner/data.bin'], output=payload)
    run(['-q', 'main', 'script.sh', '-d', 'selected'])
    assert read('selected/script.sh') == b'#!/bin/ash\necho zip\n'
    assert not os.path.exists('selected/deep')
    run(['-q', 'main.zip', '-x', 'deep/*', '-d', 'excluded'])
    assert not os.path.exists('excluded/deep') and read('excluded/space name') == b'with spaces\n'
    run(['-qj', 'main.zip', '-d', 'flat'])
    assert read('flat/data.bin') == payload and not os.path.exists('flat/deep')
    write('selected/script.sh', b'keep')
    run(['-qn', 'main.zip', 'script.sh', '-d', 'selected'])
    assert read('selected/script.sh') == b'keep'
    run(['-q', 'main.zip', 'script.sh', '-d', 'selected'], output=None, data=b'n\n')
    assert read('selected/script.sh') == b'keep'
    run(['-q', 'main.zip', 'script.sh', '-d', 'selected'], output=None, data=b'y\n')
    assert read('selected/script.sh') == b'#!/bin/ash\necho zip\n'
    write('selected/script.sh', b'old')
    run(['-qo', 'main.zip', 'script.sh', '-d', 'selected'])
    assert read('selected/script.sh') == b'#!/bin/ash\necho zip\n'
    run(['-q', 'empty.zip', '-d', 'empty'])
    assert os.listdir('empty') == []
    run(['-p', '-'], output=b'CRC_SENTINEL\0\xff\n', data=read('stored.zip'))
    run(['-q', 'links.zip', '-d', 'links'])
    assert os.readlink('links/stored-link') == os.readlink('links/deflated-link') == 'target'
    assert read('links/deflated-link') == b'link data\n'
    run(['-p', 'links.zip'], output=b'link data\ntargettarget')
    run(['-t', 'links.zip'])
    run(['-q', 'local-link.zip', '-d', 'local-links'])
    assert read('local-links/pivot/data') == b'inside'
    for count in (65535, 65536, 65537, 524287, 524288, 524289):
        data = (bytes(range(251)) * (count // 251 + 1))[:count]
        with zipfile.ZipFile('boundary.zip', 'w', compression=zipfile.ZIP_STORED) as output:
            output.writestr('binary', data)
        run(['-p', 'boundary.zip'], output=data)
        run(['-t', 'boundary.zip'])
    run(['-q', 'boundary.zip', '-d', 'boundary'])
    assert read('boundary/binary') == data
    for count in (0, 65513, 65514, 65535):
        member = read('stored.zip')
        commented = member[:-2] + struct.pack('<H', count) + b'c' * count
        write('comment.zip', commented)
        run(['-p', 'comment.zip'], output=b'CRC_SENTINEL\0\xff\n')
    # Lexical prefixes are removed by upstream; resulting writes stay inside.
    run(['-q', 'traversal.zip', '-d', 'contained'], diagnostic=b'removing leading')
    assert read('contained/escape') == b'contained' and read('contained/absolute') == b'absolute contained'
    assert not os.path.exists('escape') and not os.path.exists('/absolute')
    os.mkdir('outside')
    write('outside/keep', b'untouched')
    os.mkdir('attack')
    os.symlink('../outside', 'attack/pivot')
    run(['-qo', 'pivot.zip', '-d', 'attack'], status=1, diagnostic=b'')
    assert not os.path.exists('outside/new') and read('outside/keep') == b'untouched'
    os.mkdir('absolute-attack')
    os.symlink('/unzip-work/outside', 'absolute-attack/pivot')
    run(['-qo', 'pivot.zip', '-d', 'absolute-attack'], status=1, diagnostic=b'')
    assert not os.path.exists('outside/new')
    os.mkdir('leaf-attack')
    os.symlink('../outside/keep', 'leaf-attack/leaf')
    run(['-qo', 'leaf.zip', '-d', 'leaf-attack'], status=1, diagnostic=b'not a regular file')
    assert read('outside/keep') == b'untouched'
    run(['-qo', 'late-link.zip', '-d', 'late-attack'], status=1, diagnostic=b'')
    assert not os.path.exists('outside/new')
    original = read('stored.zip')
    broken = bytearray(original)
    position = broken.index(b'CRC_SENTINEL')
    broken[position] ^= 1
    write('bad-crc.zip', broken)
    run(['-t', 'bad-crc.zip'], status=1, diagnostic=b'crc error')
    for name in ('stored', 'deflated'):
        broken = bytearray(read(name + '.zip'))
        size = struct.unpack_from('<I', broken, 22)[0]
        struct.pack_into('<I', broken, 22, size + 1)
        write('bad-size.zip', broken)
        run(['-t', 'bad-size.zip'], status=1, diagnostic=b'bad length')
    for end in (0, 3, 15, len(original) - 1, len(original) - 10, len(original) // 2):
        write('truncated.zip', original[:end])
        run(['-t', 'truncated.zip'], status=1, diagnostic=b'')
    broken = bytearray(original)
    broken[6] |= 1
    write('encrypted.zip', broken)
    run(['-t', 'encrypted.zip'], status=1, diagnostic=b'encryption')
    broken = bytearray(original)
    broken[8] = 99
    write('method.zip', broken)
    run(['-t', 'method.zip'], status=1, diagnostic=b'unsupported method')
    broken = bytearray(original)
    position = broken.index(b'PK\x01\x02')
    broken[position + 3] ^= 1
    write('bad-directory.zip', broken)
    run(['-t', 'bad-directory.zip'], status=1, diagnostic=b'invalid central directory')
    broken = bytearray(original)
    broken[3] ^= 1
    write('bad-local.zip', broken)
    run(['-t', 'bad-local.zip'], status=1, diagnostic=b'invalid local file header')
    broken = bytearray(original)
    broken[30] = 0
    write('bad-name.zip', broken)
    run(['-t', 'bad-name.zip'], status=1, diagnostic=b'invalid member name')
    run([], status=1, diagnostic=b'invalid applet arguments')
    run(['-Q'], status=1, diagnostic=b'invalid applet arguments')
    run(['missing'], status=1, diagnostic=b"can't open")
    for command in ('unzip -l main.zip >&-', 'unzip -p stored.zip >&-'):
        result = subprocess.run(command, shell=True, capture_output=True)
        assert result.returncode != 0 and result.stderr, (command, result)
        print('PASS unzip output failure', command, flush=True)
    with open('/unzip-result', 'w') as output:
        output.write('UPSTREAM_UNZIP_OK\n')
    print('UPSTREAM_UNZIP_OK', flush=True)
''')
