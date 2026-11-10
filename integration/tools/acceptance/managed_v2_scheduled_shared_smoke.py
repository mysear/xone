#!/usr/bin/env python3
"""Whole native shared schedule banks through ordinary PG/Kafka/two routers.

All source headers remain January 2026 observations. The explicit fixture age
budget does not confer current-chain trust or public execution admission.
"""
import argparse
import copy
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import secrets
import subprocess
import tempfile
import threading
import time
import urllib.request

import managed_v2_scheduled_shared_projection as projection
import managed_v2_scheduled_smoke as single
import managed_v2_multi_smoke as multi

ROOT=projection.ROOT
OUT=ROOT/'integration/artifacts/local'
TARGET=OUT/'managed-v2-scheduled-shared-three-service-smoke.json'
base=single.base
require=projection.require
FAULTS=('missing-peer-getter','missing-peer-word','reserved-peer-word','shared-wallet-underfund','peer-fee-getter','future-head')


class ReplayChain:
    provider=base.Chain.provider

    def __init__(self,raw,thin,namespace):
        require(namespace in ('mixed','active'),'unknown native shared mode')
        self.namespace=namespace
        self.lock=threading.RLock()
        self.headers,self.receipts,self.frames={},{},{}
        self.reads,self.getter_reads,self.storage_reads,self.code_reads={},{},{},{}
        self.fault_hits=0
        branch=raw['branches'][namespace]
        self.branch=branch
        self.vault=branch['vault']
        def walk(value):
            if isinstance(value,dict):
                if {'hash','parentHash','number','timestamp','receiptsRoot','transactions'}<=value.keys():
                    key=value['hash']
                    require(key not in self.headers or self.headers[key]==value,'conflicting original shared header')
                    self.headers[key]=copy.deepcopy(value)
                if {'transaction','receipt','signed_transaction'}<=value.keys():
                    r=value['receipt']
                    self.receipts.setdefault(r['transactionHash'],{})[r['blockHash']]=copy.deepcopy(r)
                for child in value.values():walk(child)
            elif isinstance(value,list):
                for child in value:walk(child)
        walk(branch)
        for bank in (b for b in raw['banks'] if b['namespace']==namespace):
            walk(bank['header'])
            getters,methods={},{}
            for row in bank['pools']:
                for g in row['state']['getters']:
                    key=(g['to'],g['data'])
                    require(key not in getters or getters[key]==g['result'],'conflicting raw shared call')
                    getters[key],methods[key]=g['result'],g['method']
            self.frames[bank['id']]=dict(id=bank['id'],header=copy.deepcopy(bank['header']),
                pools=[copy.deepcopy(row['pool']) for row in thin['banks'][bank['id']]['pools']],
                getters=getters,methods=methods,
                codes={a['address']:branch['native_runtime'][a['code_sha256']] for a in bank['accounts']},
                storage={(r['address'],r['key']):'0x'+r['value'][2:].rjust(64,'0') for r in bank['storage']})
        self.by_hash={frame['header']['hash']:frame for frame in self.frames.values()}
        require(len(self.by_hash)==len(self.frames),'duplicate common-H bank')
        first=next(iter(self.frames))
        self.configs=[copy.deepcopy(row['config']) for row in thin['banks'][first]['pools']]
        self.select(first)

    def anchor(self):
        return {'blockHash':self.frame['header']['hash'],'requireCanonical':True}

    def select(self,ident,fault=None):
        with self.lock:
            require(ident in self.frames and (fault is None or fault in FAULTS),'unknown common bank/fault')
            self.frame=copy.deepcopy(self.frames[ident])
            self.head=int(self.frame['header']['number'],16)
            self.timestamp=int(self.frame['header']['timestamp'],16)
            self.canonical={}
            h=self.frame['header']
            while True:
                n=int(h['number'],16)
                require(n not in self.canonical,'cyclic original ancestry')
                self.canonical[n]=h
                if n==0:break
                require(h['parentHash'] in self.headers,'missing original parent')
                parent=self.headers[h['parentHash']]
                require(int(parent['number'],16)==n-1,'original parent height gap')
                h=parent
            self.fault,self.fault_hits=fault,0
            self.fault_evidence=None
            peer=self.frame['pools'][1]['pool_id']
            if fault in ('missing-peer-getter','peer-fee-getter'):
                method='getActualSupply' if fault=='missing-peer-getter' else 'getSwapFeePercentage'
                key=next(k for k,m in self.frame['methods'].items() if k[0]==peer and m==method)
                before=self.frame['getters'][key]
                after=None if fault=='missing-peer-getter' else '0x'+format(int(before,16)+1,'064x')
                self.fault_evidence=dict(method='eth_call',key=key,before=before,after=after)
            elif fault=='shared-wallet-underfund':
                token=self.frame['pools'][0]['multi_asset']['assets'][0]['token']
                key=next(k for k,m in self.frame['methods'].items() if k[0]==token and m=='balanceOf' and k[1].endswith(self.vault[2:]))
                cash=sum(int(p['multi_asset']['managed_v2']['token_info'][0]['cash']) for p in self.frame['pools'])
                self.fault_evidence=dict(method='eth_call',key=key,before=self.frame['getters'][key],after='0x'+format(cash-1,'064x'))
            elif fault in ('missing-peer-word','reserved-peer-word'):
                target=peer if self.namespace=='active' else self.frame['pools'][0]['pool_id']
                if fault=='missing-peer-word':key=(target,'0x9')
                else:key=next(k for k in self.frame['storage'] if k[0]==target and len(k[1])==66)
                before=self.frame['storage'][key]
                after=None if fault=='missing-peer-word' else '0x'+format(int(before,16)|(1<<133),'064x')
                self.fault_evidence=dict(method='eth_getStorageAt',key=key,before=before,after=after)
            if fault and fault!='future-head':
                require(self.fault_evidence['before']!=self.fault_evidence['after'],'source fault has no effect')

    def rpc(self,method,args):
        with self.lock:
            self.reads[method]=self.reads.get(method,0)+1
            if method=='eth_chainId':return '0x7a69'
            if method=='eth_getBlockByNumber':
                n=self.head if args[0] in ('latest','safe','finalized') else int(args[0],16)
                require(n in self.canonical,'unknown canonical native header')
                value=copy.deepcopy(self.canonical[n])
                if self.fault=='future-head' and n==self.head:
                    value['timestamp']=hex(int(time.time())+120)
                    self.fault_hits+=1
                return value
            if method=='eth_getTransactionReceipt':
                rows=[r for h,r in self.receipts.get(args[0],{}).items() if int(r['blockNumber'],16) in self.canonical and self.canonical[int(r['blockNumber'],16)]['hash']==h]
                require(len(rows)==1,'unknown/noncanonical native receipt')
                return copy.deepcopy(rows[0])
            require(method in ('eth_call','eth_getStorageAt','eth_getCode'),'unexpected shared original RPC')
            anchor=args[2] if method=='eth_getStorageAt' else args[1]
            require(type(anchor) is dict and set(anchor)=={'blockHash','requireCanonical'} and anchor['requireCanonical'] is True,'exact canonical anchor required')
            frame=self.by_hash.get(anchor['blockHash'])
            require(frame is not None and int(frame['header']['number'],16) in self.canonical and self.canonical[int(frame['header']['number'],16)]['hash']==anchor['blockHash'],'unknown canonical source H')
            if method=='eth_call':
                require(type(args[0]) is dict and set(args[0])=={'to','data'},'exact raw getter request')
                key=(args[0]['to'].lower(),args[0]['data'].lower())
                table=frame['getters']
                counts=self.getter_reads
            elif method=='eth_getStorageAt':
                key=(args[0].lower(),args[1])
                table=frame['storage']
                counts=self.storage_reads
            else:
                key=args[0].lower()
                table=frame['codes']
                counts=self.code_reads
            require(key in table,'read absent from original common-H source')
            label=frame['id']+':'+(':'.join(key) if isinstance(key,tuple) else key)
            counts[label]=counts.get(label,0)+1
            if frame['header']['hash']==self.frame['header']['hash'] and self.fault_evidence and method==self.fault_evidence['method'] and key==self.fault_evidence['key']:
                self.fault_hits+=1
                require(self.fault_evidence['after'] is not None,'injected missing original peer dependency')
                return self.fault_evidence['after']
            return table[key]


