#!/usr/bin/env python3
"""Original native swap_manager corpus for the bounded adaptive exact-in API."""
import argparse
import copy
import hashlib
import json
from pathlib import Path
import subprocess
import tomllib
ROOT=Path(__file__).resolve().parents[3]
REF=ROOT/'integration/tools/reference/orca-adaptive-swap'
BINARY=Path('/tmp/xone-orca-adaptive-swap-native/debug/orca-adaptive-swap-reference')
OUTPUT=[ROOT/'integration/test/contracts/orca-adaptive-swap.json',ROOT/'dex-router/test/contracts/orca-adaptive-swap.json']
def require(condition, message):
    if not condition: raise RuntimeError(str(message))

class Oracle:
    def __init__(self,binary):
        self.proc=subprocess.Popen([str(binary)],stdin=subprocess.PIPE,stdout=subprocess.PIPE,text=True)
        self.identity=self.request({'action':'identity'})
        require(self.identity['executed_binary_sha256']==hashlib.sha256(binary.read_bytes()).hexdigest(), 'native adaptive swap evidence check failed')
        require(self.identity['execution']=='unchanged-original-native-swap-manager-and-post-update', 'native adaptive swap evidence check failed')
        self.prices={}
    def request(self,value):
        self.proc.stdin.write(json.dumps(value)+'\n');self.proc.stdin.flush()
        result=self.proc.stdout.readline()
        if not result:raise RuntimeError(f'oracle stopped: {self.proc.poll()}')
        return json.loads(result)
    def price(self,tick):
        if tick not in self.prices:self.prices[tick]=self.request({'action':'prices','ticks':[tick]})[0]
        return self.prices[tick]
    def close(self):
        self.proc.stdin.close();require(self.proc.wait(timeout=20)==0, 'native adaptive swap evidence check failed')

