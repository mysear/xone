#!/usr/bin/env python3
"""Fixed original Pump legacy classic/SOL instruction reference, not a V2 substitute."""
from pathlib import Path
import base64,hashlib,json,sys
ROOT=Path(__file__).resolve().parents[3];BASE=ROOT/'integration/tools/reference/pump-bonding';sys.path.insert(0,str(BASE/'harness'))
import native
from layout import account
sys.path.insert(0,str(ROOT/'integration/tools/acceptance'));from r01_public_catalog_solana_rebuild import programdata_pda
PROFILE='pump-bonding-sol-classic';ADAPTER='pump-bonding/7a19affaa2c6-sol-1';NATIVE='native:solana:sol'
CODE=hashlib.sha256((BASE/'upstream/artifacts/pump.so').read_bytes()).hexdigest();FEE_CODE=hashlib.sha256((BASE/'upstream/artifacts/fee.so').read_bytes()).hexdigest()
def pool(bank,ids):
    keys=[ids[k]for k in ['curve','mint','vault','creator_vault','fee_recipient','buyback','global_id','fee_id','global_volume','user_volume']];records=[]
    for key in sorted(keys):
        a=bank[key];records.append(dict(address=key,owner=a['owner'],lamports=a['lamports'],data=base64.b64decode(a['data_base64']).hex()))
    curve,_=account('BondingCurve',base64.b64decode(bank[ids['curve']]['data_base64']))
    state=dict(programdata=programdata_pda(native.PUMP),deployed_slot=458921440,program_code_sha256=CODE,fee_programdata=programdata_pda(native.FEES),fee_deployed_slot=456132171,fee_program_code_sha256=FEE_CODE,token_program_code_sha256='53e587eee80c786bcfba1450397d92179a0f01def776d19fb4ddf690d64431cc',slot=467957307,timestamp=1794355000,user=ids['user'],vault=ids['vault'],creator_vault=ids['creator_vault'],fee_recipient=ids['fee_recipient'],buyback_recipient=ids['buyback'],accounts=records)
    return dict(chain_key='solana:mainnet-beta',pool_id=ids['curve'],protocol=PROFILE,adapter_version=ADAPTER,token_behavior='standard',token0=ids['mint'],token1=NATIVE,reserve0=str(curve['real_token_reserves']),reserve1=str(curve['real_quote_reserves']),fee_numerator=0,fee_denominator=10000,pump=state)
def main():
    records=[];cases=[]
    rows=[(kind,amount,complete)for kind in ['buy','sell','buy_exact_sol_in']for complete in [False,True]for amount in [1,2,3,10,100,1000,1000000,1000000000,10000000000,1000000000000000]]
    for index,(kind,amount,complete)in enumerate(rows):
        c=native.execute(kind=kind,amount=amount,limit=10**15 if kind=='buy'else 1,complete=complete,followups=(10000,20000)if amount==1000000 else());c['index']=index;records.append(c);steps=[]
        for n in c['sequence']if c['passed']else[c]:steps.append(dict(pool=pool(n['before'],c['ids']),kind=kind,amount=n['amount'],passed=n['passed'],user_sol_delta=n['user_sol_delta'],user_token_delta=n['user_token_delta'],after=pool(n['after'],c['ids'])))
        cases.append(dict(index=index,kind=kind,complete=complete,steps=steps))
    out=ROOT/'integration/artifacts/local/core-pump-bonding-original-20261111.json';out.write_text(json.dumps(dict(schema_version=1,generator_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),native_sha256=hashlib.sha256(Path(native.__file__).read_bytes()).hexdigest(),program_code_sha256=CODE,fee_program_code_sha256=FEE_CODE,authority='controlled constructed classic/SOL banks; acquired fixed Pump and Fee binaries with actual Token/System CPIs; no authenticated canonical source bank',records=records,production_ready=False),indent=2)+'\n')
    result=dict(schema_version=1,source_sha256=hashlib.sha256(out.read_bytes()).hexdigest(),cases=cases)
    for name in ['integration/test/contracts/pump-bonding.json','dex-router/test/contracts/pump-bonding.json']:(ROOT/name).write_text(json.dumps(result,separators=(',',':'))+'\n')
    print(json.dumps(dict(passed=True,cases=len(cases),full=sum(c['passed']for c in records),rejected=sum(not c['passed']for c in records))))
if __name__=='__main__':main()
