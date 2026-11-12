"""Strict, bounded Borsh projection from pinned official Pump IDL; never executes it."""
from pathlib import Path
import json,sys
ROOT=Path(__file__).resolve().parents[5];BASE=Path(__file__).resolve().parents[1]
sys.path.insert(0,str(ROOT/'integration/tools/acceptance'))
from r01_public_catalog_solana_rebuild import b58encode,b58decode
IDLS={name:json.loads((BASE/'upstream/idl'/f'{name}.json').read_bytes())for name in ['pump','pump_fees']}
def decode(t,raw,offset=0,idl='pump'):
    if isinstance(t,str):
        size=32 if t=='pubkey'else 1 if t=='bool'else int(t[1:])//8
        if size<=0 or offset+size>len(raw):raise ValueError('PUMP_TRUNCATED_LAYOUT')
        b=raw[offset:offset+size]
        if t=='pubkey':v=b58encode(b)
        elif t=='bool':
            if b not in [b'\0',b'\1']:raise ValueError('PUMP_BOOL')
            v=b==b'\1'
        else:v=int.from_bytes(b,'little',signed=t.startswith('i'))
        return v,offset+size
    if 'array'in t:
        kind,count=t['array'];out=[]
        for _ in range(count):v,offset=decode(kind,raw,offset,idl);out.append(v)
        return out,offset
    if 'vec'in t:
        count,offset=decode('u32',raw,offset,idl)
        if count>256:raise ValueError('PUMP_VECTOR_BUDGET')
        out=[]
        for _ in range(count):v,offset=decode(t['vec'],raw,offset,idl);out.append(v)
        return out,offset
    if 'defined'in t:
        typ=next(x for x in IDLS[idl]['types']if x['name']==t['defined']['name']);out={}
        for f in typ['type']['fields']:out[f['name']],offset=decode(f['type'],raw,offset,idl)
        return out,offset
    raise ValueError('PUMP_UNSUPPORTED_TYPE')
def encode(t,v,idl='pump'):
    if isinstance(t,str):
        if t=='pubkey':
            raw=b58decode(v)
            if len(raw)!=32:raise ValueError('PUMP_PUBLIC_KEY')
            return raw
        if t=='bool':
            if type(v)is not bool:raise ValueError('PUMP_BOOL')
            return bytes([v])
        return int(v).to_bytes(int(t[1:])//8,'little',signed=t.startswith('i'))
    if 'array'in t:
        if len(v)!=t['array'][1]:raise ValueError('PUMP_ARRAY')
        return b''.join(encode(t['array'][0],x,idl)for x in v)
    if 'vec'in t:return len(v).to_bytes(4,'little')+b''.join(encode(t['vec'],x,idl)for x in v)
    if 'defined'in t:
        typ=next(x for x in IDLS[idl]['types']if x['name']==t['defined']['name']);return b''.join(encode(f['type'],v[f['name']],idl)for f in typ['type']['fields'])
    raise ValueError('PUMP_UNSUPPORTED_TYPE')
def account(name,raw,idl='pump'):
    a=next(a for a in IDLS[idl]['accounts']if a['name']==name)
    if raw[:8]!=bytes(a['discriminator']):raise ValueError('PUMP_DISCRIMINATOR')
    value,end=decode({'defined':{'name':name}},raw,8,idl)
    if any(raw[end:]):raise ValueError('PUMP_NONZERO_UNKNOWN_FIELDS')
    return value,end
def account_bytes(name,value,space=None,idl='pump'):
    a=next(a for a in IDLS[idl]['accounts']if a['name']==name);raw=bytes(a['discriminator'])+encode({'defined':{'name':name}},value,idl)
    if space is not None:
        if len(raw)>space:raise ValueError('PUMP_ACCOUNT_SPACE')
        raw+=bytes(space-len(raw))
    return raw
if __name__=='__main__':
    pins=json.loads((BASE/'public-bank-pins.json').read_bytes())
    for ref in pins['files']:
        raw=(BASE/ref['path']).read_bytes()
        if len(raw)in [1088,4097]:
            name,idl=('Global','pump')if len(raw)==1088 else('FeeConfig','pump_fees');v,end=account(name,raw,idl);print(json.dumps(dict(name=name,bytes=len(raw),decoded=end,value=v)))
