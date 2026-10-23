"""Upstream realpath semantics checked against Python path resolution."""
from textwrap import dedent


REALPATH_CHECK = dedent(r'''
    import os, subprocess

    os.makedirs('real-dir/sub')
    with open('real-dir/file', 'w') as output:
        output.write('target')
    os.symlink('real-dir', 'real-link')
    os.symlink('missing-target', 'dangling-link')
    with open('real-link-target', 'w') as output:
        output.write(os.readlink('real-link'))
    cases = [
        ('realpath real-dir/sub/../file', '/real-dir/file\n', 0),
        ('realpath ./real-link/sub/../../real-dir/file', '/real-dir/file\n', 0),
        ('realpath real-dir/file real-link/file', '/real-dir/file\n/real-dir/file\n', 0),
        ('realpath .', '/\n', 0),
        ('realpath missing-name', '/missing-name\n', 0),
        ('realpath real-dir/missing-name', '/real-dir/missing-name\n', 0),
        ('realpath dangling-link', '/missing-target\n', 0),
        ('realpath missing-dir/file', '', 1),
        ('realpath real-dir/missing-name/', '', 1),
        ('realpath', '', 1),
    ]
    for command, expected, status in cases:
        result = subprocess.run(command, shell=True, capture_output=True)
        assert (result.returncode, result.stdout, bool(result.stderr)) == (
            status, expected.encode(), bool(status)), (command, result)
        print('PASS realpath', command, flush=True)
    for path in ('real-dir', 'real-dir/sub', 'real-link/sub/..',
                 'real-dir/./sub/../file', 'missing-name', 'real-dir/new-file'):
        result = subprocess.run('realpath ' + path, shell=True, capture_output=True)
        expected = os.path.realpath(path).encode() + b'\n'
        assert (result.returncode, result.stdout, result.stderr) == (0, expected, b''), (path, result)
    with open('realpath-result', 'w') as output:
        output.write('UPSTREAM_REALPATH_OK\n')
    print('UPSTREAM_REALPATH_OK', flush=True)
''')
