#!/usr/bin/env python3
"""Independent raw banks, exact credit, vault dust, hard limit and original reexecution."""
from pathlib import Path
import base64,hashlib,importlib.util,json,sys
from meteora_mm_reference import ROOT,PAIR,CODE,raw,u64,state,require
from r01_public_catalog_solana_rebuild import b58encode
TOOL=ROOT/'integration/tools/ci/meteora-exact-out-reference.py'
spec=importlib.util.spec_from_file_location('meteora_xo_original',TOOL);original=importlib.util.module_from_spec(spec);spec.loader.exec_module(original)
ORIGINAL=original.TARGET;PROJECTED=ROOT/'integration/test/contracts/meteora-exact-out.json'
def sha(p):return hashlib.sha256(Path(p).read_bytes()).hexdigest()
def verify(evidence,projected,source_digest,reexecute=False):
    require(evidence['generator_sha256']==projected['generator_sha256']==sha(TOOL),'xo-generator');require(projected['source_sha256']==source_digest,'xo-source');require(evidence['native_helper_sha256']==sha(original.BASE/'harness/native_preflight.py'),'xo-helper');require(evidence['source_metadata_sha256']==sha(original.BASE/'source-metadata.json'),'xo-metadata')
    selected=original.selectors();require(len(evidence['records'])==len(projected['cases'])==len(selected)==164,'xo-coverage')
    full=rejected=dust=controls=0
    for index,(e,c,(mode,zero,target))in enumerate(zip(evidence['records'],projected['cases'],selected)):
        n=e['native'];require(e['index']==c['index']==index and e['mode']==mode and c['name']==mode['name'],'xo-case-identity');require(n['kind']=='EXACT_OUT'and n['specified_amount']==str(target)and n['limit']==str(original.BUDGET)and n['zero_for_one']==zero and n['program_sha256']==CODE and n['controlled_mm_only']is True,'xo-original-template')
        steps=n['sequence']if n['passed']else[n];expected_steps=3 if n['passed']and index<120 and target==1000000 else 1;require(len(steps)==len(c['steps'])==expected_steps,'xo-sequence-coverage');previous=None
        for native,p in zip(steps,c['steps']):
            before,after=native['before'],native['after'];require(previous is None or before==previous,'xo-sequence-continuity');require(set(before)==set(after),'xo-bank-membership');pair=state(before,p['pool'],n['clock']);state(after,p['after'],n['clock']);require(p['target']==native['specified_amount']and p['required_input']==native['payer_input_delta']and p['zero_for_one']==zero and p['passed']==native['passed']==('err'not in native['native']),'xo-step-binding')
            if not native['passed']:
                require(before==after and native['payer_input_delta']==native['recipient_output_delta']=='0','xo-rollback');rejected+=1;continue
            require(native['recipient_output_delta']==native['specified_amount'],'xo-exact-recipient-credit');logs=native['native'].get('logs',[]);require(any('Instruction: SwapExactOut2'in s for s in logs),'xo-actual-instruction')
            pay=[];receive=[];input_mint=pair['token_x_mint'if zero else'token_y_mint'];output_mint=pair['token_y_mint'if zero else'token_x_mint'];bin_debit=0
            for key,b in before.items():
                x,y=raw(b),raw(after[key]);require(len(x)==len(y)and b['owner']==after[key]['owner']and b['executable']==after[key]['executable'],'xo-account-shape')
                if len(x)==10136:
                    side=8 if zero else 0;bin_debit+=sum(u64(x,56+j*144+side)-u64(y,56+j*144+side)for j in range(70))
                if len(x)!=165:continue
                delta=u64(y,64)-u64(x,64)
                if x[109:113]==b'\x01\0\0\0':require(after[key]['lamports']-b['lamports']==delta,'xo-wsol-backing')
                if key in [pair['reserve_x'],pair['reserve_y']]:continue
                if b58encode(x[:32])==input_mint and b58encode(x[32:64])==n['payer']:pay.append(delta)
                if b58encode(x[:32])==output_mint and b58encode(x[32:64])==n['recipient']:receive.append(delta)
            require(len(pay)==len(receive)==1 and -pay[0]==int(native['payer_input_delta'])>0 and receive[0]==int(native['specified_amount']),'xo-physical-funds')
            vin=pair['reserve_x'if zero else'reserve_y'];vout=pair['reserve_y'if zero else'reserve_x'];require(u64(raw(after[vin]),64)-u64(raw(before[vin]),64)==-pay[0]and u64(raw(before[vout]),64)-u64(raw(after[vout]),64)==receive[0],'xo-vault-transfers')
            require(bin_debit>=receive[0],'xo-bin-output-coverage');fee_offset=224 if zero else 216;require(u64(raw(after[PAIR]),fee_offset)==u64(raw(before[PAIR]),fee_offset),'xo-no-fictitious-output-protocol-fee')
            if bin_debit>receive[0]:dust+=1
            previous=after;full+=1
        control=e['input_minus_one']
        if n['passed']:
            require(control is not None and control['passed']is False and 'err'in control['native']and control['payer_input_delta']==control['recipient_output_delta']=='0'and control['kind']=='EXACT_OUT'and control['specified_amount']==str(target)and control['limit']==str(int(n['payer_input_delta'])-1)and control['before']==n['before']==control['after'],'xo-input-limit-control');controls+=1
        else:require(control is None,'xo-unexpected-control')
        if reexecute:
            followups=(10000,20000)if index<120 and target==1000000 else()
            fresh=original.execute(mode,zero,target,followups=followups)['native'];
            if n['passed']:
                negative=original.execute(mode,zero,target,budget=int(n['payer_input_delta'])-1)['native']
                require(not negative['passed']and 'err'in negative['native']and negative['before']==control['before']==negative['after']==control['after'],'xo-fresh-hard-input-limit')
            require(fresh['passed']==n['passed']and fresh['before']==n['before']and fresh['after']==n['after']and fresh['payer_input_delta']==n['payer_input_delta']and fresh['recipient_output_delta']==n['recipient_output_delta'],'xo-fresh-original-bank')
            require(len(fresh['sequence'])==len(n['sequence']),'xo-fresh-sequence-coverage')
            for a,b in zip(fresh['sequence'],n['sequence']):require(a['before']==b['before']and a['after']==b['after']and a['payer_input_delta']==b['payer_input_delta']and a['recipient_output_delta']==b['recipient_output_delta'],'xo-fresh-original-sequence')
    require((full,rejected,controls)==(162,22,142)and dust>20,'xo-result-coverage')
    return dict(passed=True,cases=164,full_swap_steps=full,rejected=rejected,input_limit_controls=controls,vault_dust_steps=dust,reexecuted=reexecute,all_requested_features_complete=False,production_ready=False)
if __name__=='__main__':print(json.dumps(verify(json.loads(ORIGINAL.read_bytes()),json.loads(PROJECTED.read_bytes()),sha(ORIGINAL),reexecute='--reexecute'in sys.argv)))
