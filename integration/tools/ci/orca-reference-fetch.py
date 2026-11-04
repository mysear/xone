#!/usr/bin/env python3
"""Verify the exact archived Orca revision and its complete, unmodified source tree."""
import argparse
import hashlib
import io
import json
from pathlib import Path, PurePosixPath
import tarfile
import urllib.request

ROOT = Path(__file__).resolve().parents[3]
REFERENCE = ROOT / 'integration/tools/reference/orca-whirlpools'
REVISION = 'cc683ec92acb5e7606caa3d8e514d9383e9c7c11'
ARCHIVE_SHA256 = '9ddee363a1aec82d4130dd94deb52c78c659dff1e071d0fa7d56a1cbd8e450fb'
URL = f'https://codeload.github.com/orca-so/whirlpools/tar.gz/{REVISION}'

def digest(data):
    return hashlib.sha256(data).hexdigest()

def archive_files(data):
    assert digest(data) == ARCHIVE_SHA256, 'pinned archive hash mismatch'
    result = {}
    with tarfile.open(fileobj=io.BytesIO(data), mode='r:gz') as tar:
        for member in tar.getmembers():
            if member.isdir():
                continue
            assert member.isfile(), f'non-regular archive member: {member.name}'
            parts = PurePosixPath(member.name).parts[1:]
            assert parts and '..' not in parts and not PurePosixPath(*parts).is_absolute()
            path = str(PurePosixPath(*parts))
            assert path not in result
            result[path] = tar.extractfile(member).read()
    return result

def verify():
    manifest = json.loads((REFERENCE / 'source-metadata.json').read_text())
    assert manifest['repository'] == 'orca-so/whirlpools'
    assert manifest['revision'] == REVISION and manifest['archive_sha256'] == ARCHIVE_SHA256
    archive = archive_files((REFERENCE / 'upstream.tar.gz').read_bytes())
    actual = {str(p.relative_to(REFERENCE / 'upstream')) for p in (REFERENCE / 'upstream').rglob('*') if p.is_file()}
    expected = {f['path']: f for f in manifest['files']}
    assert actual == set(expected) == set(archive), 'source/archive/manifest file set mismatch'
    for path, entry in expected.items():
        data = (REFERENCE / 'upstream' / path).read_bytes()
        assert data == archive[path], f'modified original source: {path}'
        assert digest(data) == entry['sha256'] and len(data) == entry['bytes'], path
    return manifest

def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument('--fetch', action='store_true', help='restore verified upstream source archive; never patch it')
    args = ap.parse_args()
    if args.fetch:
        data = urllib.request.urlopen(URL, timeout=60).read()
        files = archive_files(data)
        manifest = json.loads((REFERENCE / 'source-metadata.json').read_text())
        assert set(files) == {f['path'] for f in manifest['files']}
        (REFERENCE / 'upstream.tar.gz').write_bytes(data)
        for path, content in files.items():
            target = REFERENCE / 'upstream' / path
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(content)
    m = verify()
    print(json.dumps({'revision': REVISION, 'archive_sha256': ARCHIVE_SHA256, 'unchanged_source_files_verified': len(m['files'])}, sort_keys=True))

if __name__ == '__main__':
    main()
