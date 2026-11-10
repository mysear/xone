#!/usr/bin/env python3
"""Add immutable fresh anchors and proof-bound, public-state fork references.

The loopback fork archive only forwards explicit public read methods. Every lazy
prestate read is retained, then account/storage MPT proofs are collected at H.
Offline replay has no network fallback and cannot invent an unrecorded read.
"""
import argparse
from collections import defaultdict
import datetime
import hashlib
from http.server import BaseHTTPRequestHandler,ThreadingHTTPServer
import json
from pathlib import Path
import re
import subprocess
import sys
import threading
import tempfile
import time
sys.path.insert(0,str(Path(__file__).resolve().parent))
from r01_public_catalog import ROOT,CatalogError,require,sha256,load_json,read_blob,scope_key,public_rpc_endpoint
from r01_public_catalog_capture import fetch_rpc,write_new,reference

BASE=ROOT/'integration/test/source/deployment-catalog'
STATE_METHODS={'eth_getBalance':1,'eth_getTransactionCount':1,'eth_getCode':1,'eth_getStorageAt':2,'eth_getProof':2,'eth_call':1}
READ_METHODS=frozenset(STATE_METHODS)|{'eth_chainId','net_version','web3_clientVersion','eth_getBlockByNumber','eth_getBlockByHash','eth_getTransactionByHash','eth_getTransactionReceipt','eth_blockNumber','eth_getBlockReceipts'}

def anchor_key(row):
    a=row['anchor'];return (row['chain'],row['protocol'],row['version'],row.get('address'),a['height'],a['hash'])

def history_scope_errors(rows,required):
    keys=[anchor_key(r) for r in rows];expected=[anchor_key(r) for r in required];errors=[]
    if len(keys)!=len(set(keys)):errors.append('DUPLICATE_HISTORY_ANCHOR')
    if len(expected)!=len(set(expected)):errors.append('DUPLICATE_HISTORY_SCOPE')
    errors.extend('HISTORY_ANCHOR_MISSING:'+str(k) for k in set(expected)-set(keys))
    errors.extend('HISTORY_ANCHOR_UNDECLARED:'+str(k) for k in set(keys)-set(expected))
    return errors

def canonical_key(method,params):return method+'\n'+json.dumps(params,sort_keys=True,separators=(',',':'))

def pin_request(method,params,height,block_hash):
    require(method in READ_METHODS,'READ_ONLY_FORK_METHOD:'+method)
    params=json.loads(json.dumps(params));tag=hex(height)
    if method in STATE_METHODS:
        index=STATE_METHODS[method]
        require(len(params)>index,'FORK_STATE_SELECTOR_REQUIRED');value=params[index]
        if isinstance(value,dict):require(value.get('blockHash')==block_hash and value.get('requireCanonical') is True,'FORK_STATE_HASH')
        else:require(value in ('latest','pending',tag),'FORK_STATE_HEIGHT')
        params[index]=tag
    if method=='eth_getBlockReceipts':
        require(len(params)==1 and params[0] in (tag,block_hash),'FORK_RECEIPT_ANCHOR_REQUIRED')
    if method=='eth_getBlockByNumber':
        if params[0] in ('latest','safe','finalized'):params[0]=tag
        if params[0]=='earliest':params[0]='0x0'
        require(isinstance(params[0],str) and int(params[0],16)<=height,'FORK_FUTURE_HEADER')
    return params

# Filename sequence and actual archive transport attempts have different units.
FORK_PUBLIC_ATTEMPTS=450
FORK_ARCHIVE_RECORDS=8192
FORK_ARCHIVE_BYTES=128*1024*1024
FORK_SOURCE_FILES=256
FORK_SOURCE_BYTES=64*1024*1024
DERIVED_PRESTATE='derived-prestate-from-public-MPT-proof'
BATCH_PROJECTION='projection-of-retained-public-batch'
CORETH_STORAGE_ENCODING='coreth-partitioned-storage/v1'
# This immutable inventory independently pins the prior typed evidence/bindings.
# New encodings or unsealed bindings must acquire their own reviewed authority.
CORETH_BINDING_INVENTORY={
    'path':'integration/test/source/deployment-catalog/evm-normal-v4/sealed-inputs.json',
    'sha256':'691a7419a364334fa2f909193d8f7cf0e45db2c3badebfe20cf397e10d2b6d80'}
PROOF_UNIONS={'derived-union-of-same-H-public-proof-chunks',
              'derived-union-of-same-H-single-key-public-proofs'}
TRANSPORT_FIELDS={'endpoint','observed_at','http_status','error','transport',
                  'exit_code','response_sha256','attempted_public_read'}

def _word(value):
    require(isinstance(value,str) and re.fullmatch(r'0x[0-9a-fA-F]{1,64}',value),
            'FORK_DERIVED_UINT256')
    return '0x'+format(int(value,16),'064x')

def _proof_shape(proof):
    require(isinstance(proof,dict),'FORK_DERIVED_PROOF_SHAPE')
    require(isinstance(proof.get('address'),str) and
            re.fullmatch(r'0x[0-9a-fA-F]{40}',proof['address']), 'FORK_DERIVED_ADDRESS')
    require(all(k in proof for k in ('balance','nonce','codeHash','storageHash','accountProof')),
            'FORK_DERIVED_PROOF_SHAPE')
    account={k:proof[k] for k in ('balance','nonce','codeHash','storageHash','accountProof')}
    account.update(address=proof['address'].lower(),balance=_word(proof['balance']),
                   nonce=_word(proof['nonce']))
    for key in ('codeHash','storageHash'):
        require(isinstance(account[key],str) and re.fullmatch(r'0x[0-9a-fA-F]{64}',account[key]),
                'FORK_DERIVED_ACCOUNT_HASH')
    require(isinstance(account['accountProof'],list) and account['accountProof'],
            'FORK_DERIVED_ACCOUNT_PROOF')
    slots=proof.get('storageProof');require(isinstance(slots,list) and len(slots)<=4096,
                                           'FORK_DERIVED_STORAGE_BUDGET')
    storage={}
    for slot in slots:
        key=_word(slot['key']);require(key not in storage,'FORK_DERIVED_DUPLICATE_SLOT')
        require(isinstance(slot.get('proof'),list),'FORK_DERIVED_STORAGE_PROOF')
        storage[key]={'key':key,'value':_word(slot['value']),'proof':slot['proof']}
    return account,storage

