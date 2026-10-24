"""Exercise upstream xargs parsing, bounded batches and cooperative children."""
from textwrap import dedent


XARGS_CHECK = dedent(r'''
    import os, subprocess

    def run(arguments, data=b'', output=b'', status=0, diagnostic=None):
        result = subprocess.run(['xargs'] + arguments, input=data, capture_output=True)
        assert (result.returncode, result.stdout) == (status, output), (arguments, data[:80], result)
        if diagnostic is None:
            assert not result.stderr, (arguments, result)
        else:
            assert diagnostic in result.stderr, (arguments, result)
        print('PASS upstream xargs', repr(arguments), flush=True)
        return result

    os.mkdir('xargs-work')
    with open('xargs-work/args', 'w') as script:
        script.write('#!/bin/ash\nprintf "<%s>\\n" "$@"\n')
    os.chmod('xargs-work/args', 0o700)
    dump = '/xargs-work/args'
    run([], b'one two\nthree\tlast', b'one two three last\n')
    run([], b'', b'\n')
    run(['--no-run-if-empty'], b'', b'')
    run(['-r'], b' \t\r\n', b'')
    run(['-n', '2'], b'a b c d e', b'a b\nc d\ne\n')
    run([dump], b"one 'two words' three\\ four \"double quoted\"", b'<one>\n<two words>\n<three four>\n<double quoted>\n')
    run([dump], b"'' \"\" one ''", b'<>\n<>\n<one>\n<>\n')
    run(['-n', '1', dump], b"'' x \"\"", b'<>\n<x>\n<>\n')
    run(['-0', dump], b'a b\0quote\"\0slash\\\0\0last', b'<a b>\n<quote\">\n<slash\\>\n<>\n<last>\n')
    run(['-0', '-n', '2', dump], b'one\0two\0three\0', b'<one>\n<two>\n<three>\n')
    run(['-E', 'STOP'], b'a b STOP c d', b'a b\n')
    run(['-E', ''], b'_ literal', b'_ literal\n')
    run(['-I', '{}', dump, 'prefix:{}:{}'], b' first line\nsecond\n', b'<prefix:first line:first line>\n<prefix:second:second>\n')
    run(['-0', '-I', '%', dump, '%/%'], b'a b\0c d\0', b'<a b/a b>\n<c d/c d>\n')
    run(['-I', '', dump], b'a', status=1, diagnostic=b'empty replacement')
    run([dump], b"'", status=1, diagnostic=b'unmatched quote')
    run([dump], b'"text', status=1, diagnostic=b'unmatched quote')
    run([dump], b'abc\\', status=1, diagnostic=b'unfinished escape')
    run(['-s', '12'], b'aa bb cc dd', b'aa bb\ncc dd\n')
    run(['-s', '12', '-x', '-n', '3'], b'aa bb cc', status=1, diagnostic=b'argument line too long')
    run(['-s', '11', '-x', '-n', '3'], b'aa bb', b'aa bb\n')
    run(['-s', '11', '-x', '-n', '3'], b'aa bb cc', status=1, diagnostic=b'argument line too long')
    run(['-s', '11', '-x', '-n', '2'], b'aa bb cc', b'aa bb\ncc\n')
    run(['-0', '-s', '11', '-x', '-n', '3'], b'aa\0bb\0', b'aa bb\n')
    run(['-0', '-s', '11', '-x', '-n', '3'], b'aa\0bb\0\0', status=1, diagnostic=b'argument line too long')
    run(['-n', '0'], b'a', status=1, diagnostic=b'not in 1..')
    run(['-s', '1'], b'a', status=1, diagnostic=b"can't fit single argument")
    run(['-Q'], b'a', status=1, diagnostic=b'invalid applet arguments')
    run(['missing-xargs-program'], b'a', status=127, diagnostic=b'No such file')
    with open('xargs-work/not-executable', 'w') as script:
        script.write('#!/bin/ash\nexit 0\n')
    run(['/xargs-work/not-executable'], b'a', status=126, diagnostic=b'Permission denied')
    run(['ash', '-c', 'exit 7'], b'a', status=123)
    run(['ash', '-c', 'exit 255'], b'a', status=124, diagnostic=b'exited with status 255')
    run(['-t', dump], b'a b', b'<a>\n<b>\n', diagnostic=(dump + ' a b\n').encode())
    with open('xargs-work/input', 'wb') as source:
        source.write(b'from file\n')
    run(['-a', '/xargs-work/input'], b'', b'from file\n')
    run(['-a', '/xargs-work/missing'], status=1, diagnostic=b'No such file')
    run(['-a', '/xargs-work'], status=1, diagnostic=b'read error')
    run(['-p'], b'a', status=1, diagnostic=b'/dev/tty')
    run(['-o'], b'a', status=1, diagnostic=b'/dev/tty')
    with open('xargs-work/worker', 'w') as script:
        script.write('#!/bin/ash\n'
                     'printf started > /xargs-work/start-$1\n'
                     'peer=one; test "$1" = one && peer=two\n'
                     'n=0; while ! test -f /xargs-work/start-$peer; do\n'
                     '  n=$((n+1)); test "$n" -gt 2000 && exit 77\n'
                     '  sleep .01\n'
                     'done\n'
                     'printf "%s\\n" "$1" > /xargs-work/done-$1\n')
    os.chmod('xargs-work/worker', 0o700)
    run(['-P', '2', '-n', '1', '/xargs-work/worker'], b'one two')
    assert open('xargs-work/done-one').read() == 'one\n'
    assert open('xargs-work/done-two').read() == 'two\n'
    for name in ('start-one', 'start-two', 'done-one', 'done-two'):
        os.unlink('xargs-work/' + name)
    run(['-P', '0', '-n', '1', '/xargs-work/worker'], b'one two')
    with open('xargs-work/status-worker', 'w') as script:
        script.write('#!/bin/ash\n'
                     'if test "$1" = fatal; then\n'
                     '  n=0; while ! test -f /xargs-work/late-started; do\n'
                     '    n=$((n+1)); test "$n" -gt 2000 && exit 77; sleep .01\n'
                     '  done\n'
                     '  exit 255\n'
                     'fi\n'
                     'printf started > /xargs-work/late-started\n'
                     'sleep .1; printf done > /xargs-work/late-done; exit 7\n')
    os.chmod('xargs-work/status-worker', 0o700)
    run(['-P', '2', '-n', '1', '/xargs-work/status-worker'], b'fatal late', status=124, diagnostic=b'exited with status 255')
    assert open('xargs-work/late-done').read() == 'done'
    # Repeated batches exceed the live process quota unless children are reaped.
    payload = b' '.join(str(i).encode() for i in range(100))
    expected = b''.join(b'<' + str(i).encode() + b'>\n' for i in range(100))
    run(['-n', '1', dump], payload, expected)
    for length in (30000, 30720, 65535, 65536, 65537, 524289):
        payload = ((b'x' * 1023 + b'\0') * (length // 1024 + 1))[:length]
        result = subprocess.run(['xargs', '-0', '-n', '31', 'echo'], input=payload, capture_output=True)
        assert result.returncode == 0 and not result.stderr, (length, result)
        lines = result.stdout.splitlines()
        words = [word for line in lines for word in line.split(b' ')]
        assert words == [word for word in payload.split(b'\0') if word], length
        assert all(len(line.split(b' ')) <= 31 for line in lines), length
        print('PASS xargs binary stream length', length, flush=True)
    result = subprocess.run(['/bin/ash', '-c', 'printf a | xargs echo >&-'], capture_output=True)
    assert result.returncode == 123 and not result.stdout, result
    with open('xargs-result', 'w') as output:
        output.write('UPSTREAM_XARGS_OK\n')
    print('UPSTREAM_XARGS_OK')
''')
