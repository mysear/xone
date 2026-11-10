#!/usr/bin/env python3
"""Published original breaker sources through PG/Kafka/two real routers.

Finite historical source replay; no trusted live chain or atomic executor claim.
"""
import argparse
import copy
import hashlib
import json
import os
from pathlib import Path
import secrets
import subprocess
import tempfile
import threading
import time
import urllib.request
import importlib.util
import managed_v2_breaker_projection as projection
import managed_v2_scheduled_shared_smoke as shared
import managed_v2_multi_smoke as multi
import managed_v2_scheduled_smoke as single
ROOT=projection.ROOT;OUT=ROOT/'integration/artifacts/local'
TARGET=OUT/'managed-v2-breaker-three-service-smoke.json'
base=shared.base;require=projection.require;FAULTS=shared.FAULTS
spec=importlib.util.spec_from_file_location('breaker_smoke_wire',ROOT/'integration/tools/ci/managed-v2-breaker-wire-golden.py');wire=importlib.util.module_from_spec(spec);spec.loader.exec_module(wire)
def config_for(pool):
    config=projection.base._config(pool);config['protocol']=projection.PROTOCOL
    config['multi_asset']['token_policy']='standard-breaker-bpt'
    config['multi_asset']['managed_v2']['bpt']={'allowlist_subjects':[m['address'] for m in pool['multi_asset']['managed_v2']['bpt']['allowlist_members']]}
    return config
class ReplayChain(shared.ReplayChain):
    def __init__(self,raw,thin,namespace):
        require(namespace in raw['branches'],'unknown BPT native mode')
        self.namespace=namespace
        self.lock=threading.RLock()
        self.headers,self.receipts,self.frames={},{},{}
        self.reads,self.getter_reads,self.storage_reads,self.code_reads={},{},{},{}
        self.fault_hits=0
        thin={'banks':{b['id']:{**b,'pools':[dict(p,config=config_for(p['pool'])) for p in b['pools']]} for b in thin['banks']}}
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


    def select(self,ident,fault=None):
        with self.lock:
            self.fault_started_at_unix_ms=None
            self.fault_started_monotonic=None
            return super().select(ident,fault)

    def rpc(self,method,args):
        try:
            return super().rpc(method,args)
        finally:
            with self.lock:
                if self.fault_hits and self.fault_started_at_unix_ms is None:
                    self.fault_started_monotonic=time.monotonic()
                    self.fault_started_at_unix_ms=int(time.time()*1000)

def write(report):
    OUT.mkdir(parents=True,exist_ok=True);temporary=TARGET.with_name(TARGET.name+'.'+secrets.token_hex(6)+'.tmp')
    try:temporary.write_text(json.dumps(report,indent=2)+'\n');temporary.replace(TARGET)
    finally:temporary.unlink(missing_ok=True)

def request_for(case,max_hops=1):
    return dict(chainKey='evm:31337',kind='EXACT_IN',sellToken=case['token_in'],buyToken=case['token_out'],sellAmount=case['amount_in'],caller=case['actor'],recipient=case['actor'],slippageBps=50,verification='FAST',constraints={'maxHops':max_hops})

def wait_for_withdrawal(fetch_status, poison_started, timeout=25):
    """Use the first actual poison's clock, including blocking query time."""
    deadline=poison_started+timeout
    while time.monotonic()<deadline:
        remaining=deadline-time.monotonic()
        if remaining<=0:break
        try:
            code,body=fetch_status(remaining)
        except OSError:
            code,body=0,None
        observed=time.monotonic()
        require(observed<=deadline,'breaker withdrawal status arrived after its deadline')
        if code==503:
            return dict(observed_monotonic=observed,observed_at_unix_ms=int(time.time()*1000),latency_ms=round((observed-poison_started)*1000),status=code)
        time.sleep(min(.05,max(0,deadline-time.monotonic())))
    raise ValueError('breaker source scope was not withdrawn before its deadline')

