#!/usr/bin/env python3
"""Verify or restore the exact pinned upstream archive and unchanged source tree."""
import argparse
import hashlib
import io
import json
from pathlib import Path, PurePosixPath
import tarfile
import urllib.request

ROOT = Path(__file__).resolve().parents[3]
REFERENCE = ROOT / 'integration/tools/reference/raydium-cpmm'
REVISION = 'f4b9b26342afe9122d5ff67cd7a4f19ded4ac0a5'
ARCHIVE_SHA256 = '4f33157abbbc533ddd89c2b5ba56cae4fdc1a9b0cbb0c5757bf8df5404a123e9'


def digest(data):
    return hashlib.sha256(data).hexdigest()


def verify():
    manifest = json.loads((REFERENCE / 'source-metadata.json').read_text())
    assert manifest['revision'] == REVISION
    assert manifest['archive_sha256'] == ARCHIVE_SHA256
    archive = (REFERENCE / 'upstream.tar.gz').read_bytes()
    assert digest(archive) == ARCHIVE_SHA256, 'pinned archive hash mismatch'
    expected = {entry['path']: entry for entry in manifest['files']}
    actual = {str(p.relative_to(REFERENCE / 'upstream')) for p in (REFERENCE / 'upstream').rglob('*') if p.is_file()}
    assert actual == set(expected), 'upstream tree has missing or extra files'
    with tarfile.open(fileobj=io.BytesIO(archive), mode='r:gz') as tar:
        members = {str(PurePosixPath(m.name).relative_to(PurePosixPath(m.name).parts[0])): m for m in tar.getmembers() if m.isfile()}
        assert set(members) == set(expected), 'manifest/archive coverage mismatch'
        for relative, entry in expected.items():
            data = (REFERENCE / 'upstream' / relative).read_bytes()
            assert digest(data) == entry['sha256'], f'changed upstream source: {relative}'
            assert len(data) == entry['bytes']
            assert data == tar.extractfile(members[relative]).read(), f'archive/source mismatch: {relative}'
    return manifest


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--fetch', action='store_true', help='Restore only verified archive contents; no source edits or patches')
    args = parser.parse_args()
    if args.fetch:
        url = f'https://codeload.github.com/raydium-io/raydium-cp-swap/tar.gz/{REVISION}'
        archive = urllib.request.urlopen(url, timeout=60).read()
        assert digest(archive) == ARCHIVE_SHA256, 'download archive hash mismatch'
        manifest = json.loads((REFERENCE / 'source-metadata.json').read_text())
        expected = {entry['path']: entry for entry in manifest['files']}
        contents = {}
        with tarfile.open(fileobj=io.BytesIO(archive), mode='r:gz') as tar:
            for member in tar.getmembers():
                if not member.isfile():
                    continue
                parts = PurePosixPath(member.name).parts[1:]
                assert parts and '..' not in parts and not PurePosixPath(*parts).is_absolute()
                relative = str(PurePosixPath(*parts))
                assert relative in expected, f'unexpected archive entry: {relative}'
                data = tar.extractfile(member).read()
                assert digest(data) == expected[relative]['sha256']
                contents[relative] = data
        assert set(contents) == set(expected)
        (REFERENCE / 'upstream.tar.gz').write_bytes(archive)
        for relative, data in contents.items():
            target = REFERENCE / 'upstream' / relative
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(data)
    manifest = verify()
    print(json.dumps({'revision': REVISION, 'archive_sha256': ARCHIVE_SHA256, 'unchanged_source_files': len(manifest['files']), 'verified': True}))


if __name__ == '__main__':
    main()
