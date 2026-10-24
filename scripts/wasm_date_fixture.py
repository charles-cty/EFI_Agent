"""Independent UTC, timezone, timestamp, parsing and error checks for date."""
from textwrap import dedent


DATE_CHECK = dedent(r'''
    import datetime, os, subprocess, time

    cases = [
        ('date -u -d @0 +%s', b'0\n', 0),
        ('date -u -d @-2 +%s', b'-2\n', 0),
        ('date -u -d @-1 +%s', b'-1\n', 0),
        ('date -u -d @-1 +%Y-%m-%dT%H:%M:%S%z', b'1969-12-31T23:59:59+0000\n', 0),
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
        ('TZ=JST-9 date -d @0 +%H:%M:%S%z', b'09:00:00+0900\n', 0),
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
    # Derive civil time from UTC plus independent offset/abbreviation vectors.
    zones = [
        ('Asia/Singapore', '2024-01-01T00:00:00', 28800, '+08'),
        ('Asia/Kathmandu', '2024-01-01T00:00:00', 20700, '+0545'),
        ('Asia/Kathmandu', '1970-01-01T00:00:00', 19800, '+0530'),
        ('America/New_York', '2024-03-10T06:59:59', -18000, 'EST'),
        ('America/New_York', '2024-03-10T07:00:00', -14400, 'EDT'),
        ('America/New_York', '2024-11-03T05:59:59', -14400, 'EDT'),
        ('America/New_York', '2024-11-03T06:00:00', -18000, 'EST'),
        ('Europe/London', '2024-03-31T00:59:59', 0, 'GMT'),
        ('Europe/London', '2024-03-31T01:00:00', 3600, 'BST'),
        ('Australia/Lord_Howe', '2024-04-06T14:59:59', 39600, '+11'),
        ('Australia/Lord_Howe', '2024-04-06T15:00:00', 37800, '+1030'),
        ('Pacific/Chatham', '2024-01-01T00:00:00', 49500, '+1345'),
        ('America/New_York', '2050-01-01T00:00:00', -18000, 'EST'),
        ('America/New_York', '2050-07-01T00:00:00', -14400, 'EDT'),
        ('Australia/Sydney', '2050-01-01T00:00:00', 39600, 'AEDT'),
        ('Australia/Sydney', '2050-07-01T00:00:00', 36000, 'AEST'),
        ('JST-9', '2038-01-19T03:14:07', 32400, 'JST'),
        ('JST-9', '2038-01-19T03:14:08', 32400, 'JST'),
        ('NST3:30', '2024-01-01T00:00:00', -12600, 'NST'),
        ('<+0545>-5:45', '2024-01-01T00:00:00', 20700, '+0545'),
        ('EST5EDT,M3.2.0/2,M11.1.0/2', '2024-03-10T06:59:59', -18000, 'EST'),
        ('EST5EDT,M3.2.0/2,M11.1.0/2', '2024-03-10T07:00:00', -14400, 'EDT'),
    ]
    for zone, instant, offset, abbreviation in zones:
        utc = datetime.datetime.fromisoformat(instant).replace(tzinfo=datetime.timezone.utc)
        epoch = int(utc.timestamp())
        local = utc + datetime.timedelta(seconds=offset)
        sign = '+' if offset >= 0 else '-'
        hours, minutes = divmod(abs(offset) // 60, 60)
        suffix = '%s%02d%02d' % (sign, hours, minutes)
        expected = (local.strftime('%Y-%m-%d_%H:%M:%S') + '_' + suffix + '_' + abbreviation + '\n').encode()
        cases.append(("TZ='%s' date -d @%d +%%Y-%%m-%%d_%%H:%%M:%%S_%%z_%%Z" % (zone, epoch), expected, 0))
        # A local civil time outside a repeated DST hour must round-trip.
        if instant.endswith('00:00:00'):
            command = "TZ='%s' date -d '%s' +%%s" % (zone, local.strftime('%Y-%m-%d %H:%M:%S'))
            cases.append((command, str(epoch).encode() + b'\n', 0))
    cases.extend([
        ("TZ=:Asia/Singapore date -d @1704067200 +%H:%M:%S%z", b'08:00:00+0800\n', 0),
        ("TZ=/share/zoneinfo/Asia/Kathmandu date -d @1704067200 +%H:%M:%S%z", b'05:45:00+0545\n', 0),
        ("TZ=America/New_York date -u -d @0 +%H:%M:%S%z", b'00:00:00+0000\n', 0),
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
