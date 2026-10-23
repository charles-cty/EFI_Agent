"""Base64 integration cases with independent standard-library byte oracles."""
from textwrap import dedent


BASE64_CHECK = dedent(r'''
    import base64, subprocess

    checked = 0
    def check(command, payload, expected):
        global checked
        result = subprocess.run(command, shell=True, input=payload, capture_output=True)
        assert (result.returncode, result.stdout, result.stderr) == (0, expected, b''), (
            command, len(payload), result.returncode, result.stdout[:160], result.stderr)
        checked += 1

    # RFC 4648 vectors do not share an implementation with the byte oracle.
    for plain, encoded in ((b'', b''), (b'f', b'Zg=='), (b'fo', b'Zm8='),
                           (b'foo', b'Zm9v'), (b'foob', b'Zm9vYg=='),
                           (b'fooba', b'Zm9vYmE='), (b'foobar', b'Zm9vYmFy')):
        check('base64 -w 0', plain, encoded)
        check('base64 -d', encoded, plain)

    for length in (0, 1, 2, 3, 4, 5, 6, 56, 57, 58, 59, 60, 61,
                   479, 480, 481, 65535, 65536, 65537, 524287, 524288, 524289):
        payload = bytes((index * 73 + index // 251 + 19) % 256 for index in range(length))
        encoded = base64.b64encode(payload)
        wrapped = b''.join(encoded[offset:offset + 76] + b'\n'
                           for offset in range(0, len(encoded), 76))
        check('base64', payload, wrapped)
        check('base64 -w 0 -', payload, encoded)
        check('base64 -d', wrapped, payload)
        if length <= 481:
            for width in (1, 3, 4, 5, 77, 2147483647):
                expected = b''.join(encoded[offset:offset + width] + b'\n'
                                    for offset in range(0, len(encoded), width))
                check('base64 -w ' + str(width), payload, expected)
        print('PASS base64 boundary', length, flush=True)

    check('base64 -d', b'Z\x00m\t9\rv\n', b'foo')
    # BusyBox 1.37.0 accepts -i and skips non-alphabet bytes in either mode.
    check('base64 -d', b'Z!m@9#v$\n', b'foo')
    check('base64 -di', b'Z!m@9#v$\n', b'foo')
    check('base64 -d', b'Zg==\nZg==\n', b'ff')
    check('base64 -d', b'!@#$\n', b'')
    for command, payload in (
        ('base64 -d', b'A'), ('base64 -d', b'AA'), ('base64 -d', b'AAA'),
        ('base64 -d', b'A==='), ('base64 -w -1', b''),
        ('base64 -w 2147483648', b''), ('base64 -w bad', b''),
        ('base64 -Q', b''), ('base64 missing-base64-input', b''),
        ('base64 a b', b''), ('base64 .', b''), ('base64 -d .', b''),
        ('base64 >&-', b'foo'), ('base64 -d >&-', b'Zm9v'),
    ):
        result = subprocess.run(command, shell=True, input=payload, capture_output=True)
        assert result.returncode != 0 and result.stderr, (command, result)
        checked += 1

    payload = bytes(range(256)) * 769 + b'\x00\xffZ'
    encoded = base64.b64encode(payload)
    with open('base64-input', 'wb') as output:
        output.write(payload)
    check('base64 -w 0 base64-input > base64-encoded', b'', b'')
    check('base64 -d base64-encoded > base64-decoded', b'', b'')
    check('cat base64-input | base64 | base64 -d | tee base64-pipeline', b'', payload)
    result = subprocess.run('base64 -w 0 base64-input | head -c 65537', shell=True, capture_output=True)
    assert (result.returncode, result.stdout, result.stderr) == (
        0, encoded[:65537], b'standard output: I/O error\n'), result
    with open('base64-encoded', 'rb') as source:
        assert source.read() == encoded
    for path in ('base64-decoded', 'base64-pipeline'):
        with open(path, 'rb') as source:
            assert source.read() == payload
    with open('base64-result', 'w') as output:
        output.write('UPSTREAM_BASE64_OK\n')
    print('UPSTREAM_BASE64_OK', checked, flush=True)
''')
