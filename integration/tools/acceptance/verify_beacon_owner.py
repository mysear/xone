"""Exercise signed finality admission and revocation on the ordinary Go owner."""
import argparse
import json
import secrets
import subprocess
import time
from pathlib import Path

from current_v2_evidence import ENDPOINTS, GENESIS, POOL, config_for, free_port, http, stop


def exercise(binary, output, bootstrap, updates, finality, root):
    output.mkdir(exist_ok=False, parents=True)
    proof = {'bootstrap': json.loads(bootstrap.read_bytes()), 'finality': json.loads(finality.read_bytes())}
    if updates:
        proof['updates'] = json.loads(updates.read_bytes())
    execution = proof['finality']['data']['finalized_header']['execution']
    path = output / 'proof.json'
    def install(data):
        tmp = path.with_suffix('.tmp')
        tmp.write_text(json.dumps(data))
        tmp.replace(path)
    install(proof)
    token, port = secrets.token_urlsafe(32), free_port()
    owner = 'http://127.0.0.1:' + str(port)
    cfg = config_for('chain-parser-go', output/'owner', token, port)
    cfg['chains'] = [dict(key='evm:1', owner_service='chain-parser-go', assignment_version=1,
                          genesis_hash=GENESIS, rpc_urls=list(ENDPOINTS), pools=[POOL],
                          confirmation_tag='finalized', auto_reorg_window=128,
                          block_integrity='known-evm-native-commitments-v2', receipts_mode='bulk-auto',
                          native_proof=dict(max_bytes=64<<20, max_headers=64, max_transactions=4096,
                                            max_lookahead=32, max_window_blocks=16),
                          beacon_finality=dict(checkpoint_root=root, proof_file=str(path),
                                               max_checkpoint_age_ms=7*86400000))]
    cfg['history']['enabled'] = False
    cfg['startup']['max_baseline_age_ms'] = 3600000
    cfg['journal'].update(transport='http-development', source_urls=[owner], poll_interval_ms=2000,
                          request_timeout_ms=60000, max_payload_bytes=16<<20)
    config = output/'owner-config.json'
    config.write_text(json.dumps(cfg))
    with (output/'owner.log').open('w') as stream:
        child = subprocess.Popen([binary, '--config', config], stdout=stream, stderr=subprocess.STDOUT,
                                 start_new_session=True)
        def baseline(status):
            until = time.monotonic()+150
            while time.monotonic()<until and child.poll() is None:
                res = http(owner+'/internal/chains/evm:1/baseline', token)
                if res['status']==status:
                    return res['body']
                time.sleep(.2)
            raise ValueError('OWNER_BASELINE_STATUS_'+str(status))
        try:
            first = baseline(200)['reference']
            if first['block']['hash'] != execution['block_hash'] or first['block']['number_or_slot'] != int(execution['block_number']):
                raise ValueError('OWNER_FINALIZED_BLOCK')
            install({})
            baseline(503)
            install(proof)
            recovered = baseline(200)['reference']
            if recovered['block']!=first['block'] or recovered['journal_seq']<=first['journal_seq']:
                raise ValueError('OWNER_RECOVERY')
            return {'passed':True, 'first':first,'revoked_status':503,'recovered':recovered}
        finally:
            stop(child)


if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    for key in ('binary','output','bootstrap','finality'):
        parser.add_argument(key,type=Path)
    parser.add_argument('root')
    parser.add_argument('--updates',type=Path)
    args=parser.parse_args()
    result=exercise(args.binary.resolve(), args.output.resolve(),args.bootstrap,args.updates,args.finality,args.root)
    (args.output/'report.json').write_text(json.dumps(result,indent=2))
