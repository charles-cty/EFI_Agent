# /// script
# requires-python = ">=3.11"
# dependencies = ["tzdata==2026.5"]
# ///
"""Stage the upstream musl timezone engine and IANA data for WASI BusyBox."""
import argparse
from importlib import metadata
from pathlib import Path
import re
import shutil

import tzdata


def replace_once(text, old, new):
    if text.count(old) != 1:
        raise ValueError(f"Unexpected upstream timezone source: {old!r}")
    return text.replace(old, new)


def prepare(musl, output):
    build = output / 'ash-build'
    build.mkdir(parents=True, exist_ok=True)
    text = (musl / 'src/time/__tz.c').read_text(encoding='utf-8')
    start = text.index('#include "time_impl.h"')
    end = text.index('long  __timezone')
    text = text[:start] + '''/* Upstream musl timezone rules, with bounded WASI file buffers. */
#include <time.h>
#include <stdint.h>
#include <limits.h>
#include <stdlib.h>
#include <string.h>
#include <ctype.h>
#include "efi_timezone.h"

#define weak_alias(old, new) \\
    extern __typeof(old) new __attribute__((weak, alias(#old)))
#define LOCK(value) ((void)(value))
#define UNLOCK(value) ((void)(value))
#define __munmap efi_timezone_unmap
#define __map_file efi_timezone_map
#define getenv efi_timezone_setting

long long __year_to_secs(long long, int *);
int __month_to_secs(int, int);
extern const char __utc[];

''' + text[end:]
    text = replace_once(text, 'const char __utc[] = "UTC";', '')
    text = replace_once(text, '!libc.secure || !strcmp(s, "/etc/localtime")', '1')
    # Version-1 TZif uses a NUL version byte.
    text = replace_once(text, "if (map[4]!='1')", "if (map[4] && map[4]!='1')")
    # _GNU_SOURCE exposes the historical string.h index() function in the SDK.
    text = re.sub(r'\bindex\b', 'transition_index', text)
    (build / 'timezone.c').write_text(text, encoding='utf-8')
    shutil.copyfile(musl / 'COPYRIGHT', output / 'musl.COPYRIGHT')

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
    print(f'Prepared musl timezone engine and IANA {tzdata.IANA_VERSION} data')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--musl', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    prepare(args.musl, args.output)
