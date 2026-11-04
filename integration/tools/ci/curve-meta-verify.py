#!/usr/bin/env python3
"""Fail-closed offline checks for the isolated original MetaUSD reference.

--recompile also compares all original Vyper artifact fields. This verifier
never changes the receipt or corpus and uses explicit checks under python -O.
"""
import hashlib
import json
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[3]
REF = ROOT / 'integration/tools/reference/curve-meta'
PIN = 'edfb3da7c748edb873465e60d471f49b6fd67be2972c97878948e9b9c2c98388'
RUNTIME = 'c5351fd332fdf0f04486a838b2bceba15d7a45965e3df61e9a0ccedc674d450d'
REVISION = '5272fd2b1199f185d9cd94822be9262c387dc2c2'
BASE = '0xbebc44782c7db0a1a60cb6fe97d0b483032ff1c7'
LP = '0x6c3f90f043a72fa612cbac8115ee7e52bde6e490'
VP = [10**18, 10**18, 1234567890123456789, 1234567890123456789,
      987654321012345678, 987654321012345678, 1100000000000000000, 10**18]


def require(condition, message):
    if not condition:
        raise ValueError(message)


def sha(data):
    return hashlib.sha256(data).hexdigest()


def code_sha(code):
    require(isinstance(code, str) and code.startswith('0x'), 'Invalid runtime hex')
    return sha(bytes.fromhex(code[2:]))


def verify():
    receipt = json.loads((REF / 'generation.json').read_text())
    require(receipt.get('generation_complete') is True, 'No completed generation receipt')
    pin = receipt['source_pin']
    require(pin['repo'] == 'curvefi/curve-factory' and pin['revision'] == REVISION and
            pin['path'] == 'contracts/implementations/meta/MetaUSD.vy' and
            pin['local'] == 'integration/tools/reference/curve-models/old/MetaUSD.vy' and
            pin['compiler'] == '0.3.1' and pin['sha256'] == PIN and
            pin['runtime_sha256'] == RUNTIME and pin['runtime_bytes'] == 20060, 'Source pin changed')
    require(sha((ROOT / pin['local']).read_bytes()) == PIN, 'Original source changed')
    for record in receipt['files']:
        require(sha((ROOT / record['path']).read_bytes()) == record['sha256'], 'Recorded file changed: ' + record['path'])
    for key in ['compiler_executable', 'python_executable', 'node_executable', 'node_lock', 'javascript_compiler']:
        record = receipt[key]
        require(sha(Path(record['path']).read_bytes()) == record['sha256'], 'Dependency changed: ' + key)
    for record in receipt['node_modules']:
        p = Path(receipt['node_modules_root']) / record['path']
        require(sha(p.read_bytes()) == record['sha256'], 'Loaded JavaScript changed: ' + str(p))
    for record in receipt['compiler_packages']:
        require(sha(Path(record['path']).read_bytes()) == record['sha256'], 'Compiler dependency changed: ' + record['path'])
    raw = (ROOT / 'integration/test/contracts/curve-meta-direct.json').read_bytes()
    require(raw == (ROOT / 'dex-router/test/contracts/curve-meta-direct.json').read_bytes(), 'Corpus copies differ')
    require(sha(raw) == receipt['corpus_sha256'], 'Corpus digest differs')
    data = json.loads(raw)
    reference = data['reference']
    require(reference['source_pin'] == pin and reference['revision'] == REVISION and
            reference['runtime_sha256'] == RUNTIME and reference['runtime_bytes'] == 20060 and
            reference['compiler'] == '0.3.1', 'Reference identity changed')
    require(reference['base_pool_isolated_mock'] is True and
            reference['allow_unlimited_contract_size'] is False and
            reference['hardfork'] == 'shanghai', 'Execution boundary changed')
    require(reference['eip170_enforcement']['status'] == 0 and
            reference['implementation_initialize_reverted'] is True and
            reference['implementation_constructor_fee'] == '31337', 'Clone/size enforcement missing')
    artifacts = json.loads((REF / 'artifacts.json').read_text())
    original = artifacts['original']
    require(code_sha(original['runtime']) == RUNTIME and len(bytes.fromhex(original['runtime'][2:])) == 20060, 'Original runtime changed')
    for key, slot in [('coins', 2), ('balances', 4), ('fee', 6), ('rate_multiplier', 11)]:
        require(original['layout'][key]['slot'] == slot, 'Original layout changed: ' + key)
    metadata = json.loads((REF / 'metadata.json').read_text())
    require(metadata == json.loads((REF / 'deployments.json').read_text()), 'Deployment metadata differs')
    require(code_sha(metadata['implementation_runtime_hex']) == RUNTIME, 'Deployed implementation changed')
    require(metadata['implementation_deployment']['status'] == 1, 'Implementation deployment not successful')
    require(code_sha(metadata['base_pool_runtime_hex']) == metadata['base_pool_code_sha256'], 'Base mock hash differs')
    fixtures = artifacts['solidity_output']['contracts']['Fixtures.sol']
    def compiled_runtime(name):
        return '0x' + fixtures[name]['evm']['deployedBytecode']['object']
    require(metadata['factory_runtime_hex'] == compiled_runtime('CloneFactory'), 'Factory differs from artifact')
    require(metadata['base_pool_runtime_hex'] == compiled_runtime('SuppliedVirtualPrice'), 'Base mock differs from artifact')
    scenarios = data['scenarios']
    require(len(scenarios) == receipt['scenarios'] == 4, 'Scenario count differs')
    steps = 0
    for s, decimals, fee in zip(scenarios, [0, 6, 8, 18], [4000000, 10000000, 50000000, 100000000]):
        require(s['meta_decimals'] == decimals and s['token1'] == LP and s['base_pool'] == BASE, 'Coin profile differs')
        require(s['implementation'] == metadata['implementation'] and s['implementation_code_sha256'] == RUNTIME and
                code_sha(s['implementation_runtime_hex']) == RUNTIME, 'Implementation identity differs')
        clone = '0x363d3d373d3d3d363d73' + s['implementation'][2:] + '5af43d82803e903d91602b57fd5bf3'
        require(s['pool_runtime_hex'] == clone and code_sha(clone) == s['pool_code_sha256'], 'Clone identity differs')
        require(s['token0_runtime_hex'] == s['token1_runtime_hex'] == compiled_runtime('BookkeepingERC20'), 'Token mock differs from artifact')
        require(s['base_runtime_hex'] == compiled_runtime('SuppliedVirtualPrice'), 'Scenario base mock differs')
        require(s['initialize']['status'] == 1 and s['initialize']['repeat_initialize_reverted'] is True, 'Clone initialization missing')
        require(s['seed']['method'] == 'add_liquidity(uint256[2],uint256,address)' and s['seed']['status'] == 1, 'Original liquidity seed missing')
        require(len(s['steps']) == 8, 'Step count differs')
        previous = s['seed']
        for phase, row in enumerate(s['steps']):
            require(row['timestamp'] == row['expected']['timestamp'] == 1767225600 + phase, 'Common phase timestamp differs')
            require(int(row['base_virtual_price']) == VP[phase], 'Common base VP differs')
            require(row['index_in'] == phase % 2 and row['index_out'] == 1 - phase % 2, 'Direction differs')
            require(int(row['meta_rate']) == 10**(36-decimals) and row['amp'] == '12300' and
                    int(row['fee']) == fee and row['admin_fee'] == '5000000000', 'Math inputs differ')
            for before, after in [('balances', 'next_balances'), ('stored_balances', 'next_stored_balances'), ('admin_balances', 'next_admin_balances')]:
                require(row[before] == previous[after if phase else before], 'Sequential state changed: ' + before)
            verify_row(row)
            previous = row['expected']
            steps += 1
    require(steps == receipt['steps'] == 32, 'Total step count differs')
    probes = data['probes']
    require(len(probes) == receipt['probes'] == 8, 'Probe count differs')
    for row in probes:
        verify_row(row)
    require(sum(bool(p['expected']['reverted']) for p in probes) == receipt['reverted_probes'], 'Revert count differs')
    if '--recompile' in sys.argv:
        output = subprocess.check_output([receipt['compiler_executable']['path'], '-f', 'bytecode,bytecode_runtime,abi,layout', str(ROOT / pin['local'])], text=True).strip().splitlines()
        require(output[0] == original['bytecode'] and output[1] == original['runtime'] and
                json.loads(output[2]) == original['abi'] and json.loads(output[3]) == original['layout'], 'Original recompile differs')
    return {'scenarios': len(scenarios), 'steps': steps, 'probes': len(probes), 'reverted_probes': receipt['reverted_probes'], 'corpus_sha256': receipt['corpus_sha256']}


