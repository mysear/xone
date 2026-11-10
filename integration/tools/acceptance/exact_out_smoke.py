#!/usr/bin/env python3
"""Original ExactOut banks -> normal Go/PG/Kafka/two Routers -> original swaps.

RPC headers/empty receipts are controlled structural sources. Getter bytes and
code bodies originate in the reference. No public history/consensus claim.
"""
from pathlib import Path
import copy, json, secrets, subprocess, tempfile, time
import infrastructure as infra_module
from infrastructure import ROOT, Infrastructure, port
infra_module.BASE = Path('/tmp/xone-xo-infrastructure')
from configuration import config_for, save_config, service_command, sample_for
from kafka_smoke import wait
from reorg_smoke import await_condition
from smoke import http, wait_scope, quote
from lfj_smoke import Chain as HeaderChain, require, sha, canonical_hash
from weighted_evidence import runtime_fingerprint, write_result
from exact_out_reference import validate, validate_grid
OUT = ROOT / 'integration/artifacts/local'
REFERENCE = ROOT / 'integration/test/contracts/v2-exact-out.json'
TARGET = OUT / 'core-exact-out-normal-process-20261109.json'


class Chain(HeaderChain):
    def __init__(self, reference):
        self.reference = reference
        self.head, self.branch, self.bad, self.pinned, self.receipts = 100, 0, None, 0, 0
        self.timestamp = reference['cases'][0]['source']['header']['timestamp']
        self.banks = {}
        self.select(0)

    def select(self, index):
        self.index = index
        self.bank = copy.deepcopy(self.reference['cases'][0 if index == 'grid' else index]['source'])
        self.pools = copy.deepcopy(self.reference['cases'][0 if index == 'grid' else index]['pools'])
        self.banks[self.head] = self.bank

    def configs(self):
        return [dict(id=p['pool_id'], protocol=p['protocol'], token0=p['token0'], token1=p['token1'],
                     fee_numerator=p['fee_numerator'], fee_denominator=p['fee_denominator'],
                     program_id='', accounts=[], v2=dict(expected_factory=p['v2']['factory'],
                     expected_code_sha256=p['v2']['code_sha256'])) for p in self.pools]

    def dispatch(self, method, params):
        try:
            return self.dispatch_recorded(method, params)
        except (ValueError, KeyError, IndexError, TypeError) as error:
            print('ExactOut controlled RPC rejected:', method, str(error), flush=True)
            raise

    def dispatch_recorded(self, method, params):
        if method not in ['eth_call', 'eth_getCode', 'eth_getStorageAt']:
            return super().dispatch(method, params)
        at = 2 if method == 'eth_getStorageAt' else 1
        anchor = params[at]
        require(type(anchor) is dict and set(anchor) == {'blockHash', 'requireCanonical'}
                and anchor['requireCanonical'] is True, 'EXACT_OUT_UNBOUND_RPC')
        heights = [n for n in self.banks if self.block(n)['hash'] == anchor['blockHash']]
        require(len(heights) == 1, 'EXACT_OUT_UNRECORDED_RPC_BANK')
        bank = self.banks[heights[0]]
        self.pinned += 1
        if method == 'eth_getCode':
            for r in bank['reads']:
                if r['method'] == method and r['params'][0] == params[0]:
                    if self.bad and params[0] == self.pools[0]['v2']['factory']:
                        return '0x6000'
                    return self.reference['code_ledger'][r['result_code_sha256']]
        if method == 'eth_call':
            require(set(params[0]) == {'to', 'data'}, 'EXACT_OUT_RPC_OVERRIDES')
            for r in bank['reads']:
                if r['method'] == method and all(r['params'][0][k] == params[0][k] for k in ['to', 'data']):
                    return r['result']
        if method == 'eth_getStorageAt':
            for r in bank['reads']:
                if r['method'] == method and r['params'][0] == params[0] and int(r['params'][1], 16) == int(params[1], 16):
                    return r['result']
        raise ValueError('EXACT_OUT_UNRECORDED_ORIGINAL_READ:' + method)


