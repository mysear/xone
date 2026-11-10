#!/usr/bin/env python3
"""Fixed public Solana RPC -> normal Rust parser -> normal router differential.

Account bytes, Clock and blockhash remain exactly as captured. The loopback RPC
answers only the fixed bank and explicitly captured addresses. No account/header
repair, transaction submission, typed-pool injection, or quote result fallback.
"""
import argparse
import base64
import copy
import hashlib
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
from pathlib import Path
import re
import secrets
import shutil
import socket
import subprocess
import threading
import time
import urllib.error
import urllib.request

from r01_public_catalog_solana_rebuild import ROOT,BASE,LOADER,VerificationError,require,digest,b58encode
from r01_public_catalog_solana_bank import TOKEN,CLOCK,GENESIS,WSOL,account_bytes,load_reference,verify_rpc_record,validate_capture,verify_saved_execution,execution_quantities,replay_selected_quantities,program_payload

CLMM_WINDOWS=('full-captured','nearest-three')
CHAIN='solana:mainnet-beta'
FRESHNESS=315360000000
SPECS={'cpmm':('cpmm-sourcebank-correct','raydium-cpmm-creator','raydium_cpmm'),
       'clmm':('clmm-sourcebank','raydium-clmm-rewards','raydium_clmm'),
       'orca':('orca-sourcebank','orca-whirlpools-rewards','orca_whirlpool')}

class FixedBankRPC:
    def __init__(self,label,*,clmm_window='full-captured'):
        require(clmm_window in CLMM_WINDOWS,'unsupported-clmm-window')
        self.clmm_window=clmm_window
        require(label in SPECS,'unsupported-process-case')
        self.label=label;folder,_,_=SPECS[label]
        self.sourcebank=BASE/'solana-rebuild'/folder/'sourcebank.json'
        self.manifest=json.loads(self.sourcebank.read_text())
        self.accounts,self.binding,self.clock=validate_capture(self.manifest)
        self.slot=self.binding['observed_context_slot']
        self.block=verify_rpc_record(load_reference(self.manifest['block']))['response'].get('result')
        require(isinstance(self.block,dict) and self.block.get('blockhash'),'fixed-bank-block-missing')
        self.genesis=verify_rpc_record(load_reference(self.manifest['genesis']))['response']['result']
        require(self.genesis==GENESIS,'fixed-bank-genesis')
        require(self.manifest['pool']['mint_a']==WSOL,'bounded-case-requires-WSOL-input')
        self.trace=[];self.lock=threading.Lock()

    def token_identity(self):
        program=self.accounts[TOKEN];raw=account_bytes(program);payload=program_payload(TOKEN,self.accounts,self.slot)
        identity={'program':TOKEN,'owner':program['owner'],'program_account_sha256':digest(raw)}
        if program['owner']==LOADER:
            require(len(raw)==36 and raw[:4]==(2).to_bytes(4,'little'),'token-upgradeable-program-header')
            pd=b58encode(raw[4:]);require(pd in self.accounts and self.accounts[pd] is not None,'uncaptured-token-programdata')
            data=account_bytes(self.accounts[pd]);require(data[:4]==(3).to_bytes(4,'little'),'token-programdata-header')
            identity.update(programdata=pd,upgrade_slot=int.from_bytes(data[4:12],'little'),payload_sha256=digest(payload),payload_bytes=len(payload))
        else:
            require(raw.startswith(b'\x7fELF'),'token-inline-ELF-required');identity.update(programdata=None,payload_sha256=digest(payload),payload_bytes=len(payload))
        return identity

    def dispatch(self,method,params):
        if method=='getGenesisHash':return self.genesis
        if method=='getMultipleAccounts':
            require(isinstance(params,list) and len(params)==2,'rpc-params')
            ids,options=params
            require(isinstance(ids,list) and 0<len(ids)<=100,'rpc-account-limit')
            require(options.get('encoding')=='base64' and options.get('commitment')=='finalized','fixed-rpc-finalized-base64')
            require(type(options.get('minContextSlot',0)) is int and options.get('minContextSlot',0)<=self.slot,'fixed-bank-min-context-unavailable')
            missing=[key for key in ids if key not in self.accounts]
            require(not missing,'uncaptured-accounts:'+','.join(missing))
            return {'context':{'slot':self.slot},'value':[copy.deepcopy(self.accounts[key]) for key in ids]}
        if method=='getBlock':
            require(len(params)>=1 and type(params[0]) is int and params[0]==self.slot,'uncaptured-bank-slot')
            if len(params)>1:require(params[1].get('commitment','finalized')=='finalized','fixed-rpc-finalized-block')
            return copy.deepcopy(self.block)
        raise VerificationError('unsupported-readonly-rpc:'+method)

    def serve(self,port=0):
        fixture=self
        class Handler(BaseHTTPRequestHandler):
            def log_message(self,*args):pass
            def do_POST(self):
                started=time.time();request={}
                try:
                    length=int(self.headers.get('Content-Length','0'));require(0<length<=65536,'rpc-request-budget')
                    request=json.loads(self.rfile.read(length));result=fixture.dispatch(request['method'],request.get('params',[]));response={'jsonrpc':'2.0','id':request.get('id'),'result':result}
                except Exception as error:response={'jsonrpc':'2.0','id':request.get('id'),'error':{'code':-32004,'message':str(error)}}
                raw=json.dumps(response,separators=(',',':')).encode()
                with fixture.lock:fixture.trace.append({'observed_at_unix':started,'request':request,'response_sha256':digest(raw),'response_bytes':len(raw),'error':response.get('error'),'fixed_slot':fixture.slot})
                self.send_response(200);self.send_header('Content-Type','application/json');self.send_header('Content-Length',str(len(raw)));self.end_headers()
                try:self.wfile.write(raw)
                except (BrokenPipeError,ConnectionResetError):pass
        server=ThreadingHTTPServer(('127.0.0.1',port),Handler)
        threading.Thread(target=server.serve_forever,daemon=True).start()
        return server


