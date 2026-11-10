#!/usr/bin/env python3
"""Additive, read-only multichain original-pool public-state fork captures."""
import argparse
import copy
import datetime
import json
import re
from pathlib import Path
import subprocess
import sys
sys.path.insert(0,str(Path(__file__).resolve().parent))
from r01_public_catalog import ROOT,CatalogError,require,load_json,read_blob,sha256,IMPLEMENTATION_SLOT,BEACON_SLOT
from r01_public_catalog_capture import fetch_rpc,write_new,reference
from r01_public_catalog_history import ForkArchive,start_archive

BASE=ROOT/'integration/test/source/deployment-catalog'
OUT=BASE/'evm-continuation-v2'
CHAINS={'base':(8453,['https://base.drpc.org','https://mainnet.base.org']),'optimism':(10,['https://optimism.drpc.org','https://mainnet.optimism.io']),'bsc':(56,['https://bsc.drpc.org','https://bsc-rpc.publicnode.com','https://bsc-dataseed.bnbchain.org']),'arbitrum':(42161,['https://arb1.arbitrum.io/rpc','https://arbitrum-one-rpc.publicnode.com']),'polygon':(137,['https://polygon.drpc.org','https://polygon-bor-rpc.publicnode.com']),'avalanche':(43114,['https://avalanche-c-chain-rpc.publicnode.com','https://api.avax.network/ext/bc/C/rpc'])}
WRAPPED={'base':'0x4200000000000000000000000000000000000006','optimism':'0x4200000000000000000000000000000000000006','bsc':'0xbb4cdb9cbd36b01bd1cbaebf2de08d9173bc095c','arbitrum':'0x82af49447d8a07e3bd95bd0d56f35241523fbab1','polygon':'0x0d500b1d8e8ef31e21c99d1db9a6444d3adf1270','avalanche':'0xb31f66aa3c1e785363f0875a1b74e27b85fd66c7'}

def storage_trie_key(chain,key):
    require(chain in CHAINS,'STORAGE_KEY_CHAIN_UNSUPPORTED')
    require(isinstance(key,str) and re.fullmatch(r'0x[0-9a-fA-F]{1,64}',key) is not None,'STORAGE_KEY_UINT256')
    try:value=int(key[2:],16)
    except ValueError:raise ValueError('STORAGE_KEY_UINT256')
    require(value>=0,'STORAGE_KEY_UINT256')
    if chain=='avalanche':value &= ~(1<<248)
    return '0x'+format(value,'064x')

def state_root_semantics(chain,header):
    height=int(header['number'],16)
    if chain=='avalanche':
        require('settledHeight' in header,'AVALANCHE_NATIVE_STATE_HEIGHT_REQUIRED');state=int(header['settledHeight'],16)
        require(state<height,'AVALANCHE_SETTLED_HEIGHT')
        return {'header_height':height,'state_height':state,'state_root':header['stateRoot'],'proof_for_execution_H_established':False,'authority':'Coreth SAE header root describes settledHeight; target-H root requires native successor coverage'}
    require(chain in CHAINS,'EVM_CHAIN_UNSUPPORTED')
    return {'header_height':height,'state_height':height,'state_root':header['stateRoot'],'proof_for_execution_H_established':False,'authority':'candidate state root; independently verify native header and proof before qualification'}

def chain_plan(catalog,chain):
    require(chain in CHAINS,'EVM_CHAIN_UNSUPPORTED');cases=[];ids=[]
    for row in catalog['deployments']:
        if row['chain']!=chain:continue
        require(row['chain_key']=='evm:'+str(CHAINS[chain][0]),'PLAN_CHAIN_ID')
        require(row['sample_pools'],'PLAN_REAL_POOL_REQUIRED')
        # The wrapped-native volatile/reference pool avoids invented token balances.
        sample=next((p for p in row['sample_pools'] if WRAPPED[chain] in [t.lower() for t in p['query_tokens']]),None)
        require(sample,'PLAN_WRAPPED_NATIVE_POOL_REQUIRED');pool=sample['address'].lower();require(pool!='0x'+'00'*20,'PLAN_NONZERO_POOL')
        token_in=WRAPPED[chain];token_out=next(t.lower() for t in sample['query_tokens'] if t.lower()!=token_in)
        protocol=row['protocol'];kind='v2-solidly' if protocol in ('aerodrome-v2','velodrome-v2') else 'v2-pancake' if protocol=='pancake-v2' else 'lb' if protocol=='lfj-liquidity-book' else 'v3'
        cases.append({'catalog_deployment_id':row['id'],'protocol':protocol,'kind':kind,'pool':pool,'factory':row['address'].lower(),'factory_runtime_sha256':row['runtime']['sha256'],'pool_runtime_sha256':sample['runtime']['sha256'],'token_in':token_in,'token_out':token_out,'amount_in':'100000000000000','query_parameter':sample['query_parameter'],'pool_implementation':sample.get('implementation')})
        ids.append(row['id'])
    require(cases and len(ids)==len(set(ids)),'PLAN_FINITE_UNIQUE_SCOPE')
    return {'schema_version':1,'chain':chain,'chain_id':CHAINS[chain][0],'wrapped_native':WRAPPED[chain],'cases':cases,'source_catalog':reference(BASE/'catalog.json'),'execution_boundary':'local helper newly deployed; original wrapped token and original pools only; no public transactions'}

