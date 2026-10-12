# /// script
# requires-python = ">=3.11"
# ///
"""Integration tests for actual CPython and ash in the native wasmi runner."""
import argparse
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest


class CPythonIntegration(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.directory = tempfile.TemporaryDirectory(prefix="efi-cpython-")
        cls.tree = Path(cls.directory.name)
        shutil.copyfile(cls.options.python, cls.tree / "python.wasm")
        shutil.copyfile(cls.options.ash, cls.tree / "ash.wasm")
        (cls.tree / "probe.txt").write_bytes(b"SHELL_CAN_READ\n")
        (cls.tree / "external.py").write_text("raise RuntimeError('File import executed')\n", encoding="utf-8")

    @classmethod
    def tearDownClass(cls):
        cls.directory.cleanup()

    def execute(self, code, status=0):
        result = subprocess.run(
            [str(self.options.runner.resolve()), str(self.tree), "python", "-e", code],
            capture_output=True, timeout=120,
        )
        self.assertEqual(result.returncode, status, result.stderr.decode("utf-8", "replace"))
        if status == 0:
            self.assertEqual(result.stderr, b"")
        return result

    def test_actual_release_interpreter_and_language(self):
        result = self.execute("""
import sys, math
assert sys.version_info[:3] == (3, 14, 0)
assert sys.version_info.releaselevel == 'final'
def sum_squares(values):
    return sum(value * value for value in values)
assert sum_squares(range(11)) == 385
assert math.gcd(84, 30) == 6
print('CPYTHON_ENGINE_OK', '你好', 2 ** 130)
""")
        self.assertEqual(result.stdout, f"CPYTHON_ENGINE_OK 你好 {2 ** 130}\n".encode())

    def test_shell_raw_syntax_and_binary_capture(self):
        result = self.execute(r"""
import efi
r = efi.shell(r"printf 'a\000\377'; printf 'error' >&2")
assert r.status == 0
assert r.stdout_bytes == b'a\0\xff'
assert r.stdout == 'a\0\ufffd'
assert r.stderr_bytes == b'error'
print('BINARY_CAPTURE_OK')
""")
        self.assertEqual(result.stdout, b"BINARY_CAPTURE_OK\n")

    def test_word_quoting_preserves_asymmetric_arguments(self):
        words = ["", "a b", "a'b", "$(echo INJECTED)", "semi;colon", "line\nbreak", "你好"]
        code = rf"""
import efi
words = {words!r}
r = efi.shell(['printf', r'<%s>\n', *words])
assert r.stdout == ''.join('<' + word + '>\n' for word in words)
assert r.stderr == ''
print('QUOTING_OK')
"""
        self.assertEqual(self.execute(code).stdout, b"QUOTING_OK\n")

    def test_exit_status_default_error_and_unchecked_result(self):
        result = self.execute("""
import efi
try:
    efi.shell("printf failure >&2; exit 17")
except efi.ShellError as error:
    assert error.result.status == 17
    assert error.result.stdout_bytes == b''
    assert error.result.stderr_bytes == b'failure'
else:
    raise AssertionError('Nonzero exit was accepted')
r = efi.shell('exit 23', check=False)
assert r.status == 23 and r.stdout == '' and r.stderr == ''
assert efi.shell('printf recovered').stdout == 'recovered'
print('EXIT_RECOVERY_OK')
""")
        self.assertEqual(result.stdout, b"EXIT_RECOVERY_OK\n")

    def test_repeated_fresh_shells_and_pipeline(self):
        result = self.execute(r"""
import efi
for index in range(37):
    assert efi.shell(['printf', '%s', index]).stdout == str(index)
assert efi.shell('secret=child; printf "$secret"').stdout == 'child'
assert efi.shell('printf "${secret-unset}"').stdout == 'unset'
assert efi.shell(r"printf 'pipe data\n' | read v; echo done").stdout == 'done\n'
print('FRESH_SHELLS_OK')
""")
        self.assertEqual(result.stdout, b"FRESH_SHELLS_OK\n")

    def test_invalid_commands_fail_before_spawn_and_recover(self):
        result = self.execute(r"""
import efi
for command in ['bad\0text', 'x' * 65521, '界' * 21841, ['bad\0word']]:
    try:
        efi.shell(command)
    except ValueError:
        pass
    else:
        raise AssertionError('Invalid command accepted')
try:
    efi.shell(17)
except TypeError:
    pass
else:
    raise AssertionError('Invalid command type accepted')
assert efi.shell('echo valid').stdout == 'valid\n'
print('ARGUMENT_ERRORS_OK')
""")
        self.assertEqual(result.stdout, b"ARGUMENT_ERRORS_OK\n")

    def test_capture_limit_drains_child_and_recovers(self):
        result = self.execute("""
import efi
limit = 512 * 1024
r = efi.shell("printf '%0524288d' 7; printf tail >&2")
assert r.stdout_bytes == b'0' * (limit - 1) + b'7'
assert r.stderr_bytes == b'tail'
for redirect in ['', '>&2']:
    try:
        efi.shell("printf '%0524289d' 9 " + redirect)
    except RuntimeError as error:
        assert 'Shell capture failed' in str(error)
    else:
        raise AssertionError('Oversized capture accepted')
    assert efi.shell('printf recovered').stdout == 'recovered'
print('CAPTURE_BOUNDARY_OK')
""")
        self.assertEqual(result.stdout, b"CAPTURE_BOUNDARY_OK\n")

    def test_host_modules_and_file_interfaces_are_absent(self):
        result = self.execute("""
import sys, builtins, _io, efi
assert sys.path == []
assert len(sys.meta_path) == 2
assert {finder.__name__ for finder in sys.meta_path} == {'BuiltinImporter', 'FrozenImporter'}
sys.path.append('.')
for name in ['posix', 'os', 'time', '_datetime', '_signal', 'faulthandler',
             'socket', '_socket', 'select', 'subprocess', 'ctypes', 'external']:
    try:
        __import__(name)
    except ImportError:
        pass
    else:
        raise AssertionError('Host module available: ' + name)
for name in ['open', 'input', 'breakpoint']:
    assert not hasattr(builtins, name)
for target in ['probe.txt', -1, 3, 40, True]:
    try:
        _io.FileIO(target)
    except PermissionError:
        pass
    else:
        raise AssertionError('File interface available')
for operation in [_io.open, _io.open_code]:
    try:
        operation('probe.txt')
    except PermissionError:
        pass
    else:
        raise AssertionError('Alternate file interface available')
def opener(path, flags):
    raise AssertionError('Custom opener ran')
try:
    _io.FileIO(1, opener=opener)
except PermissionError:
    pass
else:
    raise AssertionError('Custom opener accepted')
r = efi.shell('IFS= read -r v<probe.txt; printf "%s" "$v"')
assert r.stdout == 'SHELL_CAN_READ'
print('RESTRICTED_HOST_OK')
""")
        self.assertEqual(result.stdout, b"RESTRICTED_HOST_OK\n")

    def test_multiple_python_instances_share_world_without_module_state(self):
        children = []
        for label, number in [('alpha', 3), ('beta', 7)]:
            children.append(f"""
import efi
assert not hasattr(efi, 'instance_marker')
efi.instance_marker = {label!r}
for index in range({number}):
    r = efi.shell(['printf', '%s', {label!r} + str(index)])
    assert r.stdout == {label!r} + str(index)
assert efi.instance_marker == {label!r}
efi.shell(['ash', '-c', 'printf "%s" "$1" > {label}.txt', 'ash', {label!r}])
print({label!r})
""")
        result = self.execute(f"""
import efi
efi.instance_marker = 'parent'
children = {children!r}
command = ' & '.join('python -e ' + efi.quote(code) for code in children) + ' & wait'
r = efi.shell(command)
assert sorted(r.stdout.splitlines()) == ['alpha', 'beta']
assert r.stderr == ''
assert efi.instance_marker == 'parent'
print('MULTIPLE_PYTHON_INSTANCES_OK')
""")
        self.assertEqual(result.stdout, b"MULTIPLE_PYTHON_INSTANCES_OK\n")
        for label in ('alpha', 'beta'):
            self.assertEqual((self.tree / f"{label}.txt").read_bytes(), label.encode())

    def test_uncaught_python_error_is_reported(self):
        result = self.execute("raise ValueError('EXPECTED_PYTHON_ERROR')", status=1)
        self.assertIn(b"ValueError: EXPECTED_PYTHON_ERROR", result.stderr)
        self.assertEqual(result.stdout, b"")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("runner", "python", "ash"):
        parser.add_argument(f"--{name}", type=Path, required=True)
    parser.add_argument("--test", action="append", help="Run only the named test method; repeat to select more.")
    CPythonIntegration.options = parser.parse_args()
    if CPythonIntegration.options.test:
        suite = unittest.defaultTestLoader.loadTestsFromNames(
            [f"__main__.CPythonIntegration.{name}" for name in CPythonIntegration.options.test])
    else:
        suite = unittest.defaultTestLoader.loadTestsFromTestCase(CPythonIntegration)
    result = unittest.TextTestRunner(verbosity=2).run(suite)
    raise SystemExit(0 if result.wasSuccessful() else 1)


if __name__ == "__main__":
    main()
