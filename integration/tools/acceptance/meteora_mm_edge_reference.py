#!/usr/bin/env python3
"""Raw evidence and actual token settlement for the additional SBF boundary banks."""
from pathlib import Path
import base64,hashlib,json
from meteora_mm_reference import ROOT,PAIR,CODE,require,raw,u64,state
from r01_public_catalog_solana_rebuild import b58encode
ORIGINAL=ROOT/'integration/artifacts/local/core-meteora-mm-edge-original-20261110.json';PROJECTED=ROOT/'integration/test/contracts/meteora-mm-edge.json'
def verify(evidence,projected,source_digest):
    tool=ROOT/'integration/tools/ci/meteora-mm-edge-reference.py';require(evidence['generator_sha256']==projected['generator_sha256']==hashlib.sha256(tool.read_bytes()).hexdigest(),'edge-generator');require(projected['source_sha256']==source_digest,'edge-source')
    require(len(evidence['records'])==len(projected['cases'])==25,'edge-coverage');passed=rejected=0;names=set()
    for entry,c in zip(evidence['records'],projected['cases']):
        n=entry['native'];require(c['name']==entry['mode']['name']and c['name']not in names,'edge-identity');names.add(c['name']);require(n['program_sha256']==CODE and n['controlled_mm_only']is True and ('err'not in n['native'])==n['passed']==c['passed'],'edge-native-verdict')
        pair=state(n['before'],c['pool'],n['clock']);state(n['after'],c['after'],n['clock']);require(c['input']==n['specified_amount']and c['output']==n['recipient_output_delta']and c['zero_for_one']==n['zero_for_one'],'edge-amounts')
        if not n['passed']:
            require(n['before']==n['after']and n['payer_input_delta']==n['recipient_output_delta']=='0','edge-failure-rollback');rejected+=1;continue
        zero=n['zero_for_one'];input_mint=pair['token_x_mint'if zero else'token_y_mint'];output_mint=pair['token_y_mint'if zero else'token_x_mint'];pay=[];receive=[]
        for key,b in n['before'].items():
            x,y=raw(b),raw(n['after'][key]);require(len(x)==len(y)and b['owner']==n['after'][key]['owner'],'edge-account-layout')
            if len(x)!=165:continue
            delta=u64(y,64)-u64(x,64)
            if x[109:113]==b'\x01\0\0\0':require(n['after'][key]['lamports']-b['lamports']==delta,'edge-native-backing')
            if key in [pair['reserve_x'],pair['reserve_y']]:continue
            if b58encode(x[:32])==input_mint and b58encode(x[32:64])==n['payer']:pay.append(delta)
            if b58encode(x[:32])==output_mint and b58encode(x[32:64])==n['recipient']:receive.append(delta)
        require(len(pay)==len(receive)==1 and -pay[0]==int(c['input'])and receive[0]==int(c['output'])>0,'edge-real-payer-recipient')
        if entry['mode']['name'].startswith('oracle-'):
            oracle_id=pair['oracle'];before,after=raw(n['before'][oracle_id]),raw(n['after'][oracle_id]);i=u64(before,8);length=u64(before,24);age=entry['mode']['age'];require(u64(after,8)==((i+1)%length if age>=120 else i),'edge-lifetime-ring-index')
        passed+=1
    require((passed,rejected)==(23,2),'edge-results')
    return dict(passed=True,cases=25,original_swaps=passed,rejected=rejected,all_requested_features_complete=False,production_ready=False)
if __name__=='__main__':print(json.dumps(verify(json.loads(ORIGINAL.read_bytes()),json.loads(PROJECTED.read_bytes()),hashlib.sha256(ORIGINAL.read_bytes()).hexdigest())))
