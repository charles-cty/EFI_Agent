# /// script
# requires-python = ">=3.11"
# ///
"""Run consecutive shell and JavaScript requests in live WASM Stores."""
import argparse
import json
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
from wasm_namespace_fixture import DirectoryWorkspace
from wasm_sessions_fixture import SESSION_STEPS, CLEAR_STEPS, verify_sessions
from wasm_zx_fixture import ZX_CHECK
from wasm_bash_fixture import BASH_COMPAT_CHECK


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('runner', 'quickjs', 'ash'):
        parser.add_argument('--' + name, type=Path, required=True)
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix='efi-sessions-') as temporary:
        root = Path(temporary)
        work = root / 'work'
        work.mkdir()
        shutil.copyfile(args.quickjs, work / 'quickjs.wasm')
        shutil.copyfile(args.ash, work / 'ash.wasm')
        steps = [(language, source) for language, source, _ in SESSION_STEPS]
        steps += [('clear', ''), *[(language, source) for language, source, _ in CLEAR_STEPS]]
        steps += [('cancel-shell', "echo SESSION_CANCEL_READY; sleep 100"),
                  ('shell', 'test "$(pwd)" = /; echo SHELL_CANCEL_RESET_OK'),
                  ('cancel-js', "if (survivor !== 71) throw Error('Lost survivor'); print('SESSION_CANCEL_READY'); await ash`sleep 100`;"),
                  ('js', "if (typeof survivor !== 'undefined') throw Error('Cancel retained state'); print('JS_CANCEL_RESET_OK');")]
        steps += [('shell', BASH_COMPAT_CHECK), ('js', ZX_CHECK),
                  ('shell', 'export JS_RESET_KEEP=retained; cd /tmp/session-new; exec 7> /reset-fd; printf first >&7'),
                  ('reset_js', ''), ('reset_js', ''),
                  ('js', "if (typeof checkZx !== 'undefined') throw Error('Reset kept globals'); print('JS_EXPLICIT_RESET_OK');"),
                  ('shell', 'test "$JS_RESET_KEEP" = retained && test "$(pwd)" = /tmp/session-new || exit 81; printf second >&7; echo SHELL_SURVIVED_JS_RESET')]
        repl_start = len(steps)
        steps += [
            ('js', 'var replCounter = 0; let sharedValue = 40;'),
            ('repl-js', 'sharedValue + 2'),
            ('repl-js', '++replCounter'),
            ('js', "if (replCounter !== 1) throw Error('Repeated evaluation'); sharedValue += 1;"),
            ('repl-js', 'await Promise.resolve(sharedValue + 1)'),
            ('repl-js', '({ answer: sharedValue + 1, nested: [1, 2] })'),
            ('repl-js', 'let replCycle = {}; replCycle.self = replCycle; replCycle;'),
            ('repl-js', 'let getterCalls = 0; ({ get value() { getterCalls++; return 0; } })'),
            ('js', "if (getterCalls !== 0) throw Error('Inspector invoked getter'); print('REPL_GETTER_OK');"),
            ('repl-js', "throw Error('REPL_EXPECTED_ERROR')"),
            ('repl-js', 'replCounter + sharedValue'),
            ('repl-shell', 'export REPL_SHARED=yes; cd /tmp/session-new; printf shared > /repl-file'),
            ('shell', 'test "$REPL_SHARED" = yes && test "$(pwd)" = /tmp/session-new || exit 90; echo REPL_SHELL_SHARED'),
            ('repl-shell', '/bin/cat /repl-file'),
            ('repl-js', "/hello/.test('hello')"),
            ('repl-js', '(await ash`printf awaited`).stdout'),
            ('repl-js', '123n'),
            ('repl-js', 'undefined'),
            ('repl-js', 'quit(3)'),
            ('repl-js', 'typeof sharedValue'),
            ('repl-shell', 'exit 4'),
            ('repl-shell', 'test "${REPL_SHARED-unset}" = unset || exit 91; echo REPL_SHELL_FRESH'),
            ('js', "print('MODEL_OUTPUT_ONLY'); 650731"),
            ('repl-shell', 'REPL_ERROR_KEEP=preserved'),
            ('repl-shell', 'if then'),
            ('repl-shell', 'test "$REPL_ERROR_KEEP" = preserved || exit 93; echo REPL_SYNTAX_RECOVERED'),
            ('repl-shell', "trap 'printf SHELL_EOF_TRAP > /repl-eof-trap' EXIT; (exit 7)"),
            ('eof-shell', ''),
            ('repl-shell', 'test "${REPL_ERROR_KEEP-unset}" = unset && test "$(cat /repl-eof-trap)" = SHELL_EOF_TRAP || exit 94; echo REPL_EOF_FRESH'),
            ('eof-js', ''),
            ('repl-js', 'typeof sharedValue'),
        ]
        command = [str(args.runner.resolve()), str(work), '--sessions']
        for index, (language, source) in enumerate(steps):
            path = root / f'source-{index}.txt'
            path.write_text(source, encoding='utf-8', newline='\n')
            command.extend([language, str(path)])
        result = subprocess.run(command, capture_output=True, timeout=240)
        text = result.stdout.decode('utf-8', 'replace')
        errors = result.stderr.decode('utf-8', 'replace')
        print(text, end='', flush=True)
        print(errors, end='', flush=True)
        assert result.returncode == 0, result
        results = {int(index): (int(pid), int(code), reset == 'true') for index, pid, code, reset in
                   re.findall(r'SESSION_RESULT (\d+) pid=(\d+) code=(-?\d+) reset=(true|false)', text)}
        assert len(results) == len(steps) - 3, results
        assert results[0][1:] == (1, False), results
        assert results[0][0] == results[2][0] == results[4][0] == results[7][0], results
        assert results[1][0] == results[3][0] == results[5][0] == results[6][0] == results[9][0], results
        assert results[7][1:] == (7, True) and results[8][0] != results[7][0], results
        assert results[9][1:] == (9, True) and results[10][0] != results[9][0], results
        for index in (5, 11, 13, 15, 16):
            assert results[index][1:] == (1, False), (index, results)
        for index in (1, 3, 6, 10, 12, 14, 17, 18, 20, 22):
            assert results[index][1:] == (0, False), (index, results)
        assert results[19][1:] == (1, True), results
        assert results[19][0] != results[20][0], results
        assert results[21][1:] == (1, True), results
        assert results[21][0] != results[22][0], results
        for _, _, marker in SESSION_STEPS + CLEAR_STEPS:
            if not marker.startswith('Exit code:') and marker != 'Session reset.':
                assert marker in text + errors, marker
        assert 'RETURN_DID_NOT_STOP' not in text
        assert 'STALE_REQUEST_RESUMED' not in text
        assert 'SHELL_CANCEL_RESET_OK' in text and 'JS_CANCEL_RESET_OK' in text
        assert 'ZX_PORT_VERIFIED' in text and 'JS_EXPLICIT_RESET_OK' in text
        assert 'ZX_DEFAULT_STDOUT' not in text and 'ZX_DEFAULT_STDOUT' not in errors
        assert 'ZX_DEFAULT_STDERR' in errors
        assert 'ZX_VERBOSE_STDOUT' in errors and 'ZX_VERBOSE_STDERR' in errors
        assert '$ printf ZX_VERBOSE_STDOUT; printf ZX_VERBOSE_STDERR >&2' in errors
        assert 'ZX_QUIET_STDERR' not in errors
        assert 'SHELL_SURVIVED_JS_RESET' in text
        assert 'ASH_BASH_COMPAT_VERIFIED' in text
        for index in range(repl_start - 7, repl_start):
            if steps[index][0] != 'reset_js':
                assert results[index][1:] == (0, False), (index, results)
        assert DirectoryWorkspace(work).read('reset-fd') == b'firstsecond'
        segments = {}
        start = 0
        for match in re.finditer(r'SESSION_RESULT (\d+) pid=\d+ code=-?\d+ reset=(?:true|false)', text):
            segments[int(match[1])] = text[start:match.start()].strip()
            start = match.end()
        for offset, expected in [(0, ''), (1, '42'), (2, '1'), (3, ''), (4, '42'),
                                 (10, '42'), (13, 'shared'), (14, 'true'),
                                 (16, '123n'), (17, 'undefined'), (22, 'MODEL_OUTPUT_ONLY')]:
            assert segments[repl_start + offset] == expected, (offset, segments[repl_start + offset])
        assert re.search(r'answer: 42\b', segments[repl_start + 5])
        assert 'self:' in segments[repl_start + 6]
        assert 'awaited' in segments[repl_start + 15]
        assert 'undefined' in segments[repl_start + 19]
        assert '650731' not in text
        assert 'REPL_EXPECTED_ERROR' in errors
        assert 'REPL_GETTER_OK' in text and 'REPL_SHELL_SHARED' in text and 'REPL_SHELL_FRESH' in text
        assert results[repl_start + 9][1:] == (1, False)
        assert results[repl_start + 18][1:] == (3, True)
        assert results[repl_start + 20][1:] == (4, True)
        for offset in range(18):
            if steps[repl_start + offset][0] in ('js', 'repl-js'):
                assert results[repl_start + offset][0] == results[repl_start][0]
        assert results[repl_start + 11][0] == results[repl_start + 12][0] == results[repl_start + 13][0]
        assert results[repl_start + 19][0] != results[repl_start + 18][0]
        assert results[repl_start + 21][0] != results[repl_start + 20][0]
        assert results[repl_start + 24][1:] == (2, False)
        assert results[repl_start + 23][0] == results[repl_start + 24][0] == results[repl_start + 25][0]
        assert 'REPL_SYNTAX_RECOVERED' in text
        assert results[repl_start + 26][1:] == (7, False)
        assert results[repl_start + 27][1:] == (7, True)
        assert results[repl_start + 28][0] != results[repl_start + 27][0]
        assert results[repl_start + 29][1:] == (0, True)
        assert results[repl_start + 30][0] != results[repl_start + 29][0]
        assert json.loads(segments[repl_start + 30]) == 'undefined'
        assert 'REPL_EOF_FRESH' in text
        verify_sessions(DirectoryWorkspace(work))
        print('PASS live sessions, shared REPL state/value display, await/rejections, finite stdin, error/quit/clear/cancel and saved bytes')


if __name__ == '__main__':
    main()
