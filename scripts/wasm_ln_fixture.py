"""Exercise upstream ln and shared hard-link semantics through Python and ash."""
from textwrap import dedent


LN_CHECK = dedent(r'''
    import os, stat, subprocess

    def run(arguments, status=0):
        result = subprocess.run(arguments, capture_output=True)
        assert result.returncode == status, (arguments, result)
        assert bool(result.stderr) == bool(status), (arguments, result)
        print('PASS hard links', repr(arguments), flush=True)
        return result.stdout

    def write(path, data):
        with open(path, 'wb') as output:
            output.write(data)

    def read(path):
        with open(path, 'rb') as source:
            return source.read()

    def same(*paths):
        stats = [os.lstat(path) for path in paths]
        assert len({(s.st_dev, s.st_ino) for s in stats}) == 1, (paths, stats)
        assert all(s.st_nlink == len(paths) for s in stats), (paths, stats)

    os.makedirs('ln-work/a')
    os.mkdir('ln-work/b')
    write('ln-work/a/file', b'original\0\xff')
    run(['ln', 'ln-work/a/file', 'ln-work/b/alias'])
    same('ln-work/a/file', 'ln-work/b/alias')
    os.link('ln-work/a/file', 'ln-work/python')
    same('ln-work/a/file', 'ln-work/b/alias', 'ln-work/python')
    write('ln-work/python', b'shared bytes\0\xff')
    assert read('ln-work/a/file') == read('ln-work/b/alias') == b'shared bytes\0\xff'
    os.chmod('ln-work/b/alias', 0o640)
    assert stat.S_IMODE(os.stat('ln-work/a/file').st_mode) == 0o640
    os.rename('ln-work/a/file', 'ln-work/b/alias')
    same('ln-work/a/file', 'ln-work/b/alias', 'ln-work/python')
    run(['ln', '-f', 'ln-work/a/file', 'ln-work/b/alias'])
    os.symlink('a', 'ln-work/dir-alias')
    run(['ln', '-f', 'ln-work/a/file', 'ln-work/dir-alias/file'], 1)
    for options in (['-f'], ['-b'], ['-b', '-S', '.bak']):
        run(['ln'] + options + ['ln-work/a/file', 'ln-work/a/file'], 1)
        assert read('ln-work/a/file') == b'shared bytes\0\xff'
    assert run(['ln', '-v', 'ln-work/a/file', 'ln-work/b/alias'], 1) == b''
    run(['ln', 'ln-work/a', 'ln-work/directory-hardlink'], 1)
    run(['ln', 'ln-work/missing', 'ln-work/absent'], 1)
    run(['ln', 'ln-work/a/file', 'ln-work/absent/child'], 1)
    run(['ln', '-Q'], 1)
    run(['ln', '-T', 'ln-work/a/file', 'ln-work/b'], 1)
    assert run(['ln', '-v', 'ln-work/a/file', 'ln-work/verbose']) == b"'ln-work/verbose' -> 'ln-work/a/file'\n"
    os.unlink('ln-work/verbose')
    write('ln-work/a/other', b'other')
    run(['ln', 'ln-work/a/file', 'ln-work/a/other', 'ln-work/b'])
    same('ln-work/a/other', 'ln-work/b/other')
    os.unlink('ln-work/b/file')
    # -n replaces the directory symlink itself; it must not write inside it.
    run(['ln', '-sfn', 'missing-target', 'ln-work/dir-alias'])
    assert os.readlink('ln-work/dir-alias') == 'missing-target'
    os.mkdir('ln-work/no-write', 0o555)
    run(['ln', 'ln-work/a/file', 'ln-work/no-write/denied'], 1)
    os.chmod('ln-work/no-write', 0o755)

    # Descriptor-relative source and destination use distinct directory anchors.
    a = os.open('ln-work/a', os.O_RDONLY | os.O_DIRECTORY)
    b = os.open('ln-work/b', os.O_RDONLY | os.O_DIRECTORY)
    os.link('file', 'relative', src_dir_fd=a, dst_dir_fd=b)
    os.close(a)
    os.close(b)
    same('ln-work/a/file', 'ln-work/b/alias', 'ln-work/python', 'ln-work/b/relative')
    os.unlink('ln-work/a/file')
    os.rename('ln-work/b', 'ln-work/moved')
    same('ln-work/moved/alias', 'ln-work/python', 'ln-work/moved/relative')
    run(['cp', '-l', 'ln-work/python', 'ln-work/cp-link'])
    same('ln-work/moved/alias', 'ln-work/python', 'ln-work/moved/relative', 'ln-work/cp-link')
    # -a retains links inside the new tree, without linking it to the source.
    os.mkdir('ln-work/source')
    write('ln-work/source/first', b'archive group')
    run(['ln', 'ln-work/source/first', 'ln-work/source/second'])
    run(['cp', '-a', 'ln-work/source', 'ln-work/copy'])
    same('ln-work/copy/first', 'ln-work/copy/second')
    assert os.stat('ln-work/source/first').st_ino != os.stat('ln-work/copy/first').st_ino
    assert run(['du', '-bs', 'ln-work/copy']) == b'13\tln-work/copy\n'
    assert run(['du', '-bls', 'ln-work/copy']) == b'26\tln-work/copy\n'
    write('ln-work/copy/first', b'copy shared')
    assert read('ln-work/copy/second') == b'copy shared'
    assert read('ln-work/source/second') == b'archive group'

    run(['ln', '-s', 'absent', 'ln-work/dangling'])
    run(['ln', 'ln-work/dangling', 'ln-work/dangling-alias'])
    same('ln-work/dangling', 'ln-work/dangling-alias')
    assert os.readlink('ln-work/dangling-alias') == 'absent'
    write('ln-work/target', b'target')
    os.symlink('target', 'ln-work/link')
    os.link('ln-work/link', 'ln-work/physical-link', follow_symlinks=False)
    same('ln-work/link', 'ln-work/physical-link')
    os.link('ln-work/link', 'ln-work/followed', follow_symlinks=True)
    same('ln-work/target', 'ln-work/followed')
    write('ln-work/replace', b'old')
    write('ln-work/replace.bak', b'backup source')
    run(['ln', '-b', '-S', '.bak', 'ln-work/replace.bak', 'ln-work/replace'], 1)
    assert read('ln-work/replace.bak') == b'backup source'
    assert read('ln-work/replace') == b'old'
    run(['ln', '-b', '-S', '', 'ln-work/python', 'ln-work/replace'], 1)
    assert read('ln-work/replace') == b'old'
    run(['ln', '-b', '-S', '.bak', 'ln-work/python', 'ln-work/replace'])
    assert read('ln-work/replace.bak') == b'old'
    assert read('ln-work/replace') == b'shared bytes\0\xff'
    # Failed stdout must propagate even after link creation succeeds.
    result = subprocess.run('ln -v ln-work/python ln-work/closed-output >&-', shell=True, capture_output=True)
    assert result.returncode and result.stderr, result
    assert read('ln-work/closed-output') == b'shared bytes\0\xff'

    with open('ln-work/copy/second', 'rb') as retained:
        os.unlink('ln-work/copy/first')
        assert os.fstat(retained.fileno()).st_nlink == 1
        os.unlink('ln-work/copy/second')
        assert os.fstat(retained.fileno()).st_nlink == 0
        assert retained.read() == b'copy shared'
    write('ln-result', b'UPSTREAM_LN_OK\n')
    print('UPSTREAM_LN_OK', flush=True)
''')
