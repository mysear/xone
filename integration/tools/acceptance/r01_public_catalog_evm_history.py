#!/usr/bin/env python3
"""Additive, read-only multichain original-pool public-state fork captures."""
import argparse
import copy
import json
import os
import re
from pathlib import Path
import subprocess
import sys
import tempfile
sys.path.insert(0,str(Path(__file__).resolve().parent))
from r01_public_catalog import ROOT,CatalogError,require,load_json,read_blob,rpc,sha256,validate_evm,validate_sources
from r01_public_catalog_capture import DOCUMENTS,MAX_BYTES,capture_evm,fetch_document,fetch_rpc,request,write_new,reference
from r01_public_catalog_history import ForkArchive,canonical_key,start_archive
from r01_public_catalog_samples import capture as capture_samples

BASE=ROOT/'integration/test/source/deployment-catalog'
OUT=BASE/'evm-continuation-v2'
TARGETS=ROOT/'integration/config/chain-targets.json'
DEPLOYMENTS=ROOT/'integration/config/r01-evm-history-deployments.json'
HELPER_COMPILER=ROOT/'integration/config/r01-evm-helper-compiler.json'
CHAINS = {
    'base': (8453, ['https://base.drpc.org', 'https://mainnet.base.org']),
    'optimism': (10, ['https://optimism.drpc.org', 'https://mainnet.optimism.io']),
    'bsc': (56, ['https://bsc.drpc.org', 'https://bsc-rpc.publicnode.com', 'https://bsc-dataseed1.ninicoin.io',
                 'https://public.1rpc.io/bnb', 'https://bsc-dataseed.bnbchain.org']),
    'arbitrum': (42161, ['https://arb1.arbitrum.io/rpc', 'https://arbitrum-one-rpc.publicnode.com']),
    'polygon': (137, ['https://polygon.drpc.org', 'https://polygon-bor-rpc.publicnode.com']),
    'avalanche': (43114, ['https://avalanche-c-chain-rpc.publicnode.com', 'https://api.avax.network/ext/bc/C/rpc']),
}
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

def discover_anchor(chain, call):
    chain_id, endpoints = CHAINS[chain]
    target = load_json(TARGETS)['targets'][chain]
    require(target['chain_id'] == chain_id, 'CAPTURE_CONFIG_CHAIN_ID')
    expected_genesis = target['genesis_hash']
    identities = {}
    for endpoint in endpoints:
        chain_ref, identity = call(endpoint, 'eth_chainId', [], 'chain-id')
        if identity != hex(chain_id):
            continue
        genesis_ref, genesis = call(endpoint, 'eth_getBlockByNumber', ['0x0', False], 'genesis')
        if isinstance(genesis, dict) and genesis.get('number') == '0x0' and genesis.get('hash') == expected_genesis:
            identities[endpoint] = {'chain_id': chain_ref, 'genesis': genesis_ref}

    require(len(identities) >= 2, 'PUBLIC_CHAIN_IDENTITY_QUORUM')
    for primary in identities:
        header_ref, header = call(primary, 'eth_getBlockByNumber', ['finalized', True], 'finalized-header')
        if not isinstance(header, dict) or not isinstance(header.get('transactions'), list):
            continue
        tag = header.get('number')
        if not isinstance(tag, str) or not re.fullmatch(r'0x[1-9a-f][0-9a-f]*', tag):
            continue
        if any(not isinstance(header.get(field), str) or not re.fullmatch(r'0x[0-9a-f]{64}', header[field])
               for field in ('hash', 'stateRoot')):
            continue
        if not isinstance(header.get('timestamp'), str) or not re.fullmatch(r'0x[0-9a-f]+', header['timestamp']):
            continue
        semantics = state_root_semantics(chain, header)
        for endpoint in identities:
            if endpoint == primary:
                continue
            witness_ref, witness = call(endpoint, 'eth_getBlockByNumber', [tag, False], 'witness-header')
            if not isinstance(witness, dict) or any(witness.get(field) != header[field]
                                                    for field in ('number', 'hash', 'stateRoot')):
                continue
            anchor = {
                'height': int(tag, 16), 'hash': header['hash'], 'timestamp': int(header['timestamp'], 16),
                'chain_key': target['chain_key'], 'chain_id': chain_id, 'state_root': header['stateRoot'],
                'finality_observation': 'finalized', 'native_state_semantics': semantics,
            }
            ordered = [primary, endpoint]
            record = {
                'schema_version': 2, 'anchor': anchor, 'header': header_ref,
                'genesis': identities[primary]['genesis'], 'witness': witness_ref, 'endpoints': ordered,
                'genesis_hash': expected_genesis, 'provider_identities': {source: identities[source] for source in ordered},
            }
            return record, header
    raise CatalogError('PUBLIC_FINALIZED_HEADER_WITH_WITNESS_UNAVAILABLE')

