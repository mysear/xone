#!/usr/bin/env python3
"""Independent fixed-source, raw-transfer and full account coverage checks."""
import base64,hashlib,importlib.util,json,sys
from pathlib import Path
from weighted_evidence import ROOT
sys.path.insert(0,str(ROOT/'integration/tools/reference/pump-bonding/harness'))
from layout import account as pump_account

def require(ok,why):
    if not ok:raise ValueError('FAMILY_REFERENCE:'+why)
def digest(p):return hashlib.sha256(Path(p).read_bytes()).hexdigest()
def load_module(name,path):
    spec=importlib.util.spec_from_file_location(name,ROOT/path);m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m);return m

def verify_pump(reexecute=False):
    root=ROOT/'integration/tools/reference/pump-bonding';pins=json.loads((root/'source-pins.json').read_bytes())
    for item in pins['files']:require((root/item['path']).stat().st_size==item['bytes']and digest(root/item['path'])==item['sha256'],'pump-source-file:'+item['path'])
    acquisition=json.loads((root/'public-bank-pins.json').read_bytes())
    for item in acquisition['files']:require(digest(root/item['path'])==item['sha256'],'pump-original-account:'+item['path'])
    require(digest(root/'upstream/artifacts/pump.so')=='7a19affaa2c6187e04a5fe0ea298cee81fa5eb8c24cfcc30d85f615e89f7b02d','pump-code')
    require(digest(root/'upstream/artifacts/fee.so')=='4f2955d0f6b40cc9ff7daa18ccad7d19119ba1249759f4e7e3bb691beee6b35e','pump-fee-code')
    evidence_path=ROOT/'integration/artifacts/local/core-pump-bonding-original-20261111.json';e=json.loads(evidence_path.read_bytes());f=json.loads((ROOT/'integration/test/contracts/pump-bonding.json').read_bytes());gen=load_module('pump_ref','integration/tools/ci/pump-bonding-reference.py')
    require(f['source_sha256']==digest(evidence_path)and e['generator_sha256']==digest(gen.__file__)and e['native_sha256']==digest(gen.native.__file__),'pump-source-bindings');require(len(e['records'])==len(f['cases'])==60,'pump-case-coverage');full=rejected=0
    for index,(c,row)in enumerate(zip(e['records'],f['cases'])):
        require(c['index']==row['index']==index and c['kind']==row['kind'],'pump-case-identity');steps=c['sequence']if c['passed']else[c]
        require(len(steps)==len(row['steps'])==(3 if c['passed']and c['amount']=='1000000'else 1),'pump-sequence-coverage');previous=None
        for n,s in zip(steps,row['steps']):
            require(n['passed']==s['passed']==('err'not in n['native']),'pump-original-result');require(previous is None or previous==n['before'],'pump-sequence-banks')
            require(gen.pool(n['before'],c['ids'])==s['pool']and gen.pool(n['after'],c['ids'])==s['after'],'pump-complete-projection')
            require(n['amount']==s['amount']and n['user_sol_delta']==s['user_sol_delta']and n['user_token_delta']==s['user_token_delta'],'pump-projected-funds')
            if not n['passed']:require(n['before']==n['after']and n['user_sol_delta']==n['user_token_delta']==0,'pump-rollback');rejected+=1;continue
            before,after=n['before'],n['after'];require(set(before)==set(after),'pump-bank-membership')
            user=c['ids']['user'];u_ata=c['ids']['user_ata'];delta_sol=after[user]['lamports']-before[user]['lamports'];b=base64.b64decode(before[u_ata]['data_base64']);a=base64.b64decode(after[u_ata]['data_base64']);delta_token=int.from_bytes(a[64:72],'little')-int.from_bytes(b[64:72],'little')
            require(delta_sol==n['user_sol_delta']and delta_token==n['user_token_delta'],'pump-actual-user-funds');require(sum(a['lamports']for a in after.values())==sum(a['lamports']for a in before.values()),'pump-native-conservation')
            bv=base64.b64decode(before[c['ids']['vault']]['data_base64']);av=base64.b64decode(after[c['ids']['vault']]['data_base64']);require(int.from_bytes(av[64:72],'little')-int.from_bytes(bv[64:72],'little')==-delta_token,'pump-token-conservation')
            if n['kind']=='buy':require(delta_token==int(n['amount'])and -delta_sol<=int(n['limit']),'pump-original-buy')
            elif n['kind']=='sell':require(-delta_token==int(n['amount'])and delta_sol>=int(n['limit']),'pump-original-sell')
            else:require(-delta_sol==int(n['amount'])and delta_token>=int(n['limit']),'pump-fixed-input')
            previous=after;full+=1
        if reexecute:
            fresh=gen.native.execute(kind=c['kind'],amount=int(c['amount']),limit=int(c['limit']),complete=row['complete'],followups=(10000,20000)if c['amount']=='1000000'else())
            require(fresh['passed']==c['passed']and fresh['before']==c['before']and fresh['after']==c['after']and len(fresh['sequence'])==len(c['sequence']),'pump-fresh-original')
            for a,b in zip(fresh['sequence'],c['sequence']):require(a['before']==b['before']and a['after']==b['after'],'pump-fresh-original-sequence')
    require(full==22 and rejected==44,'pump-total-results')
    return dict(passed=True,initial_cases=60,full_steps=full,rejected=rejected,reexecuted=reexecute)

