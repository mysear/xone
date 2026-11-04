#!/usr/bin/env python3
"""Independent mutation guards; all checks survive Python -OO."""
import copy
import json
from pathlib import Path
import subprocess
import sys
import tempfile
ROOT = Path(__file__).resolve().parents[3]
CHECK = ROOT / 'integration/tools/ci/v2-complete-check.py'
FIXTURE = ROOT / 'integration/test/contracts/v2-complete.json'
SUCCESS = ROOT / 'integration/tools/reference/v2-complete/verified.json'
def require(value, message):
    if not value:
        raise ValueError(message)
def main():
    original = json.loads(FIXTURE.read_text())
    require(len(original['cases']) == 24, 'fixed case inventory')
    result = subprocess.run([sys.executable, str(CHECK), '--validate-only'], capture_output=True, text=True)
    require(result.returncode == 0, result.stderr)
    edits = []
    def add(label, fn):
        edits.append((label, fn))
    add('duplicate declared actor', lambda x: x['reference']['transactions'][0]['accounts'].append(x['reference']['transactions'][0]['accounts'][0].copy()))
    add('official identity omission', lambda x: x['reference']['runtime_identities'].pop())
    add('setup getter omission', lambda x: next(t for t in x['reference']['transactions'] if t['purpose'] == 'ordinary:mint')['post_getters'][0].update(calls=[]))
    def header_forgery(x):
        x['reference']['genesis_header']['mixHash'] = '0x' + '1' * 64
        x['reference']['transactions'][0]['preblock_header']['mixHash'] = '0x' + '1' * 64
    add('coherent genesis PREVRANDAO forgery', header_forgery)
    add('case deletion', lambda x: x['cases'].pop())
    add('direction deletion', lambda x: x['cases'][1].update(direction='token0-to-token1'))
    add('post Pool deletion', lambda x: x['cases'][0]['expected'].pop('pool_after'))
    add('post raw balance', lambda x: x['cases'][0]['expected']['pool_after']['v2']['swap'].update(raw_balance0='1'))
    add('post cumulative', lambda x: x['cases'][0]['expected']['pool_after']['v2']['swap'].update(price0_cumulative_last='1'))
    add('post lock', lambda x: x['cases'][0]['expected']['pool_after']['v2']['swap'].update(unlocked=False))
    add('Pair identity', lambda x: x['cases'][0]['pool']['v2'].update(code_sha256='0'*64))
    add('Factory identity', lambda x: x['cases'][0]['pool']['v2']['swap'].update(factory_code_sha256='0'*64))
    add('bank hash', lambda x: x['cases'][0]['pool']['v2']['swap'].update(bank_block_hash='0x'+'0'*64))
    add('bank number', lambda x: x['cases'][0]['pool']['v2']['swap'].update(bank_block_number=0))
    add('bank timestamp', lambda x: x['cases'][0]['pool']['v2']['swap'].update(bank_unix_timestamp=0))
    add('execution clock', lambda x: x['cases'][0].update(execution_timestamp=0))
    add('raw PREVRANDAO', lambda x: x['cases'][0]['execution_environment'].update(prevrandao='0x'+'0'*64))
    add('header omission', lambda x: x['reference']['transactions'][0].pop('execution_block_header'))
    add('donation omission', lambda x: x['cases'][2]['pool']['v2']['swap'].update(raw_balance0=x['cases'][2]['pool']['reserve0']))
    add('donation Swap input', lambda x: x['cases'][2]['expected']['swap_event'].update(amount0_in=x['cases'][2]['amount_in']))
    add('dual donation input', lambda x: x['cases'][6]['expected']['swap_event'].update(amount1_in='0'))
    add('K forgery', lambda x: x['cases'][-1]['expected'].update(status='success', receiptstatus=1))
    add('overflow forgery', lambda x: x['cases'][-4]['expected'].update(status='success', receiptstatus=1))
    add('overflow reason', lambda x: x['cases'][-4]['evidence']['trace'].update(revert_reason='UniswapV2: K'))
    add('uint32 timestamp wrap', lambda x: x['cases'][17]['expected']['pool_after']['v2']['swap'].update(block_timestamp_last=2**32))
    add('cumulative wrap deletion', lambda x: x['cases'][18]['expected']['pool_after']['v2']['swap'].update(price0_cumulative_last=x['cases'][18]['pool']['v2']['swap']['price0_cumulative_last']))
    add('sequence reanchor', lambda x: x['sequences'][0]['steps'][1]['expected']['pool_after']['v2']['swap'].update(bank_block_hash=x['cases'][9]['state_anchor']['block_hash']))
    add('sequence deletion', lambda x: x['sequences'].pop())
    add('sequence intermediate state', lambda x: x['sequences'][0]['steps'][1]['pool'].update(reserve0='1'))
    add('source omission', lambda x: x['source_pins']['packages'][0]['files'].pop())
    add('source digest', lambda x: x['source_pins']['packages'][0]['files'][0].update(sha256='0'*64))
    add('script omission', lambda x: x['reference']['scripts'].pop(0))
    add('dependency omission', lambda x: x['reference']['loaded_dependencies'].pop())
    add('runtime dependency digest', lambda x: x['reference']['loaded_dependencies'][0].update(sha256='0'*64))
    add('Node digest', lambda x: x['reference']['node'].update(sha256='0'*64))
    add('EIP170 relaxed', lambda x: x['reference']['chain_options']['chain'].update(allowUnlimitedContractSize=True))
    add('pre account omission', lambda x: x['cases'][0]['accounts'].pop())
    add('post account omission', lambda x: x['cases'][0]['expected']['post_accounts'].pop())
    add('slot omission', lambda x: x['reference']['transactions'][-1]['accounts'][0]['storage'].clear())
    add('nonce forged', lambda x: x['reference']['transactions'][0]['post_accounts'][0].update(nonce=999))
    add('calldata forged', lambda x: x['cases'][0]['transaction'].update(data='0x'))
    add('receipt forged', lambda x: x['reference']['transactions'][0]['receipt'].update(status='0x0'))
    add('trace storage access omission', lambda x: x['reference']['transactions'][0]['storage_accesses'].pop())
    add('trace digest', lambda x: x['reference']['transactions'][0]['trace'].update(sha256='0'*64))
    add('getter wrong block', lambda x: x['cases'][0]['evidence']['pre_getters'][0].update(block_hash='0x'+'0'*64))
    add('future bank service admission', lambda x: x['cases'][17].update(service_eligible=True))
    add('Amount number', lambda x: x['cases'][0].update(amount_in=1000))
    add('lossy u64', lambda x: x['cases'][0]['pool']['v2']['swap'].update(bank_block_number=2**64))
    with tempfile.TemporaryDirectory(prefix='v2-guards-') as d:
        candidate = Path(d) / 'candidate.json'
        for label, edit in edits:
            x = copy.deepcopy(original)
            edit(x)
            candidate.write_text(json.dumps(x, indent=2) + '\n')
            SUCCESS.write_text('{"status":"passed","stale":true}\n')
            r = subprocess.run([sys.executable, str(CHECK), '--input', str(candidate), '--validate-only'], capture_output=True, text=True)
            require(r.returncode != 0, 'accepted mutation: ' + label)
            require(not SUCCESS.exists(), 'stale success survived: ' + label)
            print('rejected:', label, flush=True)
        candidate.write_text('{')
        SUCCESS.write_text('{"status":"passed"}\n')
        r = subprocess.run([sys.executable, str(CHECK), '--input', str(candidate), '--validate-only'], capture_output=True, text=True)
        require(r.returncode != 0 and not SUCCESS.exists(), 'entry parse failure did not invalidate success')
    print(f'{len(edits)+1} guards rejected; baseline 24 cases; stale success invalidated; optimized={sys.flags.optimize}')
if __name__ == '__main__':
    main()
