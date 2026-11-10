#!/usr/bin/env python3
"""Bounded same-H supplemental proofs for actual normal-getter storage reads.

V3 tick keys come from the already exact-rebuilt compiler storage layout and an
original-runtime bitmap getter. Raw failed responses and every input version
remain separate. This acquires public proofs only; it never fills unknown slots.
"""
import argparse
from concurrent.futures import ThreadPoolExecutor
import json
from pathlib import Path
import subprocess
import sys
sys.path.insert(0,str(Path(__file__).resolve().parent))
from r01_public_catalog import ROOT,load_json,read_blob,require
from r01_public_catalog_capture import reference,write_new
from r01_public_catalog_evm_seed import curl_read
BASE=ROOT/'integration/test/source/deployment-catalog/evm-normal-v4'

def storage_plan(input_path,probe_path):
    input_=load_json(input_path);row=input_['row'];probe=load_json(probe_path);require(probe['id']==row['id'] and probe['anchor']==row['anchor'],'SUPPLEMENT_PROBE_ANCHOR');accounts={}
    for miss in probe['missing']:
        if miss['kind']=='storage':accounts.setdefault(miss['address'],set()).add(miss['key'])
    layout_ref=row['inputs']['source_runtime'];source=json.loads(read_blob(ROOT,layout_ref));actual=next(r for r in source['entries'] if r['id']==row['id']);require(actual['pool_rebuild']['source_rebuild_verified'],'SUPPLEMENT_EXACT_SOURCE_REBUILD')
    layout=actual['pool_rebuild']['storage_layout'];tick_mapping=next((s for s in layout['storage'] if s['label']=='ticks'),None)
    bitmap=next((c for c in probe['calls'] if c['data'].startswith('0x5339c296') and c['value'] is not None),None)
    details=None
    if tick_mapping and bitmap:
        mapping=layout['types'][tick_mapping['type']];structure=layout['types'][mapping['value']];require(mapping['encoding']=='mapping' and structure['encoding']=='inplace','SUPPLEMENT_TICK_LAYOUT');key_type=layout['types'][mapping['key']]['label'];require(key_type=='int24','SUPPLEMENT_TICK_MAPPING_KEY')
        offsets=sorted({int(m['slot']) for m in structure['members']});require(offsets and len(offsets)<=8 and max(offsets)<8,'SUPPLEMENT_STATIC_TICK_SLOT_BUDGET')
        for member in structure['members']:require(layout['types'][member['type']]['encoding']=='inplace' and int(layout['types'][member['type']]['numberOfBytes'])<=32,'SUPPLEMENT_DYNAMIC_TICK_FIELD')
        spacing=int(next(c['value'] for c in probe['calls'] if c['data']=='0xd0c93a7c'),16);center=int(bitmap['data'][10:],16);center=center-(1<<256) if center>>(255) else center;bits=int(bitmap['value'],16);ticks=[(center*256+bit)*spacing for bit in range(256) if bits>>bit&1]
        script="const {ethers}=require(process.argv[1]);const spec=JSON.parse(process.argv[2]);const keys=[];for(const tick of spec.ticks){const base=BigInt(ethers.utils.keccak256(ethers.utils.defaultAbiCoder.encode(['int24','uint256'],[tick,spec.slot])));for(const n of spec.offsets)keys.push('0x'+((base+BigInt(n))% (1n<<256n)).toString(16).padStart(64,'0'));}console.log(JSON.stringify(keys));"
        derived=json.loads(subprocess.run(['node','-e',script,str(ROOT/'integration/.runtime/stable-reference/node_modules/ethers'),json.dumps({'ticks':ticks,'slot':tick_mapping['slot'],'offsets':offsets})],capture_output=True,text=True,check=True,timeout=20).stdout)
        accounts.setdefault(row['pool'],set()).update(derived);details={'mapping_slot':tick_mapping['slot'],'key_type':key_type,'member_slots':offsets,'initialized_tick_count':len(ticks),'tick_indices':ticks,'bitmap':bitmap['data'],'compiler_layout_source':layout_ref,'member_layout':structure}
    require(accounts and sum(len(v) for v in accounts.values())<=2048,'SUPPLEMENT_FINITE_STORAGE_PLAN')
    return {'schema_version':4,'id':row['id'],'anchor':row['anchor'],'input':reference(input_path),'probe':reference(probe_path),'accounts':{a:sorted(slots) for a,slots in accounts.items()},'tick_layout':details,'authority':'compiler storage layout already bound to complete original runtime plus actual bitmap getter and actual failed SLOAD keys; not a quote model'}

