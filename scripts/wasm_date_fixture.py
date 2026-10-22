"""Independent UTC, timestamp, parsing and error checks for upstream date."""
from textwrap import dedent


DATE_CHECK = dedent(r'''
    import datetime, os, subprocess, time

    cases = [
        ('date -u -d @0 +%s', b'0\n', 0),
        ('date -u -d @-2 +%s', b'-2\n', 0),
        ('date -u -d @0 +%Y-%m-%dT%H:%M:%S%z', b'1970-01-01T00:00:00+0000\n', 0),
        ('date -u -d @1234567890', b'Fri Feb 13 23:31:30 UTC 2009\n', 0),
        ('date --utc --date=@0 -R', b'Thu, 01 Jan 1970 00:00:00 +0000\n', 0),
        ('date -u -d @0 -I', b'1970-01-01\n', 0),
        ('date -u -d @0 -Ihours', b'1970-01-01T00+00:00\n', 0),
        ('date -u -d @0 -Im', b'1970-01-01T00:00+00:00\n', 0),
        ('date -u -d @0 -Iseconds', b'1970-01-01T00:00:00+00:00\n', 0),
        ('date -u -d @0 -Ins', b'1970-01-01T00:00:00,000000000+00:00\n', 0),
        ('date -u -d @0 +%N:%3N:%12N:%%N', b'000000000:000:000000000000:%N\n', 0),
        ("date -u -d '2024-02-03 04:05:06' +%s", b'1706933106\n', 0),
        ('date -u -d 202402030405.06 +%s', b'1706933106\n', 0),
        ("date -u -D '%Y/%m/%d-%H:%M:%S' -d 2024/02/03-04:05:06 +%s", b'1706933106\n', 0),
        ('TZ=JST-9 date -d @0 +%H:%M:%S%z', b'', 1),
        ('TZ=JST-9 date -u -d @0 +%H:%M:%S%z', b'00:00:00+0000\n', 0),
        ('date -d @0 +', b'\n', 0),
        ('date -d bad', b'', 1),
        ('date -d @broken', b'', 1),
        ('date -d 202413030405.06', b'', 1),
        ('date -d @0 -s @1', b'', 1),
        ('date -d @0 extra', b'', 1),
        ('date -Iinvalid', b'', 1),
        ('date -r absent-date-reference', b'', 1),
        ("date -D '%Y/%m/%d' -d 2024/02/03trailing", b'', 1),
        ('date -Q', b'', 1),
        ('date -d @0 +%s >&-', b'', 1),
        ('date -d @0 +' + 'A' * 1024, b'', 1),
        ('date -s @0', b'', 1),
        ('date @0', b'', 1),
    ]
    with open('date-reference', 'wb') as output:
        output.write(b'reference remains unchanged\0')
    os.utime('date-reference', ns=(1111111111222333444, 1234567890555666777))
    cases.extend([
        ('date -u -r date-reference +%s:%N', b'1234567890:555666777\n', 0),
        ('date -u --reference=date-reference -Ins', b'2009-02-13T23:31:30,555666777+00:00\n', 0),
    ])
    before = time.time_ns()
    for command, expected, status in cases:
        result = subprocess.run(command, shell=True, capture_output=True)
        assert (result.returncode, result.stdout) == (status, expected), (command, result)
        assert bool(result.stderr) == bool(status), (command, result)
        if command in ('date -s @0', 'date @0'):
            assert b'Not supported' in result.stderr, result
        print('PASS upstream date', command[:100], flush=True)
    after = time.time_ns()
    assert after >= before, (before, after)
    result = subprocess.run('date -u +%s', shell=True, capture_output=True)
    assert result.returncode == 0 and not result.stderr, result
    seconds = int(result.stdout)
    assert before // 1000000000 <= seconds <= time.time_ns() // 1000000000, seconds
    # Derive expected calendar bytes independently from timestamp fields.
    result = subprocess.run('date -u -r date-reference +%Y-%m-%d_%H:%M:%S', shell=True, capture_output=True)
    expected = datetime.datetime.fromtimestamp(1234567890, datetime.timezone.utc).strftime('%Y-%m-%d_%H:%M:%S').encode() + b'\n'
    assert (result.returncode, result.stdout, result.stderr) == (0, expected, b''), result
    with open('date-result', 'w') as output:
        output.write('UPSTREAM_DATE_OK\n')
    print('UPSTREAM_DATE_OK')
''')
