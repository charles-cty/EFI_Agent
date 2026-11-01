"""Fetch fixed upstream sources into ignored build caches."""
import argparse
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import urllib.request


REPOSITORY = Path(__file__).resolve().parents[1]
SOURCES = {
    'jq': ('jq-1.8.1',
           'https://github.com/jqlang/jq/releases/download/jq-1.8.1/jq-1.8.1.tar.gz',
           'configure'),
    'curl': ('curl-8.22.0', 'https://curl.se/download/curl-8.22.0.tar.xz', 'configure'),
    'wolfssl': ('wolfssl-5.9.4-stable',
                'https://github.com/wolfSSL/wolfssl/archive/refs/tags/v5.9.4-stable.tar.gz',
                'CMakeLists.txt'),
    'wolfssh': ('wolfssh-1.6.0-stable',
                'https://github.com/wolfSSL/wolfssh/archive/refs/tags/v1.6.0-stable.tar.gz',
                'src/ssh.c'),
}


def download(url, destination):
    destination.parent.mkdir(parents=True, exist_ok=True)
    print(f'Download {url}', file=sys.stderr, flush=True)
    with urllib.request.urlopen(url, timeout=120) as response:
        with destination.open('wb') as target:
            shutil.copyfileobj(response, target)


def fetch_file(url, destination):
    if destination.is_file():
        return destination
    destination.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='.download-', dir=destination.parent) as temporary:
        staged = Path(temporary) / destination.name
        download(url, staged)
        staged.replace(destination)
    return destination


def fetch_source(component, source=None):
    name, url, marker = SOURCES[component]
    source = (source or REPOSITORY / 'artifacts/guest-sources' / name).resolve()
    if source.exists():
        if not (source / marker).is_file():
            raise ValueError(f'{component} source directory has no {marker}: {source}')
        return source
    source.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix=f'.{component}-fetch-', dir=source.parent) as temporary:
        staging = Path(temporary)
        archive = staging / url.rsplit('/', 1)[1]
        download(url, archive)
        tar = 'tar.exe' if os.name == 'nt' else 'tar'
        subprocess.run([tar, '-xf', str(archive), '-C', str(staging)], check=True)
        (staging / name).rename(source)
    return source


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('component', choices=SOURCES)
    parser.add_argument('--source', type=Path)
    options = parser.parse_args()
    print(fetch_source(options.component, options.source))