class ForkArchive:
    def __init__(self,anchor,directory,endpoints,replay=False):
        self.anchor=anchor;self.directory=Path(directory);self.endpoints=endpoints;self.replay=replay
        self.cache={};self.records=[];self.lock=threading.RLock();self.count=0;self.public_attempts=0
        self._record_bytes=0;self._source_bytes=0;self._sources={};self._proofs={};self._batches={}
        self._storage_encoding=None;self._storage_refs={};self._storage_maps={}
        rpc=(self.directory/'rpc').resolve();indexed={}
        index=self.directory/'rpc-index.json'
        if replay or index.exists():
            require(index.stat().st_size<=4*1024*1024,'FORK_ARCHIVE_INDEX_BUDGET')
            manifest=load_json(index)
            require(type(manifest.get('schema_version')) is int and manifest['schema_version'] in (1,2),
                    'FORK_ARCHIVE_SCHEMA')
            require(manifest.get('anchor')==anchor,'FORK_ARCHIVE_ANCHOR')
            require(isinstance(manifest.get('records'),list) and len(manifest['records'])<=FORK_ARCHIVE_RECORDS,
                    'FORK_ARCHIVE_RECORD_BUDGET')
            for ref in manifest['records']:
                require(isinstance(ref,dict) and isinstance(ref.get('path'),str),'BLOB_REF')
                path=(ROOT/ref['path']).resolve()
                require(path.parent==rpc and path not in indexed,'FORK_ARCHIVE_RECORD_PATH')
                indexed[path]=ref
        # Include interrupted attempts not yet saved to the index. Their raw
        # envelopes remain immutable and are retained by the next save.
        files={};sequences=set()
        for path in (self.directory/'rpc').glob('*.json'):
            require(len(files)<FORK_ARCHIVE_RECORDS,'FORK_ARCHIVE_RECORD_BUDGET')
            match=re.fullmatch(r'([0-9]+)-[A-Za-z0-9_]+\.json',path.name)
            require(match is not None,'FORK_ARCHIVE_RECORD_NAME')
            seq=int(match[1]);require(seq>0 and seq not in sequences,'FORK_ARCHIVE_SEQUENCE')
            sequences.add(seq);self.count=max(self.count,seq)
            resolved=path.resolve();require(resolved.parent==rpc,'FORK_ARCHIVE_RECORD_PATH')
            files[resolved]=path
        require(set(indexed)<=set(files),'FORK_ARCHIVE_RECORD_MISSING')
        self._configure_storage_encoding()
        for path,ref in indexed.items():self._retain(ref)
        for path in sorted(set(files)-set(indexed),key=lambda p:int(files[p].name.split('-')[0])):
            require(files[path].stat().st_size<=32*1024*1024,'BLOB_SIZE_LIMIT')
            self._retain(reference(files[path]))

    def _source(self,ref):
        require(isinstance(ref,dict) and isinstance(ref.get('path'),str) and
                isinstance(ref.get('sha256'),str),'FORK_DERIVED_SOURCE_REF')
        key=(ref['path'],ref['sha256'])
        if key not in self._sources:
            require(len(self._sources)<FORK_SOURCE_FILES,'FORK_DERIVED_SOURCE_BUDGET')
            data=read_blob(ROOT,ref)
            require(self._source_bytes+len(data)<=FORK_SOURCE_BYTES,'FORK_DERIVED_SOURCE_BUDGET')
            value=json.loads(data);require(isinstance(value,dict),'FORK_DERIVED_SOURCE_SHAPE')
            self._source_bytes+=len(data);self._sources[key]=value
        return self._sources[key]

    def _configure_storage_encoding(self):
        path=self.directory/'prestate-evidence.json'
        if not path.exists():return
        # This first read selects a format only; the typed format must then
        # match an independently pinned digest before it can affect accounting.
        require(path.stat().st_size<=32*1024*1024,'BLOB_SIZE_LIMIT')
        candidate=load_json(path);encoding=candidate.get('storage_key_encoding')
        if encoding in (None,'ethereum-storage/v1'):return
        require(encoding==CORETH_STORAGE_ENCODING,'FORK_STORAGE_KEY_ENCODING')
        require(self.anchor.get('chain_key')=='evm:43114' and
                type(self.anchor.get('chain_id')) is int and self.anchor['chain_id']==43114 and
                candidate.get('chain')=='avalanche' and candidate.get('chain_id')=='43114',
                'FORK_STORAGE_KEY_CHAIN')
        inventory=self._source(CORETH_BINDING_INVENTORY)
        require(inventory.get('schema_version')==1 and isinstance(inventory.get('files'),list)
                and len(inventory['files'])<=FORK_ARCHIVE_RECORDS,'FORK_STORAGE_BINDING_INVENTORY')
        prefix=str(self.directory.resolve().relative_to(ROOT.resolve()))+'/'
        refs=[r for r in inventory['files'] if r['path']==prefix+'prestate-evidence.json' or
              r['path'].startswith(prefix+'storage-key-bindings/')]
        require(len(refs)<=65 and len(refs)==len({r['path'] for r in refs}),
                'FORK_STORAGE_BINDING_INVENTORY')
        self._storage_refs={r['path']:r for r in refs}
        require(prefix+'prestate-evidence.json' in self._storage_refs,'FORK_STORAGE_EVIDENCE_UNPINNED')
        evidence=self._source(self._storage_refs[prefix+'prestate-evidence.json'])
        require(evidence.get('storage_key_encoding')==CORETH_STORAGE_ENCODING and
                evidence['block']['number']==hex(self.anchor['height']) and
                evidence['block']['hash']==evidence['expected_block_hash']==self.anchor['hash'],
                'FORK_STORAGE_EVIDENCE_ANCHOR')
        self._storage_encoding=encoding

    def _storage_mapping(self,record,account,storage):
        if self._storage_encoding is None:return None
        address=account['address']
        path=str((self.directory/'storage-key-bindings'/(address+'.json')).resolve().relative_to(ROOT.resolve()))
        require(path in self._storage_refs,'FORK_STORAGE_BINDING_UNPINNED')
        binding=self._source(self._storage_refs[path])
        require(binding.get('schema_version')==1 and binding.get('rule')==CORETH_STORAGE_ENCODING and
                binding.get('address')==address and binding.get('height')==self.anchor['height'] and
                binding.get('proof_source')==record['proof_source'],'FORK_STORAGE_BINDING_SCOPE')
        if address not in self._storage_maps:
            rows=binding.get('mappings')
            require(isinstance(rows,list) and len(rows)<=4096,'FORK_STORAGE_MAPPING_BUDGET')
            mapping={}
            for row in rows:
                require(isinstance(row,dict) and set(row)=={'logical_slot','physical_slot'},
                        'FORK_STORAGE_MAPPING_SHAPE')
                logical,physical=_word(row['logical_slot']),_word(row['physical_slot'])
                # Only the low bit of the first byte is cleared. This selects
                # a retained physical key; it never rewrites a proof or value.
                expected='0x'+format(int(logical,16)&~(1<<248),'064x')
                require(logical not in mapping and physical==expected,'FORK_STORAGE_MAPPING_TRANSFORM')
                mapping[logical]=physical
            require(set(mapping.values())==set(storage),'FORK_STORAGE_MAPPING_KEYS')
            self._storage_maps[address]=mapping
        require(set(self._storage_maps[address].values())==set(storage),'FORK_STORAGE_MAPPING_KEYS')
        return self._storage_maps[address]

    def _batch_projection(self,wire):
        """Bind a latest projection to its exact batch and frozen header.

        Joint retention identifies the candidate anchor. It does not establish
        batch atomicity or replace native header/MPT verification.
        """
        ref=wire.get('batch_source');batch=self._source(ref)
        key=(ref['path'],ref['sha256'])
        if key not in self._batches:
            require(batch.get('source_kind')=='actual-public-JSON-RPC-batch','FORK_BATCH_SOURCE_KIND')
            public_rpc_endpoint(batch.get('endpoint',''))
            requests=batch.get('requests');responses=batch.get('response')
            require(isinstance(requests,list) and 1<len(requests)<=256 and
                    type(batch.get('max_methods')) is int and batch['max_methods']==len(requests),
                    'FORK_BATCH_REQUEST_BUDGET')
            require(isinstance(responses,list) and len(responses)==len(requests),'FORK_BATCH_RESPONSE_IDS')
            by_request={};by_response={}
            for request in requests:
                require(isinstance(request,dict) and set(request)=={'jsonrpc','id','method','params'} and
                        request['jsonrpc']=='2.0' and type(request['id']) is int and request['id']>0 and
                        request['id'] not in by_request,'FORK_BATCH_REQUEST_ID')
                require(request['method'] in ('eth_getBlockByNumber','eth_getProof') and
                        isinstance(request['params'],list),'FORK_BATCH_REQUEST_SHAPE')
                if request['method']=='eth_getProof':
                    require(len(request['params'])==3 and request['params'][-1]=='latest',
                            'FORK_BATCH_REQUEST_SHAPE')
                by_request[request['id']]=request
            for response in responses:
                require(isinstance(response,dict) and response.get('jsonrpc')=='2.0' and
                        type(response.get('id')) is int and response['id'] not in by_response,
                        'FORK_BATCH_RESPONSE_ID')
                require(('result' in response)!=('error' in response),'FORK_BATCH_RESPONSE_SHAPE')
                by_response[response['id']]=response
            require(set(by_request)==set(by_response),'FORK_BATCH_RESPONSE_IDS')
            require(by_request.get(1)=={'jsonrpc':'2.0','id':1,'method':'eth_getBlockByNumber',
                    'params':['latest',True]} and
                    all(r['method']=='eth_getProof' for k,r in by_request.items() if k!=1),
                    'FORK_BATCH_HEADER_REQUEST')
            path=self.directory/'anchor.json'
            require(path.stat().st_size<=32*1024*1024,'BLOB_SIZE_LIMIT')
            saved=load_json(path)
            require(saved.get('anchor')==self.anchor,'FORK_BATCH_FROZEN_ANCHOR')
            frozen=self._source(saved.get('header'))
            require(frozen.get('source_kind')==BATCH_PROJECTION and frozen.get('batch_source')==ref and
                    frozen.get('method')=='eth_getBlockByNumber' and frozen.get('params')==['latest',True]
                    and frozen.get('response')==by_response[1],'FORK_BATCH_FROZEN_PROJECTION')
            header=by_response[1].get('result')
            require(isinstance(header,dict) and header.get('number')==hex(self.anchor['height']) and
                    header.get('hash')==self.anchor['hash'] and
                    header.get('stateRoot')==self.anchor.get('state_root') and
                    header.get('timestamp')==hex(self.anchor['timestamp']),'FORK_BATCH_FROZEN_ANCHOR')
            self._batches[key]=(by_request,by_response)
        requests,responses=self._batches[key]
        response=wire.get('response');identifier=response.get('id') if isinstance(response,dict) else None
        require(type(identifier) is int and identifier in requests and
                requests[identifier]['method']==wire.get('method') and
                requests[identifier]['params']==wire.get('params') and
                response==responses[identifier],'FORK_BATCH_PROJECTION')
        require(wire.get('endpoint')==batch['endpoint'],'FORK_BATCH_ENDPOINT')

    def _proof(self,ref,depth=0):
        require(depth<=4,'FORK_DERIVED_SOURCE_DEPTH')
        wire=self._source(ref);key=(ref['path'],ref['sha256'])
        if key in self._proofs:return self._proofs[key]
        require(wire.get('method')=='eth_getProof' and isinstance(wire.get('params'),list)
                and len(wire['params'])==3,'FORK_DERIVED_PROOF_SOURCE')
        address,keys,tag=wire['params']
        require(isinstance(keys,list) and len(keys)<=4096,'FORK_DERIVED_PROOF_ANCHOR')
        kind=wire.get('source_kind')
        if kind==BATCH_PROJECTION:
            self._batch_projection(wire)
            require(tag=='latest','FORK_DERIVED_PROOF_ANCHOR')
        else:require(tag==hex(self.anchor['height']),'FORK_DERIVED_PROOF_ANCHOR')
        response=wire.get('response')
        require(isinstance(response,dict) and 'result' in response and 'error' not in response,
                'FORK_DERIVED_PROOF_RESPONSE')
        account,storage=_proof_shape(response['result'])
        require(isinstance(address,str) and address.lower()==account['address'] and
                len(keys)==len({_word(k) for k in keys}) and {_word(k) for k in keys}==set(storage),
                'FORK_DERIVED_PROOF_KEYS')
        if kind in PROOF_UNIONS:
            refs=wire.get('sources');require(isinstance(refs,list) and 0<len(refs)<=256,
                                             'FORK_DERIVED_UNION_SOURCES')
            combined={}
            for source in refs:
                child,slots=self._proof(source,depth+1)
                require(child==account,'FORK_DERIVED_UNION_ACCOUNT')
                for k,value in slots.items():
                    require(k not in combined or combined[k]==value,'FORK_DERIVED_UNION_SLOT')
                    combined[k]=value
            require(combined==storage,'FORK_DERIVED_UNION_KEYS')
        elif kind==BATCH_PROJECTION:pass
        else:
            require(kind is None,'FORK_DERIVED_PROOF_SOURCE_KIND')
            public_rpc_endpoint(wire.get('endpoint',''))
        self._proofs[key]=(account,storage)
        return account,storage

    def _derived(self,record):
        # An actual transport envelope is charged even if somebody adds a
        # derived tag, proof ref, or success flag to it.
        if TRANSPORT_FIELDS & set(record):return False
        if record.get('source_kind')!=DERIVED_PRESTATE:
            return (set(record)=={'method','params','authority','response'} and
                    record.get('authority')=='derived-from-frozen-public-header' and
                    record.get('method')=='eth_blockNumber' and record.get('params')==[] and
                    record.get('response')=={'jsonrpc':'2.0','id':1,'result':hex(self.anchor['height'])})
        require(set(record)<={'method','params','response','source_kind','proof_source',
                              'code_bytes_source','authority'},'FORK_DERIVED_RECORD_SHAPE')
        method=record['method'];params=record['params'];response=record.get('response')
        require(method in ('eth_getProof','eth_getBalance','eth_getTransactionCount',
                           'eth_getCode','eth_getStorageAt') and isinstance(params,list),
                'FORK_DERIVED_METHOD')
        require(isinstance(response,dict) and set(response)=={'jsonrpc','id','result'} and
                response['jsonrpc']=='2.0' and response['id']==1,'FORK_DERIVED_RESPONSE')
        require(len(params)==(3 if method in ('eth_getProof','eth_getStorageAt') else 2) and
                params[-1]==hex(self.anchor['height']),'FORK_DERIVED_ANCHOR')
        account,storage=self._proof(record.get('proof_source'))
        require(isinstance(params[0],str) and params[0].lower()==account['address'],
                'FORK_DERIVED_ADDRESS')
        mapping=self._storage_mapping(record,account,storage)
        value=response['result']
        if method=='eth_getProof':
            require(isinstance(params[1],list),'FORK_DERIVED_PROOF_VALUE')
            requested={_word(k) for k in params[1]}
            expected=set(storage if mapping is None else mapping)
            # batch_latest reuses a retained account proof for the predicted
            # helper even when its earlier batch request included more slots.
            # The full returned proof must still equal that exact projection;
            # no requested slot may be absent from the retained source.
            batch_subset=(mapping is None and
                          self._source(record['proof_source']).get('source_kind')==BATCH_PROJECTION)
            require(_proof_shape(value)==(account,storage) and len(params[1])==len(requested) and
                    (requested<=expected if batch_subset else requested==expected),
                    'FORK_DERIVED_PROOF_VALUE')
        elif method in ('eth_getBalance','eth_getTransactionCount'):
            require(_word(value)==account['balance' if method=='eth_getBalance' else 'nonce'],
                    'FORK_DERIVED_ACCOUNT_VALUE')
        elif method=='eth_getStorageAt':
            key=_word(params[1])
            if mapping is not None:
                require(key in mapping,'FORK_STORAGE_MAPPING_UNKNOWN')
                key=mapping[key]
            require(key in storage and _word(value)==storage[key]['value'],'FORK_DERIVED_STORAGE_VALUE')
        else:
            require(isinstance(value,str) and re.fullmatch(r'0x(?:[0-9a-fA-F]{2})*',value),
                    'FORK_DERIVED_CODE_BYTES')
            source=record.get('code_bytes_source')
            if source is None:
                require(value=='0x' and int(account['codeHash'],16) in (0,int(
                    'c5d2460186f7233c927e7db2dcc703c0e500b653ca82273b7bfad8045d85a470',16)),
                    'FORK_DERIVED_CODE_SOURCE')
            else:
                code=self._source(source)
                require(code.get('method')=='eth_getCode' and isinstance(code.get('params'),list)
                        and len(code['params'])==2 and isinstance(code['params'][0],str)
                        and code['params'][0].lower()==account['address']
                        and isinstance(code.get('response'),dict) and 'error' not in code['response']
                        and code['response'].get('result')==value,'FORK_DERIVED_CODE_SOURCE')
        return True

    def _retain(self,ref,already_charged=False):
        require(len(self.records)<FORK_ARCHIVE_RECORDS,'FORK_ARCHIVE_RECORD_BUDGET')
        data=read_blob(ROOT,ref)
        require(self._record_bytes+len(data)<=FORK_ARCHIVE_BYTES,'FORK_ARCHIVE_BYTE_BUDGET')
        record=json.loads(data)
        require(isinstance(record,dict) and isinstance(record.get('method'),str) and
                isinstance(record.get('params'),list),'FORK_ARCHIVE_RECORD_SHAPE')
        derived=False if already_charged else self._derived(record)
        self._record_bytes+=len(data);self.records.append(ref)
        if not derived and not already_charged:self.public_attempts+=1
        response=record.get('response')
        if isinstance(response,dict) and 'result' in response and 'error' not in response and response['result'] is not None:
            self.cache[canonical_key(record['method'],record['params'])]=response
        return record

    def _next_path(self,method):
        require(len(self.records)<FORK_ARCHIVE_RECORDS and self._record_bytes<FORK_ARCHIVE_BYTES,
                'FORK_ARCHIVE_RECORD_BUDGET')
        self.count+=1
        return self.directory/'rpc'/f'{self.count:04}-{method}.json'

    def request(self,method,params):
        params=pin_request(method,params,self.anchor['height'],self.anchor['hash']);key=canonical_key(method,params)
        with self.lock:
            if key in self.cache:return self.cache[key]
            require(not self.replay,'OFFLINE_FORK_READ_NOT_RECORDED:'+method+':'+str(params))
            if method=='eth_blockNumber':
                record={'method':method,'params':params,'authority':'derived-from-frozen-public-header','response':{'jsonrpc':'2.0','id':1,'result':hex(self.anchor['height'])}}
                path=self._next_path(method);write_new(path,(json.dumps(record,indent=2)+'\n').encode())
                self._retain(reference(path));return record['response']
            for endpoint in self.endpoints:
                require(self.public_attempts<FORK_PUBLIC_ATTEMPTS,'FORK_PUBLIC_REQUEST_BUDGET')
                path=self._next_path(method)
                # Charge before transport, exactly once, regardless of its result.
                # If transport raises, persist an error row before propagating.
                self.public_attempts+=1
                try:
                    ref,result=fetch_rpc(endpoint,method,params,path,budget=16*1024*1024)
                except Exception as error:
                    if not path.exists():write_new(path,(json.dumps({'endpoint':endpoint,'method':method,
                        'params':params,'error':type(error).__name__+':'+str(error)},indent=2)+'\n').encode())
                    self._retain(reference(path),already_charged=True)
                    raise
                record=self._retain(ref,already_charged=True)
                response=record.get('response')
                if isinstance(response,dict) and 'result' in response and 'error' not in response and response['result'] is not None:
                    self.cache[key]=response;return response
            raise CatalogError('FORK_PUBLIC_READ_FAILED:'+method)
    def save(self):
        if not self.replay:
            path=self.directory/'rpc-index.json'
            if path.exists():write_new(self.directory/f'rpc-index-before-{self.count}.json',path.read_bytes());path.unlink()
            write_new(path,(json.dumps({'schema_version':1,'anchor':self.anchor,'records':self.records},indent=2)+'\n').encode())
    def account_slots(self):
        accounts=defaultdict(set)
        for key,response in self.cache.items():
            method,params=key.split('\n',1);params=json.loads(params)
            if method in ('eth_getBalance','eth_getTransactionCount','eth_getCode','eth_getStorageAt'):
                address=params[0].lower();accounts[address]
                if method=='eth_getStorageAt':accounts[address].add('0x'+params[1][2:].lower().zfill(64))
        return accounts