def evidence_recorder(directory):
    sequence = max((int(path.name.split('-', 1)[0]) for path in directory.iterdir()
                    if re.fullmatch(r'[0-9]+-[a-z0-9-]+\.json', path.name)), default=0)
    observations = []

    def call(endpoint, method, params, name):
        nonlocal sequence
        sequence += 1
        ref, value = fetch_rpc(endpoint, method, params, directory / f'{sequence:03}-{name}.json')
        observations.append(ref)
        return ref, value

    return call, observations

def write_anchor(chain, directory, call, observations):
    record, header = discover_anchor(chain, call)
    record['initial_observations'] = observations.copy()
    write_new(directory / 'anchor.json', (json.dumps(record, indent=2) + '\n').encode())
    return record, header

def prepare_helper_compiler():
    pin = load_json(HELPER_COMPILER)
    blob = pin['blob']
    path = ROOT / blob['path']
    if not path.exists():
        _, binary, _ = request(pin['url'], None, MAX_BYTES)
        require(sha256(binary) == blob['sha256'], 'PUBLIC_HELPER_COMPILER_HASH')
        path.parent.mkdir(parents=True, exist_ok=True)
        with tempfile.NamedTemporaryFile(dir=path.parent) as pending:
            pending.write(binary)
            pending.flush()
            try:
                os.link(pending.name, path)
            except FileExistsError:
                pass
    read_blob(ROOT, blob)

def capture_sae_successors(saved, header, call):
    target = int(header['number'], 16)
    require(saved['anchor']['chain_id'] == 43114 and saved['anchor']['hash'] == header['hash'],
            'SAE_TARGET_ANCHOR')
    parent = header
    records = []
    for height in range(target + 1, target + 33):
        tag = hex(height)
        for endpoint in saved['endpoints']:
            ref, child = call(endpoint, 'eth_getBlockByNumber', [tag, False], 'sae-successor')
            if (isinstance(child, dict) and child.get('number') == tag and
                    child.get('parentHash') == parent['hash'] and isinstance(child.get('settledHeight'), str)):
                break
        else:
            raise CatalogError('SAE_SUCCESSOR_HEADER_REQUIRED:' + tag)
        records.append(ref)
        settled = int(child['settledHeight'], 16)
        if settled == target:
            return records
        require(settled < target, 'SAE_EXACT_TARGET_STATE_ROOT_MISSING')
        parent = child
    raise CatalogError('SAE_SUCCESSOR_WINDOW_LIMIT')

def capture_block_receipts(endpoints, header, call):
    for endpoint in endpoints:
        for selector in (header['hash'], header['number']):
            ref, receipts = call(endpoint, 'eth_getBlockReceipts', [selector], 'block-receipts')
            if not isinstance(receipts, list) or len(receipts) != len(header['transactions']):
                continue
            if any(not isinstance(receipt, dict) or receipt.get('blockHash') != header['hash']
                   for receipt in receipts):
                continue
            return ref
    raise CatalogError('FIXED_H_CANONICAL_RECEIPTS_UNAVAILABLE:' + header['number'])