def acquire(input_path,probe_path,round_,endpoint=None):
    directory=input_path.parent;require(directory.resolve().is_relative_to(BASE.resolve()),'OWNED_SUPPLEMENT_DIRECTORY');plan=storage_plan(input_path,probe_path);planpath=directory/f'storage-plan-{round_:02}.json';write_new(planpath,(json.dumps(plan,indent=2)+'\n').encode());input_=load_json(input_path);row=input_['row'];endpoints=load_json(ROOT/row['directory']/'anchor.json')['endpoints'];preferred={'base':'https://base.drpc.org','optimism':'https://mainnet.optimism.io','polygon':'https://polygon.drpc.org','arbitrum':'https://public.1rpc.io/arb'}
    selected=list(dict.fromkeys([endpoint or preferred[row['chain']]]+endpoints))[:2];chunks=[(a,slots[i:i+128]) for a,slots in plan['accounts'].items() for i in range(0,len(slots),128)];require(len(chunks)<=16,'SUPPLEMENT_REQUEST_COUNT');success=[];fail=[]
    preferred_by_address={};preflight_refs=[];unavailable=[]
    for index,(address,slots) in enumerate(plan['accounts'].items()):
        available=False
        for j,url in enumerate(selected):
            ref,value=curl_read(url,'eth_getProof',[address,[slots[0]],hex(row['anchor']['height'])],directory/f'preflight-{round_:02}-{index:02}-attempt{j+1}.json');preflight_refs.append(ref)
            if value:
                preferred_by_address[address]=url;success.append(ref);available=True;break
        if not available:unavailable.append(address)
    if unavailable:
        result={'schema_version':4,'plan':reference(planpath),'successful_proofs':success,'failed_attempts':preflight_refs,'unavailable_accounts':unavailable,'complete_transport_inventory':False,'qualification':'bounded fixed-H proof preflight failed; no bulk retry, unknown storage stays rejected'}
        write_new(directory/f'supplement-result-{round_:02}.json',(json.dumps(result,indent=2)+'\n').encode());print(json.dumps(result,indent=2));return result
    def get(item):
        index,(address,slots)=item;attempts=[]
        for j,url in enumerate([preferred_by_address[address]]):
            file=directory/f'supplement-{round_:02}-{index:02}-attempt{j+1}.json';ref,value=curl_read(url,'eth_getProof',[address,slots,hex(row['anchor']['height'])],file);attempts.append(ref)
            if value:
                require(value['address'].lower()==address and len(value['storageProof'])==len(slots),'SUPPLEMENT_RESPONSE_INVENTORY');actual={'0x'+s['key'][2:].lower().zfill(64) for s in value['storageProof']};require(actual==set(slots),'SUPPLEMENT_RESPONSE_KEYS');return ref,attempts
        return None,attempts
    with ThreadPoolExecutor(max_workers=3) as pool:
        for ref,attempts in pool.map(get,enumerate(chunks)):
            if ref:success.append(ref)
            else:fail.extend(attempts)
    result={'schema_version':4,'plan':reference(planpath),'successful_proofs':success,'failed_attempts':fail,'complete_transport_inventory':not fail,'qualification':'pending cryptographic verification against original H root; transport success is not proof validity'};write_new(directory/f'supplement-result-{round_:02}.json',(json.dumps(result,indent=2)+'\n').encode())
    if not fail:
        next_input=input_|{'supplements':input_.get('supplements',[])+success,'prior_input':reference(input_path),'supplement_plan':reference(planpath)};out=directory/f'input-supplement-{round_:02}.json';write_new(out,(json.dumps(next_input,indent=2)+'\n').encode());result['next_input']=str(out.relative_to(ROOT))
    print(json.dumps({k:v for k,v in result.items() if k!='successful_proofs'},indent=2));return result

def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--input',type=Path,required=True);p.add_argument('--probe',type=Path,required=True);p.add_argument('--round',type=int,required=True);p.add_argument('--endpoint');a=p.parse_args();acquire(a.input.resolve(),a.probe.resolve(),a.round,a.endpoint)
if __name__=='__main__':main()
