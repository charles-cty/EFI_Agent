"""Exercise upstream df using independent statvfs and capacity oracles."""
from textwrap import dedent


DF_CHECK = dedent(r'''
    import json, os, subprocess

    def run(arguments, status=0, env=None):
        result = subprocess.run(['df'] + arguments, capture_output=True, env=env)
        assert result.returncode == status, (arguments, result)
        assert bool(result.stderr) == bool(status), (arguments, result)
        print('PASS upstream df', repr(arguments), flush=True)
        return result.stdout

    def rows(output):
        lines = output.decode().splitlines()
        assert lines and lines[0].startswith('Filesystem'), lines
        return [line.split() for line in lines[1:]]

    def check(path, unit=1024, options=None):
        before = os.statvfs(path)
        lines = rows(run((options or ['-kP']) + [path]))
        after = os.statvfs(path)
        assert len(lines) == 1, lines
        source, total, used, available, percent, mount = lines[0]
        total, used, available = map(int, (total, used, available))
        round_unit = lambda value: (value + unit // 2) // unit
        assert total == round_unit(before.f_blocks * before.f_frsize), (path, total, before)
        low, high = sorted([before.f_bavail * before.f_frsize, after.f_bavail * after.f_frsize])
        # Other host activity can allocate and release blocks between queries.
        drift = 4 * 1024 * 1024 if before.f_fsid == 0 else 0
        assert round_unit(low - drift) <= available <= round_unit(high + drift), (path, available, before, after)
        low, high = sorted([(s.f_blocks - s.f_bfree) * s.f_frsize for s in (before, after)])
        assert round_unit(low - drift) <= used <= round_unit(high + drift), (path, used, before, after)
        assert 0 <= int(percent.rstrip('%')) <= 100, lines
        assert mount in ('/', '/tmp', '/dev', '/proc')
        return before

    os.mkdir('df-work')
    with open('df-work/data', 'wb') as output:
        output.write(b'workspace data')
    for path in ['/', '.', 'df-work/data', '/bin/df', '/tmp', '/dev/null', '/proc/uptime']:
        check(path)
    for unit, arguments in [(512, ['-B512', '-P']), (1, ['-B1', '-P']),
                            (1048576, ['-mP']), (1024, ['-BKiB', '-P'])]:
        check('/', unit, arguments)
    expected = os.statvfs('/')
    records = rows(run(['-iP', '/']))
    assert int(records[0][1]) == expected.f_files
    assert int(records[0][3]) <= expected.f_ffree
    assert len(rows(run(['-aTP']))) == 4
    assert len(rows(run(['-P']))) == 2
    assert rows(run(['-t', 'tmpfs', '-P']))[0][0] == 'tmpfs'
    assert rows(run(['-t', 'not-installed', '-P'])) == []
    for arguments in (['-hP', '/'], ['-HP', '/'], ['-TP', '/tmp']):
        assert rows(run(arguments)), arguments
    environment = dict(os.environ, POSIXLY_CORRECT='1')
    assert b'512-blocks' in run(['-P', '/'], env=environment)
    for arguments in (['-Q'], ['-B0'], ['-Bgarbage'], ['df-work/missing']):
        run(arguments, 1)
    os.symlink('missing', 'df-work/dangling')
    run(['df-work/dangling'], 1)
    os.symlink('data', 'df-work/link')
    check('df-work/link')
    os.mkdir('df-work/denied', 0o000)
    run(['df-work/denied/child'], 1)
    os.chmod('df-work/denied', 0o700)
    result = subprocess.run('df -P / >&-', shell=True, capture_output=True)
    assert result.returncode and result.stderr, result

    # Shared /tmp accounting includes detached live files until their last close.
    initial = os.statvfs('/tmp')
    assert initial.f_blocks == 32 * 1024 * 1024 and initial.f_frsize == 1
    with open('/tmp/df-held', 'w+b') as held:
        held.write(b'x' * 65537)
        held.flush()
        after = os.statvfs('/tmp')
        assert after.f_bfree == initial.f_bfree - 65537, (initial, after)
        assert os.fstatvfs(held.fileno()).f_bfree == after.f_bfree
        check('/tmp/df-held', 1, ['-B1', '-P'])
        os.unlink('/tmp/df-held')
        assert os.statvfs('/tmp').f_bfree == after.f_bfree
        assert os.fstatvfs(held.fileno()).f_bfree == after.f_bfree
    assert os.statvfs('/tmp').f_bfree == initial.f_bfree
    # No snapshot descriptor leaks across repeated C and Python calls.
    for _ in range(140):
        os.statvfs('/')
    with open('df-work/data', 'rb') as source:
        assert os.fstatvfs(source.fileno()).f_blocks == expected.f_blocks
        os.unlink('df-work/data')
        assert os.fstatvfs(source.fileno()).f_blocks == expected.f_blocks
    for fd in (-1, 127):
        try:
            os.fstatvfs(fd)
        except OSError:
            pass
        else:
            raise AssertionError(fd)
    snapshot = os.statvfs('/')
    with open('df-work/measurements.json', 'w') as output:
        json.dump(list(snapshot), output)
    with open('df-result', 'wb') as output:
        output.write(b'UPSTREAM_DF_OK\n')
    print('UPSTREAM_DF_OK', flush=True)
''')
