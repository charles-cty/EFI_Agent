# /// script
# requires-python = ">=3.11"
# ///
"""Exercise upstream stream applets through real ash and independent byte oracles."""
import argparse
from pathlib import Path
import random
import shutil
import subprocess
import tempfile
from wasm_namespace_fixture import DirectoryWorkspace


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--runner', type=Path, required=True)
    parser.add_argument('--ash', type=Path, required=True)
    parser.add_argument('--case', action='append', help='Run only named cases')
    args = parser.parse_args()
    lines = b''.join(f'line-{index:02}\n'.encode() for index in range(1, 23))
    chunks = lines.splitlines(keepends=True)
    binary_lines = b'a\0b\nsecond\nlast\0tail'
    payload = (bytes(range(256)) * 2050)[:524289]
    fixtures = {'lines': lines, 'partial': b'one\ntwo\nlast', 'empty': b'',
                'binary-lines': binary_lines, 'binary': payload,
                'a': b'A\n', 'b': b'B\n', 'plain': b'abcdef',
                'tabs': b'a\tb\n12\n', 'long-line': b'x' * 1100003,
                'numbered': b'first\n\nlast\n', 'visible': b'a\t\0\177\377\n',
                'nul-numbered': b'a\0b\n\nlast\0tail',
                'wc-lines': b'a b\n\tcc\nlast'}
    # Expected outputs are derived from input bytes and simple index/count
    # operations. They do not call the guest applets under test.
    cases = [
        ('cat-raw', 'cat binary', payload, 0),
        ('cat-concat', "printf IN | cat a - b", b'A\nINB\n', 0),
        ('cat-stdin', "printf 'a\\000b' | cat", b'a\0b', 0),
        ('cat-continues', 'cat a missing b', b'A\nB\n', 1),
        ('cat-number', 'cat -n numbered', b'     1\tfirst\n     2\t\n     3\tlast\n', 0),
        ('cat-nonblank', 'cat -bn numbered', b'     1\tfirst\n\n     2\tlast\n', 0),
        ('cat-number-partial', 'cat -n partial', b'     1\tone\n     2\ttwo\n     3\tlast', 0),
        ('cat-number-binary', 'cat -b nul-numbered', b'     1\ta\0b\n\n     2\tlast\0tail', 0),
        ('cat-number-files', 'cat -n a b', b'     1\tA\n     2\tB\n', 0),
        ('cat-number-continued-files', 'cat -n partial a', b'     1\tone\n     2\ttwo\n     3\tlastA\n', 0),
        ('cat-number-read-error-continues', 'cat -n directory a', b'     1\tA\n', 1),
        ('cat-number-repeated-stdin', "printf 'a\\nb' | cat -n - -", b'     1\ta\n     2\tb', 0),
        ('cat-visible', 'cat -A visible', b'a^I^@^?M-^?$\n', 0),
        ('head-stdin', 'cat lines | head', b''.join(chunks[:10]), 0),
        ('head-lines', 'head -n 2 lines', b''.join(chunks[:2]), 0),
        ('head-zero', 'head -n 0 lines', b'', 0),
        ('head-bytes', 'head -c 65537 binary', payload[:65537], 0),
        ('head-byte-suffix', 'head -c 1k binary', payload[:1024], 0),
        ('head-except-bytes', 'head -c -65537 binary', payload[:-65537], 0),
        ('head-except-lines', 'head -n -2 partial', b'one\n', 0),
        ('head-except-zero', 'head -n -0 partial', fixtures['partial'], 0),
        ('head-except-binary-lines', 'head -n -1 binary-lines', b'a\0b\nsecond\n', 0),
        ('head-except-too-many', 'head -n -10 partial', b'', 0),
        ('head-quiet', 'head -q -n 1 a b', b'A\nB\n', 0),
        ('head-verbose', 'head -v -n 1 a', b'==> a <==\nA\n', 0),
        ('head-headers', 'head -n 1 a b', b'==> a <==\nA\n\n==> b <==\nB\n', 0),
        ('tail-stdin', 'cat lines | tail', b''.join(chunks[-10:]), 0),
        ('tail-lines', 'tail -n 2 lines', b''.join(chunks[-2:]), 0),
        ('tail-zero', 'tail -n 0 lines', b'', 0),
        ('tail-partial', 'tail -n 2 partial', b'two\nlast', 0),
        ('tail-long-line', 'tail -n 1 long-line', fixtures['long-line'], 0),
        ('tail-bytes', 'tail -c 65537 binary', payload[-65537:], 0),
        ('tail-byte-stream', 'cat binary | tail -c 65537', payload[-65537:], 0),
        ('tail-byte-suffix', 'tail -c 1k binary', payload[-1024:], 0),
        ('tail-from-line', 'tail -n +2 lines', b''.join(chunks[1:]), 0),
        ('tail-from-zero', 'tail -n +0 partial', fixtures['partial'], 0),
        ('tail-from-byte', 'tail -c +2 binary', payload[1:], 0),
        ('tail-from-byte-zero', 'tail -c +0 binary', payload, 0),
        ('tail-huge-lines', 'tail -n 4294967295 partial', fixtures['partial'], 0),
        ('tail-huge-from-byte', 'tail -c +4294967295 partial', b'', 0),
        ('tail-huge-from-line', 'tail -n +4294967295 partial', b'', 0),
        ('head-reject-overflow', 'head -n -4294967295 partial', b'', 1),
        ('tail-huge-bytes-stream', 'cat partial | tail -c 4294967295', fixtures['partial'], 0),
        ('tail-quiet', 'tail -q -n 1 a b', b'A\nB\n', 0),
        ('tail-headers', 'tail -n 1 a b', b'==> a <==\nA\n\n==> b <==\nB\n', 0),
        ('tail-verbose', 'tail -v -n 1 a', b'==> a <==\nA\n', 0),
        ('wc-default', 'wc wc-lines', [2, 4, len(fixtures['wc-lines']), 'wc-lines'], 0),
        ('wc-stream', 'cat wc-lines | wc -l -w -c', [2, 4, len(fixtures['wc-lines'])], 0),
        ('wc-binary', 'wc -c binary', [len(payload), 'binary'], 0),
        ('wc-longest', 'wc -L tabs', [9, 'tabs'], 0),
        ('wc-empty', 'wc empty', [0, 0, 0, 'empty'], 0),
        ('wc-totals', 'wc -l a b', [1, 'a', 1, 'b', 2, 'total'], 0),
        ('wc-offset', 'exec 7<plain; read -n 3 value <&7; wc -c <&7', [3], 0),
        ('tee-files', "printf 'tee\\000data' | tee tee-a tee-b", b'tee\0data', 0),
        ('tee-append', "printf '+append' | tee -a tee-a", b'+append', 0),
        ('tee-continues', "printf keep | tee missing-parent/file tee-survivor", b'keep', 1),
        ('tee-dash', 'printf X | tee -', b'XX', 0),
        ('pipeline-break', 'yes | head -c 196613', (b'y\n' * 98307)[:196613], 0),
        ('tail-seek-error', 'tail -c 3 plain >&-', b'', 1),
        ('head-read-error', 'head directory', b'', 1),
        ('tee-output-error', 'printf data | tee >&-', b'', 1),
    ]
    for command in ('cat', 'head', 'tail', 'wc', 'tee'):
        cases.extend([
            (command + '-empty-stdin', command + ' < empty', [] if command == 'wc' else b'', 0),
            (command + '-closed-stdin', command + ' <&-', b'', 1),
            (command + '-bad-option', command + ' -Q', b'', 1),
        ])
    # wc emits zero counts for empty input and for a read error, with a
    # nonzero status in the latter case.
    cases = [(name, command, [0, 0, 0] if name in ('wc-empty-stdin', 'wc-closed-stdin') else expected, status)
             for name, command, expected, status in cases]
    generator = random.Random(731)
    for index in range(12):
        rows = [bytes(generator.choice((0, 9, 32, 65, 90, 128, 255))
                      for _ in range(generator.randrange(0, 80))) + b'\n'
                for _ in range(generator.randrange(1, 20))]
        if index % 2 and rows[-1] != b'\n':
            rows[-1] = rows[-1][:-1]
        path = f'generated-{index}'
        fixtures[path] = b''.join(rows)
        count = generator.choice((0, 1, len(rows) - 1, len(rows), len(rows) + 1))
        numbered = b''.join(f'{number:6}\t'.encode() + row
                            for number, row in enumerate(rows, 1))
        cases.extend([
            (f'generated-head-{index}', f'head -n -{count} {path}', b''.join(rows[:-count] if count else rows), 0),
            (f'generated-tail-{index}', f'tail -n {count} {path}', b''.join(rows[-count:] if count else []), 0),
            (f'generated-number-{index}', f'cat -n {path}', numbered, 0),
        ])
    with tempfile.TemporaryDirectory(prefix='efi-stream-applets-') as temporary:
        root = Path(temporary)
        shutil.copyfile(args.ash, root / 'ash.wasm')
        (root / 'directory').mkdir()
        for name, data in fixtures.items():
            (root / name).write_bytes(data)
        failures = []
        selected = set(args.case or [])
        assert not selected - {row[0] for row in cases}, selected
        for name, command, expected, status in cases:
            if selected and name not in selected:
                continue
            result = subprocess.run([str(args.runner.resolve()), str(root), 'ash', '-c', command],
                                    capture_output=True, timeout=120)
            actual = result.stdout
            if isinstance(expected, list):
                actual = [int(word) if word.isdigit() else word.decode() for word in actual.split()]
            error = None
            if actual != expected:
                error = f'output mismatch: length={len(result.stdout)}, tail={result.stdout[-120:]!r}'
            elif result.returncode != bool(status):
                error = f'host status={result.returncode}, wanted={bool(status)}'
            elif status and f'Guest exited with {status}'.encode() not in result.stderr:
                error = f'guest status mismatch: {result.stderr[-500:]!r}'
            elif not status and result.stderr != (b'yes: Broken pipe\n' if name == 'pipeline-break' else b''):
                error = f'unexpected stderr: {result.stderr[-500:]!r}'
            if error:
                failures.append(f'{name}: {error}')
                print(f'FAIL {failures[-1]}', flush=True)
            else:
                print(f'PASS upstream stream {name}', flush=True)
        if not selected:
            view = DirectoryWorkspace(root)
            for name, expected in [('tee-a', b'tee\0data+append'), ('tee-b', b'tee\0data'),
                                   ('tee-survivor', b'keep')]:
                if view.read(name) != expected:
                    failures.append(f'{name}: independent saved byte mismatch')
            print('PASS independent tee output files' if not failures else 'CHECK tee output files complete', flush=True)
        assert not failures, '\n'.join(failures)


if __name__ == '__main__':
    main()
