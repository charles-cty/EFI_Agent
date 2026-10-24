"""Exercise upstream du against logical and measured allocation oracles."""
from textwrap import dedent


DU_CHECK = dedent(r'''
    import json, os, stat, struct, subprocess

    def run(arguments, expected=None, status=0):
        result = subprocess.run(['du'] + arguments, capture_output=True)
        assert result.returncode == status, (arguments, result)
        if status:
            assert result.stderr, (arguments, result)
        else:
            assert not result.stderr, (arguments, result)
        if expected is not None:
            assert result.stdout == expected, (arguments, result, expected)
        print('PASS upstream du', repr(arguments), flush=True)
        return result.stdout

    def records(data):
        result = {}
        for line in data.decode().splitlines():
            count, name = line.split('\t', 1)
            assert name not in result, data
            result[name] = int(count)
        return result

    def write(path, data):
        with open(path, 'wb') as output:
            output.write(data)

    os.makedirs('du-work/tree/sub')
    write('du-work/tree/small', b'abc')
    write('du-work/tree/sub/large', bytes(range(251)) * 261 + b'end')
    write('du-work/tree/sub/empty', b'')
    os.symlink('sub/large', 'du-work/tree/link')
    os.symlink('sub', 'du-work/tree/dir-link')
    tree = 'du-work/tree'
    large = tree + '/sub/large'
    size = 251 * 261 + 3
    run(['-b', large], f'{size}\t{large}\n'.encode())
    assert os.stat(tree).st_nlink == 3 and os.stat(tree + '/sub').st_nlink == 2
    apparent = 3 + size + len('sub/large') + len('sub')
    run(['-bs', tree], f'{apparent}\t{tree}\n'.encode())
    actual = os.stat(tree + '/small').st_blocks + os.stat(large).st_blocks
    run(['-ks', tree], f'{(actual + 1) // 2}\t{tree}\n'.encode())
    run(['-sk', tree], f'{(actual + 1) // 2}\t{tree}\n'.encode())
    details = records(run(['-ab', tree]))
    assert details == {
        tree + '/small': 3, large: size, tree + '/sub/empty': 0,
        tree + '/sub': size, tree + '/link': 9, tree + '/dir-link': 3,
        tree: apparent,
    }, details
    assert records(run(['-bd', '1', tree])) == {tree + '/sub': size, tree: apparent}
    assert records(run(['-bd', '0', tree])) == {tree: apparent}
    run(['-bH', tree + '/link'], f'{size}\t{tree}/link\n'.encode())
    run(['-bL', tree + '/link'], f'{size}\t{tree}/link\n'.encode())
    run(['-b', tree + '/link'], f'9\t{tree}/link\n'.encode())
    run(['-bHs', tree + '/dir-link'], f'{size}\t{tree}/dir-link\n'.encode())
    totals = records(run(['-bc', tree + '/small', large]))
    assert totals == {tree + '/small': 3, large: size, 'total': size + 3}, totals
    run(['-ms', large], f'{(os.stat(large).st_blocks + 2047) // 2048}\t{large}\n'.encode())
    human = run(['-h', large])
    assert human.endswith(('\t' + large + '\n').encode()) and b'K' in human, human
    os.symlink('..', tree + '/sub/back')
    # Repeated directory identities are skipped by the upstream inode table.
    followed = records(run(['-bLs', tree + '/sub']))
    assert followed == {tree + '/sub': size * 2 + 3}, followed
    os.unlink(tree + '/sub/back')
    os.symlink('absent', 'du-work/dangling')
    run(['-b', 'du-work/dangling'], b'6\tdu-work/dangling\n')
    run(['-bL', 'du-work/dangling'], status=1)
    run(['du-work/missing'], status=1)
    run(['-d', '-1', tree], status=1)
    run(['-Q'], status=1)
    os.mkdir('du-work/denied', 0o000)
    run(['du-work/denied'], status=1)
    os.chmod('du-work/denied', 0o700)
    result = subprocess.run('du -s du-work/tree >&-', shell=True, capture_output=True)
    assert result.returncode and result.stderr, result
    print('PASS du closed output', flush=True)

    measured = {}
    for count in (0, 1, 511, 512, 513, 4095, 4096, 4097,
                  65535, 65536, 65537, 524287, 524288, 524289):
        name = 'du-work/boundary-' + str(count)
        write(name, (bytes(range(251)) * (count // 251 + 1))[:count])
        info = os.stat(name)
        with open(name, 'rb') as source:
            assert os.fstat(source.fileno()).st_blocks == info.st_blocks
        physical = int(run(['-k', name]).split(b'\t', 1)[0])
        logical = int(run(['-b', name]).split(b'\t', 1)[0])
        assert logical == count and physical == (info.st_blocks + 1) // 2
        measured[name] = [count, info.st_blocks, physical]
    entries = {entry.name: entry.stat(follow_symlinks=False) for entry in os.scandir('du-work')}
    for name, (count, blocks, _) in measured.items():
        assert entries[name.split('/')[-1]].st_blocks == blocks
    if os.path.exists('host-sparse'):
        info = os.stat('host-sparse')
        assert info.st_size == 8 * 1024 * 1024 + 1
        assert info.st_blocks * 512 < info.st_size // 2, info
        sparse_du = int(run(['-k', 'host-sparse']).split(b'\t', 1)[0])
        run(['-b', 'host-sparse'], f'{info.st_size}\thost-sparse\n'.encode())
        measured['host-sparse'] = [info.st_size, info.st_blocks, sparse_du]
    write('du-work/measurements.json', json.dumps(measured).encode())
    # Repeated stat calls must close every private allocation-reply descriptor.
    for _ in range(160):
        os.stat(large)
    with open(large, 'rb') as source:
        os.unlink(large)
        detached = os.fstat(source.fileno())
        assert detached.st_nlink == 0
        assert detached.st_blocks == actual - os.stat(tree + '/small').st_blocks
    write('du-result', b'UPSTREAM_DU_OK\n')
    print('UPSTREAM_DU_OK', flush=True)
''')