def tick_window_declaration(fixture):
    """Declare parser coverage without changing the captured RPC/native inputs.

    declaration_sha256 is a harness commitment, distinct from production's
    pool-ID-only scope_hash. It binds every selected array to its original bytes.
    """
    m=fixture.manifest;pool=m['pool'];captured=sorted(m.get('extras',{}).get('tick_arrays',[]),key=lambda t:t['start'])
    require(len({t['start'] for t in captured})==len(captured),'duplicate-captured-array-start')
    require(len({t['address'] for t in captured})==len(captured),'duplicate-captured-array-address')
    rows=[]
    for t in captured:
        require(t['address'] in fixture.accounts and fixture.accounts[t['address']] is not None,'window-array-not-captured:'+t['address'])
        rows.append({'start':t['start'],'address':t['address'],'raw_account_sha256':digest(account_bytes(fixture.accounts[t['address']]))})
    selected=rows;span=None
    if fixture.label!='cpmm':
        span=(60 if fixture.label=='clmm' else 88)*pool['tick_spacing']
        require(span>0,'window-tick-spacing')
        require(all(t['start']%span==0 for t in rows),'window-array-alignment')
        if fixture.label=='clmm' and fixture.clmm_window=='nearest-three':
            center=pool['tick_current']//span*span;starts=[center-span,center,center+span]
            indexed={t['start']:t for t in rows}
            require(all(start in indexed for start in starts),'nearest-three-array-not-captured')
            selected=[indexed[start] for start in starts]
        require(bool(selected),'empty-tick-window')
        require(all(b['start']-a['start']==span for a,b in zip(selected,selected[1:])),'window-array-sequence')
        require(selected[0]['start']<=pool['tick_current']<selected[-1]['start']+span,'window-current-tick-outside')
    declaration={'requested_clmm_window':fixture.clmm_window,'effective_clmm_window':fixture.clmm_window if fixture.label=='clmm' else 'not-applicable',
        'pool':pool['pool'],'sourcebank_sha256':digest(fixture.sourcebank.read_bytes()),'sourcebank_slot':fixture.slot,
        'clock_account_sha256':digest(account_bytes(fixture.accounts[CLOCK])),
        'captured_arrays':rows,'declared_arrays':selected,'declared_starts':[t['start'] for t in selected],
        'tick_current':pool.get('tick_current'),'tick_spacing':pool.get('tick_spacing'),'array_span':span,
        'tick_lower_inclusive':selected[0]['start'] if selected else None,
        'tick_upper_exclusive':selected[-1]['start']+span if selected else None,
        'input_amount':1000000,'mint_in':WSOL,'mint_out':pool['mint_b'],
        'whole_pool_or_all_inputs_claimed':False,'window_outside_liquidity_assumed_zero':False,
        'native_replay_account_scope':'unchanged full captured superset'}
    declaration['declaration_sha256']=digest(json.dumps(declaration,sort_keys=True,separators=(',',':')).encode())
    return declaration