def start_archive(archive):
    class Handler(BaseHTTPRequestHandler):
        def log_message(self,*args):pass
        def do_POST(self):
            try:
                length=int(self.headers.get('Content-Length','0'));require(0<length<=1024*1024,'FORK_LOOPBACK_BODY_LIMIT')
                incoming=json.loads(self.rfile.read(length));require(isinstance(incoming,dict),'FORK_BATCH_UNSUPPORTED')
                result=dict(archive.request(incoming['method'],incoming.get('params',[])));result['id']=incoming.get('id');body=json.dumps(result).encode();status=200
            except Exception as e:
                body=json.dumps({'jsonrpc':'2.0','id':locals().get('incoming',{}).get('id'),'error':{'code':-32099,'message':str(e)}}).encode();status=200
            self.send_response(status);self.send_header('Content-Type','application/json');self.send_header('Content-Length',str(len(body)));self.end_headers();self.wfile.write(body)
    server=ThreadingHTTPServer(('127.0.0.1',0),Handler);threading.Thread(target=server.serve_forever,daemon=True).start()
    return server,'http://127.0.0.1:'+str(server.server_port)

def capture(endpoints,directory,resume=False):
    directory=Path(directory);directory.mkdir(parents=True,exist_ok=resume)
    def initial(method,params,name,endpoint=None):
        path=directory/(name+'.json')
        if resume and path.exists():
            record=load_json(path);require(record['method']==method and record['params']==params,'RESUMED_ANCHOR_REQUEST');ref,value=reference(path),record.get('response',{}).get('result')
        else:ref,value=fetch_rpc(endpoint or endpoints[0],method,params,path)
        require(value is not None,'FRESH_ANCHOR_RPC:'+method);return ref,value
    chainref,chain=initial('eth_chainId',[],'chain-id');require(chain=='0x1','FRESH_ETHEREUM_ONLY')
    headerref,header=initial('eth_getBlockByNumber',['finalized',True],'finalized-header')
    anchor={'height':int(header['number'],16),'hash':header['hash'],'state_root':header['stateRoot'],'timestamp':int(header['timestamp'],16),'chain_key':'evm:1','authority':'public-finalized-observation-then-Ethereum-commitment-verification'}
    tag=header['number'];wref,witness=initial('eth_getBlockByNumber',[tag,False],'witness-header',endpoints[1]);require(witness['hash']==header['hash'],'FRESH_WITNESS')
    if not (directory/'anchor.json').exists():write_new(directory/'anchor.json',(json.dumps({'schema_version':1,'anchor':anchor,'header':headerref,'identity':chainref,'witness':wref,'endpoints':endpoints,'observed_at':datetime.datetime.now(datetime.timezone.utc).isoformat()},indent=2)+'\n').encode())
    archive=ForkArchive(anchor,directory,endpoints)
    # Public preflight is part of the same finite archive and later proof boundary.
    for address in ['0x28c6c06298d514db089934071355e5743bf21d60','0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48','0xb4e16d0168e52d35cacd2c6185b44281ec28c9dc','0x88e6a0c2ddd26feeb64f039a2c41296fcb3f5640','0x1f98431c8ad98523631ae4a59f267346ea31f984']:
        archive.request('eth_getCode',[address,tag])
    server,url=start_archive(archive)
    try:
        run=subprocess.run(['node',str(ROOT/'integration/tools/acceptance/r01_public_catalog_fork.cjs'),'capture',str(directory.resolve()),url],capture_output=True,text=True,timeout=240)
        attempt=len(list(directory.glob('fork.stderr*.log')))+1
        write_new(directory/f'fork.stdout-{attempt}.log',run.stdout.encode());write_new(directory/f'fork.stderr-{attempt}.log',run.stderr.encode())
        require(run.returncode==0,'ORIGINAL_STATE_FORK_FAILED:'+run.stderr[-2500:])
        raw_transactions=load_json(directory/'raw-transactions.json')
        accounts=archive.account_slots();require(0<len(accounts)<=32,'FORK_PROOF_ACCOUNT_BUDGET');deployments=[]
        for index,(address,slots) in enumerate(sorted(accounts.items())):
            require(len(slots)<=256,'FORK_STORAGE_PROOF_BUDGET')
            response=archive.request('eth_getProof',[address,sorted(slots),tag])['result']
            require(len(response['storageProof'])==len(slots),'FORK_STORAGE_PROOF_INCOMPLETE')
            for proof,key in zip(response['storageProof'],sorted(slots)):proof['key']=key
            code=archive.request('eth_getCode',[address,tag])['result']
            deployments.append({'address':address,'code':code,'proof':response})
        evidence={'schema_version':1,'chain_id':'1','expected_block_hash':anchor['hash'],'block':header,'raw_transactions':raw_transactions,'deployments':deployments}
        proofpath=directory/'prestate-evidence.json';write_new(proofpath,(json.dumps(evidence,indent=2)+'\n').encode())
        contractpath=directory/'ethereum-contract-evidence.json';write_new(contractpath,(json.dumps(evidence|{'deployments':[d for d in deployments if d['code']!='0x']},indent=2)+'\n').encode())
        verified=subprocess.run([str(ROOT/'chain-parser-go/bin/source-evidence'),'--verify',str(contractpath)],capture_output=True,text=True,timeout=60)
        write_new(directory/'proof-verification.stderr.log',verified.stderr.encode());require(verified.returncode==0,'FORK_PRESTATE_PROOF_FAILED:'+verified.stderr[-2000:])
        write_new(directory/'prestate-proof-report.json',verified.stdout.encode())
        _,closing=initial('eth_getBlockByNumber',[tag,False],'closing-header');require(closing['hash']==anchor['hash'],'FRESH_ANCHOR_REORG')
        write_new(directory/'proof-boundary.json',(json.dumps({'schema_version':1,'anchor':anchor,'accounts':{a:sorted(s) for a,s in accounts.items()},'all_lazy_public_prestate_reads_proof_bound':True,'original_chain_mined_transaction':False},indent=2)+'\n').encode())
    finally:
        server.shutdown();server.server_close();archive.save()
    check=subprocess.run(['node',str(ROOT/'integration/tools/acceptance/r01_public_catalog_proof.cjs'),str(directory/'prestate-evidence.json'),str(directory/'rpc-index.json'),str(directory/'all-prestate-proof-report.json')],capture_output=True,text=True,timeout=60)
    write_new(directory/'all-prestate-proof.stderr.log',check.stderr.encode());require(check.returncode==0,'ALL_PRESTATE_PROOF_FAILED:'+check.stderr[-2000:])
    return directory

