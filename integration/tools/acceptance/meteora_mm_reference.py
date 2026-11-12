#!/usr/bin/env python3
"""Independent raw-bank, transfer, source-pin and projected-state verification."""
from pathlib import Path
import base64,hashlib,json,sys
ROOT=Path(__file__).resolve().parents[3]
sys.path.insert(0,str(ROOT/'integration/tools/reference/meteora-dlmm/harness'))
from idl_layout import BASE,IDL,decode
PAIR='EtAdVRLFH22rjWh3mcUasKFF27WtHhsaCvK27tPFFWig';PROGRAM=IDL['address']
ORIGINAL=ROOT/'integration/artifacts/local/core-meteora-dlmm-mm-original-20261110.json'
PROJECTED=ROOT/'integration/test/contracts/meteora-mm.json'
CODE='942f609d03440911f15c40e8df1733d808670768fd9042af1de956964fb2f0e6'
def require(ok,message):
    if not ok:raise ValueError('METEORA_REFERENCE:'+message)
def raw(account):return base64.b64decode(account['data_base64'],validate=True)
def u64(data,offset):return int.from_bytes(data[offset:offset+8],'little')
def sha(path):return hashlib.sha256(Path(path).read_bytes()).hexdigest()
def state(bank,pool,clock):
    require(set(pool)=={'chain_key','pool_id','protocol','adapter_version','token_behavior','token0','token1','reserve0','reserve1','fee_numerator','fee_denominator','meteora'},'pool-fields')
    pair,end=decode({'defined':{'name':'LbPair'}},raw(bank[PAIR]),8)
    require(end==904 and bank[PAIR]['owner']==PROGRAM and not bank[PAIR]['executable'],'pair-layout-owner')
    require(pool['pool_id']==PAIR and pool['protocol']=='meteora-dlmm-mm'and pool['adapter_version']=='meteora-dlmm/1532e540-mm-1'and pool['token0']==pair['token_x_mint']and pool['token1']==pair['token_y_mint'],'projected-pool-identity')
    m=pool['meteora'];require(m['pair_data']==raw(bank[PAIR]).hex()and m['oracle_data']==raw(bank[pair['oracle']]).hex(),'complete-pair-oracle-bytes')
    require(m['slot']==clock['slot']and m['timestamp']==clock['unix_timestamp']and m['program_code_sha256']==CODE and not m.get('derived',False),'projected-bank-identity')
    arrays=[]
    for address,a in bank.items():
        if len(raw(a))!=10136:continue
        value,end=decode({'defined':{'name':'BinArray'}},raw(a),8)
        require(end==10136 and value['lb_pair']==PAIR and a['owner']==PROGRAM,'complete-array-layout')
        arrays.append(dict(address=address,index=value['index'],data=raw(a).hex()))
    require(m['bin_arrays']==sorted(arrays,key=lambda a:a['index']),'complete-array-membership-order-bytes')
    for side,name in enumerate(['reserve_x','reserve_y']):
        balance=u64(raw(bank[pair[name]]),64);require(m['raw_vault'+str(side)]==balance and pool['reserve'+str(side)]==str(balance),'projected-vault')
    return pair