def observe_window_scope(fixture,baseline,quote=None):
    """Bind the declared arrays to the actual normal Parser baseline and quote."""
    declaration=tick_window_declaration(fixture);pools=baseline['pools']
    require(len(pools)==1 and pools[0]['pool_id']==declaration['pool'],'window-normal-pool-scope')
    expected=digest(json.dumps([declaration['pool']],separators=(',',':')).encode())
    scope=baseline['reference']['scope_hash'];require(scope==expected,'window-production-pool-scope-hash')
    if fixture.label!='cpmm':
        decoded=pools[0][SPECS[fixture.label][2]]['tick_arrays']
        actual=[{'start':t['start_tick_index'],'address':t['address']} for t in decoded]
        wanted=[{k:t[k] for k in ('start','address')} for t in declaration['declared_arrays']]
        require(actual==wanted,'window-normal-decoded-array-scope')
    quote_scope=None if quote is None else quote['snapshotRef']['scope_hash']
    if quote is not None:require(quote_scope==scope,'window-normal-quote-scope')
    return {'declaration_sha256':declaration['declaration_sha256'],'parser_scope_hash':scope,'quote_scope_hash':quote_scope,
        'production_scope_hash_includes_tick_window':False,'production_scope_hash_basis':'SHA256(canonical JSON of sorted pool IDs)',
        'parser_baseline_body_sha256':digest(json.dumps(baseline,sort_keys=True,separators=(',',':')).encode()),
        'decoded_arrays_match_declaration':True}


def pool_config(fixture):
    m=fixture.manifest;p=m['pool'];row=m['catalog_row'];label=fixture.label;_,profile,key=SPECS[label]
    # These are normal protocol decoders, selected for the actual captured
    # initialized reward state. The raw accounts are never converted to a typed
    # fixture pool or rewritten to fit a profile.
    accounts=[p['pool'],p['vault_a'],p['vault_b'],p['config'],p['mint_a'],p['mint_b']]
    if label=='clmm':accounts.append(p['observation'])
    raw=account_bytes(fixture.accounts[p['pool']]);config=account_bytes(fixture.accounts[p['config']])
    fee=int.from_bytes(config[12:20],'little') if label=='cpmm' else int.from_bytes(config[47:51],'little') if label=='clmm' else int.from_bytes(raw[45:47],'little')
    token=fixture.token_identity()
    identity={'expected_programdata':row['upgrade']['programdata'],'expected_program_code_sha256':row['runtime']['sha256'],'expected_deployed_slot':row['upgrade']['slot'],'expected_token_program_code_sha256':token['payload_sha256']}
    if token['programdata'] is not None:
        identity['token_program_upgrade']={'programdata':token['programdata'],'deployed_slot':token['upgrade_slot']}
    if label!='cpmm':
        identity.update(tick_array_starts=tick_window_declaration(fixture)['declared_starts'],max_steps=8192)
    return {'id':p['pool'],'protocol':profile,'token0':p['mint_a'],'token1':p['mint_b'],'fee_numerator':fee,'fee_denominator':1000000,'program_id':row['address'],'accounts':accounts,key:identity}


def make_configs(fixture,directory,rpc_url,parser_port,router_port,token):
    require(rpc_url.startswith('http://127.0.0.1:'),'loopback-only-fixture')
    from configuration import config_for
    chain={'key':CHAIN,'owner_service':'chain-parser-rust','assignment_version':1,'genesis_hash':GENESIS,'rpc_urls':[rpc_url],'pools':[pool_config(fixture)],'confirmation_tag':'finalized','auto_reorg_window':128}
    configs={}
    for alias,name,port in [('parser','chain-parser-rust',parser_port),('router','dex-router',router_port)]:
        c=config_for(name,directory/alias,token,port);c['service']['mode']='rpc';c['chains']=[copy.deepcopy(chain)]
        c['startup']['max_baseline_age_ms']=FRESHNESS;c['logging']['level']='info'
        c['journal'].update(transport='http-development',source_urls=[f'http://127.0.0.1:{parser_port}'],poll_interval_ms=1000,request_timeout_ms=15000,max_payload_bytes=32*1024*1024)
        c['control'].update(backend='disabled',dsn_file='',instance_id='r01-solana-'+alias)
        if alias=='router':
            c['journal']['source_auth_tokens']={f'http://127.0.0.1:{parser_port}':token}
            c['chains'][0].update(rpc_urls=[],pools=[])
            c['routing'].update(max_hops=1,max_splits=1,max_state_age_ms=FRESHNESS,quote_ttl_ms=60000,route_activation_cost='0')
            c['api'].update(quote_timeout_ms=10000,quote_concurrency=1)
        configs[alias]=c
    return configs


def decimal_amount(value,name):
    require(isinstance(value,str) and re.fullmatch(r'0|[1-9][0-9]*',value) is not None,'invalid-quote-'+name)
    return int(value)


