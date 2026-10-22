"""Independent filesystem effects for upstream BusyBox mutation applets."""
from textwrap import dedent


MUTATION_APPLETS_CHECK = dedent(r'''
    import os, stat, subprocess

    def run(command, status=0, output=b'', diagnostic=None, data=None):
        result = subprocess.run(command, shell=True, input=data, capture_output=True)
        assert (result.returncode, result.stdout) == (status, output), (command, result)
        if diagnostic is None:
            assert bool(result.stderr) == bool(status), (command, result)
        else:
            assert diagnostic in result.stderr, (command, result)
        print('PASS upstream mutation', command, flush=True)
        return result

    def mode(path):
        return stat.S_IMODE(os.stat(path).st_mode)

    run('mkdir mutation-applets')
    os.chdir('mutation-applets')
    run('mkdir -m 750 explicit')
    assert mode('explicit') == 0o750
    run('mkdir -p -m u=rwx,g=rx,o= parents/a/b')
    assert mode('parents') == mode('parents/a') == 0o755 and mode('parents/a/b') == 0o750
    run('mkdir -p -m 700 parents/a/b')
    assert mode('parents/a/b') == 0o750
    run('umask 077; mkdir -p masked/a/b; mkdir -m 777 unmasked')
    assert all(mode(path) == 0o700 for path in ('masked', 'masked/a', 'masked/a/b'))
    assert mode('unmasked') == 0o777
    run('umask 777; mkdir -p restricted/a')
    assert mode('restricted') == 0o300
    os.chmod('restricted', 0o700)
    assert mode('restricted/a') == 0
    os.chmod('restricted/a', 0o700)
    run('mkdir existing-ok parents/a/b later-ok', 1)
    assert os.path.isdir('existing-ok') and os.path.isdir('later-ok')
    run('mkdir /', 1)
    run('mkdir .', 1)
    run('mkdir -p / .')
    run('mkdir', 1)
    run('mkdir -m wrong invalid-mode', 1)
    assert not os.path.exists('invalid-mode')
    run("mkdir -v 'space 位'", output="created directory: 'space 位'\n".encode())
    run('mkdir --parents --mode=710 long/a')
    assert mode('long/a') == 0o710
    os.symlink('parents', 'parent-link')
    run('mkdir -p parent-link/through')
    assert os.path.isdir('parents/through')
    run('touch ordinary')
    run('mkdir -p ordinary/child', 1)
    run('mkdir -v bad-output >&-', 1)
    assert os.path.isdir('bad-output')

    run('rmdir parents', 1)
    run('rmdir -p long/a')
    assert not os.path.exists('long')
    run('rmdir --ignore-fail-on-non-empty parents')
    run('rmdir parent-link', 1)
    assert os.path.islink('parent-link')
    run('rmdir missing', 1)
    run('rmdir', 1)
    run("rmdir -v 'space 位'", output="rmdir: removing directory, 'space 位'\n".encode())
    run('rmdir -v bad-output >&-', 1)
    assert not os.path.exists('bad-output')

    run('rm -f absent')
    run('rm -f')
    run('rm', 1)
    run('rm absent', 1)
    run('rm parents', 1)
    run('rm -rf parents/.', 1)
    run('rm -rf parents/..', 1)
    assert os.path.isdir('parents/a/b')
    run('rm -rf parent-link')
    assert not os.path.lexists('parent-link') and os.path.isdir('parents/a/b')
    os.symlink('absent', 'dangling')
    run('rm dangling')
    assert not os.path.lexists('dangling')
    with open('ordinary', 'wb') as output:
        output.write(b'kept while open\0')
    retained = open('ordinary', 'rb')
    run('rm ordinary')
    assert not os.path.exists('ordinary') and retained.read() == b'kept while open\0'
    retained.close()
    run('touch prompt')
    run('rm -i prompt', diagnostic=b"remove 'prompt'?", data=b'n\n')
    assert os.path.exists('prompt')
    run('rm -i prompt', diagnostic=b"remove 'prompt'?", data=b' Y\n')
    assert not os.path.exists('prompt')
    run('touch prompt; rm -if prompt')
    assert not os.path.exists('prompt')
    run('mkdir blocked; touch blocked/child; chmod 500 blocked')
    run('rm -rf blocked', 1)
    assert os.path.exists('blocked/child')
    run('chmod 700 blocked; rm -rf blocked')
    run('mkdir unreadable; chmod 300 unreadable')
    run('rm -rv unreadable', 1, diagnostic=b"can't open 'unreadable'")
    os.chmod('unreadable', 0o700)
    run('rm -rv unreadable', output=b"removed directory: 'unreadable'\n")
    run('rm -rf parents masked restricted existing-ok later-ok explicit unmasked')
    assert not any(os.path.exists(path) for path in ('parents', 'masked', 'restricted'))
    run('touch verbose; rm -v verbose', output=b"removed 'verbose'\n")
    run('touch verbose; rm -v verbose >&-', 1)
    assert not os.path.exists('verbose')

    run('touch -c absent')
    assert not os.path.exists('absent')
    run('touch', 1)
    run('touch -d not-a-date invalid-date', 1)
    assert not os.path.exists('invalid-date')
    run('touch -r missing target', 1)
    assert not os.path.exists('target')
    run('touch -d @1234567890 stamp')
    assert os.stat('stamp').st_mtime_ns == 1234567890000000000
    os.utime('stamp', ns=(1111111111222333444, 1234567890555666777))
    run('touch -r stamp reference')
    st = os.stat('reference')
    assert (st.st_atime_ns, st.st_mtime_ns) == (1111111111222333444, 1234567890555666777)
    run('touch -a -d @1400000000 reference')
    st = os.stat('reference')
    assert (st.st_atime_ns, st.st_mtime_ns) == (1400000000000000000, 1234567890555666777)
    run('touch -m -t 202402030405.06 reference')
    st = os.stat('reference')
    assert (st.st_atime_ns, st.st_mtime_ns) == (1400000000000000000, 1706933106000000000)
    os.symlink('reference', 'stamp-link')
    run('touch -h -d @1500000000 stamp-link')
    assert os.lstat('stamp-link').st_mtime_ns == 1500000000000000000
    assert os.stat('reference').st_mtime_ns == 1706933106000000000
    run('touch -d @1600000000 stamp-link')
    assert os.stat('reference').st_atime_ns == 1600000000000000000
    assert os.stat('reference').st_mtime_ns == 1600000000000000000
    assert os.lstat('stamp-link').st_mtime_ns == 1500000000000000000
    os.symlink('missing-target', 'dangling-stamp')
    run('touch -h -d @1700000000 dangling-stamp')
    assert not os.path.exists('missing-target')
    assert os.lstat('dangling-stamp').st_mtime_ns == 1700000000000000000
    run('touch -d @1700000001 dangling-stamp')
    assert os.stat('missing-target').st_mtime_ns == 1700000001000000000
    run('touch no-parent/file after-error', 1)
    assert os.path.isfile('after-error')
    run('touch -d @1700000002 .')
    assert os.stat('.').st_mtime_ns == 1700000002000000000
    os.chdir('/')
    with open('mutation-applets-result', 'w') as output:
        output.write('UPSTREAM_MUTATION_APPLETS_OK\n')
    print('UPSTREAM_MUTATION_APPLETS_OK')
''')
