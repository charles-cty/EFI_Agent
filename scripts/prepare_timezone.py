# /// script
# requires-python = ">=3.11"
# dependencies = ["tzdata==2026.5"]
# ///
"""Stage IANA timezone data and licenses for the vendored BusyBox port."""
import argparse
from importlib import metadata
from pathlib import Path
import shutil

import tzdata


def prepare(output):
    output.mkdir(parents=True, exist_ok=True)
    repository = Path(__file__).resolve().parents[1]
    shutil.copyfile(repository / 'vendor/musl-timezone/COPYRIGHT', output / 'musl.COPYRIGHT')
    database = output / 'share/zoneinfo'
    if database.exists():
        shutil.rmtree(database)
    source = Path(tzdata.__file__).parent / 'zoneinfo'
    # Ship TZif and zone tables, without Python package scaffolding or caches.
    shutil.copytree(source, database,
                    ignore=shutil.ignore_patterns('*.py', '*.pyc', '__pycache__'))
    distribution = metadata.distribution('tzdata')
    licenses = [entry for entry in distribution.files
                if entry.name in ('LICENSE', 'LICENSE_APACHE')]
    if len(licenses) != 2:
        raise ValueError('The tzdata package must supply both license files')
    with (output / 'tzdata.LICENSE').open('wb') as target:
        for entry in sorted(licenses):
            target.write(distribution.locate_file(entry).read_bytes() + b'\n')
    print(f'Prepared IANA {tzdata.IANA_VERSION} data and timezone licenses')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    prepare(args.output)
