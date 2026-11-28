#!/usr/bin/env python3
"""Rebuild the fixed-block Slipstream replay input from archived public RPC responses."""

import argparse
import json
from pathlib import Path
import re
import subprocess
from tempfile import TemporaryDirectory

from r01_public_catalog import ROOT, load_json, read_blob, require
from r01_public_catalog_capture import reference, write_new
from r01_public_catalog_evm_normal import BASE


def slot_key(key):
    require(isinstance(key, str) and re.fullmatch(r'0x[0-9a-fA-F]{1,64}', key), 'SLIPSTREAM_STORAGE_KEY')
    return '0x' + key[2:].lower().zfill(64)


def transcript(ref, method, tag):
    wire = json.loads(read_blob(ROOT, ref))
    params = wire['params']
    response = wire['response']
    require(wire['exit_code'] == 0 and wire['method'] == method and
            len(params) == (3 if method == 'eth_getProof' else 2) and params[-1] == tag and
            response['jsonrpc'] == '2.0' and response['id'] == 1 and 'result' in response,
            'SLIPSTREAM_ARCHIVE_TRANSCRIPT:' + ref['path'])
    address = params[0].lower()
    require(re.fullmatch(r'0x[0-9a-f]{40}', address), 'SLIPSTREAM_ARCHIVE_ADDRESS')
    return address, params, response['result']


def merge(evidence, proof_refs, code_refs):
    tag = evidence['block']['number']
    deployments = {row['address']: row for row in evidence['deployments']}
    codes = {}
    for ref in code_refs:
        address, _, code = transcript(ref, 'eth_getCode', tag)
        require(address not in deployments and address not in codes and isinstance(code, str),
                'SLIPSTREAM_ARCHIVE_NEW_CODE')
        codes[address] = code

    slots = {address: {slot_key(slot['key']): slot for slot in row['proof']['storageProof']}
             for address, row in deployments.items()}
    for ref in proof_refs:
        address, params, proof = transcript(ref, 'eth_getProof', tag)
        require(proof['address'].lower() == address, 'SLIPSTREAM_ARCHIVE_PROOF_ADDRESS')
        entries = {slot_key(slot['key']): slot for slot in proof['storageProof']}
        require(sorted(entries) == sorted(map(slot_key, params[1])) and
                len(entries) == len(proof['storageProof']), 'SLIPSTREAM_ARCHIVE_PROOF_KEYS')
        account_proof = {field: value for field, value in proof.items() if field != 'storageProof'}
        if address not in deployments:
            require(address in codes, 'SLIPSTREAM_ARCHIVE_CODE_MISSING:' + address)
            deployments[address] = {'address': address, 'code': codes.pop(address), 'proof': account_proof}
            slots[address] = {}
        row = deployments[address]
        require(all(row['proof'].get(field) == value for field, value in account_proof.items()),
                'SLIPSTREAM_ARCHIVE_ACCOUNT_CHANGED:' + address)
        for key, slot in entries.items():
            normalized = {**slot, 'key': key}
            prior = slots[address].get(key)
            require(prior is None or {**prior, 'key': key} == normalized,
                    'SLIPSTREAM_ARCHIVE_SLOT_CHANGED:' + key)
            slots[address][key] = normalized
        row['proof'] = {**account_proof, 'storageProof': list(slots[address].values())}
        row.pop('proof_source', None)
        row.pop('code_source', None)

    require(not codes, 'SLIPSTREAM_ARCHIVE_UNUSED_CODE')
    return {**evidence, 'deployments': list(deployments.values())}


def write_json(path, value):
    write_new(path, (json.dumps(value, indent=2) + '\n').encode())


def rebuild(capture_path, base, proof_paths, code_paths, output):
    require(output.is_relative_to(BASE.resolve()) and not output.exists(), 'OWNED_SLIPSTREAM_ARCHIVE')
    capture_ref = reference(capture_path)
    capture = json.loads(read_blob(ROOT, capture_ref))
    row = capture['catalog_row']
    paths = {name: reference(base / name) for name in (
        'anchor.json', 'rpc-index.json', 'fork-execution.json', 'prestate-evidence-v2.json')}
    anchor = json.loads(read_blob(ROOT, paths['anchor.json']))
    evidence = json.loads(read_blob(ROOT, paths['prestate-evidence-v2.json']))
    execution = read_blob(ROOT, paths['fork-execution.json'])
    require(row['id'] == 'base-aerodrome-slipstream' and evidence['chain'] == 'base' and
            row['anchor']['hash'] == anchor['anchor']['hash'] == evidence['block']['hash'] and
            hex(row['anchor']['height']) == evidence['block']['number'] and
            row['inputs']['execution']['sha256'] == paths['fork-execution.json']['sha256'],
            'SLIPSTREAM_ARCHIVE_ANCHOR')
    proof_refs = [reference(path) for path in proof_paths]
    code_refs = [reference(path) for path in code_paths]
    verifier = Path(__file__).with_name('r01_public_catalog_evm_commitments.cjs')
    subprocess.run(['node', str(verifier), str(base), '', 'prestate-evidence-v2.json'],
                   check=True, stdout=subprocess.DEVNULL, timeout=120)

    output.parent.mkdir(parents=True, exist_ok=True)
    with TemporaryDirectory(dir=output.parent) as directory:
        stage = Path(directory) / 'archive'
        bank = stage / 'vm-bank'
        for name in ('anchor.json', 'rpc-index.json'):
            write_new(bank / name, read_blob(ROOT, paths[name]))
        write_new(bank / 'fork-execution.json', execution)
        write_json(bank / 'prestate-evidence.json', merge(evidence, proof_refs, code_refs))
        subprocess.run(['node', str(verifier), str(bank), str(stage / 'proof-verification.json')],
                       check=True, stdout=subprocess.DEVNULL, timeout=120)

        row['directory'] = str((output / 'vm-bank').relative_to(ROOT))
        row['inputs']['execution'] = {**paths['fork-execution.json'],
                                     'path': row['directory'] + '/fork-execution.json'}
        write_json(stage / 'capture.json', capture)
        write_json(stage / 'proof-inputs.json', {'capture': capture_ref, 'base': paths,
                                                'proof_responses': proof_refs, 'code_responses': code_refs})
        require(not output.exists(), 'OWNED_SLIPSTREAM_ARCHIVE')
        stage.rename(output)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--capture', type=Path, required=True)
    parser.add_argument('--base', type=Path, required=True)
    parser.add_argument('--proof', type=Path, nargs='+', required=True)
    parser.add_argument('--code', type=Path, nargs='*', default=[])
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    rebuild(args.capture, args.base.resolve(), args.proof, args.code, args.out.resolve())


if __name__ == '__main__':
    main()