def rebind(r):
    p, q, req = r['execution_plan'], r['quote'], r['request']
    p['plan_hash'] = canonical_hash({k: p[k] for k in ['engine', 'context', 'snapshot', 'allocation',
                                  'min_buy_amount', 'max_sell_amount', 'economic_policy_hash']})
    q['planHash'] = p['plan_hash']
    q['requestHash'] = canonical_hash(dict(request=req, view_id=q['quoteViewId'],
                                        policy=q['economicPolicyHash'], engine=p['engine']))
    q['quoteId'] = 'xo-' + canonical_hash(dict(request_hash=q['requestHash'], plan_hash=p['plan_hash'],
                                             expires=q['validity']['expiresAtMs']))


def guards(captures, projection, projector, tag):
    base = dict(schema_version=1, records=[copy.deepcopy(captures[0])])
    checks, receipts = [], []
    def reject(name, change, reason):
        value = copy.deepcopy(base)
        change(value['records'][0])
        inp, out = [OUT / f'core-xo-guard-{tag}-{name}-{suffix}.json' for suffix in ['input', 'output']]
        write_result(inp, value)
        result = subprocess.run([str(projector), str(inp), str(out)], capture_output=True, text=True, timeout=90)
        log=OUT / f'core-xo-guard-{tag}-{name}.log'
        log.write_text(result.stdout + result.stderr)
        receipts.append(dict(name=name,observed_exit_code=result.returncode,input=dict(path=str(inp.relative_to(ROOT)),sha256=sha(inp)),log=dict(path=str(log.relative_to(ROOT)),sha256=sha(log))))
        require(result.returncode != 0 and reason in result.stdout + result.stderr and not out.exists(), 'EXACT_OUT_MODEL_GUARD:' + name)
        checks.append(name)
    reject('plan-hash', lambda r: r['execution_plan'].__setitem__('plan_hash', '00' * 32), 'PLAN_HASH_MISMATCH')
    reject('public-output', lambda r: r['quote'].__setitem__('minBuyAmount', '2'), 'EXACT_OUT_PROJECT_PUBLIC_ALLOCATION')
    reject('source-clock', lambda r: r['snapshot']['pools'][0]['v2']['swap'].__setitem__('bank_unix_timestamp', 1), 'V2_SWAP_SNAPSHOT_BANK')
    def changed_output(r):
        r['execution_plan']['allocation']['routes'][0]['amounts'][0] = '2'
        rebind(r)
    reject('rehashed-hop-amount', changed_output, 'EXACT_OUT_INVALID_PLAN')
    reject('public-refund', lambda r: r['quote'].__setitem__('refund', '1'), 'EXACT_OUT_PROJECT_PUBLIC_ALLOCATION')
    inp = OUT/f'core-xo-guard-{tag}-native-input.json'
    write_result(inp, base)
    baseline = copy.deepcopy(projection)
    baseline['records'] = baseline['records'][:1]
    baseline['input_sha256'] = sha(inp)
    for name, mutation, reason in [
        ('native-poststate', lambda m: m['records'][0]['steps'][0]['after'][0].__setitem__('reserve0','1'), 'EXACT_OUT_REPLAY_COMPLETE_POSTSTATE'),
        ('native-route-amount', lambda m: m['records'][0]['steps'][0]['route']['amounts'].__setitem__(0,'2'), 'EXACT_OUT_REPLAY_ROUTE'),
    ]:
        value=copy.deepcopy(baseline); mutation(value)
        modeled,out=[OUT/f'core-xo-guard-{tag}-{name}-{suffix}.json' for suffix in ['model','native']]
        write_result(modeled,value)
        result=subprocess.run(['node',str(ROOT/'integration/tools/ci/v2-exact-out-plan-replay.cjs'),str(inp),str(modeled),str(out)],capture_output=True,text=True,timeout=90)
        log=OUT/f'core-xo-guard-{tag}-{name}.log'
        log.write_text(result.stdout+result.stderr)
        receipts.append(dict(name=name,observed_exit_code=result.returncode,input=dict(path=str(inp.relative_to(ROOT)),sha256=sha(inp)),model=dict(path=str(modeled.relative_to(ROOT)),sha256=sha(modeled)),log=dict(path=str(log.relative_to(ROOT)),sha256=sha(log))))
        require(result.returncode!=0 and reason in result.stdout+result.stderr and not out.exists(),'EXACT_OUT_NATIVE_GUARD:'+name)
        checks.append(name)
    return dict(passed=True, mutations=len(checks), checks=checks, receipts=receipts,
                authority='finite corruption controls; no general adversarial proof')


