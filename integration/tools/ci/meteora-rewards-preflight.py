#!/usr/bin/env python3
"""Characterize real reward growth in the pinned SBF, with controlled active banks."""
from pathlib import Path
import copy,hashlib,importlib.util,json,shutil,sys,tempfile
ROOT=Path(__file__).resolve().parents[3];BASE=ROOT/'integration/tools/reference/meteora-dlmm';sys.path.insert(0,str(BASE/'harness'))
import native_extended as native
from idl_layout import account,decode,encode
sys.path.insert(0,str(ROOT/'integration/tools/acceptance'))
from r01_public_catalog_solana_bank import native_imports

def execute(zero=True,exact=False,amount=1000000,end=1734392500,last=1734392400,rate=1000<<64,empty=False,followups=(),second=False):
    source=BASE/'commons/tests/fixtures/EtAdVRLFH22rjWh3mcUasKFF27WtHhsaCvK27tPFFWig';pair,_=account('LbPair',source/'lb_pair.bin');_,Pubkey,*_=native_imports();pid=Pubkey.from_string(native.IDL['address']);pk=Pubkey.from_string(source.name)
    reward=dict(mint=pair['token_x_mint'],vault=str(Pubkey.find_program_address([bytes(pk),(0).to_bytes(8,'little')],pid)[0]),funder=pair['creator'],reward_duration=100,reward_duration_end=end,reward_rate=rate,last_update_time=last,cumulative_seconds_with_empty_liquidity_reward=7);pair['reward_infos'][0]=reward
    if second:pair['reward_infos'][1]=dict(reward,mint=pair['token_y_mint'],vault=str(Pubkey.find_program_address([bytes(pk),(1).to_bytes(8,'little')],pid)[0]),reward_rate=500<<64)
    with tempfile.TemporaryDirectory(prefix='xone-rewards-bank-')as temp:
        base=Path(temp);folder=base/'commons/tests/fixtures'/source.name;shutil.copytree(source,folder);dest=base/'commons/tests/artifacts/lb_clmm_prod.so';dest.parent.mkdir(parents=True);dest.write_bytes((BASE/'commons/tests/artifacts/lb_clmm_prod.so').read_bytes());p=folder/'lb_pair.bin';p.write_bytes(p.read_bytes()[:8]+encode({'defined':{'name':'LbPair'}},pair))
        if empty:
            p=folder/'bin_array_1.bin';array,_=account('BinArray',p);array['bins'][pair['active_id']%70]['liquidity_supply']=0;array['bins'][pair['active_id']%70]['amount_x']=array['bins'][pair['active_id']%70]['amount_y']=0;p.write_bytes(p.read_bytes()[:8]+encode({'defined':{'name':'BinArray'}},array))
        prior=native.BASE;native.BASE=base
        try:return native.execute(zero=zero,amount=amount,minimum=10**15 if exact else 1,exact_out=exact,mm_only=True,protocol_share=2000,elapsed=300,dynamic_control=7500,followups=followups)
        finally:native.BASE=prior
if __name__=='__main__':
    rows=[]
    for zero in [True,False]:
        for exact in [False,True]:
            for empty in [False,True]:
                n=execute(zero,exact,empty=empty);rows.append(n)
                before,_=decode({'defined':{'name':'LbPair'}},__import__('base64').b64decode(n['before'][n['pair_id']]['data_base64']),8);after,_=decode({'defined':{'name':'LbPair'}},__import__('base64').b64decode(n['after'][n['pair_id']]['data_base64']),8)
                print(json.dumps(dict(zero=zero,exact=exact,empty=empty,passed=n['passed'],before=before['reward_infos'][0],after=after['reward_infos'][0],err=n['native'].get('err'))))
    (ROOT/'integration/artifacts/local/core-meteora-reward-preflight-20261111.json').write_text(json.dumps(dict(records=rows),indent=2)+'\n')
