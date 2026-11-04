#!/usr/bin/env python3
"""Reproducibly generate independent service-local receipt-root vectors."""
from pathlib import Path
import hashlib
import json
import shutil
import subprocess
ROOT = Path(__file__).resolve().parents[3]
SOURCE = ROOT / 'integration/tools/reference/receipt-root'
RUNTIME = ROOT / 'integration/.runtime/receipt-root-reference'
RUNTIME.mkdir(parents=True, exist_ok=True)
for name in ('package.json', 'package-lock.json'):
    shutil.copyfile(SOURCE / name, RUNTIME / name)
subprocess.run(['npm', 'ci', '--prefix', str(RUNTIME), '--ignore-scripts', '--no-audit', '--no-fund'], check=True)
log = {'address': '0x' + 'ab' * 20, 'topics': ['0x' + 'cd' * 32, '0x' + 'ef' * 32], 'data': '0x001234ff'}
cases = [{'name': 'empty-block', 'receipts': []}]
for kind in range(5):
    cases.append({'name': f'type-{kind}', 'receipts': [{'type': hex(kind), 'status': hex(kind % 2), 'logs': [log] if kind % 2 else []}]})
cases.append({'name': 'legacy-post-state', 'receipts': [{'type': '0x0', 'root': '0x' + '42' * 32, 'logs': [log]}]})
for size in (2, 17, 128, 130, 257):
    cases.append({'name': f'mixed-{size}', 'receipts': [{'type': hex(i % 5), 'status': hex(i % 2), 'logs': [log] if i % 3 else []} for i in range(size)]})
result = subprocess.run(['node', str(SOURCE / 'oracle.cjs')], input=json.dumps({'cases': cases}), text=True, capture_output=True, check=True)
data = json.loads(result.stdout)
data['generator_sha256'] = hashlib.sha256((SOURCE / 'oracle.cjs').read_bytes()).hexdigest()
data['package_lock_sha256'] = hashlib.sha256((SOURCE / 'package-lock.json').read_bytes()).hexdigest()
target = ROOT / 'chain-parser-go/test/fixtures/receipt-roots.json'
target.parent.mkdir(exist_ok=True)
target.write_text(json.dumps(data, indent=2) + '\n')
print(f'Generated {len(cases)} independent receipt-root vectors: {target}')

# Freeze the finite synthetic process-fixture payloads too. Ordinary tests and
# process runs read these checked-in roots without invoking Node or networking.
witness_log = {'address': '0x' + f'{999:040x}', 'topics': ['0x'+'ab'*32], 'data': '0x01'}
fixtures = [[], [{'logs': []}], [{'logs': [witness_log]}]]
for price in (20, 30, 40, 100):
    for complex_input in (0, 1):
        entry = {'address': '0x'+'03'*20,
                 'topics': ['0xd78ad95fa46c994b6551d0da85fc275fe613ce37657fb8d5e3d130840159d822', '0x'+'00'*32, '0x'+'00'*32],
                 'data': '0x'+f'{1:064x}{complex_input:064x}{0:064x}{price:064x}'}
        fixtures.append([{'logs': [entry]}])
inputs = [{'name': hashlib.sha256(json.dumps({'receipts': f}, sort_keys=True).encode()).hexdigest(), 'receipts': f} for f in fixtures]
result = subprocess.run(['node', str(SOURCE/'oracle.cjs')], input=json.dumps({'cases': inputs}), text=True, capture_output=True, check=True)
catalog = json.loads(result.stdout)
catalog['cases'] = {c['name']: {'root': c['root'], 'receipts': c['receipts']} for c in catalog['cases']}
catalog['generator_sha256'] = data['generator_sha256']
catalog['package_lock_sha256'] = data['package_lock_sha256']
target = ROOT/'integration/test/contracts/receipt-root-fixtures.json'
target.write_text(json.dumps(catalog, indent=2)+'\n')
print(f'Generated {len(fixtures)} offline process-fixture roots: {target}')
