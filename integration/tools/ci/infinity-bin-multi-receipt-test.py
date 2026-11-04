#!/usr/bin/env python3
"""Prove old passed receipts cannot survive dependency/source/compile failures."""
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[3]
REF = ROOT / 'integration/tools/reference/infinity-bin-multi'

def require(condition, message):
    if not condition:
        raise ValueError(message)

def main():
    receipt_path = REF / 'receipt-test-receipt.json'
    receipt_path.write_text(json.dumps({'passed': False, 'state': 'RUNNING'}) + '\n')
    scenarios = ['missing-optional-dependency', 'invalid-source-json', 'source-pin-mismatch', 'solidity-compile-error']
    inputs = [Path(__file__), ROOT / 'integration/tools/ci/infinity-bin-multi-golden.cjs', REF / 'InfinityBinMultiReference.sol', ROOT / 'integration/tools/reference/singleton-advanced/source-metadata.json']
    hashes = {str(p.relative_to(ROOT)): hashlib.sha256(p.read_bytes()).hexdigest() for p in inputs}
    results = []
    try:
        for scenario in scenarios:
            with tempfile.TemporaryDirectory(prefix='infinity-bin-multi-receipt-') as temp:
                temp = Path(temp)
                ci = temp / 'integration/tools/ci'; ci.mkdir(parents=True)
                ref = temp / 'integration/tools/reference/infinity-bin-multi'; ref.mkdir(parents=True)
                upstream = temp / 'integration/tools/reference/singleton-advanced'; upstream.mkdir()
                script = ci / 'infinity-bin-multi-golden.cjs'
                shutil.copyfile(inputs[1], script)
                shutil.copyfile(inputs[2], ref / 'InfinityBinMultiReference.sol')
                old = ref / 'execution-receipt.json'
                old.write_text('{"passed":true,"old_sentinel":true}\n')
                if scenario != 'missing-optional-dependency':
                    runtime = temp / 'integration/.runtime/singleton-reference'; runtime.mkdir(parents=True)
                    origin = ROOT / 'integration/.runtime/singleton-reference'
                    (runtime / 'node_modules').symlink_to(origin / 'node_modules', target_is_directory=True)
                    (runtime / 'sources').symlink_to(origin / 'sources', target_is_directory=True)
                    (runtime / 'package-lock.json').symlink_to(origin / 'package-lock.json')
                metadata = inputs[3].read_bytes()
                if scenario == 'invalid-source-json':
                    metadata = b'{invalid-json'
                if scenario == 'source-pin-mismatch':
                    manifest = json.loads(metadata)
                    next(f for f in manifest['files'] if f['local'].endswith('/BinPool.sol'))['sha256'] = '0' * 64
                    metadata = json.dumps(manifest).encode()
                (upstream / 'source-metadata.json').write_bytes(metadata)
                if scenario == 'solidity-compile-error':
                    with (ref / 'InfinityBinMultiReference.sol').open('a') as out:
                        out.write('\nintentional invalid Solidity\n')
                run = subprocess.run(['node', str(script)], capture_output=True, text=True, timeout=60)
                result = json.loads(old.read_text())
                require((run.returncode != 0), scenario)
                require((result['passed'] is False and result.get('error')), scenario)
                require(('old_sentinel' not in result), scenario)
                require((not (temp / 'integration/test/contracts/infinity-bin-multi.json').exists()), 'Reference validation failed')
                results.append({'scenario': scenario, 'exit_code': run.returncode, 'error': result['error'][:300]})
        require((hashes == {str(p.relative_to(ROOT)): hashlib.sha256(p.read_bytes()).hexdigest() for p in inputs}), 'Reference validation failed')
        receipt_path.write_text(json.dumps({'passed': True, 'inputs': hashes, 'scenarios': results}, indent=2) + '\n')
        print(json.dumps({'receipt_failure_tests': len(results), 'passed': True}))
    except BaseException as error:
        receipt_path.write_text(json.dumps({'passed': False, 'error': str(error)}) + '\n')
        raise

if __name__ == '__main__':
    main()