def replay(directory,out):
    directory=Path(directory);data=load_json(directory/'anchor.json');archive=ForkArchive(data['anchor'],directory,data['endpoints'],replay=True);server,url=start_archive(archive)
    try:
        process=subprocess.run(['node',str(ROOT/'integration/tools/acceptance/r01_public_catalog_fork.cjs'),'replay',str(directory.resolve()),url],capture_output=True,text=True,timeout=180)
        require(process.returncode==0,'OFFLINE_FORK_REPLAY_FAILED:'+process.stderr[-2500:])
        verified=subprocess.run([str(ROOT/'chain-parser-go/bin/source-evidence'),'--verify',str(directory/'ethereum-contract-evidence.json')],capture_output=True,text=True,timeout=60)
        require(verified.returncode==0,'OFFLINE_PRESTATE_PROOF_FAILED:'+verified.stderr[-2000:])
        expected=load_json(directory/'prestate-proof-report.json');require(json.loads(verified.stdout)==expected,'OFFLINE_PROOF_REPORT_DIFFERENCE')
        with tempfile.TemporaryDirectory(prefix='r01-public-history-proof-',dir=ROOT/'integration/artifacts/local') as temporary:
            proofout=Path(temporary)/'proof.json'
            proofcheck=subprocess.run(['node',str(ROOT/'integration/tools/acceptance/r01_public_catalog_proof.cjs'),str(directory/'prestate-evidence.json'),str(directory/'rpc-index.json'),str(proofout)],capture_output=True,text=True,timeout=60)
            require(proofcheck.returncode==0,'OFFLINE_ALL_PRESTATE_PROOF_FAILED:'+proofcheck.stderr[-1500:])
            require(load_json(proofout)==load_json(directory/'all-prestate-proof-report.json'),'OFFLINE_ALL_PROOF_REPORT_DIFFERENCE')
        binding=subprocess.run(['node',str(ROOT/'integration/tools/acceptance/r01_public_catalog_history_bind.cjs'),str(directory.resolve())],capture_output=True,text=True,timeout=120)
        require(binding.returncode==0,'OFFLINE_HISTORY_SOURCE_BINDING_FAILED:'+binding.stderr[-2000:])
        result=json.loads(process.stdout);result.update(prestate_commitments_verified=True,network_requests=0,source_compiler_binding=json.loads(binding.stdout))
        result['environment_differences']=environment_differences(directory,result['environment_opcode_counts'])
        if (directory/'environment-differences.json').exists():
            require(result['environment_differences']==load_json(directory/'environment-differences.json'),'OFFLINE_ENVIRONMENT_REPORT_DIFFERENCE')
        if out:Path(out).write_text(json.dumps(result,indent=2)+'\n')
        print(json.dumps(result,indent=2))
        return result
    finally:server.shutdown();server.server_close()

