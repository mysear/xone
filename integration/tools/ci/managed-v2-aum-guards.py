#!/usr/bin/env python3
"""T34 reference negative controls, including optimized Python execution."""
from pathlib import Path
import subprocess
import sys
ROOT = Path(__file__).resolve().parents[3]
REF = ROOT / 'integration/tools/reference/balancer-managed-v2-aum'
PYTHON = [sys.executable] + (['-OO'] if sys.flags.optimize >= 2 else ['-O'] if sys.flags.optimize else [])

def tdd():
    source = r'''
const assert=require('node:assert/strict');
const {storeObject,resolveObject,validateGraph}=require(process.argv[1]);
const table={}; const ref=storeObject(table,'sample',{amount:'17'});
assert.deepEqual(resolveObject(table,ref,'sample'),{amount:'17'});
validateGraph(table,[ref]);
const bad=structuredClone(table);bad[ref].value.amount='18';
assert.throws(()=>validateGraph(bad,[ref]));
assert.throws(()=>validateGraph(table,['0'.repeat(64)]));
assert.throws(()=>resolveObject(table,ref,'receipt'));
const extra=structuredClone(table);storeObject(extra,'sample',{amount:'19'});
assert.throws(()=>validateGraph(extra,[ref]));
console.log('content-addressed positive control and four negative controls passed');
'''
    subprocess.run(['node', '-e', source, str(REF / 'validate.cjs')], check=True, cwd=ROOT)

