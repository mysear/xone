"""Evidence mutation tests. Fixtures use repository raw facts, never service proof."""
import copy
import hashlib
import importlib
import json
import os
from pathlib import Path
import sys
import tempfile
import unittest
from unittest import mock

ROOT = Path(__file__).resolve().parents[4]
sys.path.insert(0, str(ROOT / 'integration/tools/acceptance'))
sys.dont_write_bytecode = True
import managed_v2_multi_smoke as smoke
import weighted_evidence as evidence


def digest(value):
    return hashlib.sha256(json.dumps(value, sort_keys=True, separators=(',', ':'), ensure_ascii=False).encode()).hexdigest()


def block(header):
    return dict(chain_key='evm:31337', number_or_slot=int(header['number'], 16), hash=header['hash'],
                parent_hash=header['parentHash'], timestamp_ms=int(header['timestamp'], 16)*1000)


def reference(header, generation=1):
    return dict(block=block(header), canonical_epoch=1, journal_seq=generation, schema_version=1,
                adapter_set_hash='a'*64, online_generation=generation, scope_hash='b'*64,
                writer_epoch=1, owner_service='chain-parser-go', owner_assignment_version=1)


def vm_state(bank, raw):
    keys = {(g['to'].lower(), g['data'].lower()) for p in bank['pools'] for g in p['getters']}
    return dict(header=bank['header'], header_sha256=digest(bank['header']),
                pools=[dict(scenario=p['scenario'], pool=p['pool']['pool_id'], getters=p['getters']) for p in bank['pools']],
                code_checks=[dict(address=i['address'], code_sha256=i['code_sha256']) for i in raw['identities']],
                unique_getter_calls=len(keys))


def tx_record(tx, phase, prefix):
    header = tx['execution_header']
    if 'ref' in header:
        header = prefix['objects'][header['ref']]['value']
    return dict(phase=phase, signed_transaction=tx['signed_transaction'], transaction=tx['transaction'],
                receipt=tx['receipt'], header=header, transaction_sha256=digest(tx['transaction']),
                receipt_sha256=digest(tx['receipt']), header_sha256=digest(header))