def capture_sae_receipt_window(directory, saved, call):
    target = saved['anchor']['height']
    window = load_json(directory / 'native-successors.json')
    successors = window['successors']
    require(window['target'] == saved['header'] and 0 < len(successors) <= 32, 'SAE_RECEIPT_SUCCESSORS')
    headers = []
    for offset, ref in enumerate(successors, 1):
        headers.append(rpc(ROOT, ref, 'eth_getBlockByNumber', [hex(target + offset), False]))
    target_header = rpc(ROOT, saved['witness'], 'eth_getBlockByNumber', [hex(target), False])
    require(target_header['hash'] == saved['anchor']['hash'], 'SAE_RECEIPT_TARGET_HASH')
    parent = [target_header, *headers][-2]
    start = int(parent['settledHeight'], 16) + 1
    require(0 <= target - start < 32 and int(headers[-1]['settledHeight'], 16) == target,
            'SAE_RECEIPT_WINDOW_RANGE')
    blocks = []
    parent_hash = target_header['parentHash']
    for height in reversed(range(start, target)):
        tag = hex(height)
        for endpoint in saved['endpoints']:
            ref, block = call(endpoint, 'eth_getBlockByNumber', [tag, True], 'sae-receipt-window-block')
            if (isinstance(block, dict) and block.get('hash') == parent_hash and block.get('number') == tag
                    and isinstance(block.get('transactions'), list)):
                break
        else:
            raise CatalogError('SAE_RECEIPT_WINDOW_BLOCK_REQUIRED:' + tag)
        blocks.append((ref, block))
        parent_hash = block['parentHash']
    blocks.reverse()
    blocks.append((saved['header'], target_header))
    receipts = [{'full_block': ref, 'receipts': capture_block_receipts(saved['endpoints'], block, call)}
                for ref, block in blocks]
    return {'settling_headers': successors, 'window': receipts, 'genesis_hash': saved['genesis_hash']}

def capture_receipts(directory):
    saved = load_json(directory / 'anchor.json')
    anchor = saved['anchor']
    output = directory / 'receipt-input.json'
    require(not output.exists(), 'IMMUTABLE_EVM_RECEIPTS_EXIST')
    call, _ = evidence_recorder(directory)
    result = {'chain_key': anchor['chain_key'], 'header': saved['witness'], 'full_block': saved['header']}
    if anchor['chain_id'] == 43114:
        result['sae'] = capture_sae_receipt_window(directory, saved, call)
    else:
        header = rpc(ROOT, saved['witness'], 'eth_getBlockByNumber', [hex(anchor['height']), False])
        require(header['hash'] == anchor['hash'] and header['number'] == hex(anchor['height']),
                'RECEIPT_HEADER_ANCHOR')
        result['receipts'] = capture_block_receipts(saved['endpoints'], header, call)
    write_new(output, (json.dumps(result, indent=2) + '\n').encode())
    return result

def capture_catalog(chain, directory):
    saved = load_json(directory / 'anchor.json')
    target = load_json(TARGETS)['targets'][chain]
    anchor = saved['anchor']
    require(anchor['chain_key'] == target['chain_key'], 'CATALOG_ANCHOR_CHAIN')
    tag = hex(anchor['height'])
    header = rpc(ROOT, saved['header'], 'eth_getBlockByNumber', ['finalized', True])
    witness = rpc(ROOT, saved['witness'], 'eth_getBlockByNumber', [tag, False])
    require(all(block['number'] == tag and block['hash'] == anchor['hash'] and
                block['stateRoot'] == anchor['state_root'] for block in (header, witness)),
            'CATALOG_ANCHOR_BLOCK')
    catalog_path = directory / 'deployments.json'
    require(not catalog_path.exists(), 'IMMUTABLE_EVM_CATALOG_EXISTS')
    attempt = Path(tempfile.mkdtemp(prefix='deployment-capture-', dir=directory))
    rows = []
    for seed in load_json(DEPLOYMENTS)['deployments']:
        if seed['chain'] != chain:
            continue
        row = {key: seed[key] for key in ('id', 'chain', 'protocol', 'address')}
        source = fetch_document(DOCUMENTS[seed['document']], attempt / (row['id'] + '.txt'))
        row.update(chain_key=target['chain_key'], genesis_hash=target['genesis_hash'],
                   role='factory', profiles=[], capture_height=anchor['height'], sources=[source])
        if row['protocol'] == 'lfj-liquidity-book':
            row['require_lfj_clone_binding'] = True
        validate_sources(ROOT, row)
        rows.append(row)
    require(rows, 'CATALOG_CHAIN_DEPLOYMENTS_MISSING')

    errors = []
    for index, endpoint in enumerate(saved['endpoints'], 1):
        endpoint_dir = attempt / str(index)
        try:
            captured = capture_evm(chain, target | {'rpc_urls': [endpoint]}, copy.deepcopy(rows),
                                   endpoint_dir, fixed_hash=anchor['hash'])
            witness_ref = saved['witness']
            if index != 1:
                witness_ref, observed = fetch_rpc(saved['endpoints'][0], 'eth_getBlockByNumber', [tag, False],
                                                  endpoint_dir / 'fixed-primary-header.json')
                require(observed and observed['number'] == tag and observed['hash'] == anchor['hash'] and
                        observed['stateRoot'] == anchor['state_root'], 'CATALOG_PRIMARY_WITNESS')
            for row in captured:
                require(row['anchor']['height'] == anchor['height'] and row['anchor']['hash'] == anchor['hash'],
                        'CATALOG_CAPTURE_MOVED')
                row['anchor']['witnesses'] = [witness_ref]
                capture_samples(row, endpoint_dir / 'samples')
                row['version'] = 'runtime-sha256:' + row['runtime']['sha256']
                validate_evm(ROOT, row, target)
            chain_plan({'deployments': captured}, chain, catalog_path=None)
        except (CatalogError, OSError) as error:
            errors.append(f'{endpoint}: {error}')
            continue
        catalog = {'schema_version': 1, 'deployments': captured}
        write_new(catalog_path, (json.dumps(catalog, indent=2) + '\n').encode())
        return {row['id']: len(row['sample_pools']) for row in captured}
    raise CatalogError('FIXED_H_DEPLOYMENT_CAPTURE_FAILED: ' + '; '.join(errors))