def environment_differences(directory,opcode_counts):
    evidence=load_json(Path(directory)/'prestate-evidence.json');execution=load_json(Path(directory)/'fork-execution.json');original=evidence['block'];cases=[]
    for case in execution['cases']:
        local=case['deposit']['before_header']
        differences={key:{'original':original.get(key),'local':local.get(key)} for key in sorted(set(original)|set(local)) if key not in ('transactions','withdrawals','uncles') and original.get(key)!=local.get(key)}
        cases.append({'protocol':case['protocol'],'original_H':int(original['number'],16),'original_header_hash':original['hash'],'local_preparation_start_height':int(local['number'],16),'local_preparation_start_hash':local['hash'],'header_differences':differences,
            'local_state_root_authority':'Ganache sparse fork overlay; not the original full world-state commitment','public_prestate_authority':'each retained lazy read checked against original H account/storage MPT proofs','original_chain_mined_transaction':False})
    for kind in ('balance_overrides','storage_overrides','code_overrides'):
        require(execution['overrides'][kind]==[],'HISTORY_UNSUPPORTED_STATE_OVERRIDE:'+kind)
    require(execution['overrides']['original_chain_hardfork_equivalence_claimed'] is False,'UNSUPPORTED_HARDFORK_EQUIVALENCE')
    require(opcode_counts.get('BLOCKHASH',0)==0,'ORIGINAL_BLOCKHASH_SEMANTICS_NOT_BOUND')
    return {'schema_version':1,'cases':cases,'environment_opcode_counts':opcode_counts,'overrides':execution['overrides'],'semantic_scope':'fixed original public prestate + exact retained simulated block environment; no claim of an original mined transaction or full current-fork VM equivalence'}


