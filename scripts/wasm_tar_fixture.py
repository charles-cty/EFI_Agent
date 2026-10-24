"""Exercise real tar with independent archive and filesystem oracles."""
from textwrap import dedent


TAR_CHECK = dedent(r'''
    import io, os, stat, subprocess, tarfile

    def run(arguments, status=0, data=None):
        result = subprocess.run(['tar'] + arguments, input=data, capture_output=True)
        if status == 0:
            assert result.returncode == 0 and not result.stderr, (arguments, result)
        else:
            assert result.returncode != 0 and result.stderr, (arguments, result)
        print('PASS upstream tar', repr(arguments), flush=True)
        return result.stdout

    def archive(path, entries, format=tarfile.GNU_FORMAT):
        with tarfile.open(path, 'w', format=format) as output:
            for name, kind, value, mode in entries:
                item = tarfile.TarInfo(name)
                item.mode = mode
                item.uid = item.gid = 1
                item.mtime = 1700000123
                item.type = kind
                if kind == tarfile.REGTYPE:
                    item.size = len(value)
                    output.addfile(item, io.BytesIO(value))
                else:
                    if kind in (tarfile.SYMTYPE, tarfile.LNKTYPE):
                        item.linkname = value
                    output.addfile(item)

    os.mkdir('tar-work')
    os.mkdir('tar-work/source')
    binary = bytes(index % 251 for index in range(524289))
    with open('tar-work/source/binary', 'wb') as output:
        output.write(binary)
    os.chmod('tar-work/source/binary', 0o640)
    os.utime('tar-work/source/binary', (1700000123, 1700000123))
    os.symlink('binary', 'tar-work/source/sym')
    os.link('tar-work/source/binary', 'tar-work/source/hard')
    os.mkdir('tar-work/source/empty', 0o750)
    run(['-cf', 'tar-work/created.tar', '-C', 'tar-work/source', '.'])
    with tarfile.open('tar-work/created.tar') as source:
        members = {item.name.lstrip('./'): item for item in source}
        assert set(members) == {'', 'empty', 'binary', 'hard', 'sym'}, members
        assert members['sym'].issym() and members['sym'].linkname == 'binary'
        assert members['binary'].islnk() != members['hard'].islnk(), members
        regular = members['hard'] if members['hard'].isreg() else members['binary']
        assert source.extractfile(regular).read() == binary
        assert regular.mode == 0o640 and regular.mtime == 1700000123
    names = run(['-tf', 'tar-work/created.tar'])
    assert b'./binary\n' in names and b'./empty/\n' in names, names
    assert b'524289' in run(['-tvf', 'tar-work/created.tar'])
    os.mkdir('tar-work/copied')
    run(['-xpf', 'tar-work/created.tar', '-C', 'tar-work/copied'])
    assert open('tar-work/copied/binary', 'rb').read() == binary
    assert os.stat('tar-work/copied/binary').st_ino == os.stat('tar-work/copied/hard').st_ino
    assert os.readlink('tar-work/copied/sym') == 'binary'
    assert stat.S_IMODE(os.stat('tar-work/copied/binary').st_mode) == 0o640
    assert os.stat('tar-work/copied/binary').st_mtime == 1700000123

    long_name = 'nested/' + 'n' * 120
    long_target = 't' * 130
    entries = [('locked', tarfile.DIRTYPE, '', 0o500),
               ('locked/data', tarfile.REGTYPE, b'locked data\x00', 0o640),
               (long_name, tarfile.REGTYPE, b'long data', 0o751),
               ('long-link', tarfile.SYMTYPE, long_target, 0o777),
               ('forward', tarfile.LNKTYPE, 'later', 0o644),
               ('later', tarfile.REGTYPE, b'forward data', 0o600)]
    for size in (0, 1, 511, 512, 513, 65535, 65536, 65537):
        entries.append(('boundary-' + str(size), tarfile.REGTYPE,
                        bytes(index % 251 for index in range(size)), 0o600))
    archive('tar-work/input.tar', entries)
    os.mkdir('tar-work/out')
    run(['--extract', '--same-permissions', '--file', 'tar-work/input.tar',
         '--directory', 'tar-work/out'])
    assert open('tar-work/out/locked/data', 'rb').read() == b'locked data\x00'
    assert stat.S_IMODE(os.stat('tar-work/out/locked').st_mode) == 0o500
    assert os.stat('tar-work/out/locked').st_mtime == 1700000123
    assert open('tar-work/out/' + long_name, 'rb').read() == b'long data'
    assert os.readlink('tar-work/out/long-link') == long_target
    assert os.stat('tar-work/out/forward').st_ino == os.stat('tar-work/out/later').st_ino
    for size in (0, 1, 511, 512, 513, 65535, 65536, 65537):
        assert open('tar-work/out/boundary-' + str(size), 'rb').read() == bytes(
            index % 251 for index in range(size))
    assert run(['-xOf', 'tar-work/input.tar', 'later']) == b'forward data'
    run(['-tf', 'tar-work/input.tar', 'absent'], 1)
    with open('tar-work/list', 'w') as output:
        output.write('later\n')
    assert run(['-tf', 'tar-work/input.tar', '-T', 'tar-work/list']) == b'later\n'
    assert b'later\n' not in run(['-tf', 'tar-work/input.tar', '--exclude=later'])
    run(['-xkf', 'tar-work/input.tar', '-C', 'tar-work/out'], 1)
    os.mkdir('tar-work/stripped')
    run(['-xpf', 'tar-work/input.tar', '-C', 'tar-work/stripped',
         '--strip-components=1', 'locked/data'])
    assert open('tar-work/stripped/data', 'rb').read() == b'locked data\x00'

    archive('tar-work/pax.tar', [('p/' + 'p' * 110, tarfile.REGTYPE, b'pax', 0o600)], tarfile.PAX_FORMAT)
    os.mkdir('tar-work/pax')
    run(['-xf', 'tar-work/pax.tar', '-C', 'tar-work/pax'])
    assert open('tar-work/pax/p/' + 'p' * 110, 'rb').read() == b'pax'
    # Finite pipes must support member skipping and binary streams above capacity.
    with open('tar-work/created.tar', 'rb') as source:
        assert run(['-xOf', '-', './binary'], data=source.read()) == binary
    result = subprocess.run('tar -cf - -C tar-work/source binary | gzip | gunzip | tar -xOf - binary',
                            shell=True, capture_output=True)
    assert (result.returncode, result.stdout, result.stderr) == (0, binary, b''), result

    run(['-czf', 'tar-work/created.tar.gz', '-C', 'tar-work/source', 'binary'])
    assert run(['-xOzf', 'tar-work/created.tar.gz', 'binary']) == binary
    assert run(['-xOf', 'tar-work/created.tar.gz', 'binary']) == binary
    assert run(['-tf', 'tar-work/created.tar.gz']) == b'binary\n'
    run(['-caf', 'tar-work/automatic.tgz', '-C', 'tar-work/source', 'binary'])
    packed = open('tar-work/automatic.tgz', 'rb').read()
    assert packed[:2] == b'\x1f\x8b'
    assert run(['-xzOf', '-', 'binary'], data=packed) == binary
    for name, damaged in [('gzip-crc', packed[:-8] + bytes([packed[-8] ^ 1]) + packed[-7:]),
                           ('gzip-short', packed[:-4])]:
        with open('tar-work/' + name, 'wb') as output:
            output.write(damaged)
        run(['-tzf', 'tar-work/' + name], 1)
    # Archive and pipe descriptors can occupy any closed standard descriptor.
    for descriptor in (0, 1, 2):
        name = 'tar-work/closed-' + str(descriptor) + '.gz'
        command = 'tar -czf ' + name + ' -C tar-work/source binary '
        command += str(descriptor) + ('<&-' if descriptor == 0 else '>&-')
        result = subprocess.run(command, shell=True, capture_output=True)
        assert (result.returncode, result.stdout, result.stderr) == (0, b'', b''), (command, result)
        assert run(['-xOzf', name, 'binary']) == binary
    result = subprocess.run('env -i tar -czf tar-work/empty-env.gz -C tar-work/source binary',
                            shell=True, capture_output=True)
    assert (result.returncode, result.stdout, result.stderr) == (0, b'', b''), result
    assert run(['-xOzf', 'tar-work/empty-env.gz', 'binary']) == binary

    os.mkdir('tar-work/outside')
    with open('tar-work/outside/keep', 'wb') as output:
        output.write(b'untouched')
    os.mkdir('tar-work/guard')
    os.symlink('../outside', 'tar-work/guard/pivot')
    archive('tar-work/escape.tar', [('pivot/keep', tarfile.REGTYPE, b'bad', 0o644)])
    run(['-xf', 'tar-work/escape.tar', '-C', 'tar-work/guard'], 1)
    archive('tar-work/escape-link.tar', [('bad-link', tarfile.LNKTYPE, '../outside/keep', 0o644)])
    run(['-xf', 'tar-work/escape-link.tar', '-C', 'tar-work/guard'], 1)
    assert open('tar-work/outside/keep', 'rb').read() == b'untouched'
    assert not os.path.lexists('tar-work/guard/bad-link')
    archive('tar-work/prefix.tar', [('../safe', tarfile.REGTYPE, b'safe', 0o644)])
    result = subprocess.run(['tar', '-xf', 'tar-work/prefix.tar', '-C', 'tar-work/guard'], capture_output=True)
    assert result.returncode == 0 and result.stderr, result
    assert open('tar-work/guard/safe', 'rb').read() == b'safe'
    assert not os.path.exists('tar-work/safe')

    original = open('tar-work/input.tar', 'rb').read()
    for name, data in [('bad-checksum', bytes([original[0] ^ 1]) + original[1:]),
                       ('short-header', original[:511]), ('short-data', original[:1025]),
                       ('empty', b'')]:
        with open('tar-work/' + name, 'wb') as output:
            output.write(data)
        run(['-xOf', 'tar-work/' + name], 1)
        run(['-tf', 'tar-work/' + name], 1)
    for index, bad_record in enumerate((b'4294967295 path=x\n', b'0 path=x\n',
                                        b'17 path=x\x00hidden\n', b'12 path=bad!')):
        header = tarfile.TarInfo('PaxHeader')
        header.type = tarfile.XHDTYPE
        header.size = len(bad_record)
        header_data = header.tobuf(format=tarfile.USTAR_FORMAT)
        with open('tar-work/pax-bad-' + str(index), 'wb') as output:
            output.write(header_data + bad_record + b'\x00' * (-len(bad_record) % 512) + original)
        run(['-tf', 'tar-work/pax-bad-' + str(index)], 1)
    for arguments in ([], ['-cf', 'tar-work/no-input.tar'], ['-Q'],
                      ['-cf', 'tar-work/missing.tar', 'tar-work/missing'],
                      ['-tf', 'tar-work/absent.tar']):
        run(arguments, 1)
    for command in ('tar -tf tar-work/input.tar >&-',
                    'tar -cf - tar-work/source/binary >&-'):
        result = subprocess.run(command, shell=True, capture_output=True)
        assert result.returncode and result.stderr, (command, result)
    with open('tar-result', 'wb') as output:
        output.write(b'UPSTREAM_TAR_OK\n')
    print('UPSTREAM_TAR_OK', flush=True)
''')