def chain_plan(catalog,chain,catalog_path=BASE/'catalog.json'):
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
    plan = {'schema_version':1,'chain':chain,'chain_id':CHAINS[chain][0],'wrapped_native':WRAPPED[chain],'cases':cases,'execution_boundary':'local helper newly deployed; original wrapped token and original pools only; no public transactions'}
    if catalog_path is not None:
        plan['source_catalog'] = reference(catalog_path)
    return plan

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

def capture(chain,directory,resume=False,catalog_path=None):
    directory=directory.resolve();require(directory.is_relative_to(OUT.resolve()),'OWNED_EVM_DIRECTORY');directory.mkdir(parents=True,exist_ok=True)
    call, observations = evidence_recorder(directory)
    if not resume:
        catalog_path = catalog_path or BASE / 'catalog.json'
        catalog = load_json(catalog_path)
        plan = chain_plan(catalog, chain, catalog_path)
        if (directory / 'anchor.json').exists():
            require(catalog_path.resolve() == (directory / 'deployments.json').resolve(), 'IMMUTABLE_EVM_ANCHOR_EXISTS')
            saved = load_json(directory / 'anchor.json')
            anchor = saved['anchor']
            require(anchor['chain_key'] == f'evm:{plan["chain_id"]}', 'CATALOG_ANCHOR_CHAIN')
            require(all(row['anchor']['hash'] == anchor['hash'] for row in catalog['deployments']),
                    'CATALOG_EXECUTION_ANCHOR')
            header = rpc(ROOT, saved['header'], 'eth_getBlockByNumber', ['finalized', True])
        else:
            saved, header = write_anchor(chain, directory, call, observations)
        write_new(directory / 'execution-plan.json', (json.dumps(plan, indent=2) + '\n').encode())
    else:
        saved = load_json(directory / 'anchor.json')
        header = json.loads(read_blob(ROOT, saved['header']))['response']['result']
        plan = load_json(directory / 'execution-plan.json')
        require(chain is None or chain == plan['chain'], 'RESUME_CHAIN_MISMATCH')
        chain = plan['chain']
    anchor = saved['anchor']
    ordered = saved['endpoints']
    primary = ordered[0]
    tag = header['number']
    prepare_helper_compiler()
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
    attempt = len(list(directory.glob('execution.stdout*.log'))) + 1
    suffix = '' if attempt == 1 else '-' + str(attempt)
    try:
        try:
            run=subprocess.run(['node',str(ROOT/'integration/tools/acceptance/r01_public_catalog_evm_fork.cjs'),'capture',str(directory),url],capture_output=True,text=True,timeout=480)
        except subprocess.TimeoutExpired as error:
            write_new(directory/('execution.stdout'+suffix+'.log'),error.stdout or b'')
            write_new(directory/('execution.stderr'+suffix+'.log'),error.stderr or b'')
            raise CatalogError('ORIGINAL_EVM_FORK_TIMEOUT') from error

        write_new(directory/('execution.stdout'+suffix+'.log'),run.stdout.encode());write_new(directory/('execution.stderr'+suffix+'.log'),run.stderr.encode());require(run.returncode==0,'ORIGINAL_EVM_FORK_FAILED:'+run.stderr[-2500:])
        execution=load_json(directory/'fork-execution.json');accounts=archive.account_slots()
        # Read and proof-bind each actually used address/slot; EOA and absence included.
        require(len(accounts)<=64,'EVM_PRESTATE_ACCOUNT_BUDGET');deployments=[];missing=[]
        for address,slots in sorted(accounts.items()):
            require(len(slots)<=256,'EVM_PRESTATE_STORAGE_BUDGET')
            try:
                proof_slots = sorted({storage_trie_key(chain, slot) for slot in slots})
                p=copy.deepcopy(archive.request('eth_getProof',[address,proof_slots,tag])['result']);code=archive.request('eth_getCode',[address,tag])['result']
                require(len(p['storageProof'])==len(proof_slots),'EVM_PROOF_SLOT_INVENTORY')
                for item in p['storageProof']:item['key']='0x'+item['key'][2:].zfill(64)
                require({p['key'] for p in p['storageProof']}==set(proof_slots),'EVM_PROOF_SLOT_KEYS');deployments.append({'address':address,'code':code,'proof':p})
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
        server.shutdown()
        server.server_close()
        archive.save()
    write_new(directory / 'simulation-header-views.json', (json.dumps(archive.views, indent=2) + '\n').encode())
    return {'directory':str(directory.relative_to(ROOT)),'cases':[{k:c.get(k) for k in ('catalog_deployment_id','passed','actual_net_recipient','error')} for c in execution['cases']],'accounts':len(accounts),'missing_proofs':len(missing)}

