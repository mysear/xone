"""Read-only fixed upstream bytemuck IDL layout. Padding stays observable."""
from pathlib import Path
import json,sys
ROOT=Path(__file__).resolve().parents[5]
sys.path.insert(0,str(ROOT/'integration/tools/acceptance'))
from r01_public_catalog_solana_rebuild import b58encode
BASE=Path(__file__).resolve().parents[1]
IDL=json.loads((BASE/'idls/dlmm.json').read_bytes())
TYPES={t['name']:t for t in IDL['types']}

def size(t):
    if isinstance(t,str):
        if t=='pubkey':return 32
        if t=='bool':return 1
        if t[0]in 'ui' and t[1:].isdigit():return int(t[1:])//8
        raise ValueError('unsupported primitive '+t)
    if 'array'in t:return size(t['array'][0])*t['array'][1]
    if 'defined'in t:
        d=TYPES[t['defined']['name']]['type']
        if d['kind']!='struct':raise ValueError('unsupported type kind')
        return sum(size(f['type'])for f in d['fields'])
    raise ValueError('unsupported layout '+str(t))

def decode(t,raw,offset=0):
    if offset<0 or offset+size(t)>len(raw):raise ValueError('truncated layout')
    if isinstance(t,str):
        n=size(t);data=raw[offset:offset+n]
        if t=='pubkey':v=b58encode(data)
        elif t=='bool':
            if data not in [b'\0',b'\1']:raise ValueError('invalid bool')
            v=data==b'\1'
        else:v=int.from_bytes(data,'little',signed=t.startswith('i'))
        return v,offset+n
    if 'array'in t:
        value=[]
        for _ in range(t['array'][1]):
            v,offset=decode(t['array'][0],raw,offset);value.append(v)
        return value,offset
    result={}
    for field in TYPES[t['defined']['name']]['type']['fields']:
        result[field['name']],offset=decode(field['type'],raw,offset)
    return result,offset

def account(name,path):
    raw=Path(path).read_bytes();definition=next(a for a in IDL['accounts']if a['name']==name)
    if raw[:8]!=bytes(definition['discriminator']):raise ValueError('discriminator')
    value,end=decode({'defined':{'name':name}},raw,8)
    return value,dict(bytes=len(raw),decoded=end,trailing_hex=raw[end:].hex())

if __name__=='__main__':
    for folder in (BASE/'commons/tests/fixtures').iterdir():
        p,pc=account('LbPair',folder/'lb_pair.bin');arrays=[]
        for name in ['bin_array_1.bin','bin_array_2.bin']:
            b,bc=account('BinArray',folder/name);arrays.append(dict(path=name,coverage=bc,index=b['index'],nonempty=sum(v['amount_x']!=0 or v['amount_y']!=0 for v in b['bins'])))
        print(json.dumps(dict(pool=folder.name,coverage=pc,active_id=p['active_id'],bin_step=p['bin_step'],mints=[p['token_x_mint'],p['token_y_mint']],static=p['parameters'],variable=p['v_parameters'],fields=list(p),arrays=arrays)))

def encode(t,value):
    if isinstance(t,str):
        if t=='pubkey':
            from r01_public_catalog_solana_rebuild import b58decode
            raw=b58decode(value)
            if len(raw)!=32:raise ValueError('public key width')
            return raw
        if t=='bool':
            if type(value)is not bool:raise ValueError('boolean type')
            return bytes([int(value)])
        return int(value).to_bytes(size(t),'little',signed=t.startswith('i'))
    if 'array'in t:
        if len(value)!=t['array'][1]:raise ValueError('array length')
        return b''.join(encode(t['array'][0],v)for v in value)
    fields=TYPES[t['defined']['name']]['type']['fields']
    if set(value)!={f['name']for f in fields}:raise ValueError('struct field coverage')
    return b''.join(encode(f['type'],value[f['name']])for f in fields)
