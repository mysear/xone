#!/usr/bin/env python3
"""Finite public deployment catalog validation. Hashes identify bytes, not authority.

Offline reports distinguish observed deployment, authenticated runtime, source rebuild,
and local execution. Qualification always requires an actual offline replay.
"""
import argparse
import base64
import hashlib
import json
import os
from pathlib import Path
import re
import stat
import subprocess
import sys
import tempfile
from urllib.parse import urlsplit, parse_qsl

ROOT = Path(__file__).resolve().parents[3]
DEFAULT = ROOT / 'integration/test/source/deployment-catalog'
IMPLEMENTATION_SLOT = '0x360894a13ba1a3210667c828492db98dca3e2076cc3735a920a3ca505d382bbc'
BEACON_SLOT = '0xa3f0ad74e5423aebfd80d3ef4346578335a9a72aeaee59ff6cb3582b35133d50'
LOADER = 'BPFLoaderUpgradeab1e11111111111111111111111'
ALPHABET = '123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz'
# Inventory identity checks do not decode or admit selected content.
INVENTORY_MAX_FILE_BYTES = 64 * 1024 * 1024
INVENTORY_MAX_TOTAL_BYTES = 512 * 1024 * 1024
INVENTORY_CHUNK_BYTES = 64 * 1024

class CatalogError(ValueError):
    pass

def require(value, code):
    if not value:
        raise CatalogError(code)

def sha256(data):
    return hashlib.sha256(data).hexdigest()

def _pairs(pairs):
    result={}
    for k,v in pairs:
        require(k not in result,'DUPLICATE_JSON:'+k)
        result[k]=v
    return result

def load_json(path):
    require(Path(path).is_file(),'CATALOG_MISSING:'+str(path))
    try:
        return json.loads(Path(path).read_bytes(),object_pairs_hook=_pairs)
    except (json.JSONDecodeError, UnicodeError) as e:
        raise CatalogError('JSON_INVALID') from e

def public_url(value):
    u=urlsplit(value)
    require(u.scheme=='https' and u.hostname and not u.username and not u.password and not u.fragment,'PUBLIC_URL_REQUIRED')
    require(not u.query or (((u.hostname=='sourcify.dev' and len(parse_qsl(u.query))==1 and parse_qsl(u.query)[0][0]=='fields' and re.fullmatch(r'[A-Za-z.,]+',parse_qsl(u.query)[0][1])) or (u.hostname=='api.github.com' and all(k in ('path','per_page','sha','page','until','since') for k,v in parse_qsl(u.query))))),'PUBLIC_URL_QUERY')
    pinned_erc20_source=(u.hostname=='raw.githubusercontent.com' and re.fullmatch(r'/OpenZeppelin/openzeppelin-contracts/[0-9a-f]{40}/contracts/token/[A-Za-z0-9_./-]+\.sol',u.path) is not None)
    require(u.hostname not in ('localhost','127.0.0.1','::1') and (pinned_erc20_source or not re.search(r'/(?:key|apikey|token|secret)(?:/|$)',u.path,re.I)),'PUBLIC_URL_CREDENTIAL')
    return value

PUBLIC_RPC_ENDPOINTS=frozenset((
    'https://eth.drpc.org','https://1rpc.io/eth','https://ethereum-rpc.publicnode.com',
    'https://public.1rpc.io/bnb','https://public.1rpc.io/arb','https://public.1rpc.io/avax/c',
    'https://mainnet.base.org','https://base.drpc.org',
    'https://bsc-dataseed.bnbchain.org','https://bsc-rpc.publicnode.com','https://bsc.drpc.org',
    'https://bsc-dataseed1.ninicoin.io','https://bsc-rpc-public.chainpulse.cc','https://bsc-rpc.blockreq.com/v1/rpc/public',
    'https://api.mainnet-beta.solana.com','https://solana-rpc.publicnode.com',
    'https://arb1.arbitrum.io/rpc','https://arbitrum-one-rpc.publicnode.com',
    'https://mainnet.optimism.io','https://optimism-rpc.publicnode.com','https://optimism.drpc.org',
    'https://polygon-bor-rpc.publicnode.com','https://polygon.drpc.org',
    'https://api.avax.network/ext/bc/C/rpc','https://avalanche-c-chain-rpc.publicnode.com'))

def public_rpc_endpoint(url):
    public_url(url)
    require(url in PUBLIC_RPC_ENDPOINTS,'PUBLIC_RPC_ENDPOINT_NOT_APPROVED')
    return url

def _blob_path(root, ref):
    require(isinstance(ref,dict) and isinstance(ref.get('path'),str),'BLOB_REF')
    relative=Path(ref['path']);base=Path(root).resolve();p=(base/relative).resolve()
    require(not relative.is_absolute() and '..' not in relative.parts and p.is_relative_to(base),'BLOB_PATH')
    require(p.is_file(),'BLOB_MISSING:'+str(relative))
    return p

def read_blob(root, ref):
    p=_blob_path(root,ref)
    require(p.stat().st_size<=32*1024*1024,'BLOB_SIZE_LIMIT')
    data=p.read_bytes()
    require(sha256(data)==ref.get('sha256'),'BLOB_HASH:'+str(Path(ref['path'])))
    return data

def json_blob(root,ref):
    return json.loads(read_blob(root,ref),object_pairs_hook=_pairs)

def minimal_proxy(code):
    m=re.fullmatch(r'0x363d3d373d3d3d363d73([0-9a-f]{40})5af43d82803e903d91602b57fd5bf3',code.lower())
    return '0x'+m[1] if m else None

def hexbytes(value):
    require(isinstance(value,str) and re.fullmatch(r'0x(?:[0-9a-fA-F]{2})*',value),'HEX_BYTES')
    return bytes.fromhex(value[2:])

