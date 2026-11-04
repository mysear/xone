#!/usr/bin/env python3
"""Bind numeric CLMM wire vectors to original-program canonical Solana PDA seeds."""
from pathlib import Path
import hashlib,json
ROOT=Path(__file__).resolve().parents[3]
ALPHABET='123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz'
def decode(text):
 n=0
 for c in text:n=n*58+ALPHABET.index(c)
 return b'\0'*(len(text)-len(text.lstrip('1')))+n.to_bytes((n.bit_length()+7)//8,'big')
def encode(data):
 n=int.from_bytes(data,'big');s=''
 while n:s=ALPHABET[n%58]+s;n//=58
 return '1'*(len(data)-len(data.lstrip(b'\0')))+s
PRIME=2**255-19;D=(-121665*pow(121666,-1,PRIME))%PRIME
PROGRAM=decode('CAMMCzo5YL8w4VFF8KVHrK22GGUsp5VTaW7grrKgrWqK')
def pda(seeds):
 for bump in range(255,-1,-1):
  digest=hashlib.sha256(b''.join(seeds)+bytes([bump])+PROGRAM+b'ProgramDerivedAddress').digest()
  y=(int.from_bytes(digest,'little')&((1<<255)-1))%PRIME;y2=y*y%PRIME
  denominator=(D*y2+1)%PRIME
  if denominator==0:return encode(digest),bump
  x2=(y2-1)*pow(denominator,-1,PRIME)%PRIME
  if pow(x2,(PRIME-1)//2,PRIME)==PRIME-1:return encode(digest),bump
 raise AssertionError('PDA not found')
p=json.loads((ROOT/'integration/test/contracts/rayclmm-state.json').read_text());s=p['raydium_clmm']
p['pool_id'],s['pool_bump']=pda([b'pool',decode(s['amm_config']),decode(p['token0']),decode(p['token1'])]);pool=decode(p['pool_id'])
s['vault0']=pda([b'pool_vault',pool,decode(p['token0'])])[0];s['vault1']=pda([b'pool_vault',pool,decode(p['token1'])])[0];s['observation']=pda([b'observation',pool])[0]
for a in s['tick_arrays']:a['address']=pda([b'tick_array',pool,a['start_tick_index'].to_bytes(4,'big',signed=True)])[0]
raw=json.dumps(p,indent=2)+'\n'
for project in ['integration','chain-parser-go','chain-parser-rust','dex-router']:(ROOT/project/'test/contracts/rayclmm-state.json').write_text(raw)
print(json.dumps({'pool':p['pool_id'],'pool_bump':s['pool_bump'],'sha256':hashlib.sha256(raw.encode()).hexdigest()}))