def verify_only_y(reexecute=False):
    gen=load_module('only_y_ref','integration/tools/ci/meteora-only-y-reference.py');path=gen.TARGET;e=json.loads(path.read_bytes());f=json.loads((ROOT/'integration/test/contracts/meteora-only-y.json').read_bytes());require(f['source_sha256']==digest(path)and f['generator_sha256']==e['generator_sha256']==digest(gen.__file__),'only-y-source')
    require(len(e['records'])==len(f['cases'])==len(gen.selectors())==236,'only-y-coverage');full=rejected=controls=0
    for index,(c,row,(mode,zero,amount,exact))in enumerate(zip(e['records'],f['cases'],gen.selectors())):
        n=c['native'];require(c['index']==row['index']==index and c['mode']==mode and n['kind']==('EXACT_OUT'if exact else'EXACT_IN'),'only-y-identity');steps=n['sequence']if n['passed']else[n]
        require(len(steps)==len(row['steps'])==(3 if n['passed']and index<200 and amount==1000000 else 1),'only-y-sequence')
        for a,s in zip(steps,row['steps']):
            require(s['pool']==gen.pool(a['before'],n['clock'],n['program_sha256'])and s['after']==gen.pool(a['after'],n['clock'],n['program_sha256']),'only-y-complete-projection');require(a['passed']==s['passed']==('err'not in a['native'])and s['specified_amount']==a['specified_amount']and s['actual_input']==a['payer_input_delta']and s['actual_output']==a['recipient_output_delta'],'only-y-original-verdict')
            if not a['passed']:require(a['before']==a['after'],'only-y-failure-rollback');rejected+=1;continue
            if exact:require(a['recipient_output_delta']==a['specified_amount'],'only-y-exact-target')
            else:require(a['payer_input_delta']==a['specified_amount'],'only-y-exact-input')
            # Native input/output token accounts are explicit in the retained
            # original bank. The projection preserves all pool-owned bytes.
            p=a['before'];q=a['after'];deltas={}
            for key,b in p.items():
                rb=base64.b64decode(b['data_base64']);ra=base64.b64decode(q[key]['data_base64'])
                if len(rb)==165:
                    delta=int.from_bytes(ra[64:72],'little')-int.from_bytes(rb[64:72],'little');deltas[rb[:32]]=deltas.get(rb[:32],0)+delta
                    if rb[109:113]==b'\x01\0\0\0':require(q[key]['lamports']-b['lamports']==delta,'only-y-native-backing')
            require(all(v==0 for v in deltas.values()),'only-y-total-token-conservation');full+=1
        if n['passed']and exact:
            control=c['input_minus_one'];require(control and not control['passed']and 'err'in control['native']and control['before']==n['before']==control['after'],'only-y-input-limit');controls+=1
        if reexecute:
            fresh=gen.execute(mode,zero,amount,exact,followups=(10000,20000)if index<200 and amount==1000000 else())['native'];require(fresh['passed']==n['passed']and fresh['before']==n['before']and fresh['after']==n['after']and len(fresh['sequence'])==len(n['sequence']),'only-y-fresh-bank')
            for a,b in zip(fresh['sequence'],n['sequence']):require(a['before']==b['before']and a['after']==b['after'],'only-y-fresh-sequence')
    require((full,rejected)==(237,39),'only-y-results');return dict(passed=True,cases=236,full_steps=full,rejected=rejected,input_controls=controls,reexecuted=reexecute)
if __name__=='__main__':print(json.dumps(dict(passed=True,pump=verify_pump('--reexecute'in sys.argv),only_y=verify_only_y('--reexecute'in sys.argv))))
