"""Upstream sha256sum integration with independent hashlib oracles."""
from textwrap import dedent


SHA256_CHECK = dedent(r'''
    import hashlib, subprocess

    def run(command, payload=b''):
        return subprocess.run(command, shell=True, input=payload, capture_output=True)

    vectors = (b'', b'a', b'abc', b'message digest',
               b'abcdefghijklmnopqrstuvwxyz', bytes(range(256)))
    for payload in vectors:
        result = run('sha256sum', payload)
        expected = hashlib.sha256(payload).hexdigest().encode() + b'  -\n'
        assert (result.returncode, result.stdout, result.stderr) == (0, expected, b''), result
        print('PASS sha256 vector', len(payload), flush=True)

    for length in (1, 55, 56, 63, 64, 65, 4095, 4096, 4097,
                   65535, 65536, 65537, 524287, 524288, 524289, 1024 * 1024):
        payload = bytes((index * 47 + index // 257 + 11) % 256 for index in range(length))
        expected_digest = hashlib.sha256(payload).hexdigest().encode()
        result = run('sha256sum', payload)
        assert (result.returncode, result.stdout, result.stderr) == (0, expected_digest + b'  -\n', b''), (
            length, result.returncode, result.stdout, result.stderr)
        print('PASS sha256 boundary', length, flush=True)

    payloads = {'sha-empty': b'', 'sha-one': b'one\x00\xff',
                'sha-many': bytes(range(256)) * 257}
    for path, payload in payloads.items():
        with open(path, 'wb') as output:
            output.write(payload)
    result = run('sha256sum sha-empty sha-one sha-many')
    expected = b''.join(hashlib.sha256(payload).hexdigest().encode() + b'  ' + path.encode() + b'\n'
                        for path, payload in payloads.items())
    assert (result.returncode, result.stdout, result.stderr) == (0, expected, b''), result
    with open('sha-checksums', 'wb') as output:
        output.write(expected)
    result = run('sha256sum -c sha-checksums')
    assert (result.returncode, result.stdout, result.stderr) == (
        0, b'sha-empty: OK\nsha-one: OK\nsha-many: OK\n', b''), result
    result = run('sha256sum -cs sha-checksums')
    assert (result.returncode, result.stdout, result.stderr) == (0, b'', b''), result
    with open('sha-bad-checksum', 'wb') as output:
        output.write(b'0' * 64 + b'  sha-one\n')
    result = run('sha256sum -c sha-bad-checksum')
    assert result.returncode == 1 and result.stdout == b'sha-one: FAILED\n' and result.stderr, result
    result = run('sha256sum -cs sha-bad-checksum')
    assert result.returncode == 1 and not result.stdout, result
    with open('sha-malformed', 'wb') as output:
        output.write(b'not a checksum line\n')
    result = run('sha256sum -cw sha-malformed')
    assert result.returncode == 1 and result.stderr, result

    for command in ('sha256sum missing-sha-file', 'sha256sum .', 'sha256sum -Q',
                    'sha256sum sha-one sha-many >&-'):
        result = run(command)
        assert result.returncode == 1 and result.stderr, (command, result)
    with open('sha-result', 'w') as output:
        output.write('UPSTREAM_SHA256_OK\n')
    print('UPSTREAM_SHA256_OK', flush=True)
''')
