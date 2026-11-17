#!/usr/bin/env python3
"""Run byte-identical test executables from a private, small directory on macOS.

Opt in through Cargo's target runner when a large target/deps directory stalls
macOS executable validation. Arguments, environment, streams and exit code pass
through. This neither signs executables nor changes host security settings.
"""
import hashlib
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile


def main():
    source = Path(sys.argv[1]).resolve(strict=True)
    with tempfile.TemporaryDirectory(prefix='xone-native-test-') as directory:
        target = Path(directory) / source.name
        with source.open('rb') as stream:
            before = hashlib.file_digest(stream, 'sha256').hexdigest()
        shutil.copy2(source, target)
        with target.open('rb') as stream:
            copied = hashlib.file_digest(stream, 'sha256').hexdigest()
        with source.open('rb') as stream:
            after = hashlib.file_digest(stream, 'sha256').hexdigest()
        if before != copied or before != after:
            raise RuntimeError('Native test executable changed during staging')
        print(f'XONE_STAGED_NATIVE sha256={copied} name={source.name}', file=sys.stderr, flush=True)
        return subprocess.call([str(target), *sys.argv[2:]])


if __name__ == '__main__':
    sys.exit(main())
