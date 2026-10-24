# /// script
# requires-python = ">=3.11"
# ///
"""Run consecutive shell and Python requests in live WASM interpreter Stores."""
import argparse
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
from wasm_namespace_fixture import DirectoryWorkspace
from wasm_sessions_fixture import SESSION_STEPS, CLEAR_STEPS, verify_sessions


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('runner', 'python', 'ash'):
        parser.add_argument('--' + name, type=Path, required=True)
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix='efi-sessions-') as temporary:
        root = Path(temporary)
        work = root / 'work'
        work.mkdir()
        shutil.copyfile(args.python, work / 'python.wasm')
        shutil.copyfile(args.ash, work / 'ash.wasm')
        shutil.copytree(args.python.parent / 'lib', work / 'lib')
        steps = [(language, source) for language, source, _ in SESSION_STEPS]
        steps += [('clear', ''), *[(language, source) for language, source, _ in CLEAR_STEPS]]
        steps += [('cancel-shell', "echo SESSION_CANCEL_READY; sleep 100"),
                  ('shell', 'test "$(pwd)" = /; echo SHELL_CANCEL_RESET_OK'),
                  ('cancel-python', "assert survivor == 71; import subprocess; print('SESSION_CANCEL_READY', flush=True); subprocess.run(['sleep', '100'])"),
                  ('python', "assert 'subprocess' not in globals(); print('PYTHON_CANCEL_RESET_OK')")]
        command = [str(args.runner.resolve()), str(work), '--sessions']
        for index, (language, source) in enumerate(steps):
            path = root / f'source-{index}.txt'
            path.write_text(source, encoding='utf-8', newline='\n')
            command.extend([language, str(path)])
        result = subprocess.run(command, capture_output=True, timeout=240)
        print(result.stdout.decode('utf-8', 'replace'), end='', flush=True)
        print(result.stderr.decode('utf-8', 'replace'), end='', flush=True)
        assert result.returncode == 0, result
        text = result.stdout.decode('utf-8')
        results = {int(index): (int(pid), int(code), reset == 'true') for index, pid, code, reset in
                   re.findall(r'SESSION_RESULT (\d+) pid=(\d+) code=(-?\d+) reset=(true|false)', text)}
        assert results[0][1:] == (1, False), results
        assert results[0][0] == results[2][0] == results[4][0] == results[7][0], results
        assert results[1][0] == results[3][0] == results[5][0] == results[6][0] == results[9][0], results
        assert results[5][1:] == (1, False), results
        assert results[7][1:] == (7, True) and results[8][0] != results[7][0], results
        assert results[9][1:] == (9, True) and results[10][0] != results[9][0], results
        for _, _, marker in SESSION_STEPS + CLEAR_STEPS:
            if not marker.startswith('Exit code:') and marker != 'Session reset.':
                assert marker in text or marker in result.stderr.decode('utf-8'), marker
        assert 'RETURN_DID_NOT_STOP' not in text
        assert 'SHELL_CANCEL_RESET_OK' in text and 'PYTHON_CANCEL_RESET_OK' in text
        verify_sessions(DirectoryWorkspace(work))
        print('PASS live state, anchored cwd, private descriptors, finite stdin, exception/exit/clear/cancel and saved bytes')


if __name__ == '__main__':
    main()
