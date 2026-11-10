#!/usr/bin/env python3
"""Independent C++ protobuf syntax for BPT41. All fixtures are synthetic."""
import argparse
import copy
import hashlib
import importlib.util
import json
from pathlib import Path
import unittest
ROOT=Path(__file__).resolve().parents[3]
spec=importlib.util.spec_from_file_location('scheduled_bpt_wire',Path(__file__).with_name('managed-v2-scheduled-wire-golden.py'))
scheduled=importlib.util.module_from_spec(spec);spec.loader.exec_module(scheduled)
legacy=scheduled.legacy;require=legacy.require
PROTOCOL='balancer-v2-managed-bpt'
ADAPTER='balancer-v2/20230411-managed-bpt-1'
AUTHORITY='synthetic-bpt-state-wire-contract-no-native-execution-proof'
FILENAME='managed-v2-bpt-wire.json'

def build_snapshot():
    s=scheduled.build_snapshot();s['source']='SYNTHETIC_MANAGED_BPT_WIRE'
    p=s['pools'][0];p['protocol'],p['adapter_version']=PROTOCOL,ADAPTER
    s['reference']['adapter_set_hash']=legacy.array_hash([ADAPTER])
    p['multi_asset']['managed_v2']['bpt']={'join_exit_enabled':True,'minimum_bpt_locked':'1000000','bpt_last_change_block':0,
        'allowlist_members':[{'address':'0x0000000000000000000000000000000000000001','allowed':False}]}
    validate_snapshot(s);return s

def validate_snapshot(s):
    require(type(s) is dict and type(s.get('pools')) is list and len(s['pools'])==1,'one BPT Pool required')
    p=s['pools'][0];require(p.get('protocol')==PROTOCOL and p.get('adapter_version')==ADAPTER,'BPT profile required')
    m=p['multi_asset']['managed_v2'];legacy.exact_keys(m,(*legacy.MANAGED_FIELDS,'schedule','bpt'),'BPT full state')
    b=m['bpt'];legacy.exact_keys(b,('join_exit_enabled','minimum_bpt_locked','bpt_last_change_block','allowlist_members'),'BPT controls')
    require(type(b['join_exit_enabled']) is bool and b['join_exit_enabled']==bool(int(m['schedule']['pool_state_word'])>>255&1),'BPT raw flag')
    require(b['minimum_bpt_locked']=='1000000' and int(m['virtual_supply'])>=1000000,'locked BPT minimum')
    legacy.uint(b['bpt_last_change_block'],32,'BPT registration block');require(b['bpt_last_change_block']<=m['block_number'],'BPT registration source')
    members=b['allowlist_members'];require(type(members) is list,'BPT members required');previous=''
    for member in members:
        legacy.exact_keys(member,('address','allowed'),'BPT member');legacy.address(member['address'],'BPT member')
        require(type(member['allowed']) is bool and previous<member['address'],'BPT member order/result');previous=member['address']
    require(s['source']=='SYNTHETIC_MANAGED_BPT_WIRE','synthetic BPT source required')
    base=copy.deepcopy(s);base['source']=scheduled.SOURCE;q=base['pools'][0];q['protocol'],q['adapter_version']=scheduled.PROTOCOL,scheduled.ADAPTER
    del q['multi_asset']['managed_v2']['bpt'];base['reference']['adapter_set_hash']=legacy.array_hash([scheduled.ADAPTER])
    scheduled.validate_snapshot(base)
    require(s['reference']['adapter_set_hash']==legacy.array_hash([ADAPTER]),'BPT adapter hash')

def encode(snapshot,level=''):
    amounts=legacy.AMOUNTS|{'pool_state_word','state_word','minimum_bpt_locked'}
    def blob(raw):return '"'+''.join('\\x%02x'%b for b in raw)+'"'
    rows=[]
    for name,value in snapshot.items():
        for item in value if isinstance(value,list) else [value]:
            if item is None:continue
            if isinstance(item,dict):rows.append(name+' { '+encode(item,name)+' }')
            elif name in amounts:
                legacy.amount(item,name);rows.append(name+' { uint256_be: '+blob(int(item).to_bytes(32,'big'))+' }')
            elif name in ('hash','parent_hash','block_hash','adapter_set_hash','scope_hash') or name.endswith('_sha256') or name=='pool_id' and level=='multi_asset':
                rows.append(name+': '+blob(legacy.digest_bytes(item,name)))
            else:rows.append(name+': '+json.dumps(item,ensure_ascii=True,allow_nan=False))
    return ' '.join(rows)

def encode_protobuf(s):
    validate_snapshot(s);return legacy._protoc('encode',encode(s).encode())

def build_fixture():
    s=build_snapshot();raw=encode_protobuf(s)
    require(legacy._protoc('encode',legacy._protoc('decode',raw))==raw,'C++ BPT roundtrip')
    return {'snapshot':s,'protobuf_hex':raw.hex(),'protobuf_sha256':hashlib.sha256(raw).hexdigest(),'authority':AUTHORITY}

def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--check',action='store_true');p.add_argument('--self-test',action='store_true');a=p.parse_args()
    if a.self_test:
        suite=unittest.defaultTestLoader.discover(str(ROOT/'integration/tools/acceptance/tests'),pattern='test_managed_v2_bpt_wire.py')
        if not unittest.TextTestRunner(verbosity=2).run(suite).wasSuccessful():raise SystemExit(1)
        return
    value=build_fixture();data=(json.dumps(value,indent=2)+'\n').encode()
    legacy.publish(data,[ROOT/p/'test/contracts'/FILENAME for p in scheduled.PROJECTS],check=a.check)
    print(json.dumps({'passed':True,'bytes':len(data),'sha256':hashlib.sha256(data).hexdigest()}))
if __name__=='__main__':main()
