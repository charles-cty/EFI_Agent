"""Check upstream copy/move parsing against independent filesystem effects."""
from textwrap import dedent


COPY_MOVE_CHECK = dedent(r'''
    import os, stat, subprocess

    def run(command, status=0, output=b'', diagnostic=None, data=None):
        result = subprocess.run(command, shell=True, input=data, capture_output=True)
        assert (result.returncode, result.stdout) == (status, output), (command, result)
        if diagnostic is None:
            assert bool(result.stderr) == bool(status), (command, result)
        else:
            assert diagnostic in result.stderr, (command, result)
        print('PASS upstream copy/move', command, flush=True)
        return result

    def write(path, data, mode=0o640):
        with open(path, 'wb') as output:
            output.write(data)
        os.chmod(path, mode)

    def read(path):
        with open(path, 'rb') as source:
            return source.read()

    def mode(path):
        return stat.S_IMODE(os.stat(path).st_mode)

    os.mkdir('copy-move')
    os.chdir('copy-move')
    original = b'source\0\xff retained\n'
    write('source', original)
    write('destination', b'old destination')
    run('cp source destination')
    assert read('destination') == original
    run('cp source source', 1, diagnostic=b'same file')
    assert read('source') == original
    os.symlink('source', 'source-alias')
    run('cp source source-alias', 1, diagnostic=b'same file')
    assert read('source') == original and os.path.islink('source-alias')
    run('cp source-alias source', 1, diagnostic=b'same file')
    assert read('source') == original
    run('cp missing destination', 1)
    assert read('destination') == original
    run('cp -n source destination')
    write('destination', b'keep existing')
    run('cp --no-clobber source destination')
    assert read('destination') == b'keep existing'
    run('cp -i source destination', diagnostic=b"overwrite 'destination'?", data=b'n\n')
    assert read('destination') == b'keep existing'
    run('cp -i source destination', diagnostic=b"overwrite 'destination'?", data=b'y\n')
    assert read('destination') == original
    write('destination', b'no write', 0o400)
    run('cp source destination', 1)
    assert read('destination') == b'no write'
    run('cp -f source destination')
    assert read('destination') == original and mode('destination') == 0o640
    run('cp --remove-destination source source-alias')
    assert not os.path.islink('source-alias') and read('source-alias') == original
    run('cp -P source-alias copied-file')
    os.symlink('source', 'source-link')
    run('cp -P source-link copied-link')
    assert os.readlink('copied-link') == 'source'
    run('cp -L source-link dereferenced')
    assert not os.path.islink('dereferenced') and read('dereferenced') == original
    run('cp -s source soft-copy')
    assert os.readlink('soft-copy') == 'source'
    os.symlink('missing', 'dangling')
    run('cp -P dangling dangling-copy')
    assert os.readlink('dangling-copy') == 'missing'
    run('cp -L dangling cannot-copy', 1)
    assert not os.path.lexists('cannot-copy')
    os.mkdir('source-tree')
    os.mkdir('source-tree/deep')
    os.mkdir('source-tree/empty')
    write('source-tree/deep/binary', bytes(range(256)) * 257)
    write('source-tree/plain', original, 0o750)
    os.symlink('deep/binary', 'source-tree/link')
    os.utime('source-tree/plain', ns=(1500000000000000000, 1600000000000000000))
    run('cp source-tree omitted', 1)
    assert not os.path.lexists('omitted')
    run('cp -a source-tree archive')
    assert read('archive/deep/binary') == bytes(range(256)) * 257
    assert os.path.isdir('archive/empty') and os.readlink('archive/link') == 'deep/binary'
    assert mode('archive/plain') == 0o750
    assert os.stat('archive/plain').st_mtime_ns == 1600000000000000000
    run('cp -R source-tree source-tree/nested', 1, diagnostic=b'recursion detected')
    assert not os.path.exists('source-tree/nested/nested/deep')
    run('cp --parents source-tree/deep/binary archive')
    assert read('archive/source-tree/deep/binary') == bytes(range(256)) * 257
    os.mkdir('multi')
    run('cp source copied-file multi')
    assert read('multi/source') == read('multi/copied-file') == original
    run('cp -t multi source-link')
    assert read('multi/source-link') == original
    run('cp -T source multi', 1)
    run('cp -v source verbose-copy', output=b"'source' -> 'verbose-copy'\n")
    run('cp -v source closed-copy >&-', 1)
    assert read('closed-copy') == original
    write('older', b'older')
    write('newer', b'newer')
    os.utime('older', (1400000000, 1400000000))
    os.utime('newer', (1700000000, 1700000000))
    run('cp -u older newer')
    assert read('newer') == b'newer'
    run('cp -u newer older')
    assert read('older') == b'newer'
    for count in (65535, 65536, 65537, 524287, 524288, 524289):
        payload = (bytes(range(251)) * (count // 251 + 1))[:count]
        write('boundary-source', payload)
        run('cp boundary-source boundary-copy')
        assert read('boundary-copy') == payload
    run('cp', 1)
    run('cp -Q source no-output', 1)
    write('move-one', b'first')
    write('move-two', b'second')
    os.mkdir('moves')
    run('mv -t moves move-one move-two')
    assert read('moves/move-one') == b'first' and read('moves/move-two') == b'second'
    assert not os.path.exists('move-one') and not os.path.exists('move-two')
    write('move-source', b'moved', 0o700)
    write('move-dest', b'keep')
    retained = open('move-dest', 'rb')
    run('mv -vn move-source move-dest')
    assert read('move-source') == b'moved' and read('move-dest') == b'keep'
    run('mv -i move-source move-dest', diagnostic=b"overwrite 'move-dest'?", data=b'n\n')
    assert read('move-source') == b'moved'
    run('mv -f move-source move-dest')
    assert read('move-dest') == b'moved' and retained.read() == b'keep'
    assert mode('move-dest') == 0o700 and not os.path.exists('move-source')
    retained.close()
    run('mv source-link moved-link')
    assert os.readlink('moved-link') == 'source' and not os.path.lexists('source-link')
    run('mv archive moved-tree')
    assert os.path.isdir('moved-tree/empty') and not os.path.exists('archive')
    run('mv moved-tree moved-tree/inside', 1)
    assert os.path.isdir('moved-tree')
    run('mv -T source multi', 1)
    run('mv -v missing missing-move', 1)
    run('mv -v verbose-copy verbose-moved', output=b"'verbose-copy' -> 'verbose-moved'\n")
    run('mv -v closed-copy closed-moved >&-', 1)
    assert read('closed-moved') == original
    run('mv', 1)
    run('mv -Q source nothing', 1)
    with open('/copy-move-result', 'w') as output:
        output.write('UPSTREAM_COPY_MOVE_OK\n')
    print('UPSTREAM_COPY_MOVE_OK')
''')