def validate_rejected_case(code,response,case):
    require(type(response) is dict and set(response)=={'error'},'negative quote must contain only its explicit business error')
    if case['amount_in']=='0':
        require(code==400 and response['error']=='INVALID_REQUEST','zero input must fail request validation')
    else:
        require((code,response['error']) in ((404,'NO_ROUTE'),(400,'NO_EXECUTABLE_ALLOCATION')),'nonzero rejected quote must fail for route/capacity semantics')

def run_mode(raw,thin,mode,report):
    chain=ReplayChain(raw,thin,mode)
    cases={}
    for c in raw['cases']:
        if c['namespace']==mode:cases.setdefault(c['before'],[]).append(c)
    observed=dict(namespace=mode,passed=False,source_frames=[],quotes=[],rejections=[],faults=[],restarted=False)
    report['namespaces'].append(observed);write(report)
    with multi.resources(chain) as (servers,infra),tempfile.TemporaryDirectory(prefix='xone-bpt-service-') as folder,multi.Processes() as children:
        root,token=Path(folder),secrets.token_urlsafe(32)
        names={'go':'chain-parser-go','rust':'chain-parser-rust','router1':'dex-router','router2':'dex-router'}
        ports={actor:base.port() for actor in names};urls={actor:f'http://127.0.0.1:{port}' for actor,port in ports.items()};configs={}
        for actor,project in names.items():
            config=base.config_for(project,root/actor,token,ports[actor])
            config['journal'].update(transport='kafka',source_urls=[urls[a] for a in ('go','rust')],poll_interval_ms=100,request_timeout_ms=30000)
            config['journal']['kafka'].update(brokers=[infra.broker],topics={'evm:31337':'canonical.evm-31337.v1','solana:localnet':'canonical.solana-localnet.v1'},replica_id=actor,transaction_timeout_ms=4000)
            config['control'].update(backend='disabled' if project=='dex-router' else 'postgres',dsn_file='' if project=='dex-router' else str(infra.dsnfile),instance_id=actor,lease_ttl_ms=30000,renewal_ms=1000,min_commit_remaining_ms=7000)
            config['startup']['max_baseline_age_ms']=base.FRESHNESS
            if actor=='go':config['chains'][0].update(genesis_hash=chain.canonical[0]['hash'],confirmation_tag='latest',auto_reorg_window=4,rpc_urls=[f'http://127.0.0.1:{s.server_port}' for s in servers],pools=chain.configs)
            elif actor=='rust':base.sample_for('solana:localnet',root/actor/'fixtures',chain.timestamp*1000)
            else:
                config['routing'].update(max_hops=2,max_splits=2,split_parts=10,max_state_age_ms=base.FRESHNESS,quote_ttl_ms=60000)
                config['api']['quote_timeout_ms']=15000
            configs[actor]=base.save_config(root/actor/'config',config)
        def start(actor):children.start(actor,base.service_command(names[actor],configs[actor],root/actor/'fixtures' if actor=='rust' else None),ROOT/names[actor],OUT/f'managed-v2-breaker-{mode}-{actor}.log')
        def http(actor,path,body=None,timeout=20):return base.http(urls[actor]+path,token,body,timeout=timeout)
        def status(actor,key='evm:31337',timeout=20):return http(actor,'/internal/chains/'+key+'/status',timeout=timeout)
        published=None
        def complete_bank():
            nonlocal published
            base.await_condition(lambda:status('go')[0]==200 and status('go')[1].get('snapshot_ref',{}).get('block',{}).get('hash')==chain.frame['header']['hash'],'breaker owner did not reach exact source H')
            code,snapshot=http('go','/internal/chains/evm:31337/baseline');require(code==200 and snapshot.get('pools')==chain.frame['pools'],'complete breaker source Pool differs')
            req=urllib.request.Request(urls['go']+'/internal/chains/evm:31337/bootstrap',headers={'Authorization':'Bearer '+token})
            with urllib.request.urlopen(req,timeout=15) as response:
                require(response.headers['content-type']=='application/x-protobuf','breaker bootstrap type');actual=base.bootstrap_snapshot(response.read())
            expected=subprocess.run(['protoc','-I',str(ROOT/'common/protoc/proto'),'--encode=dex.v1.OnlineSnapshot',str(ROOT/'common/protoc/proto/dex/v1/state.proto')],input=wire.encode(snapshot).encode(),capture_output=True,check=True).stdout
            require(actual==expected,'breaker source protobuf differs from independent C++')
            for actor in ('router1','router2'):base.await_condition(lambda:status(actor)[0]==200 and status(actor)[1].get('snapshot_ref')==snapshot['reference'],'breaker replica full source reference differs')
            published=copy.deepcopy(snapshot['reference']);observed['last_complete_bank']=dict(source_bank=chain.frame['id'],reference=published,protobuf_sha256=hashlib.sha256(actual).hexdigest(),protobuf_bytes=len(actual),independent_cpp_identical=True);return snapshot
        def quote_case(actor,c,phase='original-direct'):
            request=request_for(c);code,q=http(actor,'/v1/quote',request)
            success=c['expected_native']['status']==1 and int(c['expected_native']['actual_amount_out'])>0
            ordinary=c['operation']=='token_swap'
            if not success and not (ordinary and code==200):
                validate_rejected_case(code,q,c);observed['rejections'].append(dict(case=c['id'],actor=actor,status=code,response=q));return None
            require(code==200,('breaker quote failed',c['id'],code,q));single.validate_quote_reference(q,published)
            require(q['verification']['level']=='INDICATIVE' and q['verification']['success'] is False,'breaker verification promoted')
            require(q['consumedIn']==c['amount_in'] and q['refund']=='0' and int(q['grossBuyAmount'])>0,'actual quote input/output policy')
            if not ordinary:
                require(q['grossBuyAmount']==c['expected_native']['actual_amount_out'],'native direct breaker amount differs')
            else:
                # A same-TOKEN request may use the other healthy physical pool;
                # qualify the actual selected plan later through original VM.
                require(sum(int(r['amount_in']) for r in q['routes'])==int(c['amount_in']) and sum(int(r['amount_out']) for r in q['routes'])==int(q['grossBuyAmount']),'ordinary alternative route conservation')
                require(all(len(r['path']['edges'])==1 and r['path']['edges'][0]['pool_id'] in {p['pool_id'] for p in chain.frame['pools']} for r in q['routes']),'ordinary routes use explicit original physical pools')
            require(q['minBuyAmount']==str(int(q['grossBuyAmount'])*9950//10000),'breaker integer slippage')
            observed['quotes'].append(dict(id=mode+'/'+actor+'/'+str(len(observed['quotes'])),namespace=mode,source_bank=chain.frame['id'],case=c['id'],actor=actor,phase=phase,request=request,quote=q));return request,q
        for actor in names:start(actor)
        for actor in names:base.wait(urls[actor]+'/health/live')
        supported=[]
        for ident,frame in chain.frames.items():
            if len(frame['pools'])!=2:continue
            chain.select(ident);complete_bank();supported.append(ident);observed['source_frames'].append(copy.deepcopy(observed['last_complete_bank']))
            for c in cases.get(ident,[]):
                for actor in ('router1','router2'):quote_case(actor,c)
            for order in raw['orders']:
                if order['namespace']!=mode or order['source_bank']!=ident:continue
                request=dict(chainKey='evm:31337',kind='EXACT_IN',sellToken=order['token_in'],buyToken=order['token_out'],sellAmount=order['amount_in'],caller=order['caller'],recipient=order['recipient'],slippageBps=50,verification='FAST',constraints={'maxHops':2})
                for actor in ('router1','router2'):
                    code,q=http(actor,'/v1/quote',request);require(code==200,('breaker actual shared capacity quote failed',code,q));single.validate_quote_reference(q,published)
                    require(q['verification']['level']=='INDICATIVE' and q['verification']['success'] is False,'breaker split verification promoted')
                    require(len(q['routes'])==2 and all(len(r['path']['edges'])==2 for r in q['routes']),'actual two-route four-step capacity plan')
                    require(q['consumedIn']==order['amount_in'] and q['refund']=='0' and sum(int(r['amount_in']) for r in q['routes'])==int(order['amount_in']),'actual split input conservation')
                    require(sum(int(r['amount_out']) for r in q['routes'])==int(q['grossBuyAmount']),'actual split output conservation')
                    observed['quotes'].append(dict(id=mode+'/'+actor+'/'+str(len(observed['quotes'])),namespace=mode,source_bank=ident,actor=actor,phase='actual-two-route-shared-breaker',request=request,quote=q))
            write(report)
        positive=[c for c in raw['cases'] if c['namespace']==mode and c['operation']=='bpt_join' and c['expected_native']['status']==1 and int(c['expected_native']['actual_amount_out'])>0 and c['before'] in supported]
        require(positive,'native positive cases required')
        first,last=positive[0],positive[-1]
        for fault in FAULTS:
            chain.select(first['before']);generation=complete_bank()['reference']['online_generation'];warm,_=quote_case('router1',first,'warm-before-fault')
            chain.select(last['before'],fault=fault);withdrawals={}
            selected_at=int(time.time()*1000)
            # A head jump may require dozens of valid block commits before the
            # poisoned H is reached. Keep that bounded catchup distinct from
            # revocation latency after the first actual rejected source read.
            base.await_condition(lambda:chain.fault_started_at_unix_ms is not None,'declared breaker poison was never reached',timeout=60)
            poison_started=chain.fault_started_at_unix_ms
            poison_monotonic=chain.fault_started_monotonic
            withdrawal_events={}
            for actor in ('router1','router2'):
                try:
                    withdrawal_events[actor]=wait_for_withdrawal(lambda remaining:status(actor,timeout=min(2,remaining)),poison_monotonic,25)
                except Exception:
                    observed['pending_fault_failure']=dict(fault=fault,actor=actor,hits=chain.fault_hits,poison=copy.deepcopy(chain.fault_evidence),statuses={a:status(a) for a in ('go','router1','router2')})
                    write(report)
                    raise
                code,rejected=http(actor,'/v1/quote',warm);single.validate_withdrawn_quote(code,rejected)
                require(status(actor,'solana:localnet')[0]==200,'breaker failure revoked unrelated Solana')
                withdrawals[actor]=dict(evm_status=503,fast_quote_status=code,response=rejected,solana_status=200)
            require(chain.fault_hits>0,'breaker source fault was not read');hits=chain.fault_hits;poison=copy.deepcopy(chain.fault_evidence);withdraw_completed=int(time.time()*1000)
            chain.select(last['before']);restored=complete_bank();require(restored['reference']['online_generation']>generation,'breaker restore reused old generation');quote_case('router1',last,'recovery')
            observed['faults'].append(dict(fault=fault,hits=hits,poison=poison,source_selected_at_unix_ms=selected_at,poison_started_at_unix_ms=poison_started,catchup_to_poison_ms=poison_started-selected_at,withdrawal_completed_at_unix_ms=max(e['observed_at_unix_ms'] for e in withdrawal_events.values()),post_withdrawal_probes_completed_at_unix_ms=withdraw_completed,withdrawal_latency_ms=max(e['latency_ms'] for e in withdrawal_events.values()),poison_started_monotonic=poison_monotonic,withdrawal_events=withdrawal_events,reference=restored['reference'],withdrawals=withdrawals));write(report)
        children.stop('router2');start('router2');base.wait(urls['router2']+'/health/live');complete_bank();quote_case('router2',last,'restart');observed['restarted']=True
        code,sample=quote_case('router1',last,'admission-probe');verified={**code,'verification':'VERIFIED'}
        status_code,_=http('router1','/v1/quote',verified);require(status_code==422,'breaker VERIFIED enabled')
        status_code,_=http('router1','/v1/quotes/'+sample['quoteId']+'/build',{});require(status_code==422,'breaker public build enabled')
        observed.update(passed=True,read_counts=chain.reads,getter_read_counts=chain.getter_reads,storage_read_counts=chain.storage_reads,code_read_counts=chain.code_reads);write(report)

def compare_selected_vectors(vector_path):
    """Run the real declared consumer; missing input cannot silently skip it."""
    return subprocess.run([str(Path.home()/'.cargo/bin/cargo'),'+1.90.0','test','--locked','--manifest-path',str(ROOT/'dex-router/Cargo.toml'),'-p','xone-routing','--test','managed_v2_breaker','actual_api_selected_breaker_native_full_poststate','--','--ignored','--exact'],
            env={**os.environ,'XONE_BREAKER_SELECTED_VECTORS':str(vector_path)},capture_output=True,text=True,timeout=600,cwd=ROOT)

def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--mode',choices=('cached','dynamic','boundaries','shared','all'),default='all');args=p.parse_args()
    report=dict(schema_version=1,passed=False,state='RUNNING',namespaces=[],verified=False,production_ready=False,all_requested_features_complete=False,trust='historical January2026 local original VM source; same-chain replay endpoints',started_at_unix_ms=int(time.time()*1000));write(report)
    try:
        bank=projection.load(ROOT/'integration/test/contracts/managed-v2-breaker.json');thin=projection.project(bank);report['original_raw']=thin['original_raw'];report['source_binding_started']=base.runtime_fingerprint();report['input_hashes_started']=base.snapshot_inputs()
        for mode in ('cached','dynamic','boundaries','shared') if args.mode=='all' else (args.mode,):run_mode(bank,thin,mode,report)
        rows=[q for mode in report['namespaces'] for q in mode['quotes']]
        selected=dict(schema_version=1,original_breaker_sha256=thin['original_raw']['sha256'],quotes=rows)
        selected_path=OUT/'managed-v2-breaker-actual-selected-input.json'
        selected_path.write_text(json.dumps(selected,indent=2)+'\n')
        replay=subprocess.run(['node',str(ROOT/'integration/tools/ci/managed-v2-breaker-plan-replay.cjs')],input=json.dumps(selected).encode(),capture_output=True,timeout=1800,cwd=ROOT)
        require(replay.stdout,'breaker selected VM returned no output: '+replay.stderr.decode()[-3000:]);report['independent_actual_plan_replay']=json.loads(replay.stdout);require(replay.returncode==0 and report['independent_actual_plan_replay'].get('passed') is True,'breaker actual API native replay failed: '+str(report['independent_actual_plan_replay'].get('error')))
        native_path=OUT/'managed-v2-breaker-actual-selected-native.json'
        native_path.write_text(json.dumps(report['independent_actual_plan_replay'],indent=2)+'\n')
        vectors=projection.project_selected_report(bank,native_path,selected)
        vector_path=OUT/'managed-v2-breaker-actual-selected-vectors.json';vector_path.write_text(json.dumps(vectors,indent=2)+'\n')
        compared=compare_selected_vectors(vector_path)
        log=OUT/'managed-v2-breaker-actual-selected-model.log';log.write_text(compared.stdout+compared.stderr)
        require(compared.returncode==0 and '1 passed' in compared.stdout,'breaker actual selected full local poststate differs: '+compared.stdout[-3000:]+compared.stderr[-2000:])
        report['actual_selected_model_differential']=dict(passed=True,executions=len(vectors['executions']),selected_transactions=vectors['selected_transactions'],native_report_sha256=hashlib.sha256(native_path.read_bytes()).hexdigest(),vectors_sha256=hashlib.sha256(vector_path.read_bytes()).hexdigest(),log_sha256=hashlib.sha256(log.read_bytes()).hexdigest())
        require(report['source_binding_started']==base.runtime_fingerprint(),'breaker source/binary changed during run');require(report['input_hashes_started']==base.snapshot_inputs(),'breaker service inputs changed during run')
        report.update(passed=True,state='PASSED',inputs_unchanged=True,completed_at_unix_ms=int(time.time()*1000),limitation='finite breaker local process/reference scope only; no current-chain/public-executor/full-design acceptance')
    except Exception as error:
        report.update(state='FAILED',error=str(error),completed_at_unix_ms=int(time.time()*1000));write(report);raise
    write(report);print(json.dumps({'passed':True,'report':str(TARGET),'production_ready':False}))
if __name__=='__main__':main()
