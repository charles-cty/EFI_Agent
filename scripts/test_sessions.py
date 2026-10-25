# /// script
# requires-python = ">=3.11"
# ///
"""Run consecutive shell and JavaScript requests in live WASM Stores."""
import argparse
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
                  ('cancel-js', "if (survivor !== 71) throw Error('Lost survivor'); print('SESSION_CANCEL_READY'); await $`sleep 100`;"),
                  ('js', "if (typeof survivor !== 'undefined') throw Error('Cancel retained state'); print('JS_CANCEL_RESET_OK');")]
        steps += [('shell', BASH_COMPAT_CHECK), ('js', ZX_CHECK),
                  ('shell', 'export JS_RESET_KEEP=retained; cd /tmp/session-new; exec 7> /reset-fd; printf first >&7'),
                  ('reset_js', ''), ('reset_js', ''),
                  ('js', "if (typeof checkZx !== 'undefined') throw Error('Reset kept globals'); print('JS_EXPLICIT_RESET_OK');"),
                  ('shell', 'test "$JS_RESET_KEEP" = retained && test "$(pwd)" = /tmp/session-new || exit 81; printf second >&7; echo SHELL_SURVIVED_JS_RESET')]
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
        assert 'SHELL_SURVIVED_JS_RESET' in text
        assert 'ASH_BASH_COMPAT_VERIFIED' in text
        for index in range(len(steps) - 7, len(steps)):
            if steps[index][0] != 'reset_js':
                assert results[index][1:] == (0, False), (index, results)
        assert DirectoryWorkspace(work).read('reset-fd') == b'firstsecond'
        verify_sessions(DirectoryWorkspace(work))
        print('PASS live JS bindings/closures, await/rejections, shell state, finite stdin, error/quit/clear/cancel and saved bytes')


if __name__ == '__main__':
    main()
