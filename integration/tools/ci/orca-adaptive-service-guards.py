#!/usr/bin/env python3
"""Exercise native fixture rejection and receipt invalidation in normal/-OO modes."""
import ast
import base64
import copy
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[3]
REF = ROOT / 'integration/tools/reference/orca-adaptive-service'
GOLDEN = Path(__file__).with_name('orca-adaptive-service-golden.py')
BINARY = Path('/tmp/xone-orca-adaptive-swap-native/debug/orca-adaptive-service-reference')
CORPUS = ROOT / 'integration/test/contracts/orca-adaptive-service.json'

def require(ok, message):
    if not ok:
        raise RuntimeError(str(message))

def mutate(data, kind):
    s = data['scenarios'][0]
    b = s['banks'][0]
    if kind == 'delete-scenario':
        data['scenarios'].pop()
    elif kind == 'shorten-sequence':
        s['banks'].pop()
    elif kind == 'source-closure':
        data['identity']['source_inventory_sha256'] = '0'*64
    elif kind == 'binary-closure':
        data['identity']['executed_binary_sha256'] = '0'*64
    elif kind == 'compiler-closure':
        data['identity']['compiler'] = 'unbound compiler'
    elif kind == 'dependency-closure':
        data['cargo_dependencies'][0]['version'] = 'unbound'
    elif kind == 'native-amount':
        e = b['next_trade']['expected']
        e['amount_out'] = str(int(e['amount_out'])+1)
    elif kind == 'native-post-tick':
        b['next_trade']['expected']['next_state']['tick_arrays'][0]['ticks'][0]['fee_growth_outside_a'] = '1'
    elif kind == 'oracle-reserved-wire':
        address = s['config']['fixture']['oracle']
        account = b['accounts'][address]
        raw = bytearray(base64.b64decode(account['data'][0]))
        raw[126] ^= 1
        account['data'][0] = base64.b64encode(raw).decode()
    elif kind == 'vault-exact-transfer':
        b['next_trade']['expected']['transfer_model']['vault_deltas'][0]['after'] = '1'
    elif kind == 'source-slot':
        b['source_slots'][s['config']['id']] -= 1
    elif kind == 'drop-native-rejection':
        s['banks'][-1]['next_trade']['expected']['error'] = 'fabricated rejection'
    elif kind == 'copy-divergence':
        pass
    else:
        raise RuntimeError('unknown guard mutation')

def main():
    report_path = REF / 'guards-receipt.json'
    report_path.write_text(json.dumps({'status': 'invalid', 'reason': 'guard verification started'})+'\n')
    for path in [GOLDEN, Path(__file__), Path(__file__).with_name('orca-adaptive-service-check.py')]:
        require(not any(isinstance(n, ast.Assert) for n in ast.walk(ast.parse(path.read_text()))), f'removable assertion in {path}')
    original = json.loads(CORPUS.read_text())
    results = []
    mutations = ['delete-scenario', 'shorten-sequence', 'source-closure', 'binary-closure', 'compiler-closure',
                 'dependency-closure', 'native-amount', 'native-post-tick', 'oracle-reserved-wire',
                 'vault-exact-transfer', 'source-slot', 'drop-native-rejection', 'copy-divergence']
    with tempfile.TemporaryDirectory(prefix='orca-adaptive-service-guards-') as tmp:
        tmp = Path(tmp)
        for mode in [[], ['-OO']]:
            receipt = tmp/'receipt.json'
            path = tmp/'mutated.json'
            base = [sys.executable, *mode, str(GOLDEN), '--check', '--binary', str(BINARY), '--receipt', str(receipt)]
            valid = subprocess.run(base, capture_output=True, text=True, timeout=60)
            require(valid.returncode == 0 and json.loads(receipt.read_text())['status'] == 'verified', valid.stderr)
            for kind in mutations:
                data = copy.deepcopy(original)
                mutate(data, kind)
                path.write_text(json.dumps(data))
                receipt.write_text('{"status":"verified","stale":true}\n')
                other = CORPUS if kind == 'copy-divergence' else path
                cmd = [*base, '--corpus', str(path), '--parser-corpus', str(path), '--router-corpus', str(other)]
                result = subprocess.run(cmd, capture_output=True, text=True, timeout=60)
                require(result.returncode != 0 and 'Traceback' in result.stderr, f'accepted {kind} in {mode}')
                require(json.loads(receipt.read_text())['status'] == 'invalid', f'stale receipt survived {kind}')
                results.append(dict(mode='OO' if mode else 'normal', mutation=kind, rejected=True, receipt_invalidated=True))
    report = dict(status='verified', positive_modes=2, negative_checks=results,
                  fixture_sha256=hashlib.sha256(CORPUS.read_bytes()).hexdigest(),
                  binary_sha256=hashlib.sha256(BINARY.read_bytes()).hexdigest(),
                  driver_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest())
    report_path.write_text(json.dumps(report, sort_keys=True, indent=2)+'\n')
    print(json.dumps(dict(status='verified', positive_modes=2, negative_checks=len(results), fixture_sha256=report['fixture_sha256']), sort_keys=True))

if __name__ == '__main__':
    main()