def export_history(directory,replayed):
    directory=Path(directory).resolve();evidence=load_json(directory/'prestate-evidence.json');binding=replayed['source_compiler_binding'];old=load_json(BASE/'catalog.json');entries=[]
    old_eth=[r for r in old['deployments'] if r['chain']=='ethereum'];proofs={d['address'].lower():d for d in evidence['deployments']}
    def add(chain,protocol,version,address,anchor,runtime,proof=None,execution_case=None,role='pool',source=True):
        row={'chain':chain,'protocol':protocol,'version':version,'address':address.lower(),'anchor':anchor,'runtime_sha256':runtime,'role':role,'source_compiler_runtime_verified':source,
             'proof':proof,'execution_case':execution_case,'original_chain_mined_transaction':False,'domain_profile_support_is_separate_from_R01':True}
        row['id']='r01-history-'+sha256(json.dumps(anchor_key(row),separators=(',',':')).encode())[:24]
        row['qualification']='QUALIFIED_FIXED_H_REFERENCE' if proof and execution_case is not None and source else 'NOT_QUALIFIED'
        entries.append(row)
    for row in old_eth:
        add(row['chain'],row['protocol'],row['version'],row['address'],{k:row['anchor'][k] for k in ('height','hash')},row['runtime']['sha256'],
            {'legacy_evidence':row['rebuild']['inputs']['evidence']} if row.get('rebuild') else None,role=row['role'])
    anchor={k:binding['anchor'][k] for k in ('height','hash')};proof_ref=reference(directory/'prestate-evidence.json')
    common={'all_accounts':proof_ref,'go_contract_report':reference(directory/'prestate-proof-report.json'),'all_prestate_report':reference(directory/'all-prestate-proof-report.json')}
    for row in old_eth:
        address=row['address'].lower();actual=proofs[address]
        require(sha256(bytes.fromhex(actual['code'][2:]))==row['runtime']['sha256'],'SAME_VERSION_NEW_ANCHOR_RUNTIME')
        case={'kind':'source-verified-factory-associated-pool','pool':binding['cases'][1]['pool'],'directory':str(directory.relative_to(ROOT))} if row['protocol']=='uniswap-v3' else None
        add('ethereum',row['protocol'],row['version'],address,anchor,row['runtime']['sha256'],common,case,role=row['role'])
    template=load_json(BASE/'v3-pool-template/template.json')
    for index,case in enumerate(binding['cases']):
        version='runtime-sha256:'+case['identity']['actual_runtime_sha256'] if case['protocol']=='uniswap-v2' else 'pool-template-sha256:'+template['runtime_template_sha256']
        add('ethereum',case['protocol'],version,case['pool'],anchor,case['identity']['actual_runtime_sha256'],common,{'kind':'actual-original-runtime-net-recipient','case_index':index,'directory':str(directory.relative_to(ROOT))})
    for row in old_eth:
        if row['protocol']=='uniswap-v3':
            sample=row['sample_pools'][0];version='pool-template-sha256:'+template['runtime_template_sha256']
            add('ethereum','uniswap-v3',version,sample['address'],{k:row['anchor'][k] for k in ('height','hash')},sample['runtime']['sha256'],source=True)
            add('ethereum','uniswap-v3',version,sample['address'],anchor,sha256(bytes.fromhex(proofs[sample['address']]['code'][2:])),common,source=True)
    require(not history_scope_errors(entries,entries),'HISTORY_DUPLICATE')
    payload={'schema_version':2,'kind':'same-deployed-version-multiple-immutable-historical-anchors','entries':entries,'capture_directory':str(directory.relative_to(ROOT)),'scope':'original public fixed-H bank references, explicit simulation environment; legacy local-new-deployment cases remain separate'}
    scope={'schema_version':2,'required':[{k:r[k] for k in ('chain','protocol','version','address','anchor')} for r in entries]}
    write_new(BASE/'history-catalog.json',(json.dumps(payload,indent=2)+'\n').encode());write_new(BASE/'history-scope.json',(json.dumps(scope,indent=2)+'\n').encode())
    selected=[r for r in entries if r['qualification']=='QUALIFIED_FIXED_H_REFERENCE']
    write_new(BASE/'history-catalog-qualified.json',(json.dumps(payload|{'entries':selected},indent=2)+'\n').encode());write_new(BASE/'history-scope-qualified.json',(json.dumps(scope|{'required':[{k:r[k] for k in ('chain','protocol','version','address','anchor')} for r in selected]},indent=2)+'\n').encode())
    write_new(directory/'environment-differences.json',(json.dumps(environment_differences(directory,replayed['environment_opcode_counts']),indent=2)+'\n').encode())
    write_new(directory/'source-execution-binding-report.json',(json.dumps(binding,indent=2)+'\n').encode())
    return payload


