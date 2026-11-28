#!/usr/bin/env python3
"""Freeze and immediately acquire fixed-H proofs before a bounded fork replay.

Every derived cache value is explicitly marked with its public proof source. No
synthetic response is represented as a captured JSON-RPC wire response.
"""
import argparse
import concurrent.futures
import datetime
import json
from pathlib import Path
import subprocess
import sys
import time
sys.path.insert(0,str(Path(__file__).resolve().parent))
from r01_public_catalog import ROOT,load_json,read_blob,require,sha256,public_rpc_endpoint
from r01_public_catalog_capture import MAX_BYTES,fetch_rpc,request as request_http,write_new,reference
from r01_public_catalog_evm_history import CHAINS,TARGETS,state_root_semantics,EvmArchive,start_archive,storage_trie_key
BASE=ROOT/'integration/test/source/deployment-catalog';OUT=BASE/'evm-continuation-v2'

def curl_read(endpoint,method,params,path):
    from r01_public_catalog_history import READ_METHODS
    require(method in READ_METHODS,'SEED_PUBLIC_READ_ONLY');public_rpc_endpoint(endpoint)
    body=json.dumps({'jsonrpc':'2.0','id':1,'method':method,'params':params})
    p=subprocess.run(['curl','--silent','--show-error','--fail-with-body','--max-time','18','--max-filesize',str(16*1024*1024),'--user-agent','XONE-source-evidence/1.0','--header','Content-Type: application/json','--data-binary',body,endpoint],capture_output=True,timeout=22)
    require(len(p.stdout)<=16*1024*1024,'SEED_RESPONSE_BUDGET')
    try:response=json.loads(p.stdout)
    except ValueError:response=None
    record={'endpoint':endpoint,'method':method,'params':params,'observed_at':datetime.datetime.now(datetime.timezone.utc).isoformat(),'user_agent':'XONE-source-evidence/1.0','transport':'curl HTTPS, 18 second bound','exit_code':p.returncode,'response':response,'response_sha256':sha256(p.stdout),'stderr':p.stderr.decode(errors='replace')}
    write_new(path,(json.dumps(record,indent=2)+'\n').encode());return reference(path),response.get('result') if isinstance(response,dict) else None

def previous_boundary(previous):
    path=previous/'proof-boundary.json'
    if path.exists():return load_json(path)['accounts']
    accounts={}
    for ref in load_json(previous/'rpc-index.json')['records']:
        r=json.loads(read_blob(ROOT,ref))
        if not isinstance(r.get('response'),dict) or 'result' not in r['response']:continue
        if r['method'] not in {'eth_getCode','eth_getBalance','eth_getTransactionCount','eth_getStorageAt'}:continue
        address=r['params'][0].lower();accounts.setdefault(address,set())
        if r['method']=='eth_getStorageAt':accounts[address].add('0x'+r['params'][1][2:].zfill(64).lower())
    require(accounts,'SEED_ACTUAL_PREVIOUS_READ_BOUNDARY_REQUIRED')
    return {a:sorted(slots) for a,slots in accounts.items()}

def helper_boundary(sender, nonce, previous_helper, wrapped_slots):
    script = """
const {ethers} = require(process.argv[1]);
const {defaultAbiCoder, keccak256} = ethers.utils;
const [owner, nonce, prior, encodedSlots] = process.argv.slice(2);
const address = ethers.utils.getContractAddress({from: owner, nonce}).toLowerCase();
const slots = new Set(JSON.parse(encodedSlots));
const keys = [];
for (let slot = 0; slot < 128; slot++) {
    const balance = account => keccak256(defaultAbiCoder.encode(['address', 'uint256'], [account, slot]));
    const allowance = account => keccak256(defaultAbiCoder.encode(['address', 'bytes32'], [account, balance(owner)]));
    if (slots.has(balance(prior))) keys.push(balance(address));
    if (slots.has(allowance(prior))) keys.push(allowance(address));
}
console.log(JSON.stringify({address, keys}));
"""
    ethers = ROOT / 'integration/.runtime/stable-reference/node_modules/ethers'
    result = subprocess.run(['node', '-e', script, str(ethers), sender, nonce, previous_helper,
                             json.dumps(wrapped_slots)], capture_output=True, text=True, check=True, timeout=20)
    return json.loads(result.stdout)