def base(oracle,tick=0,spacing=64):
    span=88*spacing;start=(tick//span)*span
    rewards=[dict(mint=[0]*32,vault=[0]*32,extension=[3 if i==0 else 0]*32,
        emissions_per_second_x64='0',growth_global_x64='0') for i in range(3)]
    return dict(whirlpool=[7]*32,token_program='TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA',tick_spacing=spacing,
        fee_rate=3000,protocol_fee_rate=2500,sqrt_price_x64=oracle.price(tick),tick_current_index=tick,
        liquidity='1000000',fee_growth_global_a='123',fee_growth_global_b='456',protocol_fee_owed_a=77,protocol_fee_owed_b=88,
        reward_last_updated_timestamp=100,reward_infos=rewards,
        oracle=dict(whirlpool=[7]*32,trade_enable_timestamp=100,reserved=list(range(128)),adaptive_fee_info=dict(
            constants=dict(filter_period=10,decay_period=100,reduction_factor=5000,adaptive_fee_control_factor=1000,
                max_volatility_accumulator=100000,tick_group_size=8 if spacing>=8 else 1,major_swap_threshold_ticks=64,
                reserved=list(range(16))),variables=dict(last_reference_update_timestamp=100,last_major_swap_timestamp=100,
                volatility_reference=20000,tick_group_index_reference=0,volatility_accumulator=40000,reserved=list(reversed(range(16)))))),
        tick_arrays=[dict(whirlpool=[7]*32,start_tick_index=s,layout='fixed',initialized_bitmap='0',ticks=[])
            for s in [start-span,start,start+span] if s>=(-443636//span)*span and s<=443636])
def add_tick(state,tick,net=0,gross=None):
    if gross is None:gross=max(abs(net),1000)
    span=88*state['tick_spacing']
    a=next(a for a in state['tick_arrays'] if a['start_tick_index']<=tick<a['start_tick_index']+span)
    a['ticks'].append(dict(tick=tick,liquidity_gross=str(gross),liquidity_net=str(net),fee_growth_outside_a='11',
        fee_growth_outside_b='29',reward_growths_outside=['0']*3))
    a['ticks'].sort(key=lambda t:t['tick'])
    a['initialized_bitmap']=str(sum(1<<((t['tick']-a['start_tick_index'])//state['tick_spacing']) for t in a['ticks']))
def params(oracle,left=True,amount=1000,time=101,limit=None):
    if limit is None:limit=-1024 if left else 1024
    return dict(amount=amount,a_to_b=left,sqrt_price_limit=oracle.price(limit),timestamp=time,max_steps=8192)
def generate(oracle):
    cases=[];failures=[]
    def add(name,state,p):
        req=dict(state=copy.deepcopy(state),params=copy.deepcopy(p));expected=oracle.request(req)
        require('error' not in expected, f'{name}: {expected}')
        cases.append(dict(name=name,request=req,expected=expected));return expected
    for left in [True,False]:
        for factor in [0,1000,99999]:
            for amount in [1,2,10,1000,10000,100000,200000]:
                state=base(oracle)
                for t,net in [(-128,-200000),(-64,100000),(0,0),(64,-100000),(128,200000)]:add_tick(state,t,net)
                state['oracle']['adaptive_fee_info']['constants']['adaptive_fee_control_factor']=factor
                add(f'groups-{left}-{factor}-{amount}',state,params(oracle,left,amount))
        for timestamp in [100,109,110,199,200,3700,3701,2**64-1]:
            add(f'time-{left}-{timestamp}',base(oracle),params(oracle,left,100000,timestamp))
        for start in [-512,-8,-1,0,1,8,512]:
            state=base(oracle,start)
            add(f'negative-and-cap-range-{left}-{start}',state,params(oracle,left,10000))
        high=base(oracle)
        high['oracle']['adaptive_fee_info']['constants'].update(adaptive_fee_control_factor=99999,max_volatility_accumulator=1000000)
        high['oracle']['adaptive_fee_info']['variables'].update(volatility_reference=900000,volatility_accumulator=1000000)
        for amount in [1,10,10000,100000]:add(f'hard-fee-cap-{left}-{amount}',high,params(oracle,left,amount))
        for zero in [True,False]:
            state=base(oracle);state['liquidity']='0'
            if not zero:add_tick(state,-64 if left else 64,-1000000 if left else 1000000)
            add(f'zero-L-{left}-{zero}',state,params(oracle,left,100000))
        wrap=base(oracle)
        wrap.update(fee_growth_global_a=str(2**128-2),fee_growth_global_b=str(2**128-3),protocol_fee_owed_a=2**64-1,protocol_fee_owed_b=2**64-1)
        add_tick(wrap,-64 if left else 64,0)
        add(f'fee-growth-and-owed-wrap-{left}',wrap,params(oracle,left,100000))
        for limit in [-64,-8,-1] if left else [1,8,64]:
            state=base(oracle);add_tick(state,limit if limit%64==0 else (-64 if left else 64),0)
            add(f'partial-limit-{left}-{limit}',state,params(oracle,left,2**64-1,limit=limit))
        state=base(oracle);p=params(oracle,left,1000);p['sqrt_price_limit']='0'
        add(f'implicit-limit-full-fill-{left}',state,p)
        p['amount']=2**64-1
        req=dict(state=state,params=p);expected=oracle.request(req)
        require('error' in expected, 'native adaptive swap evidence check failed');failures.append(dict(name=f'proof-exhaustion-{left}',request=req,expected=expected))
    for left in [True,False]:
        state=base(oracle,-64 if left else 64)
        # Exact start boundary uses the native -1 current tick convention.
        state['tick_current_index']-=1
        add(f'shifted-initial-boundary-{left}',state,params(oracle,left,1000))
    shifted=base(oracle,-443636)
    shifted['tick_current_index']=-443637;shifted['liquidity']='0'
    add_tick(shifted,-443584,1000000)
    add('shifted-min-start',shifted,params(oracle,False,1000,limit=-443000))
    for group in [1,2,8,64]:
        state=base(oracle);state['oracle']['adaptive_fee_info']['constants']['tick_group_size']=group
        add(f'group-size-{group}',state,params(oracle,True,100000))
    sequences=[]
    for left_first in [True,False]:
        state=base(oracle);initial=copy.deepcopy(state);trades=[]
        for index,time in enumerate([101,101,110,111,200,3701]):
            left=left_first if index%2==0 else not left_first
            p=params(oracle,left,10000,time)
            expected=oracle.request(dict(state=state,params=p));require('error' not in expected, expected)
            trades.append(dict(params=p,expected=expected));state=expected['next_state']
        sequences.append(dict(name=f'alternating-{left_first}',initial_state=initial,trades=trades))
    return dict(schema='orca-adaptive-exact-in-native-v1',online_profile_enabled=False,
        scope='bounded complete fixed-array classic SPL mathematical swap; initialized rewards unsupported; no account/transaction claim',
        identity=oracle.identity,cargo_dependencies=tomllib.loads((REF/'Cargo.lock').read_text())['package'],cases=cases,failures=failures,sequences=sequences)
MANIFEST_SHA256='13f4a65d41edb2cb764e852ae1e42bdaa62d65c958c1ed67cf46f400b8cf7591'
def validate_manifest(data):
    require(data.get('schema')=='orca-adaptive-exact-in-native-v1' and data.get('online_profile_enabled')is False,'native adaptive swap schema/scope differs')
    manifest={'cases':[c['name']for c in data['cases']],'failures':[c['name']for c in data['failures']],'sequences':[(s['name'],len(s['trades']))for s in data['sequences']]}
    names=manifest['cases']+manifest['failures']+[s[0]for s in manifest['sequences']]
    require(len(names)==len(set(names)),'duplicate native adaptive swap member')
    require(hashlib.sha256(json.dumps(manifest,sort_keys=True,separators=(',',':')).encode()).hexdigest()==MANIFEST_SHA256,'native adaptive swap member manifest changed')
def main():
    parser=argparse.ArgumentParser();parser.add_argument('--binary',type=Path,default=BINARY);parser.add_argument('--check',action='store_true');parser.add_argument('--corpus',type=Path,default=OUTPUT[0]);parser.add_argument('--router-corpus',type=Path,default=OUTPUT[1]);a=parser.parse_args()
    oracle=Oracle(a.binary)
    try:
        if a.check:
            data=json.loads(a.corpus.read_text());validate_manifest(data);require(a.corpus.read_bytes()==a.router_corpus.read_bytes(), 'native adaptive swap evidence check failed')
            before=dict(data['identity']);now=dict(oracle.identity);before.pop('executed_binary_sha256');now.pop('executed_binary_sha256');require(before==now, 'native adaptive swap evidence check failed')
            require(data['cargo_dependencies']==tomllib.loads((REF/'Cargo.lock').read_text())['package'], 'native adaptive swap evidence check failed')
            for case in data['cases']+data['failures']:require(oracle.request(case['request'])==case['expected'], case['name'])
            for sequence in data['sequences']:
                state=sequence['initial_state']
                for trade in sequence['trades']:
                    actual=oracle.request(dict(state=state,params=trade['params']));require(actual==trade['expected'], 'native adaptive swap evidence check failed');state=actual['next_state']
        else:
            data=generate(oracle);validate_manifest(data);text=json.dumps(data,sort_keys=True,indent=2)+'\n'
            for output in OUTPUT:output.write_text(text)
        print(json.dumps(dict(cases=len(data['cases']),native_failures=len(data['failures']),sequences=len(data['sequences']),
            trades=sum(len(s['trades']) for s in data['sequences']),recorded_binary_sha256=data['identity']['executed_binary_sha256'],replay_binary_sha256=oracle.identity['executed_binary_sha256'])))
    finally:oracle.close()
if __name__=='__main__':main()