def b58encode(value):
    n=int.from_bytes(value,'big');out=''
    while n:
        n,r=divmod(n,58);out=ALPHABET[r]+out
    return '1'*(len(value)-len(value.lstrip(b'\0')))+out

def b58decode(value):
    try:
        n=0
        for c in value:n=n*58+ALPHABET.index(c)
        return b'\0'*(len(value)-len(value.lstrip('1')))+(n.to_bytes((n.bit_length()+7)//8,'big') if n else b'')
    except ValueError as e:raise CatalogError('BASE58') from e

def decode_program(data):
    require(len(data)==36 and int.from_bytes(data[:4],'little')==2,'PROGRAM_LOADER_STATE')
    return b58encode(data[4:])

def decode_programdata(data):
    require(len(data)>49 and int.from_bytes(data[:4],'little')==3 and data[12] in (0,1),'PROGRAMDATA_STATE')
    require(data[45:49]==b'\x7fELF','PROGRAMDATA_ELF')
    return {'upgrade_slot':int.from_bytes(data[4:12],'little'),
            'upgrade_authority':b58encode(data[13:45]) if data[12] else None,
            'payload_sha256':sha256(data[45:]),'payload_bytes':len(data)-45}

def scope_key(row):
    return tuple(row.get(k) for k in ('chain','protocol','version'))

def scope_errors(entries,required):
    errors=[];keys=[scope_key(r) for r in entries];expected=[scope_key(r) for r in required]
    if len(keys)!=len(set(keys)) or len([r.get('id') for r in entries])!=len(set(r.get('id') for r in entries)):errors.append('DUPLICATE_DEPLOYMENT')
    if len(expected)!=len(set(expected)):errors.append('DUPLICATE_SCOPE')
    errors.extend('SCOPE_MISSING:'+':'.join(k) for k in sorted(set(expected)-set(keys)))
    for item in required:
        candidates=[r for r in entries if scope_key(r)==scope_key(item)]
        for row in candidates:
            for field in ('address','chain_key','genesis_hash'):
                if field in item and row.get(field)!=item[field]:errors.append('SCOPE_BINDING:'+field+':'+str(row.get('id')))
            if 'anchor' in item and {k:row.get('anchor',{}).get(k) for k in ('height','hash')}!=item['anchor']:errors.append('SCOPE_BINDING:anchor:'+str(row.get('id')))
    errors.extend('SCOPE_UNDECLARED:'+':'.join(k) for k in sorted(set(keys)-set(expected)))
    return errors

def rpc(root,ref,method,params):
    record=json_blob(root,ref)
    public_rpc_endpoint(record['endpoint'])
    require(record['method']==method and record['params']==params,'RPC_REQUEST_BINDING:'+method)
    require(record.get('http_status')==200 and isinstance(record.get('response'),dict),'RPC_RESPONSE')
    response=record['response'];require(response.get('jsonrpc')=='2.0' and response.get('id')==1,'RPC_ENVELOPE')
    require('error' not in response and 'result' in response,'RPC_ERROR:'+method)
    return response['result']

def identity(root,row,target):
    require(row['chain_key']==target['chain_key'] and row['genesis_hash']==target['genesis_hash'],'CHAIN_BINDING')
    i=row['identity']
    if target['family']=='evm':
        require(int(rpc(root,i['chain_id'],'eth_chainId',[]),16)==target['chain_id'],'CHAIN_ID')
        genesis=rpc(root,i['genesis'],'eth_getBlockByNumber',['0x0',False])
        require(genesis['number']=='0x0' and genesis['hash']==target['genesis_hash'],'GENESIS_BINDING')
    else:
        require(rpc(root,i['genesis'],'getGenesisHash',[])==target['genesis_hash'],'GENESIS_BINDING')

def validate_evm(root,row,target):
    identity(root,row,target)
    block=row['anchor'];height=block['height'];require(type(height) is int and height>0,'HEIGHT')
    tag=hex(height)
    header=rpc(root,block['header'],'eth_getBlockByNumber',[tag,False])
    require(int(header['number'],16)==height and header['hash']==block['hash'],'HEIGHT_HASH_BINDING')
    require(re.fullmatch(r'0x[0-9a-f]{64}',block['hash']) is not None,'BLOCK_HASH')
    for witness in block.get('witnesses',[]):
        observed=rpc(root,witness,'eth_getBlockByNumber',[tag,False])
        require(observed['hash']==block['hash'] and observed['number']==header['number'],'WITNESS_HASH')
    runtime=row['runtime'];address=row['address'].lower()
    require(re.fullmatch(r'0x[0-9a-f]{40}',address) is not None,'ADDRESS')
    code=rpc(root,runtime['observation'],'eth_getCode',[address,tag]);data=hexbytes(code)
    require(data and sha256(data)==runtime['sha256'],'RUNTIME_BINDING')
    if row.get('factory_binding'):
        binding=row['factory_binding']
        result=rpc(root,binding['observation'],'eth_call',[{'to':binding['address'].lower(),'data':binding['calldata']},tag])
        require(len(hexbytes(result))==32 and '0x'+result[-40:].lower()==address,'FACTORY_POOL_BINDING')
    upgrade=row['upgrade'];require(upgrade['kind'] in ('eip1967','eip1167','slots-observed-custom-unresolved','direct-source-proven'),'UPGRADE_KIND')
    slots={}
    for slot in (IMPLEMENTATION_SLOT,BEACON_SLOT):
        value=rpc(root,upgrade['slots'][slot],'eth_getStorageAt',[address,slot,tag])
        require(len(hexbytes(value))==32,'UPGRADE_SLOT');slots[slot]=value.lower()
    clone=minimal_proxy(code)
    dependencies={d['role']:d for d in upgrade.get('dependencies',[])}
    require(len(dependencies)==len(upgrade.get('dependencies',[])),'DUPLICATE_UPGRADE_DEPENDENCY')
    if upgrade['kind']=='eip1967':require(any(int(v,16) for v in slots.values()),'EIP1967_ZERO_TARGET')
    if clone:
        require(upgrade['kind']=='eip1167' and 'implementation' in dependencies,'PROXY_IMPLEMENTATION_MISSING')
        require(dependencies['implementation']['address'].lower()==clone,'PROXY_IMPLEMENTATION_TARGET')
    if int(slots[IMPLEMENTATION_SLOT],16):
        require(upgrade['kind']=='eip1967' and 'implementation' in dependencies,'PROXY_IMPLEMENTATION_MISSING')
        require(dependencies['implementation']['address'].lower()=='0x'+slots[IMPLEMENTATION_SLOT][-40:],'PROXY_IMPLEMENTATION_TARGET')
    if int(slots[BEACON_SLOT],16):
        require('beacon' in dependencies and 'implementation' in dependencies,'PROXY_BEACON_MISSING')
        require(dependencies['beacon']['address'].lower()=='0x'+slots[BEACON_SLOT][-40:],'PROXY_BEACON_TARGET')
        beacon=dependencies['beacon']['address'].lower()
        value=rpc(root,upgrade['beacon_call'],'eth_call',[{'to':beacon,'data':'0x5c60da1b'},tag])
        require(len(hexbytes(value))==32 and dependencies['implementation']['address'].lower()=='0x'+value[-40:].lower(),'PROXY_BEACON_IMPLEMENTATION')
    for d in dependencies.values():
        if d.get('getter_observation'):
            getter=rpc(root,d['getter_observation'],'eth_call',[{'to':address,'data':d['getter_selector']},tag])
            require(len(hexbytes(getter))==32 and '0x'+getter[-40:].lower()==d['address'].lower(),'DEPENDENCY_GETTER_TARGET')
        value=rpc(root,d['observation'],'eth_getCode',[d['address'].lower(),tag])
        require(hexbytes(value) and sha256(hexbytes(value))==d['sha256'],'PROXY_DEPENDENCY_RUNTIME')
    if upgrade['kind']=='direct-source-proven':
        require(row.get('proof') and row.get('rebuild'),'DIRECT_SOURCE_PROOF_REQUIRED')
        require(not clone and all(int(s,16)==0 for s in slots.values()),'PROXY_DIRECT_CONTRADICTION')
    require(row['version']=='runtime-sha256:'+runtime['sha256'],'DEPLOYED_VERSION_BINDING')
    validate_samples(root,row,tag)
    if block.get('recheck'):
        recheck=rpc(root,block['recheck'],'eth_getBlockByNumber',[tag,False])
        require(recheck['hash']==block['hash'] and recheck['number']==header['number'],'POST_CAPTURE_HASH_BINDING')
    return {'runtime_observed':True,'runtime_sha256':runtime['sha256'],'anchor':{'height':height,'hash':block['hash']},'upgrade_bound':clone is not None or upgrade['kind'] in ('eip1967','direct-source-proven')}

def validate_sbf_build_attempts(root,row,payload):
    from r01_public_catalog_solana import compare_programdata_build, CatalogError as SBFComparisonError
    results=[]
    for ref in row.get('sbf_build_attempts',[]):
        recorded=json_blob(root,ref);artifact=read_blob(root,recorded['artifact'])
        require(recorded['program']==row['address'] and recorded['full_programdata_payload_sha256']==sha256(payload),'SBF_BUILD_DEPLOYMENT_BINDING')
        require(recorded['artifact_bytes']==len(artifact) and recorded['programdata_payload_bytes']==len(payload),'SBF_BUILD_LENGTH_BINDING')
        first=next((i for i,(a,b) in enumerate(zip(artifact,payload)) if a!=b),None)
        require(first==recorded['first_byte_difference'],'SBF_BUILD_FIRST_DIFFERENCE')
        try:comparison=compare_programdata_build(payload,artifact);match=True;reason=None
        except SBFComparisonError as error:comparison=None;match=False;reason=str(error)
        require(recorded['deployed_rebuild_verified'] is match,'SBF_BUILD_RECORDED_MATCH')
        if not match:require(recorded['comparison_error']==reason,'SBF_BUILD_MISMATCH_REASON')
        results.append({'source_commit':recorded['source_commit'],'compiler_toolchain':recorded['compiler_toolchain'],'artifact_sha256':sha256(artifact),'artifact_bytes':len(artifact),'first_byte_difference':first,'executable_match':match,'comparison':comparison,'reason':reason,'compiler_reexecution_required_for_source_qualification':True})
    return results


def validate_solana(root,row,target):
    identity(root,row,target)
    a=row['anchor'];slot=a['height'];require(type(slot) is int and slot>0,'SLOT')
    response=rpc(root,row['runtime']['observation'],'getMultipleAccounts',[[row['address'],row['upgrade']['programdata']],{'encoding':'base64','commitment':'finalized'}])
    require(response['context']['slot']==slot,'SOLANA_CONTEXT_SLOT')
    accounts=response['value'];require(len(accounts)==2 and all(accounts),'PROGRAM_ACCOUNTS')
    program,data=accounts
    require(program['owner']==LOADER and program['executable'] is True and data['owner']==LOADER and data['executable'] is False,'PROGRAM_OWNER_EXECUTABLE')
    require(all(account['data'][1]=='base64' for account in accounts),'PROGRAM_ENCODING')
    pbytes,dbytes=[base64.b64decode(v['data'][0],validate=True) for v in accounts]
    require(decode_program(pbytes)==row['upgrade']['programdata'],'PROGRAMDATA_POINTER')
    details=decode_programdata(dbytes)
    require(details['upgrade_slot']<=slot,'UPGRADE_AFTER_CONTEXT')
    require(details['upgrade_slot']==row['upgrade']['slot'] and details['upgrade_authority']==row['upgrade']['authority'],'PROGRAM_UPGRADE_BINDING')
    require(details['payload_sha256']==row['runtime']['sha256'] and sha256(pbytes)==row['runtime']['program_sha256'],'PROGRAM_RUNTIME_BINDING')
    block=rpc(root,a['header'],'getBlock',[slot,{'commitment':'finalized','transactionDetails':'none','rewards':False,'maxSupportedTransactionVersion':0}])
    require(block['blockhash']==a['hash'] and block['parentSlot']<slot,'SOLANA_BLOCK_HASH')
    # context.slot is the node bank read; getBlock is a separate observation.
    # minContextSlot is a lower bound, never an exact historical state selector.
    require(row['version']=='sbf-sha256:'+details['payload_sha256']+'@upgrade:'+str(details['upgrade_slot']),'DEPLOYED_VERSION_BINDING')
    attempts=validate_sbf_build_attempts(root,row,dbytes[45:])
    return {'sbf_build_attempts':attempts,'runtime_observed':True,'runtime_sha256':details['payload_sha256'],'anchor':{'slot':slot,'hash':a['hash']},'upgrade_bound':True,'historical_bank_proof':False}

def official_directory_url(row,url):
    public_url(url)
    domains={'uniswap-v2':['https://docs.uniswap.org/','https://developers.uniswap.org/','https://raw.githubusercontent.com/Uniswap/'],
             'uniswap-v3':['https://docs.uniswap.org/','https://developers.uniswap.org/'],
             'pancake-v2':['https://developer.pancakeswap.finance/'],'pancake-v3':['https://developer.pancakeswap.finance/'],
             'aerodrome-v2':['https://raw.githubusercontent.com/aerodrome-finance/'],
             'aerodrome-slipstream':['https://raw.githubusercontent.com/aerodrome-finance/'],
             'velodrome-v2':['https://raw.githubusercontent.com/velodrome-finance/'],
             'raydium-cpmm':['https://docs.raydium.io/'],'raydium-clmm':['https://docs.raydium.io/'],
             'orca-whirlpools':['https://raw.githubusercontent.com/orca-so/'],
             'lfj-liquidity-book':['https://raw.githubusercontent.com/lfj-gg/']}
    require(any(url.startswith(prefix) for prefix in domains.get(row['protocol'],[])),'OFFICIAL_DIRECTORY_ORIGIN')

def validate_sources(root,row):
    sources=row.get('sources',[])
    for source in sources:
        public_url(source['url']);read_blob(root,source['blob'])
        require(source.get('authority') in ('official-deployment-directory','official-source','reference-source-manifest'),'SOURCE_AUTHORITY')
    official=[s for s in sources if s['authority']=='official-deployment-directory']
    require(official,'OFFICIAL_DIRECTORY_MISSING')
    for source in official:official_directory_url(row,source['url'])
    # Exact address must occur in the retained public document, not only in config.
    directory_address=row.get('factory_binding',{}).get('address',row['address'])
    require(any(directory_address.lower().encode() in read_blob(root,s['blob']).lower() for s in official),'OFFICIAL_ADDRESS_BINDING')
    for source in row.get('reference_sources',[]):read_blob(root,source)
    for attempt in row.get('failed_public_proof_requests',[]):
        record=json_blob(root,attempt);public_rpc_endpoint(record['endpoint']);require(record['method']=='eth_getProof','FAILED_PROOF_METHOD')

def validate_samples(root,row,tag):
    if row.get('sample_provider_header'):
        header=rpc(root,row['sample_provider_header'],'eth_getBlockByNumber',[tag,False])
        require(header['hash']==row['anchor']['hash'],'SAMPLE_PROVIDER_ANCHOR')
    selectors={'uniswap-v3':'0x1698ee82','pancake-v3':'0x1698ee82','pancake-v2':'0xe6a43905',
               'aerodrome-v2':'0x79bc57d5','velodrome-v2':'0x79bc57d5','aerodrome-slipstream':'0x28af8d0b','lfj-liquidity-book':'0x704037bd'}
    samples=row.get('sample_pools',[])
    require(len(set(s['address'] for s in samples))==len(samples),'DUPLICATE_SAMPLE_POOL')
    for sample in samples:
        parameter=sample['query_parameter']
        selector=selectors[row['protocol']]
        expected=selector+''.join(t[2:].lower().zfill(64) for t in sample['query_tokens'])+(hex(parameter)[2:].zfill(64) if parameter is not None else '')
        require(sample['calldata']==expected,'SAMPLE_CALLDATA_BINDING')
        value=rpc(root,sample['factory_call'],'eth_call',[{'to':row['address'].lower(),'data':expected},tag])
        offset=1 if row['protocol']=='lfj-liquidity-book' else 0
        require(sample['result_address_word']==offset,'SAMPLE_ADDRESS_WORD')
        word=value[2+64*offset:2+64*(offset+1)]
        require(len(word)==64 and '0x'+word[-40:].lower()==sample['address'].lower(),'FACTORY_SAMPLE_ADDRESS')
        address=sample['address'].lower()
        code=rpc(root,sample['runtime']['observation'],'eth_getCode',[address,tag])
        require(hexbytes(code) and sha256(hexbytes(code))==sample['runtime']['sha256'],'SAMPLE_RUNTIME_BINDING')
        clone=minimal_proxy(code)
        if clone:
            require(sample.get('implementation'),'SAMPLE_IMPLEMENTATION_MISSING')
            impl=sample['implementation'];require(impl['address'].lower()==clone,'SAMPLE_IMPLEMENTATION_TARGET')
            value=rpc(root,impl['observation'],'eth_getCode',[clone,tag])
            require(hexbytes(value) and sha256(hexbytes(value))==impl['sha256'],'SAMPLE_IMPLEMENTATION_RUNTIME')
        getters=sample['token_getters']
        if row['protocol']!='lfj-liquidity-book':
            require([g['selector'] for g in getters]==['0x0dfe1681','0xd21220a7'],'POOL_TOKEN_GETTER_INVENTORY')
            for g in getters:
                value=rpc(root,g['observation'],'eth_call',[{'to':address,'data':g['selector']},tag])
                require(len(hexbytes(value))==32 and '0x'+value[-40:].lower()==g['address'],'POOL_TOKEN_GETTER_BINDING')
            require(set(g['address'] for g in getters)==set(t.lower() for t in sample['query_tokens']),'POOL_TOKEN_PAIR_BINDING')


def preflight_rebuild(root,row):
    build=row['rebuild'];inputs=build['inputs']
    evidence=json_blob(root,inputs['evidence']);artifact=json_blob(root,inputs['artifact']);metadata=json.loads(artifact['metadata'],object_pairs_hook=_pairs)
    require(evidence['chain_id']==row['chain_key'].split(':')[1] and evidence['expected_block_hash']==row['anchor']['hash'] and int(evidence['block']['number'],16)==row['anchor']['height'],'PROOF_ANCHOR_BINDING')
    deployments=evidence['deployments'];require(len(deployments)==1 and deployments[0]['address'].lower()==row['address'].lower(),'PROOF_ADDRESS_BINDING')
    require(sha256(hexbytes(deployments[0]['code']))==row['runtime']['sha256'],'PROOF_RUNTIME_BINDING')
    require(set(inputs['source_files'])==set(metadata['sources']),'COMPILER_SOURCE_INVENTORY')
    source_files={name:sha256(read_blob(root,ref)) for name,ref in inputs['source_files'].items()}
    compiler=read_blob(root,inputs['compiler']);pin=json_blob(root,inputs['compiler_pin'])
    require(sha256(compiler)==pin['soljson_sha256'],'COMPILER_PIN_BINDING')
    public_url(inputs['compiler_public_url']);public_url(inputs['compiler_npm_url'])
    require(inputs['compiler_public_url']=='https://binaries.soliditylang.org/bin/soljson-v'+metadata['compiler']['version']+'.js','COMPILER_PUBLIC_BUILD_BINDING')
    report=json_blob(root,build['report'])
    require(report['chain_id']==evidence['chain_id'] and report['block_hash']==row['anchor']['hash'] and report['address'].lower()==row['address'].lower(),'REBUILD_DEPLOYMENT_BINDING')
    require(report['source_files']==source_files and report['artifact_sha256']==inputs['artifact']['sha256'] and report['evidence_sha256']==inputs['evidence']['sha256'],'REBUILD_SOURCE_BINDING')
    require(report['compiler_sha256']==sha256(compiler) and report['compiler']==metadata['compiler']['version']+'.Emscripten.clang','REBUILD_COMPILER_BINDING')
    require(report['rebuilt_runtime_sha256']==row['runtime']['sha256'] and report['source_rebuild_verified'] is True,'REBUILD_RUNTIME_BINDING')
    proof=json_blob(root,row['proof']['report'])
    require(proof['passed'] is True and proof['chain_id']==evidence['chain_id'] and proof['block_hash']==row['anchor']['hash'] and proof['block_number']==row['anchor']['height'],'PROOF_REPORT_BINDING')
    require(len(proof['deployments'])==1 and proof['deployments'][0]['address'].lower()==row['address'].lower() and proof['deployments'][0]['runtime_sha256']==row['runtime']['sha256'],'PROOF_REPORT_RUNTIME')
    return report


def preflight_public_source(root,row):
    binding=row['public_source_compilation']
    manifest=json_blob(root,binding['input_manifest'])
    matches=[entry for entry in manifest['entries'] if entry['id']==row['id']]
    require(len(matches)==1 and binding['entry_id']==row['id'],'PUBLIC_SOURCE_ENTRY')
    entry=matches[0]
    require(entry['runtime_sha256']==row['runtime']['sha256'] and entry['runtime_observation']==row['runtime']['observation'],'PUBLIC_SOURCE_RUNTIME_BINDING')
    public_url(entry['source']['url']);source=json_blob(root,entry['source']['blob'])
    require(sha256(hexbytes(source['runtimeBytecode']['onchainBytecode']))==row['runtime']['sha256'],'SOURCE_REPOSITORY_RUNTIME_BINDING')
    compiler=entry['compiler'];public_url(compiler['url'])
    require(compiler['url']=='https://binaries.soliditylang.org/bin/soljson-v'+compiler['version']+'.js','PUBLIC_COMPILER_ORIGIN_VERSION')
    listing=json_blob(root,compiler['official_list'])
    build=[b for b in listing['builds'] if b.get('longVersion')==compiler['version']]
    require(len(build)==1 and build[0]['sha256'].removeprefix('0x')==compiler['blob']['sha256'],'OFFICIAL_COMPILER_LIST_PIN')
    require(source['compilation']['compilerVersion']==compiler['version'] and source['stdJsonInput']['settings']==source['compilation']['compilerSettings'],'PUBLIC_SOURCE_COMPILER_SETTINGS')
    require(all(isinstance(v.get('content'),str) for v in source['stdJsonInput']['sources'].values()),'PUBLIC_SOURCE_CONTENT_REQUIRED')
    report=json_blob(root,binding['report']);ids=[r['id'] for r in report['entries']]
    require(len(ids)==len(set(ids)) and set(ids)==set(e['id'] for e in manifest['entries']),'PUBLIC_SOURCE_REPORT_INVENTORY')
    result=next(r for r in report['entries'] if r['id']==row['id'])
    require(result['runtime_sha256']==row['runtime']['sha256'] and result['source_rebuild_verified']==binding['reported_rebuilt'],'PUBLIC_SOURCE_REPORT_BINDING')
    if result['source_rebuild_verified']:
        require(result['compiled_runtime_sha256']==row['runtime']['sha256'] and result['compiler_sha256']==compiler['blob']['sha256'] and result['compiler']==compiler['version']+'.Emscripten.clang','PUBLIC_REBUILD_COMPILER_RUNTIME')
        sources={name:sha256(data['content'].encode()) for name,data in source['stdJsonInput']['sources'].items()}
        require(result['metadata']['compiler']['version']==compiler['version'],'PUBLIC_REBUILD_METADATA_COMPILER')
        require(result['source_files']==sources and result['settings']==source['stdJsonInput']['settings'],'PUBLIC_REBUILD_FULL_SOURCE_SETTINGS')
    return entry,result


def preflight_exact_source(root,row):
    binding=row['exact_source_compilation'];manifest=json_blob(root,binding['input_manifest'])
    ids=[e['id'] for e in manifest['entries']]
    require(len(ids)==len(set(ids)),'EXACT_SOURCE_DUPLICATE_ENTRY')
    matches=[e for e in manifest['entries'] if e['id']==row['id']]
    require(len(matches)==1 and binding['entry_id']==row['id'],'EXACT_SOURCE_ENTRY')
    entry=matches[0]
    require(entry['runtime_sha256']==row['runtime']['sha256'] and entry['runtime_observation']==row['runtime']['observation'],'EXACT_SOURCE_RUNTIME_BINDING')
    rpc(root,entry['runtime_observation'],'eth_getCode',[row['address'].lower(),hex(row['anchor']['height'])])
    compiler=entry['compiler'];public_url(compiler['url'])
    require(compiler['url']=='https://binaries.soliditylang.org/bin/soljson-v'+compiler['version']+'.js','EXACT_COMPILER_ORIGIN')
    listing=json_blob(root,compiler['official_list']);builds=[b for b in listing['builds'] if b.get('longVersion')==compiler['version']]
    require(len(builds)==1 and builds[0]['sha256'].removeprefix('0x')==compiler['blob']['sha256'],'EXACT_COMPILER_OFFICIAL_PIN')
    read_blob(root,compiler['blob']);inputs=json_blob(root,entry['input']);artifact=json_blob(root,entry['artifact']);metadata=json.loads(artifact['metadata'],object_pairs_hook=_pairs)
    require(metadata['compiler']['version']==compiler['version'],'EXACT_METADATA_COMPILER')
    require(set(metadata['sources'])==set(inputs['sources']),'EXACT_SOURCE_FILE_INVENTORY')
    hashes={name:sha256(v['content'].encode()) for name,v in inputs['sources'].items()}
    report=json_blob(root,binding['report']);results=[r for r in report['entries'] if r['id']==row['id']]
    require(len(results)==1 and len(report['entries'])==len(set(r['id'] for r in report['entries'])) and set(r['id'] for r in report['entries'])==set(ids),'EXACT_SOURCE_REPORT_INVENTORY')
    result=results[0]
    require(result['source_rebuild_verified'] is True and result['metadata_exact'] is True,'EXACT_SOURCE_REBUILD_REQUIRED')
    require(result['runtime_sha256']==row['runtime']['sha256'] and result['compiler_sha256']==compiler['blob']['sha256'],'EXACT_SOURCE_REPORT_RUNTIME_COMPILER')
    require(result['source_files']==hashes and result['settings']==inputs['settings'] and result['metadata']==metadata,'EXACT_SOURCE_REPORT_INPUTS')
    return entry,result


def replay_exact_sources(root,entries):
    if not entries:return {}
    with tempfile.TemporaryDirectory(prefix='r01-public-exact-replay-',dir=Path(root)/'integration/artifacts/local') as directory:
        manifest=Path(directory)/'input.json';output=Path(directory)/'output.json';manifest.write_text(json.dumps({'entries':entries}))
        p=subprocess.run(['node',str(ROOT/'integration/tools/acceptance/r01_public_catalog_exact_sources.cjs'),str(manifest),str(output)],capture_output=True,text=True,timeout=180)
        require(p.returncode==0,'EXACT_SOURCE_REPLAY_FAILED:'+p.stderr[-1500:]+p.stdout[-1000:])
        return {r['id']:r for r in load_json(output)['entries']}


def replay_public_sources(root,entries):
    if not entries:return {}
    with tempfile.TemporaryDirectory(prefix='r01-public-source-replay-',dir=Path(root)/'integration/artifacts/local') as directory:
        manifest=Path(directory)/'input.json';output=Path(directory)/'output.json'
        manifest.write_text(json.dumps({'entries':entries}))
        p=subprocess.run(['node',str(ROOT/'integration/tools/acceptance/r01_public_catalog_sources.cjs'),str(root),str(manifest),str(output)],capture_output=True,text=True,timeout=180)
        require(p.returncode==0,'PUBLIC_SOURCE_REPLAY_FAILED:'+p.stderr[-1000:])
        return {r['id']:r for r in load_json(output)['entries']}


def sample_gaps(row):
    return ['SWAP_POOL_RUNTIME_SCOPE_MISSING'] if row.get('role')=='factory' and not row.get('sample_pools') else []

def validate_catalog(root,catalog,scope,targets,replay=False):
    require(catalog.get('deployments') and scope.get('required'),'EMPTY_SCOPE_CATALOG')
    errors=scope_errors(catalog.get('deployments',[]),scope.get('required',[]));reports=[]
    public_sources={};exact_sources={}
    for row in catalog['deployments']:
        if row.get('public_source_compilation'):
            try:public_sources[row['id']]=preflight_public_source(root,row)
            except (CatalogError,KeyError,TypeError,ValueError,OSError) as e:errors.append(str(row['id'])+':'+str(e))
    for row in catalog['deployments']:
        if row.get('exact_source_compilation'):
            try:exact_sources[row['id']]=preflight_exact_source(root,row)
            except (CatalogError,KeyError,TypeError,ValueError,OSError) as e:errors.append(str(row['id'])+':'+str(e))
    exact_replayed=replay_exact_sources(root,[e for e,r in exact_sources.values()]) if replay else {}
    public_replayed=replay_public_sources(root,[e for e,r in public_sources.values()]) if replay else {}
    require(catalog.get('schema_version')==1 and scope.get('schema_version')==1,'SCHEMA')
    for row in catalog.get('deployments',[]):
        gaps=sample_gaps(row);report={'deployment_commitment_verified':False,'source_rebuild_verified':False,'original_runtime_receipt_execution_verified':False,'original_chain_historical_execution':False,'id':row.get('id'),'chain':row.get('chain'),'protocol':row.get('protocol'),'version':row.get('version'),'status':'NOT_QUALIFIED'}
        try:
            target=targets['targets'][row['chain']]
            validate_sources(root,row)
            if row.get('rebuild'):preflight_rebuild(root,row)
            observation=(validate_evm if target['family']=='evm' else validate_solana)(root,row,target)
            report.update(observation)
            if row['id'] in public_sources:
                entry,recorded=public_sources[row['id']]
                report['public_source_rebuild_recorded']=recorded['source_rebuild_verified']
                actual=public_replayed.get(row['id'])
                report['public_source_rebuild_replayed']=bool(actual and actual['source_rebuild_verified'])
                report['source_rebuild_verified']=report['public_source_rebuild_replayed']
                if actual and recorded['source_rebuild_verified']:
                    for key in ('source_rebuild_verified','compiled_runtime_sha256','compiler_sha256','source_files','settings','immutable_bindings','metadata','wrapper_sha256'):
                        require(actual.get(key)==recorded.get(key),'PUBLIC_REBUILD_REPLAY_DIFFERENCE:'+key)
            if row['id'] in exact_sources:
                entry,recorded=exact_sources[row['id']];actual=exact_replayed.get(row['id'])
                report['exact_source_rebuild_recorded']=True
                report['exact_source_rebuild_replayed']=bool(actual and actual['source_rebuild_verified'])
                if actual:
                    require(actual==recorded,'EXACT_SOURCE_REPLAY_DIFFERENCE')
                    report['source_rebuild_verified']=True
                    report['metadata_exact']=True
                    report['implementation_targets']=actual['implementation_targets']
            if not row.get('profiles'):gaps.append('LOCAL_DOMAIN_PROFILE_MISSING')
            if not observation['upgrade_bound']:gaps.append('CUSTOM_UPGRADE_UNRESOLVED')
            if target['family']=='solana':gaps.append('HISTORICAL_BANK_PROOF_MISSING')
            if not row.get('proof'):gaps.append('STATE_COMMITMENT_PROOF_MISSING')
            if not row.get('rebuild') and not report.get('public_source_rebuild_replayed') and not report.get('exact_source_rebuild_replayed'):gaps.append('DEPLOYED_SOURCE_COMPILER_REBUILD_MISSING')
            if not row.get('execution'):gaps.append('ORIGINAL_RUNTIME_RECIPIENT_EXECUTION_MISSING')
            if row.get('proof') and row.get('rebuild') and row.get('execution'):
                # The replay tool rechecks proof, compiler bytes, all source files and
                # runtime; a pre-written {passed:true} report has no authority.
                for key in ('proof','rebuild','execution'):read_blob(root,row[key]['report'])
                if replay:
                    replay_binding(root,row)
                    report.update(deployment_commitment_verified=True,source_rebuild_verified=True,original_runtime_receipt_execution_verified=True)
                    if not gaps:report['status']='QUALIFIED'
                else:gaps.append('OFFLINE_REPLAY_REQUIRED')
            if row.get('qualification')=='QUALIFIED':require(report['status']=='QUALIFIED','UNSUPPORTED_QUALIFICATION_CLAIM')
        except (CatalogError,KeyError,ValueError,TypeError,OSError,subprocess.SubprocessError) as e:
            errors.append(str(row.get('id'))+':'+str(e));gaps.append('EVIDENCE_INVALID:'+str(e))
        report['missing']=gaps;report['consensus_verified']=False
        report['execution_scope']=row.get('execution',{}).get('scope')
        reports.append(report)
    return {'schema_version':1,'structurally_valid':not errors,'qualified':not errors and bool(reports) and all(r['status']=='QUALIFIED' for r in reports),'errors':errors,'deployments':reports,'scope_count':len(scope.get('required',[])),'consensus_verified':False}

def replay_binding(root,row):
    require(row['chain_key']=='evm:1','REPLAY_UNSUPPORTED_CHAIN')
    inputs=row['rebuild']['inputs'];proof=json_blob(root,inputs['evidence'])
    require(proof['chain_id']=='1' and proof['block']['number']==hex(row['anchor']['height']) and proof['expected_block_hash']==row['anchor']['hash'],'PROOF_ANCHOR_BINDING')
    require(len(proof['deployments'])==1 and proof['deployments'][0]['address'].lower()==row['address'].lower(),'PROOF_ADDRESS_BINDING')
    require(sha256(hexbytes(proof['deployments'][0]['code']))==row['runtime']['sha256'],'PROOF_RUNTIME_BINDING')
    # Only owned helper runs; paths and digests are explicit inputs.
    p=subprocess.run(['node',str(ROOT/'integration/tools/acceptance/r01_public_catalog_replay.cjs'),'verify',str(Path(root).resolve()),json.dumps(row,separators=(',',':'))],text=True,capture_output=True,timeout=120)
    require(p.returncode==0,'REPLAY_FAILED:'+p.stderr[-2000:])
    result=json.loads(p.stdout)
    require(result.get('passed') is True and result['runtime_sha256']==row['runtime']['sha256'],'REPLAY_RESULT')

def _verify_inventory_blob(root,ref,remaining_bytes):
    """Hash one opaque retained file without admitting its content to read_blob."""
    p=_blob_path(root,ref);base=Path(root).resolve()
    require(type(remaining_bytes) is int and 0<=remaining_bytes<=INVENTORY_MAX_TOTAL_BYTES,
            'INVENTORY_TOTAL_SIZE_LIMIT')
    def identity(info):
        return (info.st_dev,info.st_ino,info.st_mode,info.st_size,
                info.st_mtime_ns,info.st_ctime_ns)
    try:
        before=p.stat()
        require(before.st_size<=INVENTORY_MAX_FILE_BYTES,'INVENTORY_FILE_SIZE_LIMIT')
        require(before.st_size<=remaining_bytes,'INVENTORY_TOTAL_SIZE_LIMIT')
        digest=hashlib.sha256();size=0
        with p.open('rb') as stream:
            opened=os.fstat(stream.fileno())
            require(stat.S_ISREG(opened.st_mode) and identity(opened)==identity(before)
                    and p.resolve()==p and p.is_relative_to(base) and _blob_path(base,ref)==p,
                    'INVENTORY_FILE_CHANGED:'+ref['path'])
            while True:
                # Read one extra byte at the expected EOF to detect growth;
                # every read remains bounded even when the source changes.
                block=stream.read(min(INVENTORY_CHUNK_BYTES,before.st_size-size+1))
                if not block:break
                size+=len(block)
                require(size<=INVENTORY_MAX_FILE_BYTES,'INVENTORY_FILE_SIZE_LIMIT')
                require(size<=remaining_bytes,'INVENTORY_TOTAL_SIZE_LIMIT')
                require(size<=before.st_size,'INVENTORY_FILE_CHANGED:'+ref['path'])
                digest.update(block)
            require(size==before.st_size and identity(os.fstat(stream.fileno()))==identity(before)
                    and p.resolve()==p and p.is_relative_to(base)
                    and identity(p.stat())==identity(before) and _blob_path(base,ref)==p,
                    'INVENTORY_FILE_CHANGED:'+ref['path'])
    except OSError as error:
        raise CatalogError('INVENTORY_FILE_CHANGED:'+ref['path']) from error
    require(digest.hexdigest()==ref.get('sha256'),'BLOB_HASH:'+ref['path'])
    return size

def check_inventory(root,manifest):
    require(manifest.get('schema_version')==1,'INVENTORY_SCHEMA')
    directory=(Path(root)/manifest['data_directory']).resolve()
    require(directory.is_relative_to(Path(root).resolve()) and directory.is_dir(),'INVENTORY_DIRECTORY')
    expected=[ref['path'] for ref in manifest['files']]
    require(len(expected)==len(set(expected)),'INVENTORY_DUPLICATE')
    actual={str(p.relative_to(Path(root).resolve())) for p in directory.rglob('*') if p.is_file() and p.resolve()!=(directory/'manifest.json').resolve()}
    require(set(expected)==actual,'INVENTORY_MEMBERSHIP')
    tool_refs=manifest.get('tools',[])
    tool_files={str(p.resolve().relative_to(Path(root).resolve())) for pattern in ('integration/tools/acceptance/r01_public_catalog*.py','integration/tools/acceptance/r01_public_catalog*.cjs','integration/tools/acceptance/tests/test_r01_public_catalog*.py') for p in Path(root).glob(pattern)}
    require(len(tool_refs)==len({r['path'] for r in tool_refs}) and {r['path'] for r in tool_refs}==tool_files,'INVENTORY_TOOL_MEMBERSHIP')
    remaining=INVENTORY_MAX_TOTAL_BYTES
    for ref in manifest['files']:
        remaining-=_verify_inventory_blob(root,ref,remaining)
    # Tools and selected content retain the original semantic read budget.
    for ref in tool_refs:
        require(_blob_path(root,ref).stat().st_size<=remaining,'INVENTORY_TOTAL_SIZE_LIMIT')
        data=read_blob(root,ref)
        require(len(data)<=remaining,'INVENTORY_TOTAL_SIZE_LIMIT')
        remaining-=len(data)
    return True

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root',type=Path,default=ROOT)
    parser.add_argument('--catalog',type=Path,default=DEFAULT/'catalog.json')
    parser.add_argument('--scope',type=Path,default=DEFAULT/'scope.json')
    parser.add_argument('--inventory',type=Path,default=DEFAULT/'manifest.json')
    parser.add_argument('--replay',action='store_true')
    parser.add_argument('--require-qualified',action='store_true')
    parser.add_argument('--out',type=Path)
    args=parser.parse_args()
    try:
        scope=load_json(args.scope);catalog=load_json(args.catalog)
        inventory=load_json(args.inventory);check_inventory(args.root,inventory)
        approved={ref['path'] for ref in inventory['files']}
        require(str(args.catalog.resolve().relative_to(args.root.resolve())) in approved and str(args.scope.resolve().relative_to(args.root.resolve())) in approved,'INVENTORY_SCOPE_CATALOG')
        targets=load_json(args.root/'integration/config/chain-targets.json')
        require(sha256((args.root/'integration/config/chain-targets.json').read_bytes())==scope['chain_targets_sha256'],'CHAIN_TARGETS_HASH')
        report=validate_catalog(args.root,catalog,scope,targets,args.replay)
        report['inventory_verified']=True
    except (CatalogError,OSError,KeyError,TypeError,ValueError,subprocess.SubprocessError) as e:report={'structurally_valid':False,'qualified':False,'errors':[str(e)]}
    text=json.dumps(report,indent=2)+'\n'
    if args.out:args.out.write_text(text)
    print(text,end='')
    return 0 if report['structurally_valid'] and (not args.require_qualified or report['qualified']) else 1

if __name__=='__main__':sys.exit(main())
