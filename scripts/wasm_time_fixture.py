"""Standard Python and ash checks for cooperative monotonic waits."""
from textwrap import dedent


TIME_CHECK = dedent(r'''
    import subprocess, time

    assert time.clock_getres(time.CLOCK_MONOTONIC) > 0
    before = time.monotonic_ns()
    time.sleep(0.04)
    elapsed = time.monotonic_ns() - before
    assert 40_000_000 <= elapsed < 10_000_000_000, elapsed
    # Firmware realtime has coarser resolution than the monotonic source.
    # A two-second RTC delta distinguishes a real wait from a scaled or
    # synthetic counter advancing only when it is read.
    wall_before = time.time_ns()
    time.sleep(2.1)
    wall_elapsed = time.time_ns() - wall_before
    assert 2_000_000_000 <= wall_elapsed < 10_000_000_000, wall_elapsed
    previous = time.monotonic_ns()
    for _ in range(1000):
        current = time.monotonic_ns()
        assert current >= previous
        previous = current

    for command, expected, status in [
        ('sleep 0', b'', 0),
        ('sleep .02s .001m 0h 0d', b'', 0),
        ('sleep -- 0', b'', 0),
        ('sleep', b'', 1),
        ('sleep -1', b'', 1),
        ('sleep 1q', b'', 1),
        ('(sleep .02; printf "timed\\n") | { read -t 1 value; printf "%s:%s\\n" "$value" "$?"; }', b'timed:0\n', 0),
        ('(sleep 1.2; printf "later\\n") | { value=old; read -t 1 value; status=$?; read next; printf "%s:%s:%s\\n" "$value" "$status" "$next"; }', b'old:1:later\n', 0),
    ]:
        before = time.monotonic_ns()
        result = subprocess.run(command, shell=True, capture_output=True)
        assert (result.returncode, result.stdout) == (status, expected), (command, result)
        assert bool(result.stderr) == bool(status), (command, result.stderr)
        if command == 'sleep .02s .001m 0h 0d':
            assert time.monotonic_ns() - before >= 80_000_000
    with open('monotonic-follow', 'wb') as output:
        output.write(b'first\n')
    follower = subprocess.Popen('tail -f -s 1 monotonic-follow | head -n 2',
                                shell=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    assert follower.stdout.readline() == b'first\n'
    with open('monotonic-follow', 'ab') as output:
        output.write(b'second\n')
    assert follower.stdout.readline() == b'second\n'
    # The downstream reader has its two lines. A further write lets tail
    # observe EPIPE, exit and be reaped without an unsupported signal.
    with open('monotonic-follow', 'ab') as output:
        output.write(b'third\n')
    stdout, stderr = follower.communicate()
    assert follower.returncode == 0 and stdout == b'' and b'Broken pipe' in stderr, (stdout, stderr)
    with open('monotonic-result', 'w') as output:
        output.write('MONOTONIC_WAIT_OK\n')
    print('MONOTONIC_WAIT_OK')
''')