def verify_row(row):
    e = row['expected']
    require(e['status'] == (0 if e['reverted'] else 1), 'Receipt status differs')
    for key in ['transaction_hash', 'block_hash']:
        require(isinstance(e[key], str) and len(e[key]) == 66 and e[key].startswith('0x'), 'Missing native transaction coordinate')
    for prefix in ['', 'next_']:
        obj = row if not prefix else e
        effective, raw, admin = [list(map(int, obj[prefix + key])) for key in ['balances', 'stored_balances', 'admin_balances']]
        require(len(effective) == len(raw) == len(admin) == 2 and
                all(r == b+a and min(r,b,a) >= 0 for b,r,a in zip(effective,raw,admin)), 'Raw/effective/admin identity differs')
    if e['reverted']:
        for before, after in [('balances','next_balances'), ('stored_balances','next_stored_balances'), ('admin_balances','next_admin_balances'), ('wallet_balances','next_wallet_balances')]:
            require(row[before] == e[after], 'Native rollback failed: ' + before)
    else:
        i, j, dx, dy = row['index_in'], row['index_out'], int(row['input']), int(e['amount_out'])
        require(int(e['next_stored_balances'][i]) == int(row['stored_balances'][i]) + dx and
                int(e['next_stored_balances'][j]) == int(row['stored_balances'][j]) - dy, 'Native transfer accounting differs')
        event = e['token_exchange']
        require(event['sold_id'] == str(i) and event['bought_id'] == str(j) and
                event['tokens_sold'] == str(dx) and event['tokens_bought'] == str(dy), 'Native exchange event differs')


if __name__ == '__main__':
    try:
        print(json.dumps(verify(), sort_keys=True))
    except Exception as error:
        sys.exit('curve-meta verification failed: ' + str(error))
