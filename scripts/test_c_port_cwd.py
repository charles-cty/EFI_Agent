# /// script
# requires-python = ">=3.11"
# dependencies = ["asyncssh>=2.21"]
# ///
"""Verify actual C applications, the common adapter and saved namespace bytes."""
import argparse
from pathlib import Path
import shutil
import subprocess
import tempfile

from test_wolfssh import SshPeer
from wasm_c_port_cwd_fixture import c_port_cwd_check, verify_c_port_cwd
from wasm_namespace_fixture import DirectoryWorkspace


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('runner', 'guests', 'probe'):
        parser.add_argument('--' + name, type=Path, required=True)
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix='efi-c-port-cwd-') as temporary, SshPeer() as peer:
        root = Path(temporary)
        for name in ('ash', 'python', 'quickjs', 'curl', 'wolfssh'):
            shutil.copyfile(args.guests / (name + '.wasm'), root / (name + '.wasm'))
        shutil.copyfile(args.probe, root / 'cwd_probe.wasm')
        shutil.copytree(args.guests / 'lib', root / 'lib')
        result = subprocess.run([str(args.runner.resolve()), str(root), 'python', '-c',
                                 c_port_cwd_check('127.0.0.1', peer.port, peer.pin)],
                                capture_output=True, timeout=300)
        print(result.stdout.decode('utf-8', 'replace'), end='', flush=True)
        assert result.returncode == 0 and not result.stderr, result.stderr.decode('utf-8', 'replace')
        assert peer.errors == [] and peer.commands == ['nonzero'], (peer.errors, peer.commands)
        assert peer.auth_attempts == [('guest', 'secret')], peer.auth_attempts
        verify_c_port_cwd(DirectoryWorkspace(root))
        print('PASS C adapter, curl, QuickJS, WolfSSH and independent saved cwd bytes')


if __name__ == '__main__':
    main()
