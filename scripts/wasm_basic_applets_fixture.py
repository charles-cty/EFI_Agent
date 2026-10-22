"""Independent output/status cases for upstream basic BusyBox applets."""
from textwrap import dedent


BASIC_APPLETS_CHECK = dedent(r'''
    import os, subprocess

    os.makedirs('basic-dir/sub')
    os.symlink('basic-dir', 'basic-link')
    cases = [
        ('basename /a/b///', b'b\n', 0),
        ('basename /', b'/\n', 0),
        ("basename ''", b'\n', 0),
        ('basename /a/name.txt .txt', b'name\n', 0),
        ('basename name name', b'name\n', 0),
        ('basename -a /a/one /b/two/', b'one\ntwo\n', 0),
        ('basename -s .txt /a/one.txt two.txt', b'one\ntwo\n', 0),
        ("basename 'space dir/位 name'", '位 name\n'.encode(), 0),
        ('basename', b'', 1),
        ('basename a b c', b'', 1),
        ('dirname /a/b/', b'/a\n', 0),
        ('dirname /', b'/\n', 0),
        ("dirname ''", b'.\n', 0),
        ('dirname plain', b'.\n', 0),
        ('dirname -- /a/b', b'/a\n', 0),
        ('dirname', b'', 1),
        ('dirname a b', b'', 1),
        ('/bin/pwd', b'/\n', 0),
        ('cd basic-dir/sub; /bin/pwd -P', b'/basic-dir/sub\n', 0),
        ('cd basic-dir/sub; PWD=/basic-link/sub /bin/pwd -L', b'/basic-link/sub\n', 0),
        ('cd basic-dir/sub; PWD=/missing /bin/pwd -L', b'/basic-dir/sub\n', 0),
        ('/bin/pwd -Q', b'', 1),
        ('seq 3', b'1\n2\n3\n', 0),
        ('seq 3 -1 1', b'3\n2\n1\n', 0),
        ('seq -2 1', b'-2\n-1\n0\n1\n', 0),
        ('seq -w 8 11', b'08\n09\n10\n11\n', 0),
        ('seq -s : 1 3', b'1:2:3\n', 0),
        ('seq 0 .25 1', b'0.00\n0.25\n0.50\n0.75\n1.00\n', 0),
        ('seq 3 1', b'', 0),
        ('seq bad', b'', 1),
        ('seq -Q 3', b'', 1),
        ('seq', b'', 1),
        ('seq 1 2 3 4', b'', 1),
        ("yes 'a b' c | head -n 3", b'a b c\na b c\na b c\n', 0),
        ("yes '' | head -n 2", b'\n\n', 0),
        ('yes | head -n 2', b'y\ny\n', 0),
        ('env -i', b'', 0),
        ('env -i printenv', b'', 0),
        ('env -i A=first A=last printenv A', b'last\n', 0),
        ("env -i A='a b=c' printenv A", b'a b=c\n', 0),
        ('export A=outer B=second; env -u A -u B printenv A B', b'', 1),
        ('env --ignore-environment A=value printenv A', b'value\n', 0),
        ('env - A=value printenv A', b'value\n', 0),
        ('env -i EMPTY= printenv EMPTY', b'\n', 0),
        ('env -i ash -c "exit 17"', b'', 17),
        ('env -i absent-command', b'', 127),
        ('env -Q', b'', 1),
        ('export A=one B=two; printenv A absent B', b'one\ntwo\n', 1),
        ('/bin/true ignored --help', b'', 0),
        ('/bin/false ignored --help', b'', 1),
        ('true() { printf function; }; true; /bin/true', b'function', 0),
    ]
    for name, expected, status in cases:
        result = subprocess.run(name, shell=True, capture_output=True)
        assert (result.returncode, result.stdout) == (status, expected), (name, result)
        if not status:
            # WASI returns EPIPE without delivering SIGPIPE. Upstream yes
            # reports the failed write before the successful final head stage.
            diagnostic = b'yes: Broken pipe\n' if name.startswith('yes ') else b''
            assert result.stderr == diagnostic, (name, result.stderr)
        elif name in (
            'export A=outer B=second; env -u A -u B printenv A B',
            'env -i ash -c "exit 17"',
            'export A=one B=two; printenv A absent B',
            '/bin/false ignored --help',
        ):
            assert not result.stderr, (name, result.stderr)
        else:
            assert result.stderr, (name, result)
        print('PASS upstream basic', name, flush=True)
    result = subprocess.run('env -i -0 A=1 B=', shell=True, capture_output=True)
    assert result.returncode == 0 and not result.stderr
    assert sorted(result.stdout.split(b'\0')) == [b'', b'A=1', b'B=']
    for size in (4093, 4094, 4095, 4096, 4097, 8191):
        word = 'A' * size
        line = word.encode() + b' word\n'
        expected = (line * ((65537 + len(line) - 1) // len(line)))[:65537]
        result = subprocess.run('yes ' + word + ' word | head -c 65537',
                                shell=True, capture_output=True)
        assert result.returncode == 0 and result.stdout == expected, (size, result)
        assert result.stderr == b'yes: Broken pipe\n', (size, result.stderr)
        print('PASS yes buffer boundary', size, flush=True)
    for command in ('basename /a/b', 'dirname /a/b', '/bin/pwd', 'seq 10000',
                    'env -i A=1', 'env -i A=1 printenv A', 'yes'):
        result = subprocess.run(command + ' >&-', shell=True, capture_output=True)
        assert result.returncode in (1, 255) and not result.stdout, (command, result)
    with open('basic-applets-result', 'w') as output:
        output.write('UPSTREAM_BASIC_APPLETS_OK\n')
    print('UPSTREAM_BASIC_APPLETS_OK')
''')
