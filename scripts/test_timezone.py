# /// script
# requires-python = ">=3.11"
# ///
"""Check every packaged TZif and date against independent Python zoneinfo."""
import argparse
import datetime
import io
import json
from pathlib import Path
import random
import shutil
import struct
import subprocess
import tempfile
from zoneinfo import ZoneInfo

from wasm_namespace_fixture import DirectoryWorkspace


def run(args, root, command):
    result = subprocess.run([str(args.runner.resolve()), str(root)] + command,
                            capture_output=True, timeout=300)
    print(result.stdout.decode('utf-8', 'replace'), end='', flush=True)
    assert (result.returncode, result.stderr) == (0, b''), result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('runner', 'guests', 'probe'):
        parser.add_argument('--' + name, type=Path, required=True)
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix='efi-timezone-') as temporary:
        root = Path(temporary)
        shutil.copyfile(args.probe, root / 'timezone_probe.wasm')
        for name in ('ash', 'python'):
            shutil.copyfile(args.guests / (name + '.wasm'), root / (name + '.wasm'))
        shutil.copytree(args.guests / 'lib', root / 'lib')
        shutil.copytree(args.guests / 'share', root / 'share')
        database = args.guests / 'share/zoneinfo'
        zones = sorted(path.relative_to(database).as_posix() for path in database.rglob('*')
                       if path.is_file() and path.read_bytes()[:4] == b'TZif')

        valid = b'TZif\0' + bytes(15) + struct.pack('>6I', 0, 0, 0, 0, 1, 4)
        valid += struct.pack('>iBB', 0, 0, 0) + b'UTC\0'
        (root / 'valid-v1').write_bytes(valid)
        invalid = [valid[:length] for length in (0, 1, 4, 43, 44, len(valid) - 1)]
        for position, value in ((36, 0), (36, 257), (32, 0xffffffff), (40, 0xffffffff)):
            data = bytearray(valid)
            struct.pack_into('>I', data, position, value)
            invalid.append(data)
        for position, value in ((48, 2), (49, 4), (53, ord('X'))):
            data = bytearray(valid)
            data[position] = value
            invalid.append(data)
        full = (database / 'America/New_York').read_bytes()
        invalid.extend([full[:-1], full + b'junk', full[:-2] + b'M\n',
                        b'TZif2' + bytes(1024 * 1024)])
        for index, data in enumerate(invalid):
            (root / f'invalid-{index}').write_bytes(data)

        # Python's ZoneInfo parser supplies offsets, names and historical data
        # independently of the C rules engine and its input validator.
        rng = random.Random(20261024)
        selected = ['America/New_York', 'Europe/London', 'Australia/Lord_Howe',
                    'Asia/Kathmandu', 'Pacific/Chatham', 'Africa/Casablanca']
        epochs = [-2208988800, -1, 0, 2147483647, 2147483648, 2524608000]
        epochs += [rng.randrange(-2208988800, 4133980800) for _ in range(10)]
        cases = []
        for name in selected:
            zone = ZoneInfo.from_file(io.BytesIO((database / name).read_bytes()))
            for epoch in epochs:
                utc = datetime.datetime(1970, 1, 1, tzinfo=datetime.timezone.utc)
                value = (utc + datetime.timedelta(seconds=epoch)).astimezone(zone)
                displacement = int(value.utcoffset().total_seconds())
                hours, minutes = divmod(abs(displacement) // 60, 60)
                # C strftime %z uses HHMM; Python additionally prints historical
                # offset seconds. Keep those seconds in the civil-time oracle.
                offset = '%s%02d%02d' % ('+' if displacement >= 0 else '-', hours, minutes)
                expected = value.strftime('%Y-%m-%d_%H:%M:%S_') + offset + '_' + value.tzname() + '\n'
                cases.append([name, epoch, expected])
        source = '''import json, subprocess
cases = json.loads(%r)
for name, epoch, expected in cases:
    result = subprocess.run(['date', '-d', '@' + str(epoch), '+%%Y-%%m-%%d_%%H:%%M:%%S_%%z_%%Z'], env={'TZ': name}, capture_output=True)
    assert (result.returncode, result.stdout, result.stderr) == (0, expected.encode(), b''), (name, epoch, result)
with open('timezone-result', 'wb') as target:
    target.write(b'TIMEZONE_ORACLE_OK\\n')
print('TIMEZONE_ORACLE_OK', len(cases))
''' % json.dumps(cases)
        # Stage all fixtures before namespace initialization.
        (root / 'check.py').write_text(source, encoding='utf-8')
        run(args, root, ['timezone_probe', 'valid', 'valid-v1'] +
            ['share/zoneinfo/' + name for name in zones])
        run(args, root, ['timezone_probe', 'invalid'] +
            [f'invalid-{index}' for index in range(len(invalid))])
        run(args, root, ['python', 'check.py'])
        assert DirectoryWorkspace(root).read('timezone-result') == b'TIMEZONE_ORACLE_OK\n'
        print('PASS packaged TZif validation, malformed inputs and independent date oracles')


if __name__ == '__main__':
    main()