def fixture(scope, runtime, inputs, prefix):
    raw = json.loads(smoke.REFERENCE[scope].read_bytes())
    chain = smoke.ReplayChain(scope, raw, prefix)
    report = smoke.initial(scope)
    report.update(passed=True, state='PASSED', full_service_run=True, started_at_unix_ms=200,
                  completed_at_unix_ms=300, source_binding=runtime, source_binding_started=runtime,
                  source_binding_completed=runtime, input_hashes=inputs, input_hashes_started=inputs,
                  input_hashes_completed=inputs, inputs_unchanged=True, exact_reference_sha256=chain.reference_sha256,
                  original_bank_sha256=smoke.projection.BANK_SHA256, source_frame_count=len(chain.frames), fault_count=4,
                  native_case_inventory=[dict(name=c['name'], operation=c['operation'], source_bank=smoke.source_id(scope,c),
                      after_bank=c['expected_after_bank4pools' if scope=='catalog' else 'expected_after_bank2pools']) for c in chain.cases])
    report['checks'] = ['online-go-rpc-rust-debug-solana-and-two-router-processes',
        'all-native-full-banks-and-eight-two-replica-positive-plans',
        'real-api-selected-plans-independent-vm-full-getters-and-wallet-differences' if scope=='shared' else
        'four-native-swaps-and-four-30day-collections-complete-observed-after',
        'router2-restart-complete-native-bank-restore', 'verified-and-build-rejected-on-both-replicas']
    # These counters are synthetic mutation-test evidence, not service proof.
    # Populate all recorded original calls so the parser's strict required
    # subset is covered independently of the checker's read-selection helper.
    getter_counts={frame['id']+':'+address+':'+data:1 for frame in chain.frames for address,data in frame['getters']}
    code_counts={frame['id']+':'+address:1 for frame in chain.frames for address in chain.codes}
    report.update(getter_read_counts=getter_counts,code_read_counts=code_counts,
        getter_reads=sum(getter_counts.values()),code_reads=sum(code_counts.values()),receipt_reads=1,
        read_counts={'eth_chainId':1,'eth_getBlockByNumber':1,'eth_call':sum(getter_counts.values()),
                     'eth_getCode':sum(code_counts.values()),'eth_getTransactionReceipt':1})
    for frame in chain.frames:
        ref = reference(frame['header'])
        report['source_frames'].append(dict(source_bank=frame['id'], header=frame['header'], pools=frame['pools'],
            reference=ref, raw_getter_count=raw['banks'][frame['id']]['getter_count'], unique_getter_count=len(frame['getters'])))
        report['cpp_wire'].append(dict(source_bank=frame['id'], reference=ref, pool_ids=[p['pool_id'] for p in frame['pools']],
            managed_fields=39, protobuf_sha256='c'*64, allcomplete_bootstrap=True, bytes=123,
            replicas={a:dict(ready=True,snapshot_ref=ref,scope_pool_count=len(frame['pools'])) for a in ('router1','router2')}))
    first = next(c for c in chain.cases if c['operation']=='swap')

    def positive(actor, case, phase, ref=None):
        ident=smoke.source_id(scope,case);bank=raw['banks'][ident];request=smoke.native_request(case)
        ref=ref or reference(bank['header']); native=case['expected_native']; event=native['swap_event']
        pool=next(p['pool'] for p in bank['pools'] if p['pool']['multi_asset']['pool_id']==event['pool_id'])
        route=dict(amount_in=request['sellAmount'],amount_out=native['amount_out'],path=dict(amount_in=request['sellAmount'],
            amount_out=native['amount_out'], tokens=[request['sellToken'],request['buyToken']],edges=[dict(pool_id=pool['pool_id'],
                from_token=request['sellToken'],to=request['buyToken'],protocol=pool['protocol'],zero_for_one=True)]))
        q=dict(chainKey='evm:31337',sellAmount=request['sellAmount'],consumedIn=request['sellAmount'],refund='0',
            grossBuyAmount=native['amount_out'],snapshotRef=ref,scopeHash=ref['scope_hash'], routes=[route],
            quality=dict(candidateScope=[p['pool']['pool_id'] for p in bank['pools']],excludedPools={}),
            verification=dict(level='INDICATIVE',success=False))
        wire=copy.deepcopy(next(w for w in report['cpp_wire'] if w['source_bank']==ident))
        wire['reference']=ref
        for state in wire['replicas'].values():state['snapshot_ref']=ref
        report['cpp_wire'].append(wire)
        row=dict(actor=actor,case=case['name'],phase=phase,source_bank=ident,source_hash=bank['header']['hash'],
            request=request,quote=q,route_count=1,independent_gas_reference=False,passed=True)
        if scope=='catalog':
            row['native_expected']=native
        else:
            before=vm_state(bank,raw);after=vm_state(raw['banks'][case['expected_after_bank2pools']],raw)
            tx=tx_record(case['execution'],'selected-plan',prefix)
            n=int(bank['header']['number'],16)
            history=[tx_record(o['value'],'original-prefix',prefix) for o in prefix['objects'].values()
                     if o['kind']=='transaction' and int(o['value']['receipt']['blockNumber'],16)<=28]
            history.sort(key=lambda t:int(t['header']['number'],16))
            history += [tx_record(t,'shared-history',prefix) for t in raw['transactions'] if int(t['execution_header']['number'],16)<=n]
            wallets_before=dict(sender_sell=str(int(request['sellAmount'])+99),sender_buy='1',recipient_buy='1')
            wallets_after=dict(sender_sell='99',sender_buy=str(int(native['amount_out'])+1),recipient_buy=str(int(native['amount_out'])+1))
            step=dict(pool=pool['pool_id'],pool_id=event['pool_id'],asset_in=request['sellToken'],asset_out=request['buyToken'],
                sender=request['caller'],recipient=request['recipient'],actual_amount_in=request['sellAmount'],actual_amount_out=native['amount_out'],
                before=before,after=after,transaction=tx,wallets_before=wallets_before,wallets_after=wallets_after)
            replayroute=dict(index=0,quoted_amount_in=request['sellAmount'],quoted_amount_out=native['amount_out'],
                actual_amount_in=request['sellAmount'],actual_amount_out=native['amount_out'],before=before,after=after,
                steps=[step],wallets_before=wallets_before,wallets_after=wallets_after)
            row['independent_actual']=dict(schema_version=1,passed=True,vm_started=True,verified=False,production_ready=False,
                all_requested_features_complete=False,authority='independent-local-original-bytecode-selected-shared-vault-plan-reference',
                original_bank_sha256=smoke.projection.BANK_SHA256,shared_reference_sha256=chain.reference_sha256,
                source=dict(bank=ident,block=block(bank['header']),state=before,getter_records_checked=bank['getter_count'],unique_getter_calls=before['unique_getter_calls']),
                actual_amount_in=request['sellAmount'],actual_amount_out=native['amount_out'],selected_route_count=1,routes=[replayroute],
                transactions=history+[tx],native_transaction_count=len(history),prefix_transaction_count=28,plan_transaction_count=1,
                wallets_before=wallets_before,wallets_after=wallets_after,final_state=after)
        report['positive_plans'].append(copy.deepcopy(row))
    for c in chain.cases:
        if c['operation']=='swap':
            for actor in ('router1','router2'):positive(actor,c,'source')
    for i,fault in enumerate(smoke.faults_for(scope)):
        chain.select(1,fault); ref=reference(chain.frames[0]['header'],3*i+3)
        recovered=reference(chain.frames[1]['header'],3*i+2)
        w=copy.deepcopy(next(w for w in report['cpp_wire'] if w['source_bank']==chain.frames[1]['id']))
        w['reference']=recovered
        for state in w['replicas'].values():state['snapshot_ref']=recovered
        report['cpp_wire'].append(w)
        report['faults'].append(dict(evidence=copy.deepcopy(chain.fault_evidence), ancestor_reference=reference(chain.frames[0]['header'],3*i+1),
            recovered=True, recovered_reference=recovered, actual_fault_rpc_hits=1,
            quote_rejections=[dict(actor=a,status=503,response=dict(error='CHAIN_NOT_READY')) for a in ('router1','router2')],
            unrelated_solana=[dict(actor=a,request_chain='solana:localnet',quote=dict(chainKey='solana:localnet',consumedIn='10003',grossBuyAmount='9',
                verification=dict(level='INDICATIVE',success=False))) for a in ('router1','router2')]))
        report['checks'].append('actual-next-bank-withdrawal-and-new-generation:'+fault)
        positive('router1',first,'fault-recovery:'+fault,ref)
    for actor in ('router1','router2'):
        positive(actor,first,'restart')
        report['reverts'].append(dict(actor=actor,verified_status=422,verified_response=dict(error='INDEPENDENT_SIMULATION_UNAVAILABLE'),
                                     build_status=422,build_response=dict(error='BUILD_UNAVAILABLE')))
    if scope=='shared':add_capacity_fixtures(report,raw,prefix)
    return report