def compare_quote(quote,reference):
    require(quote.get('consumedIn')=='1000000' and quote.get('refund')=='0','normal-quote-input-not-fully-consumed')
    for key in ('pool_gross','recipient_token_delta'):require(type(reference[key]) is int and reference[key]>=0,'reference-quantity-integer:'+key)
    gross=decimal_amount(quote.get('grossBuyAmount'),'pool-gross')
    net_value=quote.get('netBuyAmountEstimate')
    fee_net=None if net_value is None else decimal_amount(net_value,'after-chain-fees')
    return {'r01_pool_gross_verified':gross==reference['pool_gross'],'quantities':{
        'pool_gross':{'quantity_basis':'pool_gross','router_value':gross,'reference_value':reference['pool_gross'],'delta':gross-reference['pool_gross'],'verified':gross==reference['pool_gross']},
        'recipient_token_delta':{'quantity_basis':'recipient_token_delta','reference_value':reference['recipient_token_delta'],'router_selected_replay_value':None,'verified':False,'reason':'requires selected plan quantity replay; not inferred from gross or gas-net'},
        'after_chain_fees':{'quantity_basis':'after_chain_fees_unavailable' if fee_net is None else 'after_chain_fees_reported_uncompared','value':fee_net,'reference_value':None,'verified':False,'reason':quote.get('costs',{}).get('unavailableReason','outside_R01_no_chain_fee_reference')}},'after_chain_fees_used_to_grant_R01_qualification':False}


def plan_hash(plan):
    fields={k:plan[k] for k in ['context','snapshot','allocation','min_buy_amount','economic_policy_hash']}
    canonical=json.dumps(fields,sort_keys=True,separators=(',',':'),ensure_ascii=False)
    for old,new in [('<','\\u003c'),('>','\\u003e'),('&','\\u0026'),('\u2028','\\u2028'),('\u2029','\\u2029')]:canonical=canonical.replace(old,new)
    return digest(canonical.encode())


