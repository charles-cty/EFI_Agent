"""Independent record, field, numeric and pipeline oracles for text applets."""
from textwrap import dedent


TEXT_APPLETS_CHECK = dedent(r'''
    import decimal, itertools, os, random, subprocess

    count = 0
    def check(command, data=b'', expected=b'', status=0, diagnostic=None):
        global count
        result = subprocess.run(command, input=data, shell=True, capture_output=True)
        assert (result.returncode, result.stdout) == (status, expected), (command, result)
        if diagnostic is None:
            assert bool(result.stderr) == bool(status), (command, result)
        else:
            assert diagnostic in result.stderr, (command, result)
        count += 1
        print('PASS upstream text', command, flush=True)

    cases = [
        ('sort', b'b\na\nb\nlast', b'a\nb\nb\nlast\n', 0),
        ('sort -r', b'b\na\n', b'b\na\n', 0),
        ('sort -u', b'b\na\nb\n', b'a\nb\n', 0),
        ('sort -n', b'10\n-2\n1\n', b'-2\n1\n10\n', 0),
        ('sort -n', b'9223372036854775807\n9223372036854775806\n-9223372036854775808\n',
         b'-9223372036854775808\n9223372036854775806\n9223372036854775807\n', 0),
        ('sort -n', b'1.00000000000000000001\n1.00000000000000000000\n',
         b'1.00000000000000000000\n1.00000000000000000001\n', 0),
        ('sort -sn', b'1e3\n2\n01.0\n', b'1e3\n01.0\n2\n', 0),
        ('sort -g', b'1e3\n-2e2\n3\n', b'-2e2\n3\n1e3\n', 0),
        ('sort -h', b'2M\n10K\n1G\n', b'10K\n2M\n1G\n', 0),
        ('sort -h', b'-1K\n-2M\n0G\n1K\n0\n1025K\n1M\n', b'-2M\n-1K\n0\n0G\n1K\n1025K\n1M\n', 0),
        ('sort -M', b'Dec\nJan\nNov\n', b'Jan\nNov\nDec\n', 0),
        ('sort -V', b'v12\nv2\nv1\n', b'v1\nv2\nv12\n', 0),
        ('sort -f', b'b\nA\nC\n', b'A\nb\nC\n', 0),
        ('sort -d', b'a-b\naa\n', b'aa\na-b\n', 0),
        ('sort -i', b'a\x01z\nb\n', b'a\x01z\nb\n', 0),
        ('sort -i', b'a\0z\na\0b\n', b'a\0b\na\0z\n', 0),
        ('sort -k2,2n -k1,1r', b'a 3\nb 2\nc 2\n', b'c 2\nb 2\na 3\n', 0),
        ('sort -s -k2,2n', b'z 2\na 1\ny 2\n', b'a 1\nz 2\ny 2\n', 0),
        ('sort -su -k2,2nr', b'a 1\nb 2\nc 1\n', b'b 2\na 1\n', 0),
        ('sort -t: -k2.2,2.3', b'z:aba\na:azz\nb:aaa\n', b'b:aaa\nz:aba\na:azz\n', 0),
        ('sort -k2.2,2.3', b'z aba\na azz\nb aaa\n', b'b aaa\nz aba\na azz\n', 0),
        ('sort -k2.2b,2.3b', b'z   abb\na acd\nb  aaa\n', b'b  aaa\nz   abb\na acd\n', 0),
        ('sort -t: -k2,2', b'z:a\0c\nx:a\0a\n', b'x:a\0a\nz:a\0c\n', 0),
        ('sort -z', b'b\na\0a\nb\0', b'a\nb\0b\na\0', 0),
        ('sort', b'a\0z\na\0b\n', b'a\0b\na\0z\n', 0),
        ('sort -c', b'a\nb\n', b'', 0),
        ('sort -c', b'b\na\n', b'', 1),
        ('sort -cu', b'a\na\n', b'', 1),
        ('sort -k0', b'', b'', 2),
        ('sort -k4294967296', b'', b'', 2),
        ('sort -k2147483648', b'', b'', 2),
        ('sort -k2.2147483647,2.2147483647', b'b 1\na 2\n', b'a 2\nb 1\n', 0),
        ('sort -t xx', b'', b'', 2),
        ('sort missing-text-input', b'', b'', 2),
        ('sort .', b'', b'', 2),
        ('sort >&-', b'b\na\n', b'', 2),
        ('uniq', b'a\na\nb\na\n', b'a\nb\na\n', 0),
        ('uniq -c', b'a\na\nb', b'      2 a\n      1 b\n', 0),
        ('uniq -d', b'a\na\nb\n', b'a\n', 0),
        ('uniq -u', b'a\na\nb\n', b'b\n', 0),
        ('uniq -i', b'A\na\nb\n', b'A\nb\n', 0),
        ('uniq -f1', b'x one\ny one\nz two\n', b'x one\nz two\n', 0),
        ('uniq -s1 -w1', b'a11\nb12\nc23\n', b'a11\nc23\n', 0),
        ('uniq -w0', b'a\nb\n', b'a\n', 0),
        ('uniq -f2147483647', b'a\nb\n', b'a\n', 0),
        ('uniq -z', b'a\nb\0a\nb\0c\0', b'a\nb\0c\0', 0),
        ('uniq', b'a\0b\na\0b\na\0c\n', b'a\0b\na\0c\n', 0),
        ('uniq -i', b'A\0B\na\0b\na\0c\n', b'A\0B\na\0c\n', 0),
        ('uniq .', b'', b'', 1),
        ('uniq -w bad', b'', b'', 1),
        ('uniq >&-', b'a\n', b'', 1),
        ('cut -b1,3-4', b'abcdef\nx\n', b'acd\nx\n', 0),
        ('cut -b3,1-3', b'abcdef\n', b'abc\n', 0),
        ('cut -b3,1-3 -D', b'abcdef\n', b'cab\n', 0),
        ('cut -c2-', b'abcdef\n', b'bcdef\n', 0),
        ('cut -b1-5', b'a\0b\0c\n', b'a\0b\0c\n', 0),
        ('cut -f2-3 -d:', b'a:b:c:d\nplain\na::c\n', b'b:c\nplain\n:c\n', 0),
        ('cut -f2 -d: -s', b'a:b\nplain\na:\n', b'b\n\n', 0),
        ('cut -f1 -d: -s', b'\na:\n:\n', b'a\n\n', 0),
        ('cut -f1 -d: -D', b'\nplain\na:\n:\n', b'a\n\n', 0),
        ('cut -f1 -d::', b'', b'', 1),
        ('cut -f2 -d: -O,', b'a:b\n', b'b\n', 0),
        ('cut -f1,3 -d: -O,', b'a:b:c\n', b'a,c\n', 0),
        ('cut -f2 -d:', b'a:b\0c:d\n', b'b\0c\n', 0),
        ('cut -F2', b'a  b\nc\td\n', b'b\nd\n', 0),
        ('cut -F1 -s', b'\nplain\na b\n', b'a\n', 0),
        ("cut -F2-3 -d '[,:]+' -O/", b'a,b::c,d\n', b'b/c\n', 0),
        ("cut -F1 -d 'a*'", b'b\n', b'', 1),
        ('cut -F1', b'a\0b\n', b'', 1),
        ('cut -b0', b'', b'', 1),
        ('cut -b4-3', b'', b'', 1),
        ('cut -b1-0', b'', b'', 1),
        ('cut -b2147483648', b'', b'', 1),
        ('cut -b1 -f1', b'', b'', 1),
        ('cut -b1 -d:', b'', b'', 1),
        ('cut . -b1', b'', b'', 1),
        ('cut -b1 >&-', b'a\n', b'', 1),
        ('tr a-z A-Z', b'aBc\0z\xff', b'ABC\0Z\xff', 0),
        ("tr -d '[:digit:]'", b'a1b2c3', b'abc', 0),
        ("tr -cs '[:alnum:]' '\\n'", b'a  b!!!c', b'a\nb\nc', 0),
        ("tr -s '[:space:]'", b'a  b\t\tc\n\n', b'a b\tc\n', 0),
        ('tr abc x', b'abcxyz', b'xxxxyz', 0),
        ("tr '[=a=]' b", b'aba', b'bbb', 0),
        ("tr '\\000-\\003' ABCD", b'\0\1\2\3\4', b'ABCD\4', 0),
        ("tr -d '\\000'", b'a\0b', b'ab', 0),
        ("tr -cd '\\000'", b'a\0b\0', b'\0\0', 0),
        ("tr a ''", b'', b'', 1),
        ('tr', b'', b'', 1),
        ('tr a b >&-', b'abc', b'', 1),
    ]
    for command, data, expected, status in cases:
        check(command, data, expected, status)

    rng = random.Random(7132)
    # Compare complete records, including NUL and non-ASCII bytes.
    for delimiter in (b'\n', b'\0'):
        alphabet = [i for i in (0, 1, 9, 10, 32, 65, 97, 127, 128, 255) if i != delimiter[0]]
        records = [bytes(rng.choice(alphabet) for _ in range(rng.randrange(20))) for _ in range(160)]
        records += records[:40]
        data = delimiter.join(records) + delimiter
        flag = ' -z' if delimiter == b'\0' else ''
        for option, expected in [('', sorted(records)), (' -r', sorted(records, reverse=True)),
                                 (' -u', sorted(set(records)))]:
            check('sort' + flag + option, data, delimiter.join(expected) + delimiter)
        groups = [list(group) for _, group in itertools.groupby(records)]
        check('uniq' + flag, data, delimiter.join(group[0] for group in groups) + delimiter)
        expected = b''.join(str(len(group)).rjust(7).encode() + b' ' + group[0] + delimiter for group in groups)
        check('uniq -c' + flag, data, expected)
    for _ in range(12):
        numbers = [str(rng.randrange(-(10**25), 10**25)).encode() + b'.' + str(rng.randrange(10**12)).encode()
                   for _ in range(30)]
        check('sort -n', b'\n'.join(numbers) + b'\n', b'\n'.join(sorted(numbers, key=lambda value: (decimal.Decimal(value.decode()), value))) + b'\n')
    numbers = [str(sign * (10**power + delta)).encode()
               for power in (0, 1, 2, 9, 18, 25, 40) for delta in (-1, 0, 1) for sign in (-1, 1)]
    numbers += [b'0', b'-0', b'-00.00', b'.1', b'.10', b'.1000000001', b'-.001', b'0001.000', b'0.0000000000000000001']
    rng.shuffle(numbers)
    check('sort -n', b'\n'.join(numbers) + b'\n', b'\n'.join(sorted(numbers, key=lambda value: (decimal.Decimal(value.decode()), value))) + b'\n')
    keyed = [(str(i).encode() + b' ' + str(rng.randrange(-3, 4)).encode()) for i in range(120)]
    rng.shuffle(keyed)
    for reverse in (False, True):
        expected = sorted(keyed, key=lambda line: int(line.split()[1]), reverse=reverse)
        check('sort -s -k2,2n' + ('r' if reverse else ''), b'\n'.join(keyed) + b'\n', b'\n'.join(expected) + b'\n')
    for _ in range(12):
        line = bytes(rng.choice([0, 1, 9, 32, 65, 97, 255]) for _ in range(79))
        ranges = [(rng.randrange(1, 90), rng.randrange(1, 90)) for _ in range(5)]
        ranges = [(min(a,b), max(a,b)) for a,b in ranges]
        selected = {i - 1 for a,b in ranges for i in range(a,b+1)}
        expected = bytes(value for i,value in enumerate(line) if i in selected) + b'\n'
        check('cut -b' + ','.join(str(a)+'-'+str(b) for a,b in ranges), line+b'\n', expected)
    for size in (65535, 65536, 65537, 196613):
        data = (bytes(range(256)) * ((size + 255) // 256))[:size]
        check("tr -d '\\000'", data, data.replace(b'\0', b''))
        line = b'x' * size
        check('uniq', line+b'\n'+line+b'\n', line+b'\n')
        check('cut -b1-', line+b'\n', line+b'\n')
    with open('text-input', 'wb') as output:
        output.write(b'z\na\na\nb\n')
    check('sort -o text-input text-input')
    assert open('text-input', 'rb').read() == b'a\na\nb\nz\n'
    check('uniq text-input text-output')
    assert open('text-output', 'rb').read() == b'a\nb\nz\n'
    check('cat text-input | sort -u | uniq | tr a-z A-Z | cut -b1', expected=b'A\nB\nZ\n')
    check('cut -b1 missing-text-input text-input', expected=b'a\na\nb\nz\n', status=1)
    with open('text-applets-result', 'w') as output:
        output.write('UPSTREAM_TEXT_APPLETS_OK\n')
    print('UPSTREAM_TEXT_APPLETS_OK', count)
''')
