"""Exercise manifest-installed commands and independently inspect saved assets."""
import json

INSTALLATION_CHECK = r'''
import json, os, subprocess
with open('/programs.json') as source:
    manifest = json.load(source)
assert manifest['version'] == 1
commands = []
for module in manifest['modules']:
    assert 'licenses' not in module, module
    assert os.path.isfile('/' + module['path']), module
    assert os.stat('/' + module['path']).st_mode & 0o777 == 0o755, module
    for name in module['commands']:
        path = '/bin/' + name
        assert os.path.islink(path) and os.path.isfile(path), path
        assert os.path.samefile(path, '/' + module['path']), path
        assert os.stat(path).st_mode & 0o777 == 0o555, path
        commands.append(name)
    for path in module['libraries']:
        assert os.path.exists('/' + path), path
assert not os.path.exists('/curl.LICENSE')
assert not os.path.exists('/licenses')
result = subprocess.run(['ash', '-c', 'for name do command -v "$name" || exit; done',
                         'manifest-check'] + commands, capture_output=True)
assert result.returncode == 0 and len(result.stdout.splitlines()) == len(commands), result
assert not result.stderr, result
result = subprocess.run(['qjs', '-e', 'print("INSTALLED_QJS_OK")'], capture_output=True)
assert (result.returncode, result.stdout, result.stderr) == (0, b'INSTALLED_QJS_OK\n', b''), result
result = subprocess.run(['jq', '-cn', '{installed:true, value:([1,2,3]|add)}'],
                        capture_output=True)
assert result.returncode == 0 and not result.stderr, result
assert json.loads(result.stdout) == {'installed': True, 'value': 6}, result
result = subprocess.run(['curl', '--version'], capture_output=True)
assert result.returncode == 0 and result.stdout.startswith(b'curl ') and not result.stderr, result
with open('/installed-curl-version', 'wb') as output:
    output.write(result.stdout)
result = subprocess.run(['wolfssh'], capture_output=True)
assert result.returncode == 2 and b'Usage: wolfssh' in result.stderr, result
result = subprocess.run(['python', '-c', 'print(sum(range(10)))'], capture_output=True)
assert (result.returncode, result.stdout, result.stderr) == (0, b'45\n', b''), result
for link, target in [('python3', 'python'), ('bash', 'ash')]:
    path = '/bin/' + link
    assert os.path.islink(path), path
    module = '/python.wasm' if target == 'python' else '/ash.wasm'
    assert os.readlink(path) == module, path
    assert os.path.samefile(path, '/bin/' + target), path
    assert os.stat(path).st_mode & 0o777 == 0o555, path
    with open(path, 'rb') as source, open('/bin/' + target, 'rb') as original:
        while True:
            block = source.read(65536)
            assert block == original.read(65536), path
            if not block:
                break
    result = subprocess.run(['readlink', path], capture_output=True)
    assert (result.returncode, result.stdout, result.stderr) == (
        0, (module + '\n').encode(), b''), result
for command in ['python3', '/bin/python3']:
    result = subprocess.run([command, '-c', 'print(6 * 7)'], capture_output=True)
    assert (result.returncode, result.stdout, result.stderr) == (0, b'42\n', b''), result
for command in ['bash', '/bin/bash']:
    result = subprocess.run([command, '-c', 'python3 -c "print(6 * 7)"'], capture_output=True)
    assert (result.returncode, result.stdout, result.stderr) == (0, b'42\n', b''), result
for name, interpreter, body in [
        ('python3', '/bin/python3', 'print(6 * 7)'),
        ('bash', '/bin/bash', 'printf "42\\n"'),
        ('env-python3', '/bin/env python3', 'print(6 * 7)'),
        ('env-bash', '/bin/env bash', 'printf "42\\n"')]:
    path = '/tmp/installation-' + name
    with open(path, 'w') as output:
        output.write('#!' + interpreter + '\n' + body + '\n')
    os.chmod(path, 0o755)
    result = subprocess.run([path], capture_output=True)
    assert (result.returncode, result.stdout, result.stderr) == (0, b'42\n', b''), result
    os.unlink(path)
assert os.path.samefile('/bin/ls', '/bin/cat')
assert os.path.samefile('/bin/qjs', '/bin/quickjs')
assert not os.path.exists('/bin/node') and not os.path.exists('/bin/git')
with open('/installation-result', 'wb') as output:
    output.write(b'INSTALLATION_OK\n')
print('INSTALLATION_OK')
'''


def verify_installation(workspace, prepared):
    expected = (prepared / 'programs.json').read_bytes()
    assert workspace.read('programs.json') == expected
    manifest = json.loads(expected)
    for module in manifest['modules']:
        assert workspace.read(module['path']) == (prepared / module['path']).read_bytes()
        assert workspace.entries[module['path']]['mode'] == 0o755
        for library in module['libraries']:
            root = prepared / library
            for source in root.rglob('*'):
                path = source.relative_to(prepared).as_posix()
                if source.is_file():
                    assert workspace.read(path) == source.read_bytes(), path
                else:
                    assert workspace.entries[path]['kind'] == 'Directory', path
    for notice in (prepared / 'licenses').iterdir():
        assert notice.name not in workspace.entries, notice.name
    assert 'licenses' not in workspace.entries
    assert workspace.read('installation-result') == b'INSTALLATION_OK\n'
    assert workspace.read('installed-curl-version').startswith(b'curl ')
    assert workspace.read('lib/python3.14/site-packages/sitecustomize.py') == (
        prepared / 'lib/python3.14/site-packages/sitecustomize.py').read_bytes()
    assert not any(path.startswith(('bin/', 'ash-build/', 'curl-build/', 'wolfssl/'))
                   for path in workspace.entries)