def write(report):
    OUT.mkdir(parents=True,exist_ok=True)
    temp=TARGET.with_name(TARGET.name+'.'+secrets.token_hex(6)+'.tmp')
    try:
        temp.write_text(json.dumps(report,indent=2)+'\n')
        temp.replace(TARGET)
    finally:temp.unlink(missing_ok=True)


def request_for(order):
    return dict(chainKey='evm:31337',kind='EXACT_IN',sellToken=order['token_in'],buyToken=order['token_out'],
        sellAmount=order['amount_in'],caller=order['caller'],recipient=order['recipient'],slippageBps=50,
        verification='FAST',constraints={'maxHops':1})


def check_quote(code,q,request,reference,pools):
    require(code==200,('shared scheduled quote failed',code,q))
    single.validate_quote_reference(q,reference)
    require(q['verification']['level']=='INDICATIVE' and q['verification']['success'] is False,'shared quote verification promoted')
    require(q['consumedIn']==request['sellAmount'] and q['refund']=='0','shared split input conservation')
    require(int(q['grossBuyAmount'])>0 and q['minBuyAmount']==str(int(q['grossBuyAmount'])*(10000-request['slippageBps'])//10000),'integer output/slippage policy')
    candidates={p['pool_id'] for p in pools}
    require(set(q['quality']['candidateScope'])==candidates and q['quality']['excludedPools']=={},'shared executable candidate bank differs')
    require(len(q['routes'])==2,'funded capacity order must select two real routes')
    selected=[]
    for route in q['routes']:
        path=route['path']
        require(len(path['edges'])==1 and path['tokens']==[request['sellToken'],request['buyToken']],'only actual direct split routes are in scope')
        edge=path['edges'][0]
        p=next((p for p in pools if p['pool_id']==edge['pool_id']),None)
        require(p is not None and edge['protocol']==p['protocol'] and edge['from_token']==request['sellToken'] and edge['to']==request['buyToken'],'selected physical original pool/profile differs')
        require(path['amount_in']==route['amount_in'] and path['amount_out']==route['amount_out'] and int(route['amount_in'])>0 and int(route['amount_out'])>0,'selected route path amounts differ')
        selected.append(p['pool_id'])
    require(set(selected)==candidates and len(set(selected))==2,'both physical pools must execute')
    require(sum(int(r['amount_in']) for r in q['routes'])==int(request['sellAmount']),'exact split input sum')
    require(sum(int(r['amount_out']) for r in q['routes'])==int(q['grossBuyAmount']),'exact split output sum')


def run_mode(raw,thin,mode,report):
    chain=ReplayChain(raw,thin,mode)
    orders=[o for o in raw['orders'] if o['namespace']==mode]
    by_bank={o['source_bank']:o for o in orders}
    observed=dict(namespace=mode,passed=False,full_service_run=False,source_frames=[],quotes=[],faults=[],admission_probes=[],restarted=False)
    report['namespaces'].append(observed)
    write(report)
    with multi.resources(chain) as (servers,infra),tempfile.TemporaryDirectory(prefix='xone-scheduled-shared-') as folder,multi.Processes() as children:
        root,token=Path(folder),secrets.token_urlsafe(32)
        names={'go':'chain-parser-go','rust':'chain-parser-rust','router1':'dex-router','router2':'dex-router'}
        ports={a:base.port() for a in names}
        urls={a:f'http://127.0.0.1:{p}' for a,p in ports.items()}
        configs={}
        for actor,project in names.items():
            config=base.config_for(project,root/actor,token,ports[actor])
            config['journal'].update(transport='kafka',source_urls=[urls[a] for a in ('go','rust')],poll_interval_ms=100,request_timeout_ms=30000)
            config['journal']['kafka'].update(brokers=[infra.broker],topics={'evm:31337':'canonical.evm-31337.v1','solana:localnet':'canonical.solana-localnet.v1'},replica_id=actor,transaction_timeout_ms=4000)
            config['control'].update(backend='disabled' if project=='dex-router' else 'postgres',dsn_file='' if project=='dex-router' else str(infra.dsnfile),instance_id=actor,lease_ttl_ms=30000,renewal_ms=1000,min_commit_remaining_ms=7000)
            config['startup']['max_baseline_age_ms']=base.FRESHNESS
            if actor=='go':
                config['chains'][0].update(genesis_hash=chain.canonical[0]['hash'],confirmation_tag='latest',auto_reorg_window=4,rpc_urls=[f'http://127.0.0.1:{s.server_port}' for s in servers],pools=chain.configs)
            elif actor=='rust':base.sample_for('solana:localnet',root/actor/'fixtures',chain.timestamp*1000)
            else:
                config['routing'].update(max_hops=1,max_splits=2,max_state_age_ms=base.FRESHNESS,quote_ttl_ms=60000)
                config['api']['quote_timeout_ms']=15000
            configs[actor]=base.save_config(root/actor/'config',config)

        def start(actor):
            children.start(actor,base.service_command(names[actor],configs[actor],root/actor/'fixtures' if actor=='rust' else None),ROOT/names[actor],OUT/f'managed-v2-scheduled-shared-{mode}-{actor}.log')

        def http(actor,path,body=None):
            return base.http(urls[actor]+path,token,body,timeout=20)

        def status(actor,key='evm:31337'):
            return http(actor,'/internal/chains/'+key+'/status')

        published=None
        def complete_bank():
            nonlocal published
            base.await_condition(lambda:status('go')[0]==200 and status('go')[1].get('snapshot_ref',{}).get('block',{}).get('hash')==chain.frame['header']['hash'],'shared scheduled source owner did not reach exact native H')
            code,snapshot=http('go','/internal/chains/evm:31337/baseline')
            require(code==200 and snapshot.get('pools')==chain.frame['pools'],'complete two-pool source bank differs from native getters')
            request=urllib.request.Request(urls['go']+'/internal/chains/evm:31337/bootstrap',headers={'Authorization':'Bearer '+token})
            with urllib.request.urlopen(request,timeout=15) as response:
                require(response.headers['content-type']=='application/x-protobuf','shared bootstrap type')
                actual=base.bootstrap_snapshot(response.read())
            expected=subprocess.run(['protoc','-I',str(ROOT/'common/protoc/proto'),'--encode=dex.v1.OnlineSnapshot',str(ROOT/'common/protoc/proto/dex/v1/state.proto')],input=single.wire.encode(snapshot).encode(),capture_output=True,check=True).stdout
            require(actual==expected,'complete mixed/scheduled shared bank independent C++ protobuf differs')
            replicas={}
            for actor in ('router1','router2'):
                base.await_condition(lambda:status(actor)[0]==200 and status(actor)[1].get('snapshot_ref')==snapshot['reference'],'shared replica full reference mismatch')
                replicas[actor]=dict(ready=True,snapshot_ref=copy.deepcopy(status(actor)[1]['snapshot_ref']))
            published=copy.deepcopy(snapshot['reference'])
            observed['last_complete_bank']=dict(source_bank=chain.frame['id'],reference=published,pools_sha256=hashlib.sha256(projection.base.canonical(snapshot['pools'])).hexdigest(),protobuf_sha256=hashlib.sha256(actual).hexdigest(),protobuf_bytes=len(actual),independent_cpp_identical=True,replicas=replicas)
            return snapshot

        def quote(actor,order,phase):
            request=request_for(order)
            code,q=http(actor,'/v1/quote',request)
            check_quote(code,q,request,published,chain.frame['pools'])
            row=dict(id=mode+'/'+actor+'/'+str(len(observed['quotes'])),namespace=mode,source_bank=chain.frame['id'],order_id=order['id'],actor=actor,phase=phase,request=copy.deepcopy(request),quote=copy.deepcopy(q))
            observed['quotes'].append(row)
            write(report)
            return request,q

        for actor in names:start(actor)
        for actor in names:base.wait(urls[actor]+'/health/live')
        for ident in chain.frames:
            chain.select(ident)
            complete_bank()
            observed['source_frames'].append(copy.deepcopy(observed['last_complete_bank']))
            if ident in by_bank:
                for actor in ('router1','router2'):quote(actor,by_bank[ident],'native-checkpoint-two-route-split')
            write(report)
        require(len(observed['quotes'])==12,'all six source checkpoint API plans quoted by both replicas')
        for fault in FAULTS:
            chain.select(orders[0]['source_bank'])
            generation=complete_bank()['reference']['online_generation']
            warm,_=quote('router1',orders[0],'cache-before-fault')
            chain.select(orders[1]['source_bank'],fault=fault)
            withdrawals={}
            for actor in ('router1','router2'):
                base.await_condition(lambda:status(actor)[0]==503,'faulted shared scheduled bank remained ready: '+fault)
                code,rejected=http(actor,'/v1/quote',warm)
                single.validate_withdrawn_quote(code,rejected)
                require(status(actor,'solana:localnet')[0]==200,'shared source failure revoked unrelated Solana')
                solana_code,solana=http(actor,'/v1/quote',dict(chainKey='solana:localnet',kind='EXACT_IN',sellToken='A',buyToken='B',sellAmount='10003',caller='A',recipient='B',slippageBps=50,verification='FAST',constraints={'maxHops':1}))
                require(solana_code==200 and solana['verification']['level']=='INDICATIVE' and int(solana['grossBuyAmount'])>0,'unrelated actual Solana quote failed')
                withdrawals[actor]=dict(evm_status=503,fast_quote_status=code,fast_quote_response=rejected,solana_status=200,solana_quote_id=solana['quoteId'])
            require(chain.fault_hits>0,'declared source poison was not read')
            hits=chain.fault_hits
            poison=copy.deepcopy(chain.fault_evidence)
            chain.select(orders[1]['source_bank'])
            restored=complete_bank()
            require(restored['reference']['online_generation']>generation,'shared restore reused old generation')
            quote('router1',orders[1],'new-generation-after-fault')
            observed['faults'].append(dict(fault=fault,hits=hits,poison=poison,previous_generation=generation,reference=restored['reference'],withdrawals=withdrawals))
            write(report)
        children.stop('router2')
        start('router2')
        base.wait(urls['router2']+'/health/live')
        complete_bank()
        observed['restarted']=True
        observed['restart_reference']=copy.deepcopy(observed['last_complete_bank'])
        for actor in ('router1','router2'):
            request,q=quote(actor,orders[1],'restart-and-admission-check')
            request['verification']='VERIFIED'
            code,reply=http(actor,'/v1/quote',request)
            require(code==422 and 'INDEPENDENT_SIMULATION_UNAVAILABLE' in json.dumps(reply),'public scheduled shared VERIFIED enabled')
            build_code,_=http(actor,'/v1/quotes/'+q['quoteId']+'/build',{})
            require(build_code==422,'public scheduled shared build enabled')
            observed['admission_probes'].append(dict(actor=actor,verified_status=code,build_status=build_code,reason='INDEPENDENT_SIMULATION_UNAVAILABLE'))
        observed.update(passed=True,full_service_run=True,read_counts=chain.reads,getter_read_counts=chain.getter_reads,storage_read_counts=chain.storage_reads,code_read_counts=chain.code_reads,fault_count=len(FAULTS))
        write(report)


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--mode',choices=('mixed','active','all'),default='all')
    args=parser.parse_args()
    report=dict(schema_version=1,passed=False,state='RUNNING',namespaces=[],verified=False,production_ready=False,independent_gas_reference=False,all_requested_features_complete=False,started_at_unix_ms=int(time.time()*1000),
        trust='historical January2026 original local source replay; two endpoints share one chain; no independent provider trust',historical_fixture_age_budget_ms=base.FRESHNESS)
    write(report)
    try:
        bank=projection.load(ROOT/'integration/test/contracts/managed-v2-scheduled-shared.json')
        thin=projection.project(bank)
        report['original_raw']=thin['original_raw']
        report['source_binding_started']=base.runtime_fingerprint()
        report['input_hashes_started']=base.snapshot_inputs()
        modes=('mixed','active') if args.mode=='all' else (args.mode,)
        for mode in modes:run_mode(bank,thin,mode,report)
        rows=[row for mode in report['namespaces'] for row in mode['quotes']]
        selected=dict(schema_version=1,original_shared_sha256=thin['original_raw']['sha256'],quotes=rows)
        replay=subprocess.run(['node',str(ROOT/'integration/tools/ci/managed-v2-scheduled-shared-plan-replay.cjs')],input=json.dumps(selected).encode(),capture_output=True,timeout=1200,cwd=ROOT)
        require(replay.stdout,'actual selected shared VM returned no receipt: '+replay.stderr.decode()[-3000:])
        report['independent_actual_plan_replay']=json.loads(replay.stdout)
        require(replay.returncode==0 and report['independent_actual_plan_replay'].get('passed') is True,'actual selected shared VM failed: '+str(report['independent_actual_plan_replay'].get('error')))
        actual_path=OUT/'managed-v2-scheduled-shared-actual-selected-native.json'
        actual_path.write_text(json.dumps(report['independent_actual_plan_replay'],indent=2)+'\n')
        vectors=projection.project_selected_report(bank,report['independent_actual_plan_replay'])
        vector_path=OUT/'managed-v2-scheduled-shared-actual-selected-vectors.json'
        vector_path.write_text(json.dumps(vectors,indent=2)+'\n')
        compared=subprocess.run([str(Path.home()/'.cargo/bin/cargo'),'+1.90.0','test','--locked',
            '--manifest-path',str(ROOT/'dex-router/Cargo.toml'),'-p','xone-routing','--test',
            'managed_v2_scheduled_shared_native','actual_api_selected_two_route_full_poststates','--','--ignored','--exact'],
            env={**os.environ,'XONE_SCHEDULED_SHARED_SELECTED_VECTORS':str(vector_path)},
            capture_output=True,text=True,timeout=600,cwd=ROOT)
        compared_log=OUT/'managed-v2-scheduled-shared-actual-selected-model.log'
        compared_log.write_text(compared.stdout+compared.stderr)
        require(compared.returncode==0 and '1 passed' in compared.stdout,
                'actual API-selected original complete state differs from local carried model: '+compared.stdout[-3000:]+compared.stderr[-2000:])
        report['actual_selected_model_differential']=dict(passed=True,executions=len(vectors['executions']),
            selected_transactions=vectors['selected_transactions'],native_report_sha256=hashlib.sha256(actual_path.read_bytes()).hexdigest(),
            vector_sha256=hashlib.sha256(vector_path.read_bytes()).hexdigest(),log_sha256=hashlib.sha256(compared_log.read_bytes()).hexdigest())
        require(report['source_binding_started']==base.runtime_fingerprint(),'source/binary changed during shared service run')
        require(report['input_hashes_started']==base.snapshot_inputs(),'shared service inputs changed during run')
        report.update(passed=True,state='PASSED',inputs_unchanged=True,source_binding_completed=base.runtime_fingerprint(),completed_at_unix_ms=int(time.time()*1000),
            limitation='Finite two-pool three-underlying scheduled/static and scheduled/scheduled original local sources and actual API splits; all wider original design acceptance remains separate.')
    except Exception as error:
        report.update(state='FAILED',error=str(error),completed_at_unix_ms=int(time.time()*1000))
        write(report)
        raise
    write(report)
    print(json.dumps(dict(passed=report['passed'],report=str(TARGET),production_ready=False)))


if __name__=='__main__':main()
