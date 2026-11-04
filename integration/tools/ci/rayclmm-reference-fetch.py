#!/usr/bin/env python3
"""Verify/restore unchanged pinned Raydium CLMM and locked uint archive sources."""
import argparse
import hashlib
import io
import json
from pathlib import Path, PurePosixPath
import tarfile
import urllib.request

ROOT = Path(__file__).resolve().parents[3]
REFERENCE = ROOT / 'integration/tools/reference/raydium-clmm'
REVISION = '6d7a0f82d2f293c553c69aaf943377367c6c4cc6'
PINS = {
    'upstream.tar.gz': ('raydium-io/raydium-clmm', REVISION, '13841cd1cddf0fc906970f70f8a639cb595e484c8ff260176aec0009cc561727'),
    'parity-common.tar.gz': ('raydium-io/parity-common', '43a0303b9c2a23a00508816bd6d5a2e9381f143c', 'f47bf73b98ccb7c5c70fd5e94d31493e979b709ed241df03500cb48ce08bec8b'),
}

def digest(data):
    return hashlib.sha256(data).hexdigest()

def contents(blob):
    out = {}
    with tarfile.open(fileobj=io.BytesIO(blob), mode='r:gz') as tar:
        for member in tar:
            if not member.isfile():
                continue
            parts = PurePosixPath(member.name).parts[1:]
            assert parts and '..' not in parts and not PurePosixPath(*parts).is_absolute()
            relative = str(PurePosixPath(*parts))
            assert relative not in out
            out[relative] = tar.extractfile(member).read()
    return out

def checksums(tree):
    # Cargo directory-source metadata only; source contents stay byte-identical.
    for manifest in tree.rglob('Cargo.toml'):
        if '[package]' not in manifest.read_text():
            continue
        parent = manifest.parent
        files = {str(p.relative_to(parent)): digest(p.read_bytes()) for p in parent.rglob('*') if p.is_file() and p.name != '.cargo-checksum.json'}
        (parent / '.cargo-checksum.json').write_text(json.dumps({'files': files, 'package': None}))

def verify():
    metadata = json.loads((REFERENCE / 'source-metadata.json').read_text())
    assert metadata['revision'] == REVISION and metadata['source_adaptations'] == []
    assert len(metadata['archives']) == len(PINS)
    for entry in metadata['archives']:
        repo, rev, sha = PINS[entry['archive']]
        assert (entry['repository'], entry['revision'], entry['archive_sha256']) == (repo, rev, sha)
        blob = (REFERENCE / entry['archive']).read_bytes()
        assert digest(blob) == sha, entry['archive']
        archived = contents(blob)
        manifest = {f['path']: f for f in entry['files']}
        tree = REFERENCE / entry['tree']
        actual = {str(p.relative_to(tree)) for p in tree.rglob('*') if p.is_file() and p.name != '.cargo-checksum.json'}
        assert set(archived) == set(manifest) == actual, f"tree coverage: {entry['tree']}"
        for name, data in archived.items():
            assert tree.joinpath(name).read_bytes() == data, f'changed source: {name}'
            assert manifest[name]['bytes'] == len(data) and manifest[name]['sha256'] == digest(data)
        for p in tree.rglob('.cargo-checksum.json'):
            checksum = json.loads(p.read_text())
            assert checksum['package'] is None
            files = {str(x.relative_to(p.parent)): digest(x.read_bytes()) for x in p.parent.rglob('*') if x.is_file() and x.name != '.cargo-checksum.json'}
            assert checksum['files'] == files, f'Cargo checksum drift: {p}'
    return metadata

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--fetch', action='store_true')
    args = parser.parse_args()
    if args.fetch:
        metadata = json.loads((REFERENCE / 'source-metadata.json').read_text())
        for entry in metadata['archives']:
            repo, rev, sha = PINS[entry['archive']]
            blob = urllib.request.urlopen(f'https://codeload.github.com/{repo}/tar.gz/{rev}', timeout=60).read()
            assert digest(blob) == sha, 'download hash mismatch'
            (REFERENCE / entry['archive']).write_bytes(blob)
            for name, data in contents(blob).items():
                target = REFERENCE / entry['tree'] / name
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_bytes(data)
        checksums(REFERENCE / 'deps/parity-common')
    metadata = verify()
    print(json.dumps({'verified': True, 'revision': REVISION, 'unchanged_files': sum(len(a['files']) for a in metadata['archives']), 'archives': {a['archive']: a['archive_sha256'] for a in metadata['archives']}}))

if __name__ == '__main__':
    main()