def observe_batch_finality(witness, header, directory):
    height = int(header['number'], 16)
    deadline = time.monotonic() + 30
    for attempt in range(10):
        finalized_ref, finalized = fetch_rpc(witness, 'eth_getBlockByNumber', ['finalized', False],
                                             directory / f'finality-observation-{attempt}.json')
        if finalized and int(finalized['number'], 16) >= height:
            canonical_ref, canonical = fetch_rpc(witness, 'eth_getBlockByNumber', [header['number'], False],
                                                  directory / 'finalized-canonical-header.json')
            require(canonical and all(canonical.get(key) == header[key] for key in ('number', 'hash', 'stateRoot')),
                    'BATCH_FINALIZED_CANONICAL_HEADER')
            record = {'subsequent_finalized_header': finalized_ref, 'independent_canonical_witness': canonical_ref}
            write_new(directory / 'subsequent-finality.json', (json.dumps(record, indent=2) + '\n').encode())
            return
        if time.monotonic() >= deadline:
            break
        time.sleep(1)
    require(False, 'BATCH_FINALITY_UNAVAILABLE')


def capture(chain,previous,directory,tag,batch_latest=False,endpoint_override=None,single_keys=False,partitioned_storage=False):
    require(not partitioned_storage or chain=='avalanche','PARTITIONED_STORAGE_CHAIN')
    require(not batch_latest or (chain != 'avalanche' and tag == 'latest' and not single_keys),
            'BATCH_LATEST_PROFILE')
    physical=lambda key:storage_trie_key(chain,key) if partitioned_storage else key
    previous=previous.resolve();directory=directory.resolve();require(directory.is_relative_to(OUT.resolve()) and not directory.exists(),'NEW_SEED_DIRECTORY');directory.mkdir(parents=True)
    old=load_json(previous/'fork-execution.json');plan=load_json(previous/'execution-plan.json')
    require(old['chain']==plan['chain']==chain and plan['chain_id']==CHAINS[chain][0],'SEED_OLD_CHAIN')
    old_index=load_json(previous/'rpc-index.json');code={}
    for ref in old_index['records']:
        r=json.loads(read_blob(ROOT,ref))
        if r['method']=='eth_getCode' and isinstance(r.get('response'),dict) and 'result' in r['response']:code[r['params'][0].lower()]=(r['response']['result'],ref)
    endpoint=endpoint_override or ('https://bsc-rpc.publicnode.com' if chain=='bsc' else 'https://avalanche-c-chain-rpc.publicnode.com' if chain=='avalanche' else 'https://arb1.arbitrum.io/rpc');public_rpc_endpoint(endpoint)
    prefetched={}
    boundary=previous_boundary(previous);sender=old['sender'];boundary.setdefault(sender,[])
    wrapped=old['cases'][0]['token_in'];previous_helper=old['cases'][0]['local_helper']
    if batch_latest:
        require(len(boundary) < 64 and all(len(slots) <= 256 for slots in boundary.values()),
                'BATCH_PROOF_READ_BUDGET')
        _,sender_pre=curl_read(endpoint,'eth_getProof',[sender,[],'latest'],directory/'sender-prediction.json');require(sender_pre,'BATCH_SENDER_PREDICTION')
        prediction=helper_boundary(sender,sender_pre['nonce'],previous_helper,boundary[wrapped])
        boundary.setdefault(prediction['address'],[])
        boundary[wrapped]=sorted(set(boundary[wrapped]+prediction['keys']))
        requests=[{'jsonrpc':'2.0','id':1,'method':'eth_getBlockByNumber','params':['latest',True]}]+[{'jsonrpc':'2.0','id':i+2,'method':'eth_getProof','params':[a,slots,'latest']} for i,(a,slots) in enumerate(sorted(boundary.items()))]
        status, body, final_endpoint = request_http(endpoint, json.dumps(requests).encode(), MAX_BYTES)
        wire = json.loads(body)
        require(status == 200 and isinstance(wire, list), 'BATCH_PUBLIC_RESPONSE_REQUIRED')
        write_new(directory/'public-batch.json',(json.dumps({'endpoint':endpoint,'http_status':status,
            'final_endpoint':final_endpoint,'requests':requests,'response':wire,'response_sha256':sha256(body),
            'source_kind':'actual-public-JSON-RPC-batch','max_methods':len(requests)},indent=2)+'\n').encode())
        batchref=reference(directory/'public-batch.json');responses={r['id']:r for r in wire}
        require(len(responses)==len(wire) and set(responses)=={r['id'] for r in requests},'BATCH_RESPONSE_INVENTORY')
        header=responses[1].get('result');require(header,'BATCH_HEADER_REQUIRED')
        write_new(directory/'frozen-header.json',(json.dumps({'endpoint':endpoint,'http_status':status,
            'method':'eth_getBlockByNumber','params':['latest',True],'response':responses[1],
            'source_kind':'projection-of-retained-public-batch','batch_source':batchref},indent=2)+'\n').encode())
        href=reference(directory/'frozen-header.json')
        for request in requests[1:]:
            address,slots,_=request['params'];path=directory/'public-proofs'/(address+'.json');response=responses[request['id']];write_new(path,(json.dumps({'endpoint':endpoint,'method':'eth_getProof','params':request['params'],'response':response,'source_kind':'projection-of-retained-public-batch','batch_source':batchref},indent=2)+'\n').encode());prefetched[address]=(reference(path),response.get('result'))
        require(prefetched[sender][1] and prefetched[sender][1]['nonce']==sender_pre['nonce'],'BATCH_SENDER_NONCE_CHANGED')
    else:
        href,header=fetch_rpc(endpoint,'eth_getBlockByNumber',[tag,True],directory/'frozen-header.json')
    require(header,'SEED_HEADER_REQUIRED')
    native_headers=None
    if chain=='avalanche':
        tipref,tip=href,header;target_height=int(tip['settledHeight'],16)
        href,header=fetch_rpc(endpoint,'eth_getBlockByNumber',[hex(target_height),True],directory/'frozen-native-target.json');require(header,'SAE_CANONICAL_TARGET_REQUIRED');native_headers=[header]
        for offset in range(1,33):
            ref,child=curl_read(endpoint,'eth_getBlockByNumber',[hex(target_height+offset),False],directory/'native-successors'/f'{offset:02}.json');require(child,'SAE_SUCCESSOR_HEADER_REQUIRED');native_headers.append(child)
            if int(child['settledHeight'],16)>=target_height:break
        require(int(native_headers[-1]['settledHeight'],16)==target_height,'SAE_EXACT_TARGET_STATE_ROOT_MISSING')
        write_new(directory/'target-finality-observation.json',(json.dumps({'finalized_tip':tipref,'target_height':target_height,'target_header':href,'target_depth_below_observed_finalized_tip':int(tip['number'],16)-target_height,'original_tag':tag},indent=2)+'\n').encode())
    height=int(header['number'],16);h=header['number'];anchor={'height':height,'hash':header['hash'],'timestamp':int(header['timestamp'],16),'chain_key':'evm:'+str(CHAINS[chain][0]),'chain_id':CHAINS[chain][0],'state_root':header['stateRoot'],'finality_observation':tag,'native_state_semantics':state_root_semantics(chain,header)}
    records=[];proofs=[];failed=[]
    def get(item):
        address,logical_slots=item;slots=sorted(set(physical(k) for k in logical_slots))
        if batch_latest:ref,value=prefetched[address]
        elif single_keys and slots:
            refs=[];ref,value=curl_read(endpoint,'eth_getProof',[address,[],h],directory/'single-proofs'/address/'account.json');refs.append(ref)
            if value:
                value['storageProof']=[]
                for i,key in enumerate(slots):
                    sr,sv=curl_read(endpoint,'eth_getProof',[address,[key],h],directory/'single-proofs'/address/(str(i)+'.json'));refs.append(sr)
                    require(sv,'SINGLE_KEY_PUBLIC_PROOF_MISSING')
                    for field in ['storageHash','codeHash','balance','nonce']:require(value[field]==sv[field],'SINGLE_KEY_ACCOUNT_CHANGED:'+field)
                    require(len(sv['storageProof'])==1,'SINGLE_KEY_PROOF_INVENTORY');value['storageProof'].extend(sv['storageProof'])
                combined=directory/'public-proofs'/(address+'.json');write_new(combined,(json.dumps({'method':'eth_getProof','params':[address,slots,h],'response':{'result':value},'source_kind':'derived-union-of-same-H-single-key-public-proofs','sources':refs},indent=2)+'\n').encode());ref=reference(combined)
        else:ref,value=curl_read(endpoint,'eth_getProof',[address,slots,h],directory/'public-proofs'/(address+'.json'))
        return address,logical_slots,ref,value
    with concurrent.futures.ThreadPoolExecutor(max_workers=3) as pool:acquired=list(pool.map(get,sorted(boundary.items())))
    # Bind the upcoming CREATE address and allowance slot to the actual frozen nonce.
    sender_proof=next(v for a,s,r,v in acquired if a==sender);require(sender_proof,'SEED_SENDER_PROOF')
    helper=prediction if batch_latest else helper_boundary(sender,sender_proof['nonce'],previous_helper,boundary[wrapped])
    code.setdefault(helper['address'],('0x',None))
    if not batch_latest:
        for address,slots,label in [(helper['address'],[],'local-helper-absence'),
                                    (wrapped,sorted(set(boundary[wrapped]+helper['keys'])),'wrapped-new-allowance')]:
            ref,value=curl_read(endpoint,'eth_getProof',[address,sorted(set(physical(k) for k in slots)),h],directory/(label+'.json'))
            acquired=[x for x in acquired if x[0]!=address];acquired.append((address,slots,ref,value))
    seq=0
    def derived(method,params,value,proofref,coderef=None):
        nonlocal seq
        seq+=1;record={'method':method,'params':params,'response':{'jsonrpc':'2.0','id':1,'result':value},'source_kind':'derived-prestate-from-public-MPT-proof','proof_source':proofref,'code_bytes_source':coderef,'authority':'derived cache value, not a captured RPC wire response'};path=directory/'rpc'/f'{seq:04}-{method}.json';write_new(path,(json.dumps(record,indent=2)+'\n').encode());records.append(reference(path))
    for address,slots,proofref,value in acquired:
        if not value:failed.append({'address':address,'slots':slots,'observation':proofref});continue
        if address not in code:
            coderef,runtime=curl_read(endpoint,'eth_getCode',[address,h],directory/'code'/(address+'.json'));require(runtime is not None,'SEED_CODE_UNAVAILABLE')
        else:runtime,coderef=code[address]
        for sp in value['storageProof']:sp['key']='0x'+sp['key'][2:].zfill(64)
        proofs.append({'address':address,'code':runtime,'proof':value})
        derived('eth_getProof',[address,slots,h],value,proofref)
        for method,result in [('eth_getBalance',value['balance']),('eth_getTransactionCount',value['nonce']),('eth_getCode',runtime)]:derived(method,[address,h],result,proofref,coderef if method=='eth_getCode' else None)
        by_key={sp['key']:sp for sp in value['storageProof']}
        for logical in slots:
            key=physical(logical);require(key in by_key,'LOGICAL_PHYSICAL_PROOF_KEY_MISSING')
            derived('eth_getStorageAt',[address,logical,h],'0x'+hex(int(by_key[key]['value'],16))[2:].zfill(64),proofref)
        if partitioned_storage:
            write_new(directory/'storage-key-bindings'/(address+'.json'),(json.dumps({'schema_version':1,'rule':'coreth-partitioned-storage/v1','address':address,'height':height,'proof_source':proofref,'mappings':[{'logical_slot':k,'physical_slot':physical(k)} for k in slots],'authority':'ava-labs/avalanchego 983815fd993acf846a27749b598f0fea4d876a75 vms/saevm/cchain/libevm/extstate/statedb.go storageKey: key[0] &^= 0x01; raw proof key remains physical'},indent=2)+'\n').encode())
    evidence={'schema_version':2,'chain':chain,'chain_id':str(CHAINS[chain][0]),'expected_block_hash':header['hash'],'block':header,'deployments':proofs,'missing_proofs':failed,'root_semantics':anchor['native_state_semantics']}
    if native_headers is not None:evidence['native_headers']=native_headers
    if partitioned_storage:evidence['storage_key_encoding']='coreth-partitioned-storage/v1'
    write_new(directory/'prestate-evidence.json',(json.dumps(evidence,indent=2)+'\n').encode())
    identityref,identity=curl_read(endpoint,'eth_chainId',[],directory/'chain-id.json');require(identity==hex(CHAINS[chain][0]),'SEED_CHAIN_ID')
    gref,genesis=curl_read(endpoint,'eth_getBlockByNumber',['0x0',False],directory/'genesis.json')
    expected_genesis=load_json(TARGETS)['targets'][chain]['genesis_hash']
    require(genesis is None or (genesis['number']=='0x0' and genesis['hash']==expected_genesis),'SEED_GENESIS')
    for i,candidate in enumerate(e for e in CHAINS[chain][1] if e!=endpoint):
        wref,wh=fetch_rpc(candidate,'eth_getBlockByNumber',[h,False],directory/f'witness-{i}.json')
        if not wh or any(wh.get(key)!=header[key] for key in ('number','hash','stateRoot')):
            continue
        cid_ref,cid=fetch_rpc(candidate,'eth_chainId',[],directory/f'witness-{i}-chain-id.json')
        wgref,wg=fetch_rpc(candidate,'eth_getBlockByNumber',['0x0',False],directory/f'witness-{i}-genesis.json')
        if cid==hex(CHAINS[chain][0]) and wg and wg['number']=='0x0' and wg['hash']==expected_genesis:
            witness=candidate
            break
    else:
        require(False,'SEED_WITNESS_UNAVAILABLE')
    saved={'schema_version':2,'anchor':anchor,'header':href,'identity':identityref,
           'genesis':gref if genesis else wgref,'genesis_hash':expected_genesis,'witness':wref,
           'witness_identity':{'chain_id':cid_ref,'genesis':wgref},'witness_hash_agrees':True,
           'endpoints':[endpoint,witness],'seed_previous':str(previous.relative_to(ROOT))}
    write_new(directory/'anchor.json',(json.dumps(saved,indent=2)+'\n').encode());write_new(directory/'execution-plan.json',(json.dumps(plan,indent=2)+'\n').encode());write_new(directory/'sender-candidates.json',(json.dumps({'addresses':[sender],'source':'retained public sender, new fixed-H nonce/balance proven'},indent=2)+'\n').encode());write_new(directory/'rpc-index.json',(json.dumps({'schema_version':2,'anchor':anchor,'records':records},indent=2)+'\n').encode())
    verifier=Path(__file__).with_name('r01_public_catalog_evm_commitments.cjs')
    check=subprocess.run(['node',str(verifier),str(directory),str(directory/'seed-commitments-report.json')],capture_output=True,text=True,timeout=60);write_new(directory/'seed-verify.log',(check.stdout+check.stderr).encode());require(check.returncode==0,'SEED_COMMITMENT_FAILED:'+check.stderr[-1400:])
    # New discoveries are allowed only through the same bounded, read-only archive.
    archive=EvmArchive(anchor,directory,saved['endpoints']);server,url=start_archive(archive)
    try:
        p=subprocess.run(['node',str(ROOT/'integration/tools/acceptance/r01_public_catalog_evm_fork.cjs'),'capture',str(directory),url],capture_output=True,text=True,timeout=480);write_new(directory/'execution.stdout.log',p.stdout.encode());write_new(directory/'execution.stderr.log',p.stderr.encode());require(p.returncode==0,'SEEDED_EXECUTION_FAILED:'+p.stderr[-1500:])
    finally:server.shutdown();server.server_close();archive.save();write_new(directory/'simulation-header-views.json',(json.dumps(archive.views,indent=2)+'\n').encode())
    result=load_json(directory/'fork-execution.json')
    if batch_latest:
        require(all(case['passed'] for case in result['cases']), 'BATCH_ORIGINAL_POOL_EXECUTION_FAILED')
        check = subprocess.run(['node', str(verifier), str(directory), str(directory/'commitments-report.json')],
                               capture_output=True, text=True, timeout=60)
        write_new(directory/'execution-verify.log', (check.stdout + check.stderr).encode())
        require(check.returncode == 0, 'BATCH_EXECUTION_COMMITMENT_FAILED:' + check.stderr[-1400:])
        observe_batch_finality(witness, header, directory)
    print(json.dumps({'chain':chain,'anchor':anchor,'seed_accounts':len(proofs),'seed_missing':len(failed),'cases':[{k:c.get(k) for k in ['protocol','passed','actual_net_recipient','error']} for c in result['cases']]},indent=2))

def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--chain',choices=['bsc','arbitrum','avalanche'],required=True);p.add_argument('--previous',type=Path,required=True);p.add_argument('--directory',type=Path,required=True);p.add_argument('--tag',choices=['latest','finalized'],required=True);p.add_argument('--batch-latest',action='store_true');p.add_argument('--endpoint');p.add_argument('--single-storage-keys',action='store_true');p.add_argument('--coreth-partitioned-storage',action='store_true');a=p.parse_args();capture(a.chain,a.previous,a.directory,a.tag,a.batch_latest,a.endpoint,a.single_storage_keys,a.coreth_partitioned_storage)
if __name__=='__main__':main()