def add_capacity_fixtures(report,raw,prefix):
    """Synthetic two-route receipt shape using native source/getter/log records.

    These memory-only fixtures exercise binding, never assert a real VM run.
    """
    base=next(p for p in report['positive_plans'] if p['phase']=='source')
    native=[c for c in raw['cases'] if c['operation']=='swap'][:2]
    for actor in ('router1','router2'):
        row=copy.deepcopy(base);row.update(actor=actor,phase='capacity-split',route_count=2)
        row['request']['sellAmount']='5000000000000000000000'
        q=row['quote'];q.update(sellAmount=row['request']['sellAmount'],consumedIn=row['request']['sellAmount'])
        a=row['independent_actual'];history=a['transactions'][:-1]
        qroutes=[];aroutes=[];selected=[];cursor=a['source']['state'];outputs=[]
        initial=10**22;sold=bought=0
        for index,case in enumerate(native):
            input_='2500000000000000000000';output=case['expected_native']['amount_out'];outputs.append(int(output))
            bank=raw['banks'][case['expected_after_bank2pools']]
            pool=next(p['pool'] for p in bank['pools'] if p['pool']['multi_asset']['pool_id']==case['expected_native']['swap_event']['pool_id'])
            qr=copy.deepcopy(base['quote']['routes'][0]);qr.update(amount_in=input_,amount_out=output)
            qr['path'].update(amount_in=input_,amount_out=output);qr['path']['edges'][0]['pool_id']=pool['pool_id']
            qroutes.append(qr)
            tx=tx_record(copy.deepcopy(case['execution']),'selected-plan',prefix)
            data=tx['transaction']['input'];words=[data[i:i+64] for i in range(10,len(data),64)]
            words[11]=format(int(input_),'064x');tx['transaction']['input']=data[:10]+''.join(words)
            for log in tx['receipt']['logs']:
                if len(log['topics'])==4:log['data']='0x'+format(int(input_),'064x')+format(int(output),'064x')
                elif log['topics'][0].startswith('0xddf252ad'):
                    quantity=input_ if log['address']==row['request']['sellToken'] else output
                    log['data']='0x'+format(int(quantity),'064x')
            tx['transaction_sha256']=digest(tx['transaction']);tx['receipt_sha256']=digest(tx['receipt'])
            before=dict(sender_sell=str(initial-sold),sender_buy=str(1+bought),recipient_buy=str(1+bought))
            sold+=int(input_);bought+=int(output)
            after_wallets=dict(sender_sell=str(initial-sold),sender_buy=str(1+bought),recipient_buy=str(1+bought))
            after=vm_state(bank,raw)
            step=dict(pool=pool['pool_id'],pool_id=pool['multi_asset']['pool_id'],asset_in=row['request']['sellToken'],
                asset_out=row['request']['buyToken'],sender=row['request']['caller'],recipient=row['request']['recipient'],
                actual_amount_in=input_,actual_amount_out=output,before=cursor,after=after,transaction=tx,
                wallets_before=before,wallets_after=after_wallets)
            aroutes.append(dict(index=index,quoted_amount_in=input_,quoted_amount_out=output,actual_amount_in=input_,
                actual_amount_out=output,before=cursor,after=after,steps=[step],wallets_before=before,wallets_after=after_wallets))
            cursor=after;selected.append(tx)
        q.update(routes=qroutes,grossBuyAmount=str(sum(outputs)))
        a.update(routes=aroutes,selected_route_count=2,plan_transaction_count=2,transactions=history+selected,
            actual_amount_in=q['consumedIn'],actual_amount_out=q['grossBuyAmount'],final_state=cursor,
            wallets_before=aroutes[0]['wallets_before'],wallets_after=aroutes[-1]['wallets_after'])
        report['positive_plans'].append(row)


class MultiAcceptanceTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.prefix=json.loads((ROOT/'integration/test/contracts/managed-v2-aum.json').read_bytes())
        cls.runtime=dict(source_tree_sha256='d'*64,binaries={p:'e'*64 for p in evidence.BINARIES})
        cls.inputs={'integration/tools/acceptance/managed_v2_multi_acceptance.py':'f'*64}
        cls.reports={s:fixture(s,cls.runtime,cls.inputs,cls.prefix) for s in smoke.SCOPES}

    def gate(self):
        try: module=importlib.import_module('managed_v2_multi_acceptance')
        except ModuleNotFoundError:self.fail('missing new multi-scope validate_process function')
        self.assertTrue(callable(getattr(module,'validate_process',None)), 'missing new multi-scope validate_process function')
        return module

    def validate(self, report):
        self.gate().validate_process(report,self.runtime,self.inputs,gate_started_at_unix_ms=100,gate_completed_at_unix_ms=400)

    def test_complete_original_fact_fixtures_pass_both_scopes(self):
        for report in self.reports.values():self.validate(report)

    def test_native_amounts_missing_null_zero_and_noncanonical_are_rejected(self):
        for value in (None,'0','01',1,'-1'):
            for scope in smoke.SCOPES:
                r=copy.deepcopy(self.reports[scope]);r['positive_plans'][0]['quote']['grossBuyAmount']=value
                with self.subTest(scope=scope,value=value),self.assertRaises(ValueError):self.validate(r)
        r=copy.deepcopy(self.reports['catalog']);del r['positive_plans'][0]['native_expected']['amount_in']
        with self.assertRaises(ValueError):self.validate(r)

    def test_shared_reference_uses_current_checked_raw_bytes(self):
        gate=self.gate();r=copy.deepcopy(self.reports['shared'])
        data=smoke.REFERENCE['shared'].read_bytes()+b'\n'
        current=hashlib.sha256(data).hexdigest()
        r['exact_reference_sha256']=current
        for row in r['positive_plans']:row['independent_actual']['shared_reference_sha256']=current
        with tempfile.TemporaryDirectory() as tmp:
            path=Path(tmp)/'shared.json';path.write_bytes(data)
            with mock.patch.dict(smoke.REFERENCE,shared=path):self.validate(r)

    def test_shared_requires_two_capacity_split_api_and_vm_plans(self):
        r=copy.deepcopy(self.reports['shared'])
        r['positive_plans']=[p for p in r['positive_plans'] if p['phase']!='capacity-split']
        with self.assertRaises(ValueError):self.validate(r)

    def test_capacity_split_cannot_be_satisfied_by_native_or_single_route_receipts(self):
        for attack in ('actor','amount','routes','vmcount','txcount','oldphase'):
            r=copy.deepcopy(self.reports['shared']);p=next(p for p in r['positive_plans'] if p['phase']=='capacity-split')
            if attack=='actor':p['actor']='router2'
            elif attack=='amount':p['request']['sellAmount']='100000000000000000000'
            elif attack=='routes':p['quote']['routes'].pop();p['route_count']=1
            elif attack=='vmcount':p['independent_actual']['selected_route_count']=1
            elif attack=='txcount':p['independent_actual']['plan_transaction_count']=1
            else:p['phase']='source'
            with self.subTest(attack=attack),self.assertRaises(ValueError):self.validate(r)

    def test_source_actor_case_once_only_and_actual_source_bank_are_bound(self):
        for change in ('actor','case','source_bank','source_hash'):
            r=copy.deepcopy(self.reports['shared']);rows=r['positive_plans'];rows[1][change]=rows[2][change]
            if change=='source_hash': rows[1][change]='0x'+'0'*64
            with self.subTest(change=change),self.assertRaises(ValueError):self.validate(r)

    def test_runtime_raw_chronology_and_nonpromotion_tampering_rejected(self):
        mutations=[('full_service_run',False),('state','RUNNING'),('exact_reference_sha256','0'*64),
            ('original_bank_sha256','0'*64),('source_binding_completed',{}),('input_hashes_started',{}),
            ('started_at_unix_ms',99),('completed_at_unix_ms',401),('independent_gas_reference',True),('verified',True)]
        for k,v in mutations:
            r=copy.deepcopy(self.reports['shared']);r[k]=v
            with self.subTest(field=k),self.assertRaises(ValueError):self.validate(r)

    def test_duplicate_frame_wrong_pool_state_and_cpp_poison_rejected(self):
        for attack in ('frame','pool','digest','zero','null','replica','reference','fields'):
            r=copy.deepcopy(self.reports['catalog']);w=r['cpp_wire'][0]
            if attack=='frame':r['source_frames'][1]=copy.deepcopy(r['source_frames'][0])
            elif attack=='pool':r['source_frames'][0]['pools'][0]['reserve0']='1'
            elif attack=='digest':w['protobuf_sha256']=None
            elif attack=='zero':w['bytes']=0
            elif attack=='null':w['replicas']['router2']=None
            elif attack=='replica':w['replicas']['router2']['scope_pool_count']=1
            elif attack=='reference':w['reference']['block']['hash']='0x'+'0'*64
            elif attack=='fields':w['managed_fields']=38
            with self.subTest(attack=attack),self.assertRaises(ValueError):self.validate(r)

    def test_fault_rpc_recovery_generation_and_unrelated_solana_required(self):
        for attack in ('generation','ancestor','hits','rpc','solana','actor','missing','recovery'):
            r=copy.deepcopy(self.reports['shared']);f=r['faults'][0]
            if attack=='generation':f['recovered_reference']['online_generation']=f['ancestor_reference']['online_generation']
            elif attack=='ancestor':f['evidence']['source_hash']=f['ancestor_reference']['block']['hash']
            elif attack=='hits':f['actual_fault_rpc_hits']=0
            elif attack=='rpc':f['evidence']['before']=f['evidence']['after']
            elif attack=='solana':f['unrelated_solana'][0]['quote']['grossBuyAmount']='0'
            elif attack=='actor':f['quote_rejections'][1]['actor']='router1'
            elif attack=='missing':r['faults'].pop()
            elif attack=='recovery':f['recovered']=False
            with self.subTest(attack=attack),self.assertRaises(ValueError):self.validate(r)

    def test_verified_build_and_restart_cannot_be_flag_only(self):
        for attack in ('build','verified','actor','restart','check'):
            r=copy.deepcopy(self.reports['catalog'])
            if attack=='build':r['reverts'][0]['build_status']=200
            elif attack=='verified':r['reverts'][0]['verified_response']={}
            elif attack=='actor':r['reverts'][1]['actor']='router1'
            elif attack=='restart':r['positive_plans'].pop()
            else:r['checks'].pop()
            with self.subTest(attack=attack),self.assertRaises(ValueError):self.validate(r)

    def test_parser_read_contract_is_distinct_from_reference_execution_observations(self):
        from managed_v2_parser_reads import required_reads
        for scope,total_getters,total_codes in [('catalog',366,75),('shared',82,15)]:
            raw=json.loads(smoke.REFERENCE[scope].read_bytes());bank=raw['banks'][next(iter(raw['banks']))]
            needed,codes=required_reads(bank)
            self.assertEqual((len(needed),len(codes)),(total_getters,total_codes))
            self.assertLess(len(needed),len({(g['to'],g['data'])for p in bank['pools']for g in p['getters']}))
            for pool in bank['pools']:
                for g in pool['getters']:
                    if g['method']=='allowance':self.assertNotIn((g['to'],g['data']),needed)
        for scope in smoke.SCOPES:
            r=copy.deepcopy(self.reports[scope]);first=r['source_frames'][0]['source_bank']
            raw=json.loads(smoke.REFERENCE[scope].read_bytes());g=next(g for p in raw['banks'][first]['pools']for g in p['getters']if g['method']=='getActualSupply')
            del r['getter_read_counts'][first+':'+g['to']+':'+g['data']]
            r['getter_reads']=sum(r['getter_read_counts'].values());r['read_counts']['eth_call']=r['getter_reads']
            with self.assertRaises(ValueError):self.validate(r)

    def test_api_edge_tokens_cannot_disagree_with_request_path_or_actual_swap(self):
        for scope in smoke.SCOPES:
            for field in ('from_token','to'):
                r=copy.deepcopy(self.reports[scope])
                for plan in r['positive_plans']:
                    for route in plan['quote']['routes']:
                        route['path']['edges'][0][field]='0x'+'0'*40
                with self.subTest(scope=scope,field=field),self.assertRaises(ValueError):self.validate(r)

    def test_each_native_bank_requires_actual_parser_getter_and_code_hits(self):
        for scope in smoke.SCOPES:
            for attack in ('missing-source','zero-source','all-empty','bad-sum','bool-count','code-source'):
                r=copy.deepcopy(self.reports[scope]);first=r['source_frames'][0]['source_bank']
                if attack in ('missing-source','zero-source','code-source'):
                    key='code_read_counts' if attack=='code-source' else 'getter_read_counts'
                    for name in list(r.get(key,{})):
                        if name.startswith(first+':'):
                            if attack=='zero-source':r[key][name]=0
                            else:del r[key][name]
                    total=sum(r.get(key,{}).values());r['code_reads' if key=='code_read_counts' else 'getter_reads']=total
                    r.setdefault('read_counts',{})['eth_getCode' if key=='code_read_counts' else 'eth_call']=total
                elif attack=='all-empty':r['getter_read_counts']={};r['getter_reads']=0;r.setdefault('read_counts',{})['eth_call']=0
                elif attack=='bad-sum':r['getter_reads']=r.get('getter_reads',0)+1
                else:
                    key=next(iter(r.get('getter_read_counts',{})),first+':forged');r.setdefault('getter_read_counts',{})[key]=True
                with self.subTest(scope=scope,attack=attack),self.assertRaises(ValueError):self.validate(r)

    def test_every_shared_vm_receipt_is_bound_and_complete(self):
        for attack in ('vm','amount','getter','result','calls','routes','tx','wallet','source','oldhash','gas','missingamount'):
            r=copy.deepcopy(self.reports['shared']);a=r['positive_plans'][-1]['independent_actual']
            if attack=='vm':a['vm_started']=False
            elif attack=='amount':a['actual_amount_out']=None
            elif attack=='getter':a['source']['state']['pools'][0]['getters'].pop()
            elif attack=='result':a['source']['state']['pools'][0]['getters'][0]['result']='0x00'
            elif attack=='calls':a['source']['unique_getter_calls']=113
            elif attack=='routes':a['selected_route_count']+=1
            elif attack=='tx':a['transactions'].pop()
            elif attack=='wallet':a['wallets_after']['sender_sell']=a['wallets_before']['sender_sell']
            elif attack=='source':a['source']['bank']='H0'
            elif attack=='oldhash':a['shared_reference_sha256']=smoke.projection.BANK_SHA256
            elif attack=='gas':a['independent_gas_reference']=True
            else:del a['routes'][0]['steps'][0]['actual_amount_in']
            with self.subTest(attack=attack),self.assertRaises(ValueError):self.validate(r)

    def test_shared_selected_transaction_must_execute_quoted_swap_and_credit(self):
        for attack in ('calldata','event','parent','clock'):
            r=copy.deepcopy(self.reports['shared']);a=r['positive_plans'][0]['independent_actual']
            tx=a['routes'][0]['steps'][0]['transaction']
            if attack=='calldata':
                tx['transaction']['input']='0xdeadbeef'
                tx['transaction_sha256']=digest(tx['transaction'])
            elif attack=='event':
                tx['receipt']['logs']=[]
                tx['receipt_sha256']=digest(tx['receipt'])
            elif attack=='parent':
                tx['header']['parentHash']='0x'+'0'*64
                tx['header_sha256']=digest(tx['header'])
                a['final_state']['header_sha256']=digest(tx['header'])
            else:
                tx['header']['timestamp']='0x1'
                tx['header_sha256']=digest(tx['header'])
                a['final_state']['header_sha256']=digest(tx['header'])
            with self.subTest(attack=attack),self.assertRaises(ValueError):self.validate(r)

    def test_review_requires_distinct_reviewers_exact_new_closure_and_no_open_findings(self):
        gate=self.gate();review=dict(passed=True,authority='independent-agent-source-review',
            scope='managed-v2-current-catalog-and-shared-vault-local-scope',runtime=self.runtime,input_hashes=self.inputs,
            verification_sha256='1'*64,spec=dict(passed=True,reviewer='spec-agent',unresolved=[]),
            quality=dict(passed=True,reviewer='quality-agent',unresolved=[]))
        gate.validate_review(review,self.runtime,self.inputs,'1'*64)
        for attack in ('failed','same','unresolved','oldscope','oldruntime','verification','flagonly'):
            r=copy.deepcopy(review)
            if attack=='failed':r['quality']['passed']=False
            elif attack=='same':r['quality']['reviewer']='spec-agent'
            elif attack=='unresolved':r['spec']['unresolved']=['issue']
            elif attack=='oldscope':r['scope']='managed-v2-implemented-local-scope'
            elif attack=='oldruntime':r['runtime']['binaries'][evidence.BINARIES[0]]='0'*64
            elif attack=='verification':r['verification_sha256']='0'*64
            else:del r['quality']
            with self.subTest(attack=attack),self.assertRaises(ValueError):gate.validate_review(r,self.runtime,self.inputs,'1'*64)

    def test_verification_requires_all_four_checks_and_five_exact_binaries(self):
        gate=self.gate();v=dict(passed=True,source_unchanged_during_verification=True,verification_tool_unchanged=True,
            source_tree_sha256=self.runtime['source_tree_sha256'], checked_binary_sha256=self.runtime['binaries'],
            commands=[dict(project=p,command=c,exit_code=0) for p,c in
                [(p,['make','check']) for p in ('chain-parser-go','chain-parser-rust','dex-router','common/protoc')] +
                [(p,['make','debug-build']) for p in ('chain-parser-go','chain-parser-rust')] +
                [('integration',['python3','tools/ci/'+tool]) for tool in ('contracts.py','boundary.py')]])
        gate.validate_verification(v,self.runtime)
        for attack in ('missing','failed','binaries','empty','debug'):
            r=copy.deepcopy(v)
            if attack=='missing':r['commands'].pop()
            elif attack=='failed':r['commands'][0]['exit_code']=1
            elif attack=='binaries':r['checked_binary_sha256'].pop(evidence.BINARIES[0])
            elif attack=='debug':r['commands']=[c for c in r['commands'] if c['command']!=['make','debug-build']]
            else:r['commands']=[]
            with self.subTest(attack=attack),self.assertRaises(ValueError):gate.validate_verification(r,self.runtime)

    def test_stage_failure_and_source_drift_close_atomic_report(self):
        gate=self.gate()
        for drift in (False,True):
            report=dict(state='RUNNING',passed=False,commands=[])
            with tempfile.TemporaryDirectory() as tmp:
                target=Path(tmp)/'gate.json'
                with mock.patch.object(gate,'runtime_fingerprint',return_value=self.runtime),mock.patch.object(gate,'snapshot_inputs',side_effect=[self.inputs,{}] if drift else [self.inputs,self.inputs]),mock.patch.object(gate.subprocess,'run',return_value=mock.Mock(returncode=0 if drift else 1)):
                    with self.assertRaises(ValueError):gate.run_stage('example',['example'],report,self.runtime,self.inputs,os.environ.copy(),target)
                saved=json.loads(target.read_bytes());self.assertEqual(saved['state'],'FAILED');self.assertFalse(saved['passed'])
                stage=saved['commands'][0];self.assertEqual(stage['state'],'FAILED');self.assertIn('log_sha256',stage)

    def test_preflight_failure_invalidates_previous_gate_pass(self):
        gate=self.gate()
        with tempfile.TemporaryDirectory() as tmp:
            target=Path(tmp)/'gate.json'
            target.write_text('{"passed":true,"state":"PASSED"}')
            with mock.patch.object(gate,'TARGET',target),mock.patch.object(gate,'checked_runtime',side_effect=ValueError('full verification failed')):
                with self.assertRaisesRegex(ValueError,'full verification failed'):gate.run()
            saved=json.loads(target.read_bytes())
            self.assertEqual(saved['state'],'FAILED');self.assertIs(saved['passed'],False)
            self.assertEqual(saved['scope'],gate.SCOPE)
            self.assertGreaterEqual(saved['completed_at_unix_ms'],saved['started_at_unix_ms'])
            self.assertEqual(list(Path(tmp).glob('*.tmp')),[])

    def test_stage_success_records_log_and_both_frozen_boundary_checks(self):
        gate=self.gate();report=dict(state='RUNNING',passed=False,commands=[])
        with tempfile.TemporaryDirectory() as tmp:
            target=Path(tmp)/'gate.json'
            def execute(*args,**kwargs):
                saved=json.loads(target.read_bytes())
                self.assertEqual(saved['commands'][0]['state'],'RUNNING')
                self.assertFalse(saved['passed'])
                kwargs['stdout'].write(b'actual mocked child output\n')
                return mock.Mock(returncode=0)
            with mock.patch.object(gate,'runtime_fingerprint',return_value=self.runtime) as runtime_mock,mock.patch.object(gate,'snapshot_inputs',return_value=self.inputs) as inputs_mock,mock.patch.object(gate.subprocess,'run',side_effect=execute):
                stage=gate.run_stage('unit',['unit'],report,self.runtime,self.inputs,os.environ.copy(),target)
            saved=json.loads(target.read_bytes())
            self.assertEqual(stage['state'],'PASSED');self.assertEqual(stage['exit_code'],0)
            self.assertEqual(stage['log_sha256'],hashlib.sha256(Path(stage['log']).read_bytes()).hexdigest())
            self.assertEqual(runtime_mock.call_count,2);self.assertEqual(inputs_mock.call_count,2)
            self.assertEqual(saved['state'],'RUNNING');self.assertFalse(saved['passed'])
            self.assertEqual(list(Path(tmp).glob('*.tmp')),[])

    def test_interrupt_and_verification_receipt_change_fail_closed(self):
        gate=self.gate()
        for interrupted in (False,True):
            with tempfile.TemporaryDirectory() as tmp:
                out=Path(tmp);target=out/'gate.json';receipt=out/'verification.json';receipt.write_bytes(b'original')
                report=dict(state='RUNNING',passed=False,commands=[],verification_sha256=hashlib.sha256(b'original').hexdigest())
                def execute(*args,**kwargs):
                    if interrupted:raise KeyboardInterrupt('interrupted child')
                    receipt.write_bytes(b'changed');return mock.Mock(returncode=0)
                with mock.patch.object(gate,'OUT',out),mock.patch.object(gate,'runtime_fingerprint',return_value=self.runtime),mock.patch.object(gate,'snapshot_inputs',return_value=self.inputs),mock.patch.object(gate.subprocess,'run',side_effect=execute):
                    with self.assertRaises(KeyboardInterrupt if interrupted else ValueError):
                        gate.run_stage('unit',['unit'],report,self.runtime,self.inputs,os.environ.copy(),target)
                saved=json.loads(target.read_bytes());self.assertEqual(saved['state'],'FAILED')
                self.assertEqual(saved['commands'][0]['state'],'FAILED');self.assertIs(saved['passed'],False)
                self.assertIsNotNone(saved['commands'][0]['log_sha256'])

    def test_commands_cover_new_and_old_scopes_without_recursive_gate(self):
        gate=self.gate();commands=gate.stage_commands();flat=[' '.join(c) for _,c in commands]
        for required in ('make managed-v2-aum-reference-guards','node tools/ci/managed-v2-aum-catalog-golden.cjs --check',
            'node tools/ci/managed-v2-shared-vault-golden.cjs --check','make managed-v2-multi-helper-test',
            'make managed-v2-multi-differential-test','python3 tools/acceptance/managed_v2_smoke.py',
            'python3 tools/acceptance/managed_v2_multi_smoke.py --scope both'):
            self.assertIn(required,flat)
        self.assertFalse(any('tools/acceptance/managed_v2_multi_acceptance.py' in c for _,c in commands))


if __name__=='__main__':unittest.main()
