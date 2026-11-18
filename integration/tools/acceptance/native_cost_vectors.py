"""Independent integer Legacy/EIP-1559 native-cost operands and expected values."""
import hashlib
import json
from pathlib import Path
MAX256=(1<<256)-1

def oracle(used,limit,base,maximum,priority):
    if not (21000<=used<=limit<(1<<64)) or not (0<=base<=maximum<=MAX256) or priority is not None and not (0<=priority<=maximum):
        return None
    tip=maximum-base if priority is None else min(priority,maximum-base)
    price=base+tip
    expected={'effective_gas_price':price,'execution_fee':used*price,'base_fee_burned':used*base,'priority_fee_paid':used*tip,'gas_limit_reserve':limit*maximum}
    if any(value>MAX256 for value in expected.values()):return None
    if expected['execution_fee']!=expected['base_fee_burned']+expected['priority_fee_paid'] or expected['execution_fee']>expected['gas_limit_reserve']:raise ValueError('INDEPENDENT_FEE_CONSERVATION')
    return {key:str(value) for key,value in expected.items()}

def document():
    operands=[]
    for used in [21000,50000,1000000]:
        for limit in [used,15000000]:
            for base,maximum,priority in [(0,0,None),(0,1,0),(100,200,None),(100,200,30),(100,110,30),(100,100,100),(10**9,2*10**9,10**9),(1<<180,1<<200,1<<190)]:
                operands.append((used,limit,base,maximum,priority))
    operands.extend([(0,100000,1,2,None),(20999,100000,1,2,None),(100001,100000,1,2,None),(21000,0,1,2,None),(21000,100000,3,2,None),(21000,100000,1,2,3),(21000,21000,0,MAX256,None),(21000,(1<<64)-1,0,MAX256,0)])
    rows=[]
    for index,(used,limit,base,maximum,priority) in enumerate(operands):
        expected=oracle(used,limit,base,maximum,priority)
        row={'id':'native-cost-'+str(index),'gas_used':str(used),'gas_limit':str(limit),'base_fee':str(base),'maximum':str(maximum),'priority':None if priority is None else str(priority),'valid':expected is not None}
        if expected is not None:row['expected']=expected
        rows.append(row)
    return {'schema_version':1,'authority':'INDEPENDENT_PYTHON_INTEGER_ARITHMETIC_NOT_CHAIN_PRICE_SOURCE','cases':rows}

def write(path):
    value=document();raw=(json.dumps(value,indent=2)+'\n').encode();path=Path(path);path.parent.mkdir(parents=True,exist_ok=False);path.write_bytes(raw)
    return {'sha256':hashlib.sha256(raw).hexdigest(),'cases':len(value['cases']),'valid':sum(row['valid'] for row in value['cases'])}