def verify(original,projected,source_sha256,verify_files=True):
    require(original['passed']is True and original['upstream_commit']=='1532e540db41ac405dbf4c5bb2ddfceaa1f98ff1','reference-identity')
    require(projected['source_sha256']==source_sha256,'source-commitment')
    if verify_files:
        metadata=json.loads((BASE/'source-metadata.json').read_bytes());require(original['source_metadata_sha256']==sha(BASE/'source-metadata.json')==projected['source_metadata_sha256'],'source-metadata')
        for item in metadata['files']:
            path=BASE/item['path'];require(path.stat().st_size==item['bytes']and sha(path)==item['sha256'],'upstream-source:'+item['path'])
        require(original['generator_sha256']==sha(BASE/'harness/reference.py'),'reference-generator')
        for name,expected in original['helper_sha256'].items():require(sha(ROOT/name)==expected,'reference-helper:'+name)
        require(projected['projector_sha256']==sha(ROOT/'integration/tools/ci/meteora-mm-state.py'),'projector-version')
    cases=original['cases'];rows=projected['cases'];require(len(cases)==len(rows)==70,'case-coverage')
    full=failed=steps=0
    for index,(c,row)in enumerate(zip(cases,rows)):
        require(c['index']==row['index']==index and c['controlled_mm_only']is True and c['program_sha256']==CODE and c['kind']=='EXACT_IN','case-identity')
        original_steps=c['sequence']if c['passed']else[c];require(len(row['steps'])==len(original_steps),'sequence-coverage');previous=None
        for step,p in zip(original_steps,row['steps']):
            before,after=step['before'],step['after'];require(set(before)==set(after),'bank-membership');require(previous is None or previous==before,'sequence-bank-continuity')
            pair=state(before,p['pool'],c['clock']);state(after,p['after'],c['clock'])
            require(p['passed']==step['passed']and p['input']==step['specified_amount']and p['output']==step['recipient_output_delta']and p['zero_for_one']==c['zero_for_one'],'step-request-result')
            require(('err'not in step['native'])==step['passed'],'native-verdict')
            if not step['passed']:
                require(before==after and step['payer_input_delta']=='0'and step['recipient_output_delta']=='0','failed-bank-rollback');failed+=1;continue
            logs=step['native'].get('logs',[]);require(any('Instruction: Swap2'in log for log in logs)and sum('Program '+PROGRAM+' success'==log for log in logs)>=1,'native-swap2-executed')
            zero=c['zero_for_one'];input_mint=pair['token_x_mint'if zero else'token_y_mint'];output_mint=pair['token_y_mint'if zero else'token_x_mint']
            tokens=[]
            for address,b in before.items():
                rb,ra=raw(b),raw(after[address]);require(len(rb)==len(ra)and b['owner']==after[address]['owner']and b['executable']==after[address]['executable'],'account-layout-continuity')
                if len(rb)==165:
                    value0=u64(rb,64);value1=u64(ra,64);delta=value1-value0
                    if rb[109:113]==b'\x01\0\0\0':require(after[address]['lamports']-b['lamports']==delta,'native-token-lamport-conservation')
                    if address not in [pair['reserve_x'],pair['reserve_y']]:tokens.append((address,rb,delta))
            from r01_public_catalog_solana_rebuild import b58encode
            pay=[delta for _,r,delta in tokens if b58encode(r[:32])==input_mint and b58encode(r[32:64])==c['payer']]
            receive=[delta for _,r,delta in tokens if b58encode(r[:32])==output_mint and b58encode(r[32:64])==c['recipient']]
            require(len(pay)==len(receive)==1 and -pay[0]==int(step['specified_amount'])==int(step['payer_input_delta'])and receive[0]==int(step['recipient_output_delta'])>0,'actual-payer-recipient-balances')
            in_vault=pair['reserve_x'if zero else'reserve_y'];out_vault=pair['reserve_y'if zero else'reserve_x']
            require(u64(raw(after[in_vault]),64)-u64(raw(before[in_vault]),64)==-pay[0]and u64(raw(before[out_vault]),64)-u64(raw(after[out_vault]),64)==receive[0],'vault-transfer-conservation')
            previous=after;full+=1;steps+=1
    require((full,failed)==(55,35),'original-result-coverage')
    return dict(passed=True,cases=70,original_swap_steps=full,rejected=failed,upstream_files=138 if verify_files else None,all_requested_features_complete=False,production_ready=False)
if __name__=='__main__':
    print(json.dumps(verify(json.loads(ORIGINAL.read_bytes()),json.loads(PROJECTED.read_bytes()),sha(ORIGINAL))))