def verify_simulation_views(expected,observed):
    require(set(observed)<=set(expected),'SIMULATION_UNRECORDED_HEADER_VIEW')
    require(all(expected[k]==v for k,v in observed.items()),'SIMULATION_HEADER_VIEW_REPLAY')
    return sorted(set(expected)-set(observed))

def recover_prestate(directory):
    directory = directory.resolve()
    require(directory.is_relative_to(OUT.resolve()), 'OWNED_EVM_DIRECTORY')
    output = directory / 'prestate-evidence-v2.json'
    require(not output.exists(), 'IMMUTABLE_EVM_PRESTATE_EXISTS')
    saved = load_json(directory / 'anchor.json')
    anchor = saved['anchor']
    evidence = load_json(directory / 'prestate-evidence.json')
    require(evidence['missing_proofs'], 'NO_MISSING_PRESTATE_PROOFS')
    tag = hex(anchor['height'])
    require(evidence['block']['number'] == tag and
            evidence['block']['hash'] == evidence['expected_block_hash'] == anchor['hash'],
            'PRESTATE_RECOVERY_ANCHOR')
    archive = EvmArchive(anchor, directory, [], replay=True)
    accounts = archive.account_slots()
    missing = evidence['missing_proofs']
    require(len(missing) <= 64 and len({row['address'] for row in missing}) == len(missing),
            'PRESTATE_RECOVERY_ACCOUNT_INVENTORY')
    call, _ = evidence_recorder(directory)
    recovered = []
    unavailable = []
    for row in missing:
        address = row['address']
        require(address in accounts and accounts[address] == set(row['storage_slots']) and
                len(accounts[address]) <= 256, 'PRESTATE_RECOVERY_STORAGE_INVENTORY')
        slots = sorted({storage_trie_key(evidence['chain'], slot) for slot in accounts[address]})
        for endpoint in saved['endpoints']:
            proof_ref, proof = call(endpoint, 'eth_getProof', [address, slots, tag], 'recovery-proof')
            if not isinstance(proof, dict):
                continue
            entries = proof.get('storageProof')
            if not isinstance(entries, list) or not all(
                    isinstance(entry, dict) and isinstance(entry.get('key'), str) and
                    re.fullmatch(r'0x[0-9a-fA-F]{1,64}', entry['key']) for entry in entries):
                continue
            keys = sorted('0x' + entry['key'][2:].lower().zfill(64) for entry in entries)
            if keys != slots:
                continue
            code_ref = None
            key = canonical_key('eth_getCode', [address, tag])
            cached_code = archive.cache.get(key)
            if cached_code is not None:
                code = cached_code['result']
            else:
                code_ref, code = call(endpoint, 'eth_getCode', [address, tag], 'recovery-code')
            if not isinstance(code, str):
                continue
            recovered.append({'address': address, 'code': code, 'proof': proof,
                              'proof_source': proof_ref, 'code_source': code_ref})
            break
        else:
            unavailable.append(address)
    require(not unavailable, 'PRESTATE_RECOVERY_UNAVAILABLE:' + ','.join(unavailable))
    evidence['deployments'].extend(recovered)
    evidence['missing_proofs'] = []
    verifier = ROOT / 'integration/tools/acceptance/r01_public_catalog_evm_commitments.cjs'
    with tempfile.NamedTemporaryFile(dir=directory) as candidate:
        candidate.write((json.dumps(evidence, indent=2) + '\n').encode())
        candidate.flush()
        check = subprocess.run(['node', str(verifier), str(directory), '', Path(candidate.name).name],
                               capture_output=True, text=True, timeout=120)
        require(check.returncode == 0, 'PRESTATE_RECOVERY_VERIFICATION_FAILED:' + check.stderr[-2000:])
        os.link(candidate.name, output)
    return json.loads(check.stdout)