def run():
    require(__debug__, 'EXACT_OUT_ASSERTIONS_REQUIRED')
    validate(); validate_grid()
    reference = json.loads(REFERENCE.read_bytes())
    chain = Chain(reference)
    before = runtime_fingerprint()
    projector = ROOT / 'dex-router/target/debug/examples/exact-out-plan-projector'
    require(projector.is_file(), 'EXACT_OUT_PROJECTOR_REQUIRED')
    tools = [Path(__file__).resolve(), projector, ROOT / 'integration/tools/ci/v2-exact-out-plan-replay.cjs']
    tool_inputs = {str(p.relative_to(ROOT)): sha(p) for p in tools}
    reference_digest = sha(REFERENCE)
    servers = [chain.provider(), chain.provider()]
    processes, logs, captures, checks = {}, {}, [], []
    report = dict(schema_version=1, passed=False, runtime_fingerprint=before,
                  source_tree_sha256=before['source_tree_sha256'], tool_inputs=tool_inputs,
                  reference_sha256=reference_digest, all_requested_features_complete=False, production_ready=False,
                  authority='controlled structural RPC headers/receipts, original getter/code banks, normal Go/PG/Kafka/two Routers; original execution is separately checked')
    try:
        with Infrastructure() as infra, tempfile.TemporaryDirectory(prefix='xone-xo-') as folder:
            folder = Path(folder)
            token = secrets.token_urlsafe(32)
            actors = {'go': 'chain-parser-go', 'rust': 'chain-parser-rust', 'router1': 'dex-router', 'router2': 'dex-router'}
            ports = {a: port() for a in actors}
            paths = {}
            freshness = max(120000, int(time.time()*1000) - chain.timestamp*1000 + 600000)
            def base(a): return 'http://127.0.0.1:' + str(ports[a])
            for actor, name in actors.items():
                cfg = config_for(name, folder/actor, token, ports[actor])
                cfg['journal'].update(transport='kafka', source_urls=[base('go'), base('rust')], poll_interval_ms=100, request_timeout_ms=15000)
                cfg['journal']['kafka'].update(brokers=[infra.broker], topics={'evm:31337':'canonical.evm-31337.v1', 'solana:localnet':'canonical.solana-localnet.v1'}, replica_id=actor, transaction_timeout_ms=4000)
                cfg['control'].update(backend='disabled' if name=='dex-router' else 'postgres', dsn_file='' if name=='dex-router' else str(infra.dsnfile), instance_id=actor, lease_ttl_ms=30000, renewal_ms=1000, min_commit_remaining_ms=7000)
                cfg['startup']['max_baseline_age_ms'] = freshness
                if actor == 'go':
                    cfg['chains'][0].update(genesis_hash='0x'+'99'*32, confirmation_tag='latest', auto_reorg_window=4,
                        rpc_urls=[f'http://127.0.0.1:{s.server_port}' for s in servers], pools=chain.configs())
                elif actor == 'rust': sample_for('solana:localnet', folder/actor/'fixtures', chain.timestamp*1000)
                else:
                    cfg['routing'].update(max_hops=3, max_splits=3, split_parts=4, max_candidates=10, max_expansions=10000, max_state_age_ms=freshness, quote_ttl_ms=60000)
                    cfg['api']['quote_timeout_ms'] = 10000
                paths[actor] = save_config(folder/actor/'config', cfg)
            def start(a):
                log = OUT/f'core-xo-{a}-{secrets.token_hex(4)}.log'; logs[a] = log
                with log.open('w') as stream:
                    processes[a] = subprocess.Popen(service_command(actors[a], paths[a], folder/a/'fixtures' if a=='rust' else None), cwd=ROOT/actors[a], stdout=stream, stderr=subprocess.STDOUT)
            def stop(a):
                process = processes.pop(a, None)
                if process:
                    process.terminate()
                    try: process.wait(timeout=10)
                    except subprocess.TimeoutExpired: process.kill(); process.wait()
            def status(a): return http(base(a)+'/internal/chains/evm:31337/status', token)
            def capture(a, index):
                if index == 'grid':
                    req = dict(chainKey='evm:31337', kind='EXACT_OUT', sellToken=reference['grid']['paths'][0][0], buyToken=reference['grid']['paths'][0][1],
                               buyAmount=reference['grid']['target_output'], maxSellAmount=reference['grid']['max_input'])
                else:
                    c = reference['cases'][index]
                    req = dict(chainKey='evm:31337', kind='EXACT_OUT', sellToken=c['path'][0], buyToken=c['path'][-1], buyAmount=c['amount_out'], maxSellAmount=c['max_input'])
                req.update(caller=reference['context']['sender'], recipient=reference['context']['recipient'], slippageBps=50, verification='FAST', constraints={'maxHops':3})
                code, q = http(base(a)+'/v1/quote-exact-out', token, req)
                require(code==200, 'EXACT_OUT_NORMAL_QUOTE:'+str((code,q)))
                code, evidence = http(base(a)+'/internal/quotes/'+q['quoteId']+'/evidence', token)
                require(code==200 and evidence['snapshot_valid'] and not evidence['expired'], 'EXACT_OUT_NORMAL_EVIDENCE')
                code, snapshot = http(base('go')+'/internal/chains/evm:31337/baseline', token)
                require(code==200 and q['snapshotRef']==snapshot['reference'], 'EXACT_OUT_NORMAL_SOURCE_REF')
                expected = copy.deepcopy(chain.pools)
                for p in expected:
                    p['v2']['swap'].update(bank_block_number=snapshot['reference']['block']['number_or_slot'], bank_block_hash=snapshot['reference']['block']['hash'])
                parsed=sorted(snapshot['pools'],key=lambda p:p['pool_id'])
                if sorted(expected,key=lambda p:p['pool_id'])!=parsed:
                    write_result(OUT/'core-exact-out-source-difference.json',dict(case_index=index,expected=sorted(expected,key=lambda p:p['pool_id']),actual=parsed))
                    raise ValueError('EXACT_OUT_NORMAL_COMPLETE_PARSED_BANK')
                require(q['minBuyAmount']==req['buyAmount'] and q['refund']=='0' and q['verification']['level']=='INDICATIVE', 'EXACT_OUT_NORMAL_PUBLIC_CONTRACT')
                if index=='grid': require(q['requiredSellAmount']==reference['grid']['best_required_input'] and len(q['routes'])==3, 'EXACT_OUT_NORMAL_GRID_MINIMUM')
                captures.append(dict(case_index=index, snapshot=snapshot, request=req, quote=q, execution_plan=evidence['execution_plan']))
                return q
            try:
                for a in ['go','rust']: start(a); wait(base(a)+'/health/ready')
                for a in ['router1','router2']: start(a); wait(base(a)+'/health/live'); wait_scope(base(a),token,'evm:31337')
                # Both directions, small divisible rounding, input/output donations,
                # one/two/three hops, and ordered shared-prefix allocation.
                for index in [0,1,2,9,12,21,24,35,36,45,48,51,54,57,60,63,66,77,78,89,'grid']:
                    if index!=0:
                        chain.head += 1; chain.select(index)
                        for a in ['router1','router2']:
                            await_condition(lambda: status(a)[1].get('snapshot_ref',{}).get('block',{}).get('number_or_slot')==chain.head,'ExactOut source bank not published')
                    for a in ['router1','router2']: capture(a,index)
                checks.append('normal-six-complete-pools-and-two-replicas-original-targets-and-ordered-grid')
                old = captures[-1]['quote']
                chain.bad = 'factory-code'; chain.head += 1; chain.select('grid')
                for a in ['router1','router2']:
                    await_condition(lambda: status(a)[0]==503,'ExactOut invalid source not revoked')
                    quote(base(a),token,'solana:localnet','B')
                chain.bad = None; chain.head += 1; chain.select('grid')
                for a in ['router1','router2']:
                    await_condition(lambda: status(a)[1].get('snapshot_ref',{}).get('online_generation',0)>old['onlineGeneration'],'ExactOut source recovery missing')
                    capture(a,'grid')
                stop('router1'); start('router1'); wait(base('router1')+'/health/live'); wait_scope(base('router1'),token,'evm:31337'); capture('router1','grid')
                checks.append('factory-code-revocation-recovery-and-router-restart-preserve-other-chain')
            finally:
                for a in list(processes): stop(a)
    finally:
        for server in servers: server.shutdown(); server.server_close()
    require(runtime_fingerprint()==before and sha(REFERENCE)==reference_digest and all(sha(ROOT/p)==d for p,d in tool_inputs.items()), 'EXACT_OUT_SOURCE_OR_BINARY_CHANGED')
    tag = secrets.token_hex(6)
    inputs, model, native = [OUT/f'core-xo-{tag}-{s}.json' for s in ['api','model','native']]
    write_result(inputs, dict(schema_version=1, records=captures))
    for name, command in [('model',[str(projector),str(inputs),str(model)]), ('native',['node',str(tools[2]),str(inputs),str(model),str(native)])]:
        result = subprocess.run(command,capture_output=True,text=True,timeout=300)
        (OUT/f'core-xo-{tag}-{name}.log').write_text(result.stdout+result.stderr)
        require(result.returncode==0,'EXACT_OUT_ACTUAL_API_'+name.upper())
    actual=json.loads(native.read_bytes())
    require(actual['passed'] and len(actual['records'])==len(captures),'EXACT_OUT_NATIVE_COVERAGE')
    negative=guards(captures,json.loads(model.read_bytes()),projector,tag)
    require(runtime_fingerprint()==before and all(sha(ROOT/p)==d for p,d in tool_inputs.items()),'EXACT_OUT_FINAL_SOURCE_CHANGED')
    report.update(passed=True, checks=checks, actual_api_records=len(captures), original_transactions=actual['original_transactions'], negative_controls=negative,
        actual_api_input=dict(path=str(inputs.relative_to(ROOT)),sha256=sha(inputs)),model_projection=dict(path=str(model.relative_to(ROOT)),sha256=sha(model)),
        original_replay=dict(path=str(native.relative_to(ROOT)),sha256=sha(native)),pinned_reads=chain.pinned,receipt_reads=chain.receipts,
        offline_freshness=dict(original_clock_ms=chain.timestamp*1000,max_age_ms=freshness,timestamp_rewritten=False), measured_at_unix_ms=int(time.time()*1000))
    return report


if __name__=='__main__':
    try: result=run()
    except BaseException as error:
        write_result(TARGET,dict(schema_version=1,passed=False,error=str(error),error_type=type(error).__name__,all_requested_features_complete=False,production_ready=False));raise
    write_result(TARGET,result)
    print(json.dumps(dict(passed=True,api_records=result['actual_api_records'],original_transactions=result['original_transactions'],negative_controls=result['negative_controls']['mutations'])))