def simulation_header(original):
    local=copy.deepcopy(original);transactions=local.pop('transactions',[]);local['transactions']=[]
    return local,{'transactions':{'original_count':len(transactions),'local_count':0},'original_native_transactions_replayed':False,'authority':'local fork initialization view only; original header/body retained without alteration'}

class EvmArchive(ForkArchive):
    def __init__(self,anchor,directory,endpoints,replay=False):
        super().__init__(anchor,directory,endpoints,replay);self.views={}
    def request(self,method,params):
        response=super().request(method,params)
        if method in ('eth_getBlockByNumber','eth_getBlockByHash') and isinstance(response.get('result'),dict):
            view,changes=simulation_header(response['result']);self.views[response['result']['hash']]=changes
            return response|{'result':view}
        return response

def capture(chain,directory,resume=False):
    directory=directory.resolve();require(directory.is_relative_to(OUT.resolve()),'OWNED_EVM_DIRECTORY');directory.mkdir(parents=True,exist_ok=True);require(resume or not (directory/'anchor.json').exists(),'IMMUTABLE_EVM_ANCHOR_EXISTS')
    plan=chain_plan(load_json(BASE/'catalog.json'),chain);endpoints=CHAINS[chain][1];sequence=0;initial=[]
    def call(endpoint,method,params,name):
        nonlocal sequence
        sequence+=1;ref,value=fetch_rpc(endpoint,method,params,directory/f'{sequence:03}-{name}.json',budget=16*1024*1024);initial.append(ref);return ref,value
    if not resume:
        primary=None;header=None;href=None
        for endpoint in endpoints:
            _,identity=call(endpoint,'eth_chainId',[],'chain-id');require(identity is None or int(identity,16)==plan['chain_id'],'CAPTURE_CHAIN_ID')
            ref,value=call(endpoint,'eth_getBlockByNumber',['finalized',True],'finalized-header')
            if isinstance(value,dict) and value.get('number'):primary=endpoint;header=value;href=ref;break
        require(header,'PUBLIC_FINALIZED_HEADER_UNAVAILABLE');tag=header['number'];ordered=[primary]+[e for e in endpoints if e!=primary]
        genesisref,genesis=call(primary,'eth_getBlockByNumber',['0x0',False],'genesis')
        witnessref,witness=call(ordered[1],'eth_getBlockByNumber',[tag,False],'witness-header');require(witness and witness['hash']==header['hash'],'PUBLIC_WITNESS_H_MISMATCH')
        anchor={'height':int(tag,16),'hash':header['hash'],'timestamp':int(header['timestamp'],16),'chain_key':'evm:'+str(plan['chain_id']),'chain_id':plan['chain_id'],'state_root':header['stateRoot'],'finality_observation':'finalized','native_state_semantics':state_root_semantics(chain,header)}
        write_new(directory/'anchor.json',(json.dumps({'schema_version':2,'anchor':anchor,'header':href,'genesis':genesisref,'witness':witnessref,'endpoints':ordered,'initial_observations':initial,'genesis_hash':genesis.get('hash') if genesis else None},indent=2)+'\n').encode());write_new(directory/'execution-plan.json',(json.dumps(plan,indent=2)+'\n').encode())
    else:
        saved=load_json(directory/'anchor.json');anchor=saved['anchor'];header=json.loads(read_blob(ROOT,saved['header']))['response']['result'];primary=saved['endpoints'][0];ordered=saved['endpoints'];tag=header['number'];plan=load_json(directory/'execution-plan.json');chain=plan['chain']
    if not (directory/'sender-candidates.json').exists():
        candidates=[];candidate_refs=[]
        # Sparse blocks may contain only a system sender or low-balance user.
        if len(header['transactions'])<6:
            for depth in range(1,9):
                ref,previous=call(primary,'eth_getBlockByNumber',[hex(anchor['height']-depth),True],'sender-history-'+str(depth))
                candidate_refs.append(ref)
                if previous:candidates.extend(t['from'].lower() for t in previous['transactions'] if t.get('from'))
        candidates=list(dict.fromkeys(candidates))
        write_new(directory/'sender-candidates.json',(json.dumps({'addresses':candidates,'source_observations':candidate_refs,'state_read_height':anchor['height'],'authority':'public previous-block senders; balances/code still read only at frozen H'},indent=2)+'\n').encode())
    archive=EvmArchive(anchor,directory,ordered);server,url=start_archive(archive)
    try:
        run=subprocess.run(['node',str(ROOT/'integration/tools/acceptance/r01_public_catalog_evm_fork.cjs'),'capture',str(directory),url],capture_output=True,text=True,timeout=480)
        attempt=len(list(directory.glob('execution.stdout*.log')))+1
        suffix='' if attempt==1 else '-'+str(attempt)
        write_new(directory/('execution.stdout'+suffix+'.log'),run.stdout.encode());write_new(directory/('execution.stderr'+suffix+'.log'),run.stderr.encode());require(run.returncode==0,'ORIGINAL_EVM_FORK_FAILED:'+run.stderr[-2500:])
        execution=load_json(directory/'fork-execution.json');accounts=archive.account_slots()
        # Read and proof-bind each actually used address/slot; EOA and absence included.
        require(len(accounts)<=64,'EVM_PRESTATE_ACCOUNT_BUDGET');deployments=[];missing=[]
        for address,slots in sorted(accounts.items()):
            require(len(slots)<=256,'EVM_PRESTATE_STORAGE_BUDGET')
            try:
                p=archive.request('eth_getProof',[address,sorted(slots),tag])['result'];code=archive.request('eth_getCode',[address,tag])['result']
                require(len(p['storageProof'])==len(slots),'EVM_PROOF_SLOT_INVENTORY')
                for item in p['storageProof']:item['key']='0x'+item['key'][2:].zfill(64)
                require({p['key'] for p in p['storageProof']}==set(slots),'EVM_PROOF_SLOT_KEYS');deployments.append({'address':address,'code':code,'proof':p})
            except (CatalogError,KeyError,TypeError) as error:missing.append({'address':address,'storage_slots':sorted(slots),'error':str(error)})
        evidence={'schema_version':2,'chain_id':str(plan['chain_id']),'chain':chain,'expected_block_hash':anchor['hash'],'block':header,'deployments':deployments,'missing_proofs':missing,'root_semantics':anchor['native_state_semantics']}
        write_new(directory/'prestate-evidence.json',(json.dumps(evidence,indent=2)+'\n').encode())
        write_new(directory/'proof-boundary.json',(json.dumps({'accounts':{a:sorted(s) for a,s in accounts.items()},'missing_proofs':missing,'all_lazy_public_prestate_reads_proof_bound':False,'native_header_body_verification_required':True},indent=2)+'\n').encode())
        closing=None
        for endpoint in ordered:
            _,candidate=call(endpoint,'eth_getBlockByNumber',[tag,False],'closing-header')
            if candidate and candidate.get('hash')==anchor['hash']:closing=candidate;break
        require(closing and closing['hash']==anchor['hash'],'EVM_CLOSING_H_MISMATCH')
    finally:
        server.shutdown();server.server_close();archive.save();write_new(directory/'simulation-header-views.json',(json.dumps(archive.views,indent=2)+'\n').encode())
    return {'directory':str(directory.relative_to(ROOT)),'cases':[{k:c.get(k) for k in ('catalog_deployment_id','passed','actual_net_recipient','error')} for c in execution['cases']],'accounts':len(accounts),'missing_proofs':len(missing)}