def full():
    import copy
    import hashlib
    import json
    import tempfile
    check = ROOT / 'integration/tools/ci/managed-v2-aum-check.py'
    success = REF / 'verified.json'
    original = json.loads((ROOT / 'integration/test/contracts/managed-v2-aum.json').read_bytes())
    subprocess.run([*PYTHON, str(check), '--validate-only'], check=True, cwd=ROOT)
    def canonical(x):
        return json.dumps(x, sort_keys=True, separators=(',', ':'), ensure_ascii=False).encode()
    def change_object(bank, target, edit):
        table = bank['objects']
        table[target] = copy.deepcopy(table[target])
        edit(table[target]['value'])
        new, memo = {}, {}
        def walk(x):
            if isinstance(x, dict) and set(x) == {'ref'}:
                return {'ref': visit(x['ref'])}
            if isinstance(x, dict):
                return {k: walk(v) for k, v in x.items()}
            if isinstance(x, list):
                return [walk(v) for v in x]
            return x
        def visit(key):
            if key not in memo:
                row = walk(table[key])
                digest = hashlib.sha256(canonical(row)).hexdigest()
                memo[key] = digest
                new[digest] = row
            return memo[key]
        top = walk({k: v for k, v in bank.items() if k != 'objects'})
        bank.clear()
        bank.update(top)
        bank['objects'] = new
    edits = []
    def add(name, edit):
        edits.append((name, edit))
    def state_change(bank, field, value, case=0, side='before'):
        key = bank['cases'][case]['execution'][side]['ref']
        change_object(bank, key, lambda v: v['state'].__setitem__(field, value))
    add('profile forgery', lambda b: b.update(profile='balancer-v2-managed-fees'))
    add('adapter forgery', lambda b: b.update(adapter_version='old'))
    add('missing case', lambda b: b['cases'].pop())
    add('closure omission', lambda b: b['reference']['inputs']['files'].pop())
    add('dependency omission', lambda b: b['reference']['node_closure']['files'].pop())
    add('compiler job omission', lambda b: b['reference']['compiler_jobs'].pop())
    add('optimizer forgery', lambda b: b['reference']['compiler_jobs'][0]['optimizer'].update(runs=200))
    add('Node binary forgery', lambda b: b['reference']['node_closure']['node'].update(sha256='0'*64))
    add('unlimited codesize', lambda b: b['reference']['chain_options']['chain'].update(allowUnlimitedContractSize=True))
    add('missing size control', lambda b: b['reference']['controls'].clear())
    add('missing object', lambda b: b['objects'].pop(b['cases'][0]['execution']['before']['ref']))
    add('orphan object', lambda b: b['objects'].update({'f'*64: {'kind': 'fake', 'value': {}}}))
    add('digest forgery', lambda b: b['objects'][b['cases'][0]['execution']['before']['ref']]['value']['state'].update(pending_aum='1'))
    add('resealed annual fee forgery', lambda b: state_change(b, 'annual_aum_fee', '0'))
    add('resealed cached share forgery', lambda b: state_change(b, 'cached_aum_share', '0'))
    add('resealed actual supply forgery', lambda b: state_change(b, 'actual_supply', '1'))
    add('resealed virtual supply forgery', lambda b: state_change(b, 'virtual_supply', '1'))
    add('resealed pending forgery', lambda b: state_change(b, 'pending_aum', '1'))
    add('resealed uint32 overflow', lambda b: state_change(b, 'last_collection', str(2**32)))
    add('resealed provider mapping forgery', lambda b: state_change(b, 'provider_fee_id', '2'))
    add('resealed paused forgery', lambda b: state_change(b, 'paused', True))
    add('resealed recovery forgery', lambda b: state_change(b, 'recovery', True))
    add('resealed swaps-disabled forgery', lambda b: state_change(b, 'swaps_enabled', False))
    add('resealed GENERAL specialization', lambda b: change_object(b, b['cases'][0]['execution']['before']['ref'], lambda v: v['state']['registration'].update(specialization=0)))
    add('source reanchor', lambda b: b['cases'][0].update(source_h=b['cases'][-1]['source_h']))
    add('derived state promoted canonical', lambda b: b['cases'][0]['execution'].update(after_authority='canonical'))
    add('getter omission resealed', lambda b: change_object(b, b['cases'][0]['execution']['before']['ref'], lambda v: v['getters'].pop()))
    add('account omission resealed', lambda b: change_object(b, b['cases'][0]['execution']['before']['ref'], lambda v: v['accounts'].pop()))
    add('amount numeric forgery', lambda b: b['cases'][0]['expectation'].update(raw_in=100))
    add('raw-fee-first forgery', lambda b: b['cases'][6]['expectation'].update(upscaled_net='0'))
    add('swap output forgery', lambda b: b['cases'][0]['expectation'].update(raw_out='1'))
    add('receipt status forgery resealed', lambda b: change_object(b, b['cases'][0]['execution']['transaction']['ref'], lambda v: v['receipt'].update(status='0x0')))
    add('calldata forgery resealed', lambda b: change_object(b, b['cases'][0]['execution']['transaction']['ref'], lambda v: v['transaction'].update(input='0x')))
    add('source pin forgery', lambda b: b['reference'].update(source_pins_sha256='0'*64))
    add('failed collection promoted success', lambda b: change_object(b, b['cases'][-1]['execution']['transaction']['ref'], lambda v: v['receipt'].update(status='0x1')))
    add('unknown capability field', lambda b: change_object(b, b['cases'][0]['execution']['before']['ref'], lambda v: v['state'].update(rate_provider='unknown')))
    add('factory split code omission', lambda b: b['reference']['factory_code_storage']['contracts'].pop())
    add('size control runtime forgery', lambda b: b['reference']['control_observation'].update(runtime='0x00'))
    add('identity omission', lambda b: b['reference']['identities'].pop())
    def corrupt_runtime(b, fragment):
        identity = next(b['objects'][r['ref']]['value'] for r in b['reference']['identities'] if fragment in b['objects'][r['ref']]['value']['label'])
        key = identity['code']['ref']
        offset = 4 if fragment == 'CircuitBreakerLib' else 90
        change_object(b, key, lambda v: v.update(hex=v['hex'][:offset] + ('00' if v['hex'][offset:offset+2] != '00' else '01') + v['hex'][offset+2:]))
    add('runtime bytes resealed', lambda b: corrupt_runtime(b, 'two:ManagedPool'))
    add('library self address resealed', lambda b: corrupt_runtime(b, 'CircuitBreakerLib'))
    def corrupt_immutable(b):
        import gzip
        identity = next(b['objects'][r['ref']]['value'] for r in b['reference']['identities'] if b['objects'][r['ref']]['value']['label'] == 'two:ManagedPool')
        info = json.loads(gzip.decompress((REF / 'build-info/ManagedPoolFactory.json.gz').read_bytes()))
        spans = next(iter(info['output']['contracts']['contracts/managed/ManagedPool.sol']['ManagedPool']['evm']['deployedBytecode']['immutableReferences'].values()))
        def edit(v):
            value = v['hex']
            for span in spans:
                start = 2 + span['start'] * 2
                end = start + span['length'] * 2
                value = value[:start] + '0' * (span['length'] * 2) + value[end:]
            v['hex'] = value
        change_object(b, identity['code']['ref'], edit)
    add('immutable binding resealed', corrupt_immutable)
    # Strict native identity getter closure. Deletions reseal/prune the graph,
    # so rejection must come from missing observations, not stale object hashes.
    identities = {original['objects'][r['ref']]['value']['label']: original['objects'][r['ref']]['value']['address'] for r in original['reference']['identities']}
    pool = original['scenarios'][0]['pool']
    identity_queries = [
        ('Factory.getVault', identities['ManagedPoolFactory'], 'getVault', []),
        ('Factory.membership', identities['ManagedPoolFactory'], 'isPoolFromFactory', [pool]),
        ('Factory.provider', identities['ManagedPoolFactory'], 'getProtocolFeePercentagesProvider', []),
        ('Factory.math', identities['ManagedPoolFactory'], 'getWeightedMath', []),
        ('Factory.recovery', identities['ManagedPoolFactory'], 'getRecoveryModeHelper', []),
        ('Factory.creation', identities['ManagedPoolFactory'], 'getCreationCodeContracts', []),
        ('Factory.pause', identities['ManagedPoolFactory'], 'getPauseConfiguration', []),
        ('Pool.name', pool, 'name', []),
        ('Pool.feeID0', pool, 'getProviderFeeId', [0]),
        ('Pool.feeID2', pool, 'getProviderFeeId', [2]),
        ('Provider.Vault', identities['ProtocolFeePercentagesProvider'], 'getVault', []),
        ('Provider.cap2', identities['ProtocolFeePercentagesProvider'], 'getFeeTypeMaximumPercentage', [2]),
        ('Provider.cap3', identities['ProtocolFeePercentagesProvider'], 'getFeeTypeMaximumPercentage', [3]),
        ('Vault.WETH', identities['Vault'], 'WETH', []),
        ('Vault.collector', identities['Vault'], 'getProtocolFeesCollector', []),
        ('Collector.vault', identities['ProtocolFeesCollector'], 'vault', []),
        ('RecoveryHelper.Vault', identities['RecoveryModeHelper'], 'getVault', []),
    ]
    def matches(row, query):
        return row['to'] == query[1] and row['method'] == query[2] and row['args'] == query[3]
    def remove_getter(bank, query, frame):
        key = (bank['scenarios'][0]['source_h'] if frame == 'source_h' else bank['cases'][1 if frame == 'before' else 0]['execution'][frame])['ref']
        def edit(value):
            value['getters'] = [r for r in value['getters'] if not matches(bank['objects'][r['ref']]['value'], query)]
        change_object(bank, key, edit)
    def forge_getter(bank, query):
        snapshot = bank['objects'][bank['scenarios'][0]['source_h']['ref']]['value']
        key = next(r['ref'] for r in snapshot['getters'] if matches(bank['objects'][r['ref']]['value'], query))
        def edit(value):
            if query[2] == 'name':
                data = b'FORGED'
                value['result'] = '0x' + f'{32:064x}{len(data):064x}' + data.hex().ljust(64, '0')
            elif query[2] in ('getCreationCodeContracts', 'getPauseConfiguration'):
                value['result'] = '0x' + f'{1:064x}{0:064x}'
            elif query[2] == 'getFeeTypeMaximumPercentage':
                value['result'] = '0x' + f'{10**18:064x}'  # rejects the old synthetic 100% cap
            else:
                value['result'] = '0x' + f'{1 if query[2] == "getProviderFeeId" and query[3] == [0] else 0:064x}'
        change_object(bank, key, edit)
    for index, query in enumerate(identity_queries):
        frame = ('source_h', 'before', 'after')[index % 3]
        add('native getter omission ' + query[0] + ' @' + frame, lambda b, q=query, f=frame: remove_getter(b, q, f))
        add('native getter forgery ' + query[0], lambda b, q=query: forge_getter(b, q))
    with tempfile.TemporaryDirectory(prefix='managed-aum-guards-') as temp:
        candidate = Path(temp) / 'bank.json'
        for label, edit in edits:
            bank = copy.deepcopy(original)
            edit(bank)
            candidate.write_text(json.dumps(bank) + '\n')
            success.write_text('{"passed":true,"stale":true}\n')
            result = subprocess.run([*PYTHON, str(check), '--input', str(candidate), '--validate-only'], capture_output=True, text=True, cwd=ROOT)
            if result.returncode == 0 or success.exists():
                raise RuntimeError('accepted tamper or stale success: ' + label)
            print('rejected:', label, flush=True)
        candidate.write_text('{')
        success.write_text('{"passed":true}\n')
        result = subprocess.run([*PYTHON, str(check), '--input', str(candidate), '--validate-only'], capture_output=True, text=True, cwd=ROOT)
        if result.returncode == 0 or success.exists():
            raise RuntimeError('malformed entry retained stale success')
    # Physical closure failures happen only in a disposable project mirror, never the real workspace/runtime.
    import shutil
    with tempfile.TemporaryDirectory(prefix='managed-aum-entry-') as temp:
        shadow = Path(temp)
        shadow_ref = shadow / REF.relative_to(ROOT)
        shutil.copytree(REF, shadow_ref, ignore=shutil.ignore_patterns('verified.json', 'generation.json'))
        shadow_ci = shadow / 'integration/tools/ci'
        shadow_ci.mkdir(parents=True, exist_ok=True)
        for name in ['managed-v2-aum-golden.cjs', 'managed-v2-aum-check.py', 'managed-v2-aum-guards.py']:
            shutil.copyfile(check.parent / name, shadow_ci / name)
        runtime_link = shadow / 'integration/.runtime'
        runtime_link.symlink_to(ROOT / 'integration/.runtime', target_is_directory=True)
        candidate = shadow / 'bank.json'
        candidate.write_bytes((ROOT / 'integration/test/contracts/managed-v2-aum.json').read_bytes())
        command = [*PYTHON, str(shadow_ci / check.name), '--input', str(candidate), '--validate-only']
        subprocess.run(command, check=True, stdout=subprocess.DEVNULL, cwd=shadow)
        for relative in ['compiler/soljson-v0.7.1.js', 'build-info/Vault.json.gz', 'tasks/v2/tasks/20210418-vault/input.ts']:
            file = shadow_ref / relative
            parked = shadow / 'parked'
            file.rename(parked)
            stale = shadow_ref / 'verified.json'
            stale.write_text('{"passed":true,"stale":true}')
            result = subprocess.run(command, capture_output=True, text=True, cwd=shadow)
            if result.returncode == 0 or stale.exists():
                raise RuntimeError('physical closure omission accepted: ' + relative)
            parked.rename(file)
            print('rejected physical omission:', relative, flush=True)
        # A generation entry failure must remove all stale target copies before dependency imports.
        for service in ('integration', 'chain-parser-go', 'dex-router'):
            target = shadow / service / 'test/contracts/managed-v2-aum.json'
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_text('{"stale_success":true}')
        stale = shadow_ref / 'verified.json'
        stale.write_text('{"passed":true}')
        runtime_link.unlink()  # only the disposable symlink; actual .runtime is never mutated
        failed = subprocess.run(['node', str(shadow_ci / 'managed-v2-aum-golden.cjs')], capture_output=True, text=True, cwd=shadow)
        if failed.returncode == 0 or stale.exists() or any((shadow / service / 'test/contracts/managed-v2-aum.json').exists() for service in ('integration', 'chain-parser-go', 'dex-router')):
            raise RuntimeError('failed generation retained stale success/output')
        if json.loads((shadow_ref / 'generation.json').read_bytes()).get('passed') is not False:
            raise RuntimeError('failed generation did not invalidate machine receipt')
    print(f'{len(edits)+5} guards rejected; optimized={sys.flags.optimize}')

if __name__ == '__main__':
    tdd() if '--tdd' in sys.argv else full()