def validate_history_catalog(catalog,scope,replayed=None):
    entries=catalog.get('entries',[]);required=scope.get('required',[]);require(entries and required,'EMPTY_HISTORY_SCOPE')
    errors=history_scope_errors(entries,required);rows=[]
    binding=replayed.get('source_compiler_binding') if replayed else None
    proofmap={}
    if replayed:
        directory=ROOT/catalog['capture_directory'];evidence=load_json(directory/'prestate-evidence.json');proofmap={d['address'].lower():d for d in evidence['deployments']}
        require(binding['anchor']['hash']==evidence['expected_block_hash'],'HISTORY_CAPTURE_ANCHOR')
    for row in entries:
        result={'id':row['id'],'chain':row['chain'],'protocol':row['protocol'],'address':row['address'],'version':row['version'],'anchor':row['anchor'],'status':'NOT_QUALIFIED','missing':[],'original_chain_mined_transaction':False}
        if not replayed:result['missing'].append('OFFLINE_ORIGINAL_STATE_REPLAY_REQUIRED')
        else:
            try:
                require(row['chain']=='ethereum','HISTORY_CHAIN_BINDING')
                fresh=row['anchor']=={k:binding['anchor'][k] for k in ('height','hash')}
                source=False;execution=False
                if row['role']=='factory':
                    require(row['protocol']=='uniswap-v3','HISTORY_FACTORY_PROTOCOL')
                    source=row['runtime_sha256']==binding['factory']['runtime_sha256'] and row['address']==binding['factory']['address']
                elif row['protocol']=='uniswap-v2':source=row['runtime_sha256']==next(c for c in binding['cases'] if c['protocol']=='uniswap-v2')['identity']['actual_runtime_sha256']
                else:source=any(p['address']==row['address'] and p['actual_runtime_sha256']==row['runtime_sha256'] and 'pool-template-sha256:'+p['runtime_template_sha256']==row['version'] for p in binding['related_v3_pools'])
                require(source,'HISTORY_SOURCE_RUNTIME_BINDING')
                if row['role']=='factory' or row['protocol']=='uniswap-v2':
                    require(row['version']=='runtime-sha256:'+row['runtime_sha256'],'HISTORY_VERSION_RUNTIME_BINDING')
                state=False
                if fresh:
                    require(isinstance(row.get('proof'),dict),'HISTORY_PROOF_REFERENCE_MISSING')
                    expected_path=str((directory/'prestate-evidence.json').relative_to(ROOT))
                    require(row['proof'].get('all_accounts',{}).get('path')==expected_path,'HISTORY_PROOF_REFERENCE_PATH')
                    for ref in row['proof'].values():read_blob(ROOT,ref)
                    require(row['address'] in proofmap,'HISTORY_PROOF_ACCOUNT_MISSING');actual=proofmap[row['address']]['code'];require(sha256(bytes.fromhex(actual[2:]))==row['runtime_sha256'],'HISTORY_PROOF_RUNTIME_BINDING');state=True
                elif row.get('proof',{}):
                    legacy=row['proof'].get('legacy_evidence')
                    if legacy:
                        raw=read_blob(ROOT,legacy);e=json.loads(raw);require(int(e['block']['number'],16)==row['anchor']['height'] and e['expected_block_hash']==row['anchor']['hash'],'LEGACY_HISTORY_ANCHOR')
                        require(e.get('chain_id')=='1','LEGACY_HISTORY_CHAIN');matched=[d for d in e['deployments'] if d['address'].lower()==row['address']];require(len(matched)==1 and sha256(bytes.fromhex(matched[0]['code'][2:]))==row['runtime_sha256'],'LEGACY_HISTORY_RUNTIME_ADDRESS')
                        p=subprocess.run([str(ROOT/'chain-parser-go/bin/source-evidence'),'--verify',str(ROOT/legacy['path'])],capture_output=True,text=True,timeout=30);require(p.returncode==0,'LEGACY_HISTORY_PROOF_FAILED');state=True
                case=row.get('execution_case')
                if fresh and case:
                    require(case.get('directory')==catalog['capture_directory'],'HISTORY_EXECUTION_DIRECTORY')
                    if case['kind']=='actual-original-runtime-net-recipient':
                        actual=binding['cases'][case['case_index']];require(actual['pool']==row['address'] and actual['protocol']==row['protocol'],'HISTORY_EXECUTION_POOL_BINDING');execution=True
                    elif case['kind']=='source-verified-factory-associated-pool':
                        require(row['role']=='factory' and row['address']==binding['factory']['address'] and case['pool'] in [c['pool'] for c in binding['cases'] if c['protocol']=='uniswap-v3'],'HISTORY_FACTORY_POOL_BINDING');execution=True
                    else:require(False,'HISTORY_EXECUTION_SCOPE')
                result.update(source_compiler_runtime_verified=source,state_commitments_verified=state,original_runtime_net_recipient_reference_verified=execution)
                if not state:result['missing'].append('FIXED_H_STATE_COMMITMENT_MISSING')
                if not execution:result['missing'].append('THIS_ADDRESS_FIXED_H_EXECUTION_REFERENCE_MISSING')
                if source and state and execution:result['status']='QUALIFIED_FIXED_H_REFERENCE'
            except (CatalogError,KeyError,ValueError,TypeError,subprocess.SubprocessError) as error:errors.append(row['id']+':'+str(error));result['missing'].append('INVALID_HISTORY_BINDING:'+str(error))
        rows.append(result)
    return {'schema_version':2,'structurally_valid':not errors,'qualified':not errors and all(r['status']=='QUALIFIED_FIXED_H_REFERENCE' for r in rows),'entries':rows,'errors':errors,'network_requests':0 if replayed else None,'original_chain_mined_transaction':False,'scope':'fixed-H original public prestate with explicitly retained local execution environment; no full-chain or production claim'}