def replay(directory):
    directory=directory.resolve();anchor=load_json(directory/'anchor.json');archive=EvmArchive(anchor['anchor'],directory,anchor['endpoints'],replay=True);server,url=start_archive(archive)
    try:
        p=subprocess.run(['node',str(ROOT/'integration/tools/acceptance/r01_public_catalog_evm_fork.cjs'),'replay',str(directory),url],capture_output=True,text=True,timeout=480);require(p.returncode==0,'EVM_OFFLINE_REPLAY_FAILED:'+p.stderr[-2500:]);result=json.loads(p.stdout);unused=verify_simulation_views(load_json(directory/'simulation-header-views.json'),archive.views);return result|{'network_requests':0,'unused_original_header_discovery_reads':unused}
    finally:server.shutdown();server.server_close()

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('command', choices=['anchor', 'catalog', 'capture', 'resume', 'replay', 'settlement', 'receipts', 'recover-prestate'])
    parser.add_argument('--chain', choices=CHAINS)
    parser.add_argument('--directory', type=Path, required=True)
    parser.add_argument('--catalog', type=Path)
    args = parser.parse_args()
    if args.command in ('anchor', 'catalog', 'capture') and args.chain is None:
        parser.error('--chain is required for anchor, catalog and capture')
    if args.catalog and args.command != 'capture':
        parser.error('--catalog is only used by capture')
    if args.command == 'settlement' and args.chain not in (None, 'avalanche'):
        parser.error('settlement requires an Avalanche anchor')
    if args.command == 'anchor':
        directory = args.directory.resolve()
        require(directory.is_relative_to(OUT.resolve()), 'OWNED_EVM_DIRECTORY')
        directory.mkdir(parents=True, exist_ok=True)
        require(not (directory / 'anchor.json').exists(), 'IMMUTABLE_EVM_ANCHOR_EXISTS')
        call, observations = evidence_recorder(directory)
        result, _ = write_anchor(args.chain, directory, call, observations)
    elif args.command == 'catalog':
        directory = args.directory.resolve()
        require(directory.is_relative_to(OUT.resolve()), 'OWNED_EVM_DIRECTORY')
        result = capture_catalog(args.chain, directory)
    elif args.command == 'settlement':
        directory = args.directory.resolve()
        require(directory.is_relative_to(OUT.resolve()), 'OWNED_EVM_DIRECTORY')
        output = directory / 'native-successors.json'
        require(not output.exists(), 'IMMUTABLE_SAE_SUCCESSORS_EXISTS')
        saved = load_json(directory / 'anchor.json')
        header = json.loads(read_blob(ROOT, saved['header']))['response']['result']
        call, _ = evidence_recorder(directory)
        result = {'target': saved['header'], 'successors': capture_sae_successors(saved, header, call)}
        write_new(output, (json.dumps(result, indent=2) + '\n').encode())
    elif args.command == 'receipts':
        directory = args.directory.resolve()
        require(directory.is_relative_to(OUT.resolve()), 'OWNED_EVM_DIRECTORY')
        result = capture_receipts(directory)
    elif args.command == 'recover-prestate':
        result = recover_prestate(args.directory)
    elif args.command in ('capture', 'resume'):
        result = capture(args.chain, args.directory, resume=args.command == 'resume', catalog_path=args.catalog)
    else:
        result = replay(args.directory)
    print(json.dumps(result, indent=2))
if __name__=='__main__':main()
