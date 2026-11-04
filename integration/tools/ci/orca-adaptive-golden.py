#!/usr/bin/env python3
"""Generate/replay FeeRateManager evidence through the unchanged native oracle.

No swap quote, SBF execution, token program, or transaction support is asserted.
The native binary verifies its complete pinned source/harness closure at startup.
"""
import argparse
import copy
import hashlib
import json
import subprocess
import tomllib
from pathlib import Path
ROOT = Path(__file__).resolve().parents[3]
REF = ROOT / 'integration/tools/reference/orca-adaptive'
OUTPUTS = [ROOT/'integration/test/contracts/orca-adaptive.json', ROOT/'dex-router/test/contracts/orca-adaptive.json']
DEFAULT_BINARY = Path('/tmp/xone-orca-adaptive-native/debug/orca-adaptive-reference')
PIN = 'cc683ec92acb5e7606caa3d8e514d9383e9c7c11'

def require(condition, message):
    if not condition:
        raise ValueError(message)

class Oracle:
    def __init__(self, path):
        self.path = path
        self.proc = subprocess.Popen([str(path)], stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True)
        self.identity = self.request({'action':'identity'})
        require((self.identity['revision'] == PIN), 'Reference validation failed')
        require((self.identity['executed_binary_sha256'] == hashlib.sha256(path.read_bytes()).hexdigest()), 'Reference validation failed')
        require((self.identity['source_files_verified'] > 100), 'Reference validation failed')
    def request(self, value):
        self.proc.stdin.write(json.dumps(value)+'\n'); self.proc.stdin.flush()
        line = self.proc.stdout.readline()
        if not line: raise RuntimeError(f'native oracle failed closed: {self.proc.poll()}')
        return json.loads(line)
    def close(self):
        self.proc.stdin.close()
        require((self.proc.wait(timeout=20) == 0), 'Reference validation failed')

def base():
    return {'constants':{'filter_period':10,'decay_period':100,'reduction_factor':5000,
        'adaptive_fee_control_factor':1000,'max_volatility_accumulator':100_000,
        'tick_group_size':8,'major_swap_threshold_ticks':64,'reserved':[0]*16},
        'variables':{'last_reference_update_timestamp':100,'last_major_swap_timestamp':100,
        'volatility_reference':20_000,'tick_group_index_reference':0,'volatility_accumulator':40_000,'reserved':[0]*16}}
def context(tick=0, timestamp=101, left=True):
    return dict(a_to_b=left,current_tick_index=tick,timestamp=timestamp,static_fee_rate=3000,tick_spacing=64,trade_enable_timestamp=0)