def selected_plan_quantities(quote,evidence,fixture,native):
    require(evidence.get('expired') is False and evidence.get('snapshot_valid') is True,'normal-plan-expired-or-invalid')
    require(evidence.get('quote')==quote,'normal-plan-quote-substitution')
    plan=evidence['execution_plan'];require(plan_hash(plan)==plan['plan_hash']==quote['planHash'],'normal-plan-hash-mismatch')
    context=plan['context'];allocation=plan['allocation'];pool=fixture.manifest['pool'];row=fixture.manifest['catalog_row']
    expected_context={'chain_key':CHAIN,'caller':native['payer'],'recipient':native['recipient'],'sell_token':WSOL,'buy_token':pool['mint_b']}
    require(all(context.get(k)==v for k,v in expected_context.items()),'normal-plan-context-mismatch')
    require(plan['snapshot']==quote['snapshotRef'],'normal-plan-snapshot-substitution')
    block=plan['snapshot']['block'];require(block.get('chain_key')==CHAIN and block.get('number_or_slot')==fixture.slot and block.get('hash')==fixture.block['blockhash'],'normal-plan-fixed-bank-binding')
    require(allocation['sell_amount']=='1000000' and allocation['refund']=='0' and quote['consumedIn']=='1000000' and quote['refund']=='0','normal-plan-input-consumption')
    routes=allocation['routes'];require(len(routes)==1 and quote['routes']==routes,'normal-plan-single-route-required')
    route=routes[0];path=route['path'];require(len(path['edges'])==1 and path['tokens']==[WSOL,pool['mint_b']],'normal-plan-single-hop-required')
    edge=path['edges'][0];require(edge['pool_id']==pool['pool'] and edge['zero_for_one'] is True and edge['to']==pool['mint_b'],'normal-plan-pool-direction')
    if edge.get('from_token') is not None:require(edge['from_token']==WSOL,'normal-plan-edge-input')
    if edge.get('protocol'):require(edge['protocol']==SPECS[fixture.label][1],'normal-plan-protocol')
    gross=decimal_amount(quote['grossBuyAmount'],'pool-gross')
    require(route['amount_in']==path['amount_in']==allocation['sell_amount'] and route['amount_out']==path['amount_out']==allocation['gross_buy_amount']==quote['grossBuyAmount'],'normal-plan-route-amounts')
    bps=context['slippage_bps'];require(type(bps) is int and bps==50,'normal-plan-bounded-slippage')
    minimum=decimal_amount(plan['min_buy_amount'],'min-output');require(minimum==gross*(10000-bps)//10000 and quote['minBuyAmount']==plan['min_buy_amount'],'normal-plan-exact-slippage-floor')
    return {'plan_hash':plan['plan_hash'],'pool':pool['pool'],'program':row['address'],'programdata':row['upgrade']['programdata'],'upgrade_slot':row['upgrade']['slot'],'full_payload_sha256':row['runtime']['sha256'],'sourcebank_slot':fixture.slot,'blockhash':fixture.block['blockhash'],'caller':context['caller'],'recipient':context['recipient'],'mint_in':WSOL,'mint_out':pool['mint_b'],'zero_for_one':True,'amount_in':int(route['amount_in']),'minimum_amount_out':minimum,'router_pool_gross':gross,'optimizer_after_route_activation_cost':{'quantity_basis':'after_configured_route_activation_cost','value':allocation['net_buy_amount'],'used_as_chain_fee_net':False},'production_instruction_provided':False,'quantity_basis':['pool_gross','recipient_token_delta','after_chain_fees_unavailable']}


def execution_adapter_trigger():
    anchors=[('dex-router/crates/quote-api/src/lib.rs','xone_simulation::verify(&cached.plan).map_err(failure)?;'),('dex-router/crates/simulation/src/lib.rs','Err(err("INDEPENDENT_SIMULATION_UNAVAILABLE"))'),('dex-router/crates/execution-plan/src/lib.rs','pub struct Plan {')]
    findings=[]
    for path,anchor in anchors:
        raw=(ROOT/path).read_bytes();lines=raw.decode().splitlines();matching=[i+1 for i,line in enumerate(lines) if anchor in line]
        findings.append({'path':path,'sha256':digest(raw),'anchor':anchor,'observed_line_numbers':matching})
    return {'production_solana_instruction_available':False,'source_findings':findings,'scope':'observed Plan/build/verify source branches plus actual HTTP build response; no new production adapter implemented'}


def router_scope_ready(response,reference):
    return response.get('http_status')==200 and response.get('body',{}).get('ready') is True and response['body'].get('snapshot_ref')==reference


def free_port():
    with socket.socket() as s:s.bind(('127.0.0.1',0));return s.getsockname()[1]


def read_http(url,token,body=None,timeout=3):
    req=urllib.request.Request(url,data=None if body is None else json.dumps(body).encode(),headers={'Authorization':'Bearer '+token,'Content-Type':'application/json'})
    try:
        with urllib.request.urlopen(req,timeout=timeout) as response:raw=response.read(32*1024*1024);code=response.status
    except urllib.error.HTTPError as error:raw=error.read(32*1024*1024);code=error.code
    except Exception as error:return {'http_status':None,'error':str(error)}
    try:value=json.loads(raw)
    except ValueError:value={'raw_text':raw.decode(errors='replace')}
    return {'http_status':code,'body':value,'body_sha256':digest(raw),'raw_body_base64':base64.b64encode(raw).decode()}


def pin_binary(source,cache):
    raw=source.read_bytes();h=digest(raw);require(source.read_bytes()==raw,'normal-binary-changed-while-copying')
    target=cache/(source.name+'-'+h);cache.mkdir(parents=True,exist_ok=True)
    if not target.exists():target.write_bytes(raw);target.chmod(0o755)
    require(digest(target.read_bytes())==h,'normal-binary-copy-hash')
    return source,{'source':str(source),'archived_identical_bytes':str(target),'path':str(source),'sha256':h,'bytes':len(raw),'source_mtime_ns':source.stat().st_mtime_ns,'mode':'normal-production-binary; no fixture-parser'}


def run_case(label,output,parser_bin,router_bin,timeout=45,*,clmm_window='full-captured'):
    require(not output.exists(),'process-case-output-exists');output.mkdir(parents=True)
    fixture=FixedBankRPC(label,clmm_window=clmm_window);native_path=BASE/'solana-rebuild'/(label+'-native-execution.json');native=json.loads(native_path.read_text());native_check=verify_saved_execution(fixture.manifest,native)
    result={'schema_version':3,'tick_window':tick_window_declaration(fixture),'case':label,'authority':'fixed public accounts/Clock/blockhash served read-only to normal Rust parser and normal router over existing http-development journal','sourcebank':str(fixture.sourcebank.relative_to(ROOT)),'sourcebank_sha256':digest(fixture.sourcebank.read_bytes()),'sourcebank_slot':fixture.slot,'clock':fixture.clock,'block':fixture.block,'token_identity':fixture.token_identity(),'native_replay':native_check,'historical_original_bank_verified':False,'current_mainnet_freshness_claimed':False,'normal_process_quote_observed':False,'r01_pool_gross_verified':False,'selected_plan_recipient_token_delta_verified':False,'production_router_instruction_execution_verified':False}
    server=fixture.serve();processes=[];streams=[]
    try:
        token=secrets.token_urlsafe(32);parser_port=free_port();router_port=free_port();configs=make_configs(fixture,output,f'http://127.0.0.1:{server.server_port}',parser_port,router_port,token)
        result['freshness_policy']={'startup_max_baseline_age_ms':FRESHNESS,'router_max_state_age_ms':FRESHNESS,'Clock_rewritten':False,'scope':'explicit offline test configuration'}
        commands={};result['binaries']={}
        for alias,source in [('parser',parser_bin),('router',router_bin)]:
            binary,pin=pin_binary(source,output/'binaries');result['binaries'][alias]=pin
            cfg_path=output/(alias+'-config.json');cfg_path.write_text(json.dumps(configs[alias],indent=2)+'\n');cfg_path.chmod(0o600)
            command=[str(binary),'--config',str(cfg_path)];commands[alias]=command
            stream=(output/(alias+'.log')).open('w');streams.append(stream)
            processes.append(subprocess.Popen(command,cwd=ROOT/('chain-parser-rust' if alias=='parser' else 'dex-router'),stdout=stream,stderr=subprocess.STDOUT))
        result['commands']=commands
        parser_url=f'http://127.0.0.1:{parser_port}';router_url=f'http://127.0.0.1:{router_port}';deadline=time.monotonic()+timeout;last={}
        while time.monotonic()<deadline:
            baseline=read_http(parser_url+'/internal/chains/'+CHAIN+'/baseline',token)
            last['parser_baseline']=baseline
            if baseline.get('http_status')==200:
                body=baseline['body'];block=body.get('reference',{}).get('block',{})
                require(block.get('number_or_slot')==fixture.slot and block.get('hash')==fixture.block['blockhash'],'normal-parser-fixed-bank-binding')
                result['parser_baseline_observed']=True
                replica=read_http(router_url+'/internal/chains/'+CHAIN+'/status',token);last['router_status_before_quote']=replica
                if router_scope_ready(replica,body['reference']):break
            if any(p.poll() is not None for p in processes):break
            # Record a bounded number of actual normal errors before leaving an
            # unsupported deployment rejected. No fabricated positive baseline.
            error_text=(output/'parser.log').read_text(errors='replace')
            if 'TOKEN_PROGRAM_VERSION_UNSUPPORTED' in error_text and len(fixture.trace)>=8:break
            time.sleep(.25)
        result.update(last)
        if last.get('parser_baseline',{}).get('http_status')==200:
            result['window_scope_observation']=observe_window_scope(fixture,last['parser_baseline']['body'])
        p=fixture.manifest['pool'];request={'chainKey':CHAIN,'kind':'EXACT_IN','sellToken':WSOL,'buyToken':p['mint_b'],'sellAmount':'1000000','caller':native['payer'],'recipient':native['recipient'],'slippageBps':50,'verification':'FAST'}
        result['quote_request']=request
        quote=read_http(router_url+'/v1/quote',token,request,timeout=15);result['normal_quote_response']=quote
        if quote.get('http_status')==200:
            q=quote['body'];result['window_scope_observation']=observe_window_scope(fixture,result['parser_baseline']['body'],q);ref=q.get('snapshotRef',{});block=ref.get('block',{})
            require(block.get('number_or_slot')==fixture.slot and block.get('hash')==fixture.block['blockhash'],'normal-quote-fixed-bank-binding')
            require(q.get('quality',{}).get('candidateScope')==[p['pool']],'normal-quote-scope')
            reference_quantities=execution_quantities(fixture.manifest,native)
            comparison=compare_quote(q,reference_quantities);result['comparison']=comparison;result['normal_process_quote_observed']=True
            quote_id=q['quoteId'];evidence=read_http(router_url+'/internal/quotes/'+quote_id+'/evidence',token)
            result['normal_router_plan_evidence']=evidence
            build=read_http(router_url+'/v1/quotes/'+quote_id+'/build',token,{},timeout=15)
            result['normal_router_build_response']=build;result['execution_adapter_trigger']=execution_adapter_trigger()
            require(evidence.get('http_status')==200,'normal-router-cached-plan-unavailable')
            selection=selected_plan_quantities(q,evidence['body'],fixture,native);result['selected_plan_quantities']=selection;result['normal_router_cached_plan_bound']=True
            result['r01_pool_gross_verified']=comparison['r01_pool_gross_verified']
            if build.get('http_status')==200:
                raise VerificationError('production-build-now-succeeded-preserve-instructions-for-direct-execution; test-ABI-fallback-not-authorized-for-this-response')
            projection=replay_selected_quantities(fixture.manifest,selection);result['selected_plan_quantity_replay']=projection
            if projection['selected_quantities_native_execution_succeeded']:
                selected=projection['quantities'];received=selected['recipient_token_delta'];expected=reference_quantities['recipient_token_delta']
                comparison['quantities']['recipient_token_delta'].update(router_selected_replay_value=received,verified=received==expected,reason='actual selected amount/minOut projected through existing test ABI and executed against original SBF/token programs')
                result['selected_plan_recipient_token_delta_verified']=received==expected
                comparison['selected_replay_pool_gross']={'quantity_basis':'pool_gross','value':selected['pool_gross'],'reference_value':reference_quantities['pool_gross'],'verified':selected['pool_gross']==reference_quantities['pool_gross']}
            result['after_chain_fees']={'quantity_basis':'after_chain_fees_unavailable','value':q.get('netBuyAmountEstimate'),'reason':q.get('costs',{}).get('unavailableReason'),'compared_with_token_delta':False}
            result['production_router_instruction_execution_verified']=False
        for alias,url in [('parser',parser_url),('router',router_url)]:result[alias+'_status']=read_http(url+'/internal/chains/'+CHAIN+'/status',token)
    except Exception as error:result.update(error_type=type(error).__name__,error=str(error))
    finally:
        for p in processes:
            if p.poll() is None:p.terminate()
        for p in processes:
            try:p.wait(timeout=10)
            except subprocess.TimeoutExpired:p.kill();p.wait()
        for f in streams:f.close()
        server.shutdown();server.server_close()
        result['rpc_trace']=fixture.trace
        result['uncaptured_account_requests']=[r for r in fixture.trace if r['error'] and 'uncaptured-account' in r['error']['message']]
        result['process_exit_codes']=[p.returncode for p in processes]
        result['binary_source_changed_during_case']={alias:digest(Path(record['source']).read_bytes())!=record['sha256'] for alias,record in result.get('binaries',{}).items()}
        if any(result['binary_source_changed_during_case'].values()):result.update(r01_pool_gross_verified=False,selected_plan_recipient_token_delta_verified=False,production_router_instruction_execution_verified=False,error='normal binary changed during case')
        result['original_account_bytes_and_clock_unchanged']=all(fixture.accounts[k]==v for k,v in validate_capture(fixture.manifest)[0].items())
        for alias in ['parser','router']:
            path=output/(alias+'.log')
            if path.exists():result[alias+'_log_sha256']=digest(path.read_bytes())
        (output/'result.json').write_text(json.dumps(result,indent=2)+'\n')
    return result


def recorded_http_body(record):
    require(record.get('http_status')==200,'recorded-successful-normal-HTTP-required')
    raw=base64.b64decode(record['raw_body_base64'],validate=True)
    require(digest(raw)==record['body_sha256'] and json.loads(raw)==record['body'],'recorded-normal-HTTP-body-integrity')
    return record['body']


def verify_recorded_case(record):
    """Recheck a recorded normal Plan and freshly replay its selected quantities.

    This does not start the Parser/Router again or assert current quote validity.
    Full process reexecution uses run; this command verifies the retained record
    against the unchanged public input and newly executed original native SBF.
    """
    fixture=FixedBankRPC(record['case'],clmm_window=record.get('tick_window',{}).get('requested_clmm_window','full-captured'))
    if 'tick_window' in record:require(record['tick_window']==tick_window_declaration(fixture),'recorded-window-declaration-substitution')
    require(record['sourcebank_sha256']==digest(fixture.sourcebank.read_bytes()),'recorded-sourcebank-hash')
    require(record['clock']==fixture.clock and record['block']==fixture.block,'recorded-fixed-bank-substitution')
    require(record.get('original_account_bytes_and_clock_unchanged') is True,'recorded-input-mutation')
    require(not any(record.get('binary_source_changed_during_case',{}).values()),'recorded-binary-changed-during-case')
    for pin in record['binaries'].values():
        archive=Path(pin['archived_identical_bytes']).resolve();require(archive.is_relative_to(ROOT/'integration/artifacts/local'),'recorded-normal-binary-cache-scope')
        require(digest(archive.read_bytes())==pin['sha256'],'recorded-normal-binary-pin')
    quote=recorded_http_body(record['normal_quote_response']);evidence=recorded_http_body(record['normal_router_plan_evidence'])
    window_observation=observe_window_scope(fixture,recorded_http_body(record['parser_baseline']),quote)
    if 'window_scope_observation' in record:require(record['window_scope_observation']==window_observation,'recorded-window-scope-substitution')
    native=json.loads((BASE/'solana-rebuild'/(fixture.label+'-native-execution.json')).read_text())
    verify_saved_execution(fixture.manifest,native)
    reference=execution_quantities(fixture.manifest,native);comparison=compare_quote(quote,reference)
    selection=selected_plan_quantities(quote,evidence,fixture,native)
    projection=replay_selected_quantities(fixture.manifest,selection)
    require(projection['selected_quantities_native_execution_succeeded'],'recorded-selected-quantity-native-failure')
    require(projection['execution']==record['selected_plan_quantity_replay']['execution'],'recorded-selected-instruction-or-state-mismatch')
    received=projection['quantities']['recipient_token_delta'];same=received==reference['recipient_token_delta']
    require(comparison['r01_pool_gross_verified'] and same,'recorded-normal-plan-math-or-recipient-difference')
    return {'schema_version':3,'tick_window':tick_window_declaration(fixture),'window_scope_observation':window_observation,'r01_pool_gross_verified':True,'selected_plan_recipient_token_delta_verified':True,'quantities':{'pool_gross':{'quantity_basis':'pool_gross','value':comparison['quantities']['pool_gross']['router_value'],'reference_value':reference['pool_gross']},'recipient_token_delta':{'quantity_basis':'recipient_token_delta','value':received,'reference_value':reference['recipient_token_delta']}},'after_chain_fees':comparison['quantities']['after_chain_fees'],'normal_process_reexecuted':False,'fresh_original_SBF_execution':True,'recorded_plan_replay_only':True,'quote_current_validity_claimed':False,'production_router_instruction_execution_verified':False,'historical_original_bank_verified':False}


def same_binary_freeze(records):
    if not records:return False
    fingerprints=[]
    for record in records:
        bins=record.get('binaries',{});changed=record.get('binary_source_changed_during_case',{})
        if set(bins)!={'parser','router'} or set(changed)!={'parser','router'} or any(value is not False for value in changed.values()):return False
        pair=tuple(bins[alias].get('sha256') for alias in ('parser','router'))
        if not all(isinstance(value,str) and re.fullmatch(r'[0-9a-f]{64}',value) for value in pair):return False
        fingerprints.append(pair)
    return len(set(fingerprints))==1


def main():
    parser=argparse.ArgumentParser(description=__doc__);subs=parser.add_subparsers(dest='command',required=True)
    prepare=subs.add_parser('prepare');prepare.add_argument('--output',type=Path,required=True)
    run=subs.add_parser('run');run.add_argument('--output',type=Path,required=True);run.add_argument('--labels',nargs='+',choices=list(SPECS),default=list(SPECS));run.add_argument('--parser-bin',type=Path,default=ROOT/'chain-parser-rust/target/debug/chain-parser-rust');run.add_argument('--router-bin',type=Path,default=ROOT/'dex-router/target/debug/dex-router');run.add_argument('--timeout',type=int,default=45)
    verify=subs.add_parser('verify-recorded');verify.add_argument('--result',type=Path,required=True)
    for command in (prepare,run):command.add_argument('--clmm-window',choices=CLMM_WINDOWS,default='full-captured')
    args=parser.parse_args()
    if args.command=='verify-recorded':print(json.dumps(verify_recorded_case(json.loads(args.result.read_text())),indent=2));return
    output=args.output.resolve();require(output.is_relative_to(ROOT/'integration/artifacts/local'),'process-output-cache-scope');output.mkdir(parents=True,exist_ok=True)
    if args.command=='prepare':
        for label in SPECS:
            fixture=FixedBankRPC(label,clmm_window=args.clmm_window);configs=make_configs(fixture,output/label,'http://127.0.0.1:19000',19001,19002,'r01-offline-process-fixture-local-token')
            (output/(label+'-inputs.json')).write_text(json.dumps({'tick_window':tick_window_declaration(fixture),'sourcebank':str(fixture.sourcebank),'fixed_slot':fixture.slot,'clock':fixture.clock,'block':fixture.block,'token_identity':fixture.token_identity(),'pool_config':pool_config(fixture),'normal_configs':configs},indent=2)+'\n')
        print('prepared fixed sourcebank configurations');return
    results=[]
    for label in args.labels:
        r=run_case(label,output/label,args.parser_bin.resolve(),args.router_bin.resolve(),args.timeout,clmm_window=args.clmm_window);results.append(r)
        print(json.dumps({'case':label,'normal_quote_observed':r['normal_process_quote_observed'],'r01_pool_gross_verified':r['r01_pool_gross_verified'],'selected_plan_recipient_token_delta_verified':r['selected_plan_recipient_token_delta_verified'],'error':r.get('error')}),flush=True)
    summary={'schema_version':3,'clmm_window':args.clmm_window,'requested_labels':args.labels,'cases':[{k:r.get(k) for k in ['case','normal_process_quote_observed','r01_pool_gross_verified','selected_plan_recipient_token_delta_verified','production_router_instruction_execution_verified','error']} for r in results],'all_r01_pool_gross_verified':all(r['r01_pool_gross_verified'] for r in results),'all_selected_plan_recipient_token_deltas_verified':all(r['selected_plan_recipient_token_delta_verified'] for r in results),'after_chain_fees':{'quantity_basis':'after_chain_fees_unavailable','used_to_grant_R01_qualification':False},'production_router_solana_transaction_adapter_claimed':False,'historical_original_bank_verified':False,'source_rebuild_match_claimed':False}
    summary['same_normal_binary_freeze']=same_binary_freeze(results)
    if not summary['same_normal_binary_freeze']:summary.update(all_r01_pool_gross_verified=False,all_selected_plan_recipient_token_deltas_verified=False)
    (output/'summary.json').write_text(json.dumps(summary,indent=2)+'\n')
    if not (summary['all_r01_pool_gross_verified'] and summary['all_selected_plan_recipient_token_deltas_verified']):raise SystemExit(1)

if __name__=='__main__':main()