def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('command',choices=['capture','resume','replay','export','verify-catalog']);p.add_argument('--directory',type=Path,required=True);p.add_argument('--primary',default='https://eth.drpc.org');p.add_argument('--witness',default='https://ethereum-rpc.publicnode.com');p.add_argument('--out',type=Path);p.add_argument('--catalog',type=Path,default=BASE/'history-catalog.json');p.add_argument('--scope',type=Path,default=BASE/'history-scope.json');p.add_argument('--require-qualified',action='store_true');a=p.parse_args()
    if a.command in ('capture','resume'):print(capture([a.primary,a.witness],a.directory,resume=a.command=='resume'))
    elif a.command=='replay':replay(a.directory,a.out)
    elif a.command=='verify-catalog':
        require((ROOT/load_json(a.catalog)['capture_directory']).resolve()==a.directory.resolve(),'HISTORY_CAPTURE_DIRECTORY')
        result=replay(a.directory,None);report=validate_history_catalog(load_json(a.catalog),load_json(a.scope),result)
        if a.out:a.out.write_text(json.dumps(report,indent=2)+'\n')
        print(json.dumps(report,indent=2));sys.exit(0 if report['structurally_valid'] and (not a.require_qualified or report['qualified']) else 1)
    else:
        result=replay(a.directory,a.out);payload=export_history(a.directory,result);print('Immutable history entries:',len(payload['entries']))
if __name__=='__main__':main()
