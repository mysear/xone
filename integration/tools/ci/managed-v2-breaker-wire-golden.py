#!/usr/bin/env python3
"""C++ protoc syntax/domain contract for breaker42; no native proof."""
import argparse
import copy
import hashlib
import importlib.util
import json
from pathlib import Path
import unittest
ROOT=Path(__file__).resolve().parents[3]
spec=importlib.util.spec_from_file_location('bpt_breaker_wire',Path(__file__).with_name('managed-v2-bpt-wire-golden.py'));base=importlib.util.module_from_spec(spec);spec.loader.exec_module(base)
legacy=base.legacy;require=legacy.require
PROTOCOL='balancer-v2-managed-breaker';ADAPTER='balancer-v2/20230411-managed-breaker-1';FILENAME='managed-v2-breaker-wire.json'
AUTHORITY='synthetic-breaker-state-wire-contract-no-native-execution-proof';ONE=10**18
FIELDS=('state_word','reference_bpt_price','reference_weight','lower_percentage','upper_percentage','lower_bpt_price_bound','upper_bpt_price_bound')
def build_snapshot():
 s=base.build_snapshot();p=s['pools'][0];p['protocol'],p['adapter_version']=PROTOCOL,ADAPTER;s['source']='SYNTHETIC_MANAGED_BREAKER_WIRE';s['reference']['adapter_set_hash']=legacy.array_hash([ADAPTER]);m=p['multi_asset']['managed_v2']
 tokens=[]
 for i,(a,w) in enumerate(zip(p['multi_asset']['assets'],p['multi_asset']['weights'])):
  if i==0:
   # Current weight equals the stored reference: original cached u32 is used.
   # Both cache fields exercise their high bit; no local pow runs in this fixture.
   price=ONE;lower=ONE//10>>48;upper=2*ONE>>48;cl=2**31+17;cu=2**32-1
   word=lower|(upper<<16)|(int(w)<<32)|(price<<96)|(cl<<192)|(cu<<224)
   ratio_l=cl*10*ONE//(2**32-1);ratio_u=cu*10*ONE//(2**32-1)
   bound_l=(price*ratio_l-1)//ONE+1;bound_u=price*ratio_u//ONE;scale=int(a['scaling'])
   values=[str(word),str(price*scale//ONE),w,str(lower<<48),str(upper<<48),str(bound_l*scale//ONE),str(bound_u*scale//ONE)]
  else:values=['0']*7
  tokens.append({'token':a['token'],**dict(zip(FIELDS,values))});m['token_info'][i]['circuit_breaker_enabled']=i==0
 m['circuit_breakers']={'tokens':tokens};validate_snapshot(s);return s

def validate_snapshot(s):
 require(type(s)is dict and type(s.get('pools'))is list and len(s['pools'])==1,'one breaker Pool')
 p=s['pools'][0];require(p.get('protocol')==PROTOCOL and p.get('adapter_version')==ADAPTER,'independent breaker identity')
 require(s['source']=='SYNTHETIC_MANAGED_BREAKER_WIRE','explicit synthetic breaker source');require(s['reference']['adapter_set_hash']==legacy.array_hash([ADAPTER]),'breaker adapter hash')
 m=p['multi_asset']['managed_v2'];legacy.exact_keys(m,(*legacy.MANAGED_FIELDS,'schedule','bpt','circuit_breakers'),'full breaker state')
 c=m['circuit_breakers'];legacy.exact_keys(c,('tokens',),'circuit list');tokens=c['tokens'];assets=p['multi_asset']['assets'];require(type(tokens)is list and len(tokens)==len(assets),'complete token array')
 for i,(t,a,w)in enumerate(zip(tokens,assets,p['multi_asset']['weights'])):
  legacy.exact_keys(t,('token',*FIELDS),'circuit token');require(t['token']==a['token'],'circuit token order')
  for name in FIELDS:legacy.amount(t[name],name)
  word=int(t['state_word']);lower=word&65535;upper=word>>16&65535;ref=word>>32&((1<<64)-1);price=word>>96&((1<<96)-1);scale=int(a['scaling'])
  require(not lower or ONE//10>>48<=lower<=ONE>>48,'lower compressed range');require(not upper or lower<=upper<=10*ONE>>48,'upper compressed range')
  require(not word or ONE//100<=ref<=ONE,'reference weight');require(t['reference_weight']==str(ref) and t['reference_bpt_price']==str(price*scale//ONE),'original raw reference getters')
  require(t['lower_percentage']==str(lower<<48) and t['upper_percentage']==str(upper<<48),'original raw percentage getters')
  require(m['token_info'][i]['circuit_breaker_enabled']==bool(word),'installed word/flag')
  for is_lower,percentage,offset,name in [(True,lower,192,'lower_bpt_price_bound'),(False,upper,224,'upper_bpt_price_bound')]:
   if percentage==0 or price==0:require(t[name]=='0','disabled direction known zero');continue
   require(ref==int(w),'synthetic fixture uses only cached branch')
   ratio=(word>>offset&((1<<32)-1))*10*ONE//(2**32-1);product=price*ratio;bound=(product-1)//ONE+1 if is_lower and product else product//ONE
   require(t[name]==str(bound*scale//ONE),'cache-hit exact bound getter')
 prior=copy.deepcopy(s);q=prior['pools'][0];q['protocol'],q['adapter_version']=base.PROTOCOL,base.ADAPTER;prior['source']='SYNTHETIC_MANAGED_BPT_WIRE';prior['reference']['adapter_set_hash']=legacy.array_hash([base.ADAPTER]);del q['multi_asset']['managed_v2']['circuit_breakers']
 for info in q['multi_asset']['managed_v2']['token_info']:info['circuit_breaker_enabled']=False
 base.validate_snapshot(prior)

def encode(s,level=''):
 amounts=base.legacy.AMOUNTS|{'pool_state_word','state_word','minimum_bpt_locked',*FIELDS}
 def blob(raw):return '"'+''.join('\\x%02x'%b for b in raw)+'"'
 out=[]
 for name,value in s.items():
  for item in value if isinstance(value,list)else[value]:
   if item is None:continue
   if isinstance(item,dict):out.append(name+' { '+encode(item,name)+' }')
   elif name in amounts:legacy.amount(item,name);out.append(name+' { uint256_be: '+blob(int(item).to_bytes(32,'big'))+' }')
   elif name in ('hash','parent_hash','block_hash','adapter_set_hash','scope_hash')or name.endswith('_sha256')or name=='pool_id'and level=='multi_asset':out.append(name+': '+blob(legacy.digest_bytes(item,name)))
   else:out.append(name+': '+json.dumps(item,ensure_ascii=True,allow_nan=False))
 return ' '.join(out)
def encode_protobuf(s):validate_snapshot(s);return legacy._protoc('encode',encode(s).encode())
def build_fixture():
 s=build_snapshot();raw=encode_protobuf(s);require(legacy._protoc('encode',legacy._protoc('decode',raw))==raw,'C++ roundtrip')
 return {'snapshot':s,'protobuf_hex':raw.hex(),'protobuf_sha256':hashlib.sha256(raw).hexdigest(),'authority':AUTHORITY}
def main():
 p=argparse.ArgumentParser(description=__doc__);p.add_argument('--check',action='store_true');p.add_argument('--self-test',action='store_true');a=p.parse_args()
 if a.self_test:
  tests=unittest.defaultTestLoader.discover(str(ROOT/'integration/tools/acceptance/tests'),pattern='test_managed_v2_breaker_wire.py')
  if not unittest.TextTestRunner(verbosity=2).run(tests).wasSuccessful():raise SystemExit(1)
  return
 v=build_fixture();data=(json.dumps(v,indent=2)+'\n').encode();legacy.publish(data,[ROOT/p/'test/contracts'/FILENAME for p in base.scheduled.PROJECTS],check=a.check);print(json.dumps({'passed':True,'bytes':len(data),'sha256':hashlib.sha256(data).hexdigest()}))
if __name__=='__main__':main()
