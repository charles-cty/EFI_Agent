# /// script
# requires-python = ">=3.11"
# ///
"""Check configured root defaults, retained sessions and explicit child environments."""
import argparse
import json
from pathlib import Path
import re
import subprocess
import tempfile
import unittest


class GuestEnvironmentIntegration(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.temporary = tempfile.TemporaryDirectory(prefix='efi-environment-')
        cls.root = Path(cls.temporary.name)
        cls.work = cls.root / 'work'
        cls.work.mkdir()
        subprocess.run([str(cls.options.launcher), 'install', str(cls.options.prepared),
                        str(cls.work), '--guest'], check=True, timeout=120)

    @classmethod
    def tearDownClass(cls):
        cls.temporary.cleanup()

    def configuration(self, environment):
        path = self.root / 'config.json'
        path.write_text(json.dumps({'api_base': 'https://example.test/v1', 'api_key': 'test',
                                    'model': 'test', 'environment': environment}), encoding='utf-8')
        return ['--config', str(path)]

    def run_guest(self, *arguments):
        result = subprocess.run([str(self.options.runner), str(self.work), *arguments],
                                capture_output=True, timeout=180)
        self.assertEqual(result.returncode, 0, result.stderr.decode('utf-8', 'replace'))
        return result

    def test_default_reaches_direct_python_jq_and_basic_repl(self):
        result = self.run_guest('python', '-c',
                                'import os; print(os.environ["PYTHON_BASIC_REPL"]); '
                                'import _pyrepl.main; _pyrepl.main.interactive_console(); '
                                'print("BASIC_REPL_EOF_OK")')
        self.assertEqual((result.stdout, result.stderr), (b'1\nBASIC_REPL_EOF_OK\n', b''))
        result = self.run_guest('jq', '-nr', 'env.PYTHON_BASIC_REPL')
        self.assertEqual((result.stdout, result.stderr), (b'1\n', b''))

    def test_configured_values_and_explicit_child_replacement(self):
        config = self.configuration({'CUSTOM': 'configured α=λ'})
        source = r'''
import os, subprocess
assert os.environ['CUSTOM'] == 'configured α=λ'
assert os.environ['PYTHON_BASIC_REPL'] == '1'
code = "import os; print(repr(os.environ.get('PYTHON_BASIC_REPL')), repr(os.environ.get('CUSTOM')))"
cases = [({}, b'None None\n'), ({'CUSTOM': 'child'}, b"None 'child'\n")]
for environment, expected in cases:
    result = subprocess.run(['python', '-c', code], env=environment, capture_output=True)
    assert (result.returncode, result.stdout, result.stderr) == (0, expected, b''), result
print('EXPLICIT_ENVIRONMENT_OK')
'''
        result = self.run_guest(*config, 'python', '-c', source)
        self.assertEqual((result.stdout, result.stderr), (b'EXPLICIT_ENVIRONMENT_OK\n', b''))
        result = self.run_guest(*config, 'ash', '-c',
                                "env -i python -c 'import os; print(os.environ.get(\"PYTHON_BASIC_REPL\"))'")
        self.assertEqual((result.stdout, result.stderr), (b'None\n', b''))

    def test_empty_override_restores_warning_behavior(self):
        config = self.configuration({'PYTHON_BASIC_REPL': ''})
        result = self.run_guest(*config, 'python', '-c',
                                'import os; assert os.environ["PYTHON_BASIC_REPL"] == ""; '
                                'import _pyrepl.main; _pyrepl.main.interactive_console()')
        self.assertIn(b"warning: can't use pyrepl:", result.stderr)

    def test_sessions_preserve_changes_and_reset_to_configured_defaults(self):
        config = self.configuration({'CUSTOM': 'configured'})
        steps = [
            ('shell', 'test "$PYTHON_BASIC_REPL:$CUSTOM" = "1:configured" || exit 81; '
             'unset PYTHON_BASIC_REPL; export CUSTOM=changed; echo INITIAL_ENVIRONMENT_OK'),
            ('shell', 'test "${PYTHON_BASIC_REPL-unset}:$CUSTOM" = "unset:changed" || exit 82; '
             'python -c \'import os; assert "PYTHON_BASIC_REPL" not in os.environ; '
             'assert os.environ["CUSTOM"] == "changed"; print("INHERITED_CHANGE_OK")\''),
            ('js', 'var result = await ash`printf "%s:%s" "$PYTHON_BASIC_REPL" "$CUSTOM"`; '
             'if (result.stdout !== "1:configured") throw Error(result.stdout); '
             'print("JS_CONFIGURED_ENVIRONMENT_OK");'),
            ('clear', ''),
            ('repl-shell', 'test "$PYTHON_BASIC_REPL:$CUSTOM" = "1:configured" || exit 83; '
             'echo RESET_ENVIRONMENT_OK'),
        ]
        arguments = [*config, '--sessions']
        for index, (kind, source) in enumerate(steps):
            path = self.root / f'source-{index}.txt'
            path.write_text(source, encoding='utf-8')
            arguments.extend([kind, str(path)])
        result = self.run_guest(*arguments)
        self.assertEqual(result.stderr, b'')
        for marker in [b'INITIAL_ENVIRONMENT_OK', b'INHERITED_CHANGE_OK',
                       b'JS_CONFIGURED_ENVIRONMENT_OK', b'RESET_ENVIRONMENT_OK']:
            self.assertIn(marker, result.stdout)
        statuses = re.findall(rb'SESSION_RESULT (\d+) pid=\d+ code=(-?\d+) reset=(true|false)',
                              result.stdout)
        self.assertEqual(statuses, [(str(index).encode(), b'0', b'false')
                                    for index in (0, 1, 2, 4)])


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('runner', 'launcher', 'prepared'):
        parser.add_argument('--' + name, type=Path, required=True)
    args, remaining = parser.parse_known_args()
    for name in ('runner', 'launcher', 'prepared'):
        setattr(args, name, getattr(args, name).resolve())
    GuestEnvironmentIntegration.options = args
    unittest.main(argv=['test_guest_environment', *remaining])


if __name__ == '__main__':
    main()
