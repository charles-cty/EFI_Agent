"""Independent option and namespace checks for upstream BusyBox ls."""
from textwrap import dedent


LS_CHECK = dedent(r'''
    import os, subprocess

    os.makedirs('ls-tree/sub')
    contents = {
        'alpha': b'a',
        'large': b'L' * 32,
        'space name': b's',
        '名': b'u',
        '.hidden': b'h',
        'ls-tree/sub/beta': b'b',
    }
    for name, data in contents.items():
        with open('ls-tree/' + name if '/' not in name else name, 'wb') as output:
            output.write(data)
    os.symlink('alpha', 'ls-tree/link')
    os.symlink('sub', 'ls-tree/dir-link')
    os.symlink('absent', 'ls-tree/dangling')
    os.symlink('..', 'ls-tree/sub/loop')
    os.utime('ls-tree/alpha', (1_700_000_001, 1_700_000_001))
    os.utime('ls-tree/large', (1_700_000_003, 1_700_000_003))
    os.mkdir('ls-sort')
    for name, data, seconds in (('small', b'x', 1_700_000_001),
                                ('medium', b'xxx', 1_700_000_002),
                                ('large', b'xxxxx', 1_700_000_003)):
        with open('ls-sort/' + name, 'wb') as output:
            output.write(data)
        os.utime('ls-sort/' + name, (seconds, seconds))

    def run(command):
        return subprocess.run(command, shell=True, capture_output=True)

    def names(result):
        assert result.returncode == 0 and not result.stderr, result
        return result.stdout.decode().splitlines()

    default = names(run('ls -1 ls-tree'))
    assert default == sorted(default, key=lambda value: value.encode()), default
    assert set(default) == {'alpha', 'dangling', 'dir-link', 'large', 'link', 'space name', 'sub', '名'}
    assert names(run('ls -A ls-tree')) == ['.hidden'] + default
    assert names(run('ls -a ls-tree')) == ['.', '..', '.hidden'] + default
    assert names(run('ls -d ls-tree ls-tree/alpha')) == ['ls-tree', 'ls-tree/alpha']
    assert names(run('ls -F ls-tree')) == [
        'alpha', 'dangling@', 'dir-link@', 'large', 'link@', 'space name', 'sub/', '名'
    ]
    assert names(run('ls -p ls-tree')) == [
        'alpha', 'dangling', 'dir-link', 'large', 'link', 'space name', 'sub/', '名'
    ]
    assert names(run('ls -r -1 ls-tree')) == list(reversed(default))

    assert names(run('ls -S -1 ls-sort')) == ['large', 'medium', 'small']
    assert names(run('ls -t -1 ls-sort')) == ['large', 'medium', 'small']

    long = run('ls -l ls-tree/alpha ls-tree/link')
    assert long.returncode == 0 and not long.stderr, long
    assert b'-rw-r--r--' in long.stdout and b'lrwxrwxrwx' in long.stdout, long.stdout
    assert b'link -> alpha' in long.stdout, long.stdout
    assert names(run('ls -L ls-tree/link')) == ['ls-tree/link']
    assert names(run('ls -L ls-tree/dir-link')) == ['beta', 'loop']
    assert names(run('ls -H ls-tree/dir-link')) == ['beta', 'loop']

    recursive = run('ls -R ls-tree')
    assert recursive.returncode == 0 and not recursive.stderr, recursive
    assert b'ls-tree/sub:\nbeta\nloop\n' in recursive.stdout, recursive.stdout
    assert b'ls-tree/dir-link:' not in recursive.stdout, recursive.stdout
    quoted = run('ls -Q "ls-tree/space name" ls-tree/名')
    assert quoted.returncode == 0 and quoted.stdout == b'"ls-tree/space name"\n"ls-tree/\xe5\x90\x8d"\n', quoted
    wide = run('ls -w 12 ls-tree')
    assert wide.returncode == 0 and not wide.stderr, wide
    assert b'alpha' in wide.stdout and '名'.encode() in wide.stdout, wide.stdout
    narrow_columns = run('COLUMNS=8 ls -C ls-sort')
    wide_columns = run('COLUMNS=80 ls -C ls-sort')
    assert narrow_columns.returncode == 0 and not narrow_columns.stderr, narrow_columns
    assert wide_columns.returncode == 0 and not wide_columns.stderr, wide_columns
    assert len(narrow_columns.stdout.splitlines()) == 3, narrow_columns.stdout
    assert len(wide_columns.stdout.splitlines()) == 1, wide_columns.stdout
    assert sorted(wide_columns.stdout.decode().split()) == ['large', 'medium', 'small']
    for option in ('-i', '-s'):
        output = run('ls ' + option + ' -1 ls-tree')
        assert output.returncode == 0 and not output.stderr, output
        assert all(line.split(maxsplit=1)[1] in default for line in output.stdout.decode().splitlines())
    for command in ('ls missing-entry', 'ls -Z ls-tree'):
        result = run(command)
        assert result.returncode != 0 and result.stderr, (command, result)

    with open('ls-result', 'wb') as output:
        output.write(('\n'.join(default) + '\n').encode())
    assert open('ls-tree/.hidden', 'rb').read() == b'h'
    print('UPSTREAM_LS_OK', flush=True)
''')
