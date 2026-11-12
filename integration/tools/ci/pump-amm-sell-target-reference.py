#!/usr/bin/env python3
"""Fixed original PumpSwap sell minimum-credit targets and input-minus-one controls."""
from pathlib import Path
import base64, hashlib, importlib.util, json, copy, struct
ROOT = Path(__file__).resolve().parents[3]
def module(name, rel):
    spec = importlib.util.spec_from_file_location(name, ROOT / rel)
    m = importlib.util.module_from_spec(spec); spec.loader.exec_module(m); return m
ref = module('amm_target_projection', 'integration/tools/ci/pump-amm-reference.py')
OUT = ROOT/'integration/artifacts/local/core-pump-amm-sell-target-original-20261112.json'
def sha(p): return hashlib.sha256(Path(p).read_bytes()).hexdigest()
def required(p, target):
    if target <= 0 or target >= int(p['reserve1']): return None
    accounts = {a['address']: bytes.fromhex(a['data']) for a in p['pump']['accounts']}
    base, real = int(p['reserve0']), int(p['reserve1'])
    quote = real + int.from_bytes(accounts[p['pool_id']][245:261], 'little', signed=True)
    supply = int.from_bytes(accounts[p['token0']][36:44], 'little')
    cfg = next(raw for key, raw in accounts.items() if len(raw) == 4097)
    fees, _ = ref.native.account('FeeConfig', cfg, 'pump_fees')
    tier = fees['fee_tiers'][0]
    for t in fees['fee_tiers']:
        if t['market_cap_lamports_threshold'] <= supply*quote//base: tier = t
    rates = [tier['fees'][key] for key in ['lp_fee_bps','protocol_fee_bps','creator_fee_bps']]
    def charges(n): return [(n*r+9999)//10000 for r in rates]
    slope = 10000-sum(rates); lower=1
    for _ in range(32):
        candidates=[]
        for residue in range(10000):
            net = residue-sum(charges(residue))
            periods=max(0,(target-net+slope-1)//slope,(lower-residue+9999)//10000)
            gross=periods*10000+residue
            if lower <= gross < quote: candidates.append(gross)
        if not candidates: return None
        gross=min(candidates); amount=(gross*base+quote-gross-1)//(quote-gross)
        if amount > supply-base: return None
        actual=amount*quote//(base+amount); parts=charges(actual)
        if actual-parts[0] > real: return None
        if actual-sum(parts) >= target: return amount
        lower=actual+1
    raise ValueError('TARGET_INVERSION_BUDGET')
def execute(boost, amount, limit, high_price=False):
    initial=json.loads((ROOT/'integration/artifacts/local/core-pump-migration-original-20261111.json').read_bytes())['migration'] if boost else None
    if high_price:
        initial=copy.deepcopy(initial)
        bank=ref.native.execute(kind='sell',amount=1,limit=1,initial_migration=initial)
        key=bank['ids']['pool_quote_token_account']; a=initial['after'][key]
        raw=bytearray(base64.b64decode(a['data_base64'])); rent=a['lamports']-int.from_bytes(raw[64:72],'little')
        struct.pack_into('<Q',raw,64,10**19); a['data_base64']=base64.b64encode(raw).decode(); a['lamports']=rent+10**19
    return ref.native.execute(kind='sell', amount=amount, limit=limit, initial_migration=initial)
def main():
    records=[]; cases=[]
    for boost, high_price in [(False,False), (True,False), (True,True)]:
        bank=execute(boost,1,1,high_price); p=ref.pool(bank['before'],bank['ids'])
        for target in [0,1,2,10,100,999,1000,1001,9899,9900,9901,10000,1000000,1000000000,3000000000,int(p['reserve1'])+1]:
            amount=required(p,target); n=control=None
            if amount is not None:
                n=execute(boost,amount,target,high_price)
                if not n['passed'] or n['user_quote_delta'] < target: raise ValueError(('ORIGINAL_TARGET_FAILED', boost,target,n['native']))
                control=execute(boost,amount-1,target,high_price)
                if control['passed'] or control['before'] != control['after'] or control['before'] != n['before']: raise ValueError('ORIGINAL_MIN_INPUT_FAILED')
            index=len(cases)
            records.append(dict(index=index,boost=boost,high_price=high_price,target=str(target),native=n,input_minus_one=control))
            cases.append(dict(index=index,boost=boost,high_price=high_price,target=str(target),passed=n is not None,pool=p,required_input=str(amount) if n else None,actual_output=str(n['user_quote_delta']) if n else None,after=ref.pool(n['after'],n['ids']) if n else None))
    OUT.write_text(json.dumps(dict(schema_version=1,passed=True,generator_sha256=sha(__file__),native_sha256=sha(ref.native.__file__),records=records,authority='controlled original PumpSwap sell minimum-output targets; canonical, actual migration-created boost pool, and explicitly controlled high-quote WSOL-backed bank; no public submission'),separators=(',',':'))+'\n')
    f=dict(schema_version=1,source_sha256=sha(OUT),cases=cases)
    for service in ['integration','dex-router']:
        (ROOT/service/'test/contracts/pump-amm-sell-target.json').write_text(json.dumps(f,separators=(',',':'))+'\n')
    print(json.dumps(dict(cases=len(cases),success=sum(c['passed'] for c in cases),surplus=sum(c['passed'] and c['actual_output']!=c['target'] for c in cases))))
if __name__=='__main__': main()
