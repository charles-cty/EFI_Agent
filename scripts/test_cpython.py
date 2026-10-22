# /// script
# requires-python = ">=3.11"
# ///
"""Integration tests for upstream CPython subprocess and cooperative WASM processes."""
import argparse
from pathlib import Path
import re
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
        shutil.copytree(cls.options.python.parent / "lib", cls.tree / "lib")
        shutil.copyfile(cls.options.ash, cls.tree / "ash.wasm")
        (cls.tree / "probe.txt").write_bytes(b"FILE_CAN_READ\n")
        (cls.tree / "external.py").write_text("value = 731\n", encoding="utf-8")
        (cls.tree / "script.py").write_text("import external; print(external.value)\n", encoding="utf-8")

    @classmethod
    def tearDownClass(cls):
        cls.directory.cleanup()

    def execute(self, code, status=0):
        result = subprocess.run(
            [str(self.options.runner.resolve()), str(self.tree), "python", "-c", code],
            capture_output=True, timeout=120,
        )
        self.assertEqual(result.returncode, status, result.stderr.decode("utf-8", "replace"))
        if status == 0:
            self.assertEqual(result.stderr, b"")
        return result

    def test_actual_release_standard_files_and_imports(self):
        result = self.execute("""
import sys, math, os, external
assert sys.version_info[:3] == (3, 14, 7)
assert sum(x*x for x in range(11)) == 385 and math.gcd(84, 30) == 6
assert external.value == 731
with open('probe.txt') as source:
    assert source.read() == 'FILE_CAN_READ\\n'
with open('python-file.bin', 'wb') as target:
    target.write(b'a\\0\\xff')
assert os.stat('python-file.bin').st_size == 3
assert 'probe.txt' in os.listdir('.')
try:
    import efi
except ImportError:
    pass
else:
    raise AssertionError('Obsolete public efi module is present')
print('STANDARD_PYTHON_OK')
""")
        self.assertEqual(result.stdout, b"STANDARD_PYTHON_OK\n")
        self.assertEqual((self.tree / "python-file.bin").read_bytes(), b"a\0\xff")

    def test_site_initialization_and_positional_timeouts(self):
        for flags, code in [
            (["-S"], "import sys;assert '_efi_process' not in sys.modules;print('SITE_OK')"),
            (["-I"], "import subprocess;assert subprocess.check_output('printf SITE_OK',shell=True)==b'SITE_OK';print('SITE_OK')"),
        ]:
            result = subprocess.run(
                [str(self.options.runner.resolve()), str(self.tree), "python", *flags, "-c", code],
                capture_output=True, timeout=120,
            )
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(result.stdout, b"SITE_OK\n")
            self.assertEqual(result.stderr, b"")
        result = self.execute("""
import subprocess, errno, decimal
assert decimal.Decimal('1.25') + decimal.Decimal('2.5') == decimal.Decimal('3.75')
p = subprocess.Popen(['ash', '-c', 'exit 0'])
for function, args in [(p.wait, (1,)), (p.communicate, (None, 1))]:
    try:
        function(*args)
    except OSError as error:
        assert error.errno == errno.ENOTSUP
    else:
        raise AssertionError('Positional timeout was accepted')
assert p.wait() == 0
print('SITE_TIMEOUT_OK')
""")
        self.assertEqual(result.stdout, b"SITE_TIMEOUT_OK\n")

    def test_binary_capture_and_standard_text_decoding(self):
        result = self.execute(r"""
import subprocess
r = subprocess.run(r"printf 'a\000\377'; printf error >&2", shell=True, capture_output=True)
assert isinstance(r, subprocess.CompletedProcess)
assert r.returncode == 0 and r.stdout == b'a\0\xff' and r.stderr == b'error'
r = subprocess.run("printf 'left\\r\\nright\\r'", shell=True, capture_output=True, text=True)
assert r.stdout == 'left\nright\n'
try:
    subprocess.check_output(r"printf '\377'", shell=True, text=True)
except UnicodeDecodeError:
    pass
else:
    raise AssertionError('Default decoding did not report invalid UTF-8')
assert subprocess.check_output(r"printf '\377'", shell=True, text=True, errors='replace') == '\ufffd'
print('BINARY_TEXT_OK')
""")
        self.assertEqual(result.stdout, b"BINARY_TEXT_OK\n")

    def test_argument_lists_are_not_shell_text(self):
        words = ["", "a b", "a'b", "$(echo INJECTED)", "semi;colon", "line\nbreak", "你好"]
        result = self.execute(rf"""
import subprocess
words = {words!r}
r = subprocess.run(['ash', '-c', 'printf "<%s>\\n" "$@"', 'ash', *words], capture_output=True, text=True, check=True)
assert r.stdout == ''.join('<' + word + '>\n' for word in words), repr(r.stdout)
assert r.stderr == ''
try:
    subprocess.run('ash -c echo', capture_output=True)
except FileNotFoundError:
    pass
else:
    raise AssertionError('shell=False interpreted shell syntax')
print('ARGUMENTS_OK')
""")
        self.assertEqual(result.stdout, b"ARGUMENTS_OK\n")

    def test_status_errors_system_and_recovery(self):
        result = self.execute("""
import subprocess, os
r = subprocess.run('printf failure >&2; exit 17', shell=True, capture_output=True)
assert r.returncode == 17 and r.stdout == b'' and r.stderr == b'failure'
try:
    r.check_returncode()
except subprocess.CalledProcessError as error:
    assert error.returncode == 17 and error.stderr == b'failure'
else:
    raise AssertionError('check_returncode accepted nonzero status')
try:
    subprocess.run('exit 23', shell=True, check=True)
except subprocess.CalledProcessError as error:
    assert error.returncode == 23
else:
    raise AssertionError('check=True accepted nonzero status')
assert os.system('exit 37') == 37 << 8
assert os.waitstatus_to_exitcode(os.system('exit 255')) == 255
assert subprocess.getstatusoutput('printf merged; printf error >&2; exit 19') == (19, 'mergederror')
assert subprocess.check_output('printf recovered', shell=True) == b'recovered'
print('STATUS_OK')
""")
        self.assertEqual(result.stdout, b"STATUS_OK\n")

    def test_large_duplex_pipes_and_capture_above_old_limit(self):
        result = self.execute(r"""
import subprocess
size = 196613
data = bytes((i * 37 + i // 251 + 19) % 256 for i in range(size))
code = "import sys;data=sys.stdin.buffer.read();sys.stderr.buffer.write(data[::-1]);sys.stdout.buffer.write(data)"
r = subprocess.run(['python', '-c', code], input=data, capture_output=True, check=True)
assert r.stdout == data and r.stderr == data[::-1]
size = 512 * 1024 + 1
assert subprocess.check_output("printf '%0524289d' 9", shell=True) == b'0' * (size - 1) + b'9'
print('DUPLEX_LARGE_OK')
""")
        self.assertEqual(result.stdout, b"DUPLEX_LARGE_OK\n")

    def test_environment_inheritance_and_explicit_environment(self):
        result = self.execute(r"""
import subprocess, os
os.environ['PORT_VALUE'] = "a b='你好"
assert subprocess.check_output('printf %s "$PORT_VALUE"', shell=True, text=True) == os.environ['PORT_VALUE']
r = subprocess.run(['ash', '-c', 'printf "%s|%s" "$PORT_VALUE" "${OTHER-unset}"'],
                   env={'PORT_VALUE': 'child=value', 'OTHER': ''}, capture_output=True, text=True)
assert r.stdout == 'child=value|'
assert subprocess.check_output('printf %s "${PORT_VALUE-unset}"', shell=True, env={}, text=True) == 'unset'
assert os.environ['PORT_VALUE'] == "a b='你好"
print('ENVIRONMENT_OK')
""")
        self.assertEqual(result.stdout, b"ENVIRONMENT_OK\n")

    def test_popen_concurrency_and_descriptor_redirection(self):
        result = self.execute(r"""
import subprocess, os
children = [subprocess.Popen(['python', '-c', "import sys;print(sys.argv[1])", label],
                            stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
            for label in ('alpha', 'beta')]
assert [p.communicate() for p in children] == [('alpha\n', ''), ('beta\n', '')]
assert [p.wait() for p in children] == [0, 0]
with open('redirected.bin', 'wb') as target:
    subprocess.run('printf output; printf error >&2', shell=True, stdout=target, stderr=subprocess.STDOUT, check=True)
with open('redirected.bin', 'rb') as source:
    assert subprocess.run(['python', '-c', 'import sys;sys.stdout.buffer.write(sys.stdin.buffer.read())'],
                          stdin=source, stdout=subprocess.PIPE, check=True).stdout == b'outputerror'
with open('passed.txt', 'w') as target:
    subprocess.run(['ash', '-c', 'printf mapped >&' + str(target.fileno())], pass_fds=(target.fileno(),), check=True)
for index in range(140):
    assert subprocess.check_output('printf ok', shell=True) == b'ok'
print('POPEN_DESCRIPTORS_OK')
""")
        self.assertEqual(result.stdout, b"POPEN_DESCRIPTORS_OK\n")
        self.assertEqual((self.tree / "passed.txt").read_bytes(), b"mapped")

    def test_errors_are_reported_before_effects(self):
        result = self.execute(r"""
import subprocess, errno, os
for args in [['ash', '-c', 'bad\0text'], ['ash', '-c', 'x' * 65536]]:
    try:
        subprocess.run(args, capture_output=True)
    except (ValueError, OSError):
        pass
    else:
        raise AssertionError('Invalid arguments were accepted')
for options in [{'cwd': 'missing'}, {'preexec_fn': lambda: None}, {'start_new_session': True},
                {'timeout': 1}, {'executable': 'ash'}]:
    try:
        if 'executable' in options:
            subprocess.run(['custom-argv0', '-c', 'printf wrong>unexpected.txt'], **options)
        else:
            subprocess.run('printf wrong>unexpected.txt', shell=True, **options)
    except OSError as error:
        assert error.errno == errno.ENOTSUP
    else:
        raise AssertionError('Unsupported process option was accepted')
assert not os.path.exists('unexpected.txt')
for function in (subprocess.call, subprocess.check_call, subprocess.check_output):
    try:
        function('printf wrong>unexpected.txt', shell=True, timeout=1)
    except OSError as error:
        assert error.errno == errno.ENOTSUP
    else:
        raise AssertionError('Unsupported timeout was accepted')
assert not os.path.exists('unexpected.txt')
try:
    subprocess.run(['missing-program-731'])
except FileNotFoundError:
    pass
else:
    raise AssertionError('Missing program accepted')
assert subprocess.check_output('printf recovered', shell=True) == b'recovered'
print('ERRORS_OK')
""")
        self.assertEqual(result.stdout, b"ERRORS_OK\n")

    def test_descriptor_boundaries_and_system_inheritance(self):
        result = self.execute(r"""
import subprocess, os, errno
with open('system-inherited.txt', 'w') as target:
    os.set_inheritable(target.fileno(), True)
    assert os.system('printf inherited >&' + str(target.fileno())) == 0
with open('last-descriptor.txt', 'w') as target:
    os.dup2(target.fileno(), 127)
    try:
        subprocess.run(['python', '-c', 'import os;os.write(127,b"edge")'], pass_fds=(127,), check=True)
    finally:
        os.close(127)
for descriptor in (3, 128):
    try:
        subprocess.run('printf wrong>unexpected-fd.txt', shell=True, pass_fds=(descriptor,))
    except OSError as error:
        assert error.errno == errno.EBADF
    else:
        raise AssertionError('Reserved or out-of-range descriptor accepted')
assert not os.path.exists('unexpected-fd.txt')
os.close(0)
assert subprocess.check_output('read word; printf %s "$word"', shell=True, input=b'closed-stdin\n') == b'closed-stdin'
print('DESCRIPTOR_BOUNDARIES_OK')
""")
        self.assertEqual(result.stdout, b"DESCRIPTOR_BOUNDARIES_OK\n")
        self.assertEqual((self.tree / "system-inherited.txt").read_bytes(), b"inherited")
        self.assertEqual((self.tree / "last-descriptor.txt").read_bytes(), b"edge")

    def test_standard_script_filename(self):
        for arguments in (["script.py"], ["-m", "script"]):
            with self.subTest(arguments=arguments):
                result = subprocess.run(
                    [str(self.options.runner.resolve()), str(self.tree), "python", *arguments],
                    capture_output=True, timeout=120,
                )
                self.assertEqual((result.returncode, result.stdout, result.stderr), (0, b"731\n", b""))

    def test_nested_directories_large_writes_and_bytecode_cache(self):
        result = self.execute("""
import os, sys, py_compile, compileall
os.makedirs('nested/space dir/cache')
pattern = bytes(range(256))
for size in (65535, 65536, 65537, 524287, 524288, 524289, 8 * 1024 * 1024):
    payload = (pattern * ((size + 255) // 256))[:size]
    path = 'nested/space dir/data-' + str(size)
    fd = os.open(path, os.O_WRONLY | os.O_CREAT)
    assert os.write(fd, payload) == size
    os.close(fd)
    assert open(path, 'rb').read() == payload
with open('nested/space dir/cache/module.py', 'w') as target:
    target.write('value = 731\\n')
compiled = py_compile.compile('nested/space dir/cache/module.py', doraise=True)
assert os.path.isfile(compiled)
assert compileall.compile_dir('nested', quiet=2)
assert os.path.isfile('nested/space dir/cache/__pycache__/module.cpython-314.pyc')
print('DIRECTORIES_WRITES_CACHE_OK')
""")
        self.assertEqual(result.stdout, b"DIRECTORIES_WRITES_CACHE_OK\n")
        data = (self.tree / 'nested/space dir/data-8388608').read_bytes()
        self.assertEqual(data, bytes(range(256)) * 32768)

    def test_finite_stdin_executes_as_script(self):
        result = self.execute("""
import os, subprocess, sys
assert not os.isatty(0) and not os.isatty(1) and not os.isatty(2)
for source, expected in [(b'', b''), (b'print(731)\\n', b'731\\n')]:
    result = subprocess.run(['python', '-'], input=source, capture_output=True)
    assert (result.returncode, result.stdout, result.stderr) == (0, expected, b''), result
result = subprocess.run(['ash', '-c', "python - <<'PY'\\nprint(193)\\nPY\\n"], capture_output=True)
assert (result.returncode, result.stdout, result.stderr) == (0, b'193\\n', b''), result
print('FINITE_STDIN_OK')
""")
        self.assertEqual(result.stdout, b"FINITE_STDIN_OK\n")

    def test_uncaught_python_error_is_reported(self):
        result = self.execute("raise ValueError('EXPECTED_PYTHON_ERROR')", status=1)
        diagnostic = re.sub(rb"\x1b\[[0-9;]*m", b"", result.stderr)
        self.assertIn(b"ValueError: EXPECTED_PYTHON_ERROR", diagnostic)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("runner", "python", "ash"):
        parser.add_argument(f"--{name}", type=Path, required=True)
    parser.add_argument("--test", action="append")
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
