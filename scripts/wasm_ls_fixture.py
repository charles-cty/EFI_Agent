"""Python tool check for upstream BusyBox ls through ash."""
from textwrap import dedent


LS_CHECK = dedent(r'''
    import os, subprocess

    os.makedirs('ls-tree/sub')
    with open('ls-tree/alpha', 'wb') as output:
        output.write(b'a')
    with open('ls-tree/.hidden', 'wb') as output:
        output.write(b'h')
    with open('ls-tree/sub/beta', 'wb') as output:
        output.write(b'b')

    cases = (
        ('ls ls-tree', b'alpha\nsub\n', 0),
        ('ls -A ls-tree', b'.hidden\nalpha\nsub\n', 0),
        ('ls -F ls-tree', b'alpha\nsub/\n', 0),
        ('ls missing-entry', b'', 1),
    )
    for command, expected, status in cases:
        result = subprocess.run(command, shell=True, capture_output=True)
        assert (result.returncode, result.stdout) == (status, expected), (command, result)
        assert bool(result.stderr) == bool(status), (command, result.stderr)

    result = subprocess.run('ls -R ls-tree', shell=True, capture_output=True)
    assert result.returncode == 0 and b'ls-tree/sub:\nbeta\n' in result.stdout, result
    with open('ls-result', 'wb') as output:
        output.write(b'alpha\nsub\n')
    assert open('ls-tree/.hidden', 'rb').read() == b'h'
    print('UPSTREAM_LS_OK', flush=True)
''')