def generate(oracle):
    ticks = sorted(set([-443636,-443635,-10000,-1000,-800,-801,-81,-80,-79,-65,-64,-63,-17,-16,-15,-9,-8,-7,-1,0,1,7,8,9,15,16,17,63,64,65,72,79,80,81,800,801,1000,10000,443635,443636]))
    prices=dict(zip(ticks,oracle.request({'action':'prices','ticks':ticks})))
    def p(t): return prices[t]
    def target(t, liquidity=1): return dict(op='target',target=p(t),liquidity=str(liquidity))
    def skip(t, nxt=None, offset=0):
        if nxt is None: nxt=t
        return dict(op='skip',price=str(int(p(t))+offset),next_tick_price=p(nxt),next_tick_index=nxt)
    cases=[]
    def add(name, ctx=None, state=None, ops=None):
        req={'context':copy.deepcopy(ctx if ctx is not None else context()),'info':copy.deepcopy(state if state is not None else base()),'operations':ops or []}
        result=oracle.request(req)
        cases.append(dict(name=name,request=req,expected=result))
        return result
    for time in [99,100,101,109,110,111,199,200,201,3699,3700,3701,2**64-1]:
        add(f'time-{time}',context(-1,time),ops=[{'op':'update'}])
    for time in [3699,3700,3701]:
        state=base();state['variables']['last_major_swap_timestamp']=3699
        add(f'major-lock-reset-{time}',context(16,time,False),state,[{'op':'update'}])
    for left in [True,False]:
        for tick in [-443637,-443636,-443635,-801,-81,-80,-79,-17,-16,-15,-9,-8,-7,-1,0,1,7,8,9,15,16,17,79,80,81,801,443635,443636]:
            ops=[]
            for _ in range(4):
                ops += [{'op':'update'},target(-443636 if left else 443636),{'op':'advance'}]
            add(f'groups-{left}-{tick}',context(tick,left=left),ops=ops)
        for start in [-1000,0,1000]:
            for liquidity in [0,1,2**128-1]:
                add(f'skip-target-{left}-{start}-L{liquidity}',context(start,left=left),ops=[{'op':'update'},target(-10000 if left else 10000,liquidity)])
        for tick in [-801,-80,-8,-1,0,1,8,80,801]:
            # Control factor zero makes a real native skip valid throughout.
            state=base();state['constants']['adaptive_fee_control_factor']=0
            start=1000 if left else -1000
            for offset in [0,1]:
                add(f'skip-boundary-{left}-{tick}-{offset}',context(start,left=left),state,
                    [{'op':'update'},target(-10000 if left else 10000),skip(tick,443636 if offset else tick,offset),{'op':'update'}])
        # No price movement: b->a must not move backward into the previous group.
        add(f'skip-no-motion-{left}',context(0,left=left),ops=[{'op':'update'},target(0,0),skip(0)])
        for pre,post in [(0,63),(0,64),(0,65),(-65,0),(-64,0),(-63,0),(-443636,443636),(443635,443636)]:
            if left: pre,post=post,pre
            add(f'major-{left}-{pre}-{post}',context(timestamp=109,left=left),ops=[{'op':'major','pre':p(pre),'post':p(post)}])
    for field,values in {'filter_period':[0,1,99,100,65535], 'decay_period':[0,10,11,65535],
        'reduction_factor':[0,1,9999,10000,65535], 'adaptive_fee_control_factor':[0,1,99999,100000,2**32-1],
        'max_volatility_accumulator':[0,1,536870911,536870912,2**32-1],
        'tick_group_size':[0,1,2,7,64,65,65535], 'major_swap_threshold_ticks':[0,1,5632,5633,65535]}.items():
        for value in values:
            state=base();state['constants'][field]=value
            if field=='max_volatility_accumulator':
                state['variables']['volatility_accumulator']=min(40_000,value)
                state['variables']['volatility_reference']=min(20_000,value)
            add(f'constant-{field}-{value}',state=state,ops=[{'op':'update'},target(-10000)])
    for spacing,group in [(65535,65535),(65535,1),(1,1),(64,8)]:
        state=base();state['constants'].update(tick_group_size=group,max_volatility_accumulator=(2**32-1)//group,adaptive_fee_control_factor=99999)
        state['variables'].update(volatility_accumulator=state['constants']['max_volatility_accumulator'],volatility_reference=0)
        ctx=context();ctx['tick_spacing']=spacing;ctx['static_fee_rate']=60000
        add(f'max-width-{spacing}-{group}',ctx,state,[target(-443636),{'op':'update'},target(-443636)])
    for left,start,end,boundary in [(False,-1000,0,-64),(True,1000,0,72),
        (True,-1000,-10000,-64),(False,1000,10000,72)]:
        # Saturated-volatility skips with nonzero control and nonzero liquidity.
        result=add(f'cap-skip-{left}-{start}',context(start,left=left),ops=[
            {'op':'update'},target(end),skip(boundary if end==0 else end),{'op':'update'},target(end)])
        require((result['trace'][2]['target']['skip'] is True), 'Reference validation failed')
    for group in [1,8,64]:
        state=base();state['constants']['tick_group_size']=group
        state['variables']['tick_group_index_reference']=-443637//group
        add(f'shifted-reference-group-{group}',context(-443637,left=False),state,[{'op':'update'},target(0)])
    for reduction in [1,3333,9999]:
        state=base();state['constants']['reduction_factor']=reduction;state['variables']['volatility_accumulator']=33333
        add(f'reduction-rounding-{reduction}',context(timestamp=110),state,[{'op':'update'}])
    ctx=context();ctx['trade_enable_timestamp']=102;add('trade-not-enabled',ctx)
    ctx['timestamp']=102;add('trade-enable-equality',ctx)
    state=base();state['constants']['reserved']=list(range(16));state['variables']['reserved']=list(reversed(range(16)))
    add('reserved-preserved',state=state,ops=[{'op':'update'},{'op':'major','pre':p(0),'post':p(65)}])
    sequences=[]
    for left_first in [True,False]:
        state=base();initial=copy.deepcopy(state);trades=[]
        for index,(time,left,tick,end) in enumerate([(101,left_first,0,-80 if left_first else 80),
            (101,not left_first,-80 if left_first else 80,0),(110,left_first,0,-801 if left_first else 801),
            (120,not left_first,-801 if left_first else 801,0),(220,left_first,0,-16 if left_first else 16),
            (3701,not left_first,-16 if left_first else 16,0),(3801,left_first,0,-8 if left_first else 8)]):
            req={'context':context(tick,time,left),'info':copy.deepcopy(state),'operations':[
                {'op':'update'},target(end,0),skip(end),{'op':'major','pre':p(tick),'post':p(end)}]}
            expected=oracle.request(req);trades.append(dict(request=req,expected=expected));state=expected['next_info']
        sequences.append(dict(name=f'alternating-same-block-{left_first}',initial_info=initial,trades=trades))
    return dict(schema='orca-adaptive-kernel-v1',online_profile_enabled=False,
        scope='native FeeRateManager/AdaptiveFeeVariables only; no full adaptive swap, parser, SBF, Token or transaction claim',
        identity=oracle.identity,cargo_dependencies=tomllib.loads((REF/'Cargo.lock').read_text())['package'],cases=cases,sequences=sequences)

def main():
    parser=argparse.ArgumentParser();parser.add_argument('--binary',type=Path,default=DEFAULT_BINARY)
    parser.add_argument('--check',action='store_true');args=parser.parse_args()
    oracle=Oracle(args.binary)
    try:
        if args.check:
            data=json.loads(OUTPUTS[0].read_text());require((OUTPUTS[0].read_bytes()==OUTPUTS[1].read_bytes()), 'Reference validation failed')
            # Relinking can change binary bytes while preserving the verified compiled closure.
            recorded=dict(data['identity']); replayed=dict(oracle.identity)
            recorded.pop('executed_binary_sha256'); replayed.pop('executed_binary_sha256')
            require((recorded==replayed), 'reference source/dependency/compiler closure drift; regenerate explicitly')
            require((data['cargo_dependencies']==tomllib.loads((REF/'Cargo.lock').read_text())['package']), 'dependency closure drift')
            for case in data['cases']: require((oracle.request(case['request'])==case['expected']), case['name'])
            for seq in data['sequences']:
                state=seq['initial_info']
                for trade in seq['trades']:
                    req=copy.deepcopy(trade['request']);req['info']=state
                    actual=oracle.request(req);require((actual==trade['expected']), 'Reference validation failed');state=actual['next_info']
        else:
            data=generate(oracle);encoded=json.dumps(data,indent=2,sort_keys=True)+'\n'
            for path in OUTPUTS:path.write_text(encoded)
        print(json.dumps(dict(cases=len(data['cases']),sequences=len(data['sequences']),
            sequence_trades=sum(len(s['trades']) for s in data['sequences']),
            recorded_binary_sha256=data['identity']['executed_binary_sha256'],
            replay_binary_sha256=oracle.identity['executed_binary_sha256'])))
    finally:oracle.close()
if __name__=='__main__':main()