def verify_simulation_views(expected,observed):
    require(set(observed)<=set(expected),'SIMULATION_UNRECORDED_HEADER_VIEW')
    require(all(expected[k]==v for k,v in observed.items()),'SIMULATION_HEADER_VIEW_REPLAY')
    return sorted(set(expected)-set(observed))

def replay(directory):
    directory=directory.resolve();anchor=load_json(directory/'anchor.json');archive=EvmArchive(anchor['anchor'],directory,anchor['endpoints'],replay=True);server,url=start_archive(archive)
    try:
        p=subprocess.run(['node',str(ROOT/'integration/tools/acceptance/r01_public_catalog_evm_fork.cjs'),'replay',str(directory),url],capture_output=True,text=True,timeout=480);require(p.returncode==0,'EVM_OFFLINE_REPLAY_FAILED:'+p.stderr[-2500:]);result=json.loads(p.stdout);unused=verify_simulation_views(load_json(directory/'simulation-header-views.json'),archive.views);return result|{'network_requests':0,'unused_original_header_discovery_reads':unused}
    finally:server.shutdown();server.server_close()

def main():
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('command',choices=['capture','resume','replay']);parser.add_argument('--chain',choices=CHAINS);parser.add_argument('--directory',type=Path,required=True);args=parser.parse_args();result=capture(args.chain,args.directory,resume=args.command=='resume') if args.command in ('capture','resume') else replay(args.directory);print(json.dumps(result,indent=2))
if __name__=='__main__':main()
