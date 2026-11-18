#!/usr/bin/env python3
"""Restore the pinned local public-response fixture from an external bundle/capture.

Repository policy keeps raw test datasets out of Git. Only this restoration tool
and immutable metadata are tracked; absence of the dataset fails explicitly.
No RPC, state edits, response normalization or fallback capture is performed.
"""
import argparse
import base64
import gzip
import json
from pathlib import Path
import tempfile
import current_v2_evidence as evidence

ROOT=Path(__file__).resolve().parents[3]
PIN=ROOT/'integration/tools/reference/current-v2-public/source-metadata.json'
DESTINATIONS=(ROOT/'integration/test/source/current-v2-public',ROOT/'chain-parser-go/test/fixtures/current-v2-public')

def restore(bundle=None,capture=None):
    evidence.require(bool(bundle)!=bool(capture),'SELECT_PINNED_BUNDLE_OR_CAPTURE')
    manifest=json.loads(PIN.read_bytes(),object_pairs_hook=evidence.pairs)
    if bundle:
        path=Path(bundle);evidence.require(path.is_file() and not path.is_symlink() and path.stat().st_size<=32<<20,'PINNED_BUNDLE_REQUIRED');compressed=path.read_bytes()
    else:
        directory=Path(capture);_,inventory=evidence.capture_index(directory)
        evidence.require(inventory==manifest['records'],'CAPTURE_DOES_NOT_MATCH_PINNED_ACQUISITION')
        rows=[{'name':row['file'],'data':base64.b64encode((directory/row['file']).read_bytes()).decode()} for row in inventory]
        decoded=json.dumps({'schema_version':1,'records':rows},separators=(',',':')).encode()
        evidence.require(evidence.sha(decoded)==manifest['decoded_sha256'],'PINNED_DECODED_CAPTURE_CHANGED')
        compressed=gzip.compress(decoded,compresslevel=9,mtime=0)
    evidence.require(len(compressed)==manifest['bundle_bytes'] and evidence.sha(compressed)==manifest['bundle_sha256'],'PINNED_BUNDLE_HASH')
    metadata=PIN.read_bytes()
    with tempfile.TemporaryDirectory(prefix='xone-current-v2-restore-') as temporary:
        root=Path(temporary);(root/'manifest.json').write_bytes(metadata);(root/'responses.json.gz').write_bytes(compressed)
        evidence.unpack_fixture(root,root/'validated-records')
    # Refuse conflicting/symlink destinations before writing either copy.
    for directory in DESTINATIONS:
        evidence.require(not directory.is_symlink(),'FIXTURE_DESTINATION_SYMLINK')
        for name,data in [('manifest.json',metadata),('responses.json.gz',compressed)]:
            path=directory/name;evidence.require(not path.is_symlink(),'FIXTURE_DESTINATION_SYMLINK')
            if path.exists():evidence.require(path.is_file() and path.read_bytes()==data,'FIXTURE_DESTINATION_CONFLICT')
    for directory in DESTINATIONS:
        directory.mkdir(parents=True,exist_ok=True)
        for name,data in [('manifest.json',metadata),('responses.json.gz',compressed)]:
            path=directory/name
            if not path.exists():
                with path.open('xb') as stream:stream.write(data)
    return {'state':'PINNED_LOCAL_FIXTURE_RESTORED','record_count':manifest['record_count'],'bundle_sha256':manifest['bundle_sha256'],'network_requests':0,'destinations':[str(p.relative_to(ROOT)) for p in DESTINATIONS]}

def main():
    parser=argparse.ArgumentParser(description=__doc__);group=parser.add_mutually_exclusive_group(required=True);group.add_argument('--bundle',type=Path);group.add_argument('--capture',type=Path);args=parser.parse_args();print(json.dumps(restore(args.bundle,args.capture)))
if __name__=='__main__':main()
