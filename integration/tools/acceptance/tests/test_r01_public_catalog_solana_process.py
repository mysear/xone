"""Fixed public-bank input and normal-process differential counterexamples."""
import copy
import json
from pathlib import Path
import sys
import unittest
import tempfile
import urllib.request
from unittest.mock import patch
sys.path.insert(0,str(Path(__file__).resolve().parents[1]))
import r01_public_catalog_solana_process as process
from r01_public_catalog_solana_rebuild import VerificationError

class FixedRPCTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):cls.fixture=process.FixedBankRPC('clmm')

    def test_real_loopback_http_serves_original_clock(self):
        f=self.fixture;server=f.serve()
        try:
            request={'jsonrpc':'2.0','id':7,'method':'getMultipleAccounts','params':[[process.CLOCK],{'encoding':'base64','commitment':'finalized'}]}
            with urllib.request.urlopen(urllib.request.Request(f'http://127.0.0.1:{server.server_port}',data=json.dumps(request).encode(),headers={'Content-Type':'application/json'}),timeout=3) as response:body=json.load(response)
            self.assertEqual(body['id'],7);self.assertEqual(body['result']['value'],[f.accounts[process.CLOCK]])
        finally:server.shutdown();server.server_close()

    def test_clock_and_account_bytes_are_never_refreshed(self):
        f=self.fixture;ids=[process.CLOCK,f.manifest['catalog_row']['upgrade']['programdata']]
        a=f.dispatch('getMultipleAccounts',[ids,{'encoding':'base64','commitment':'finalized'}])
        b=f.dispatch('getMultipleAccounts',[ids,{'encoding':'base64','commitment':'finalized'}])
        self.assertEqual(a,b);self.assertEqual(a['context']['slot'],f.slot)
        self.assertEqual(a['value'],[f.accounts[x] for x in ids])

    def test_caller_mutation_cannot_modify_captured_account(self):
        f=self.fixture;ids=[process.CLOCK];r=f.dispatch('getMultipleAccounts',[ids,{'encoding':'base64','commitment':'finalized'}]);r['value'][0]['data'][0]=''
        self.assertNotEqual(f.accounts[process.CLOCK]['data'][0],'')

    def test_uncaptured_account_cannot_be_synthesized(self):
        with self.assertRaises(VerificationError):self.fixture.dispatch('getMultipleAccounts',[['11111111111111111111111111111111'],{'encoding':'base64','commitment':'finalized'}])

    def test_other_slot_cannot_use_captured_blockhash(self):
        with self.assertRaises(VerificationError):self.fixture.dispatch('getBlock',[self.fixture.slot+1,{'commitment':'finalized'}])

    def test_blockhash_and_genesis_are_original(self):
        f=self.fixture;self.assertEqual(f.dispatch('getBlock',[f.slot,{'commitment':'finalized'}]),f.block)
        self.assertEqual(f.dispatch('getGenesisHash',[]),process.GENESIS)

    def test_transactions_cannot_be_submitted(self):
        with self.assertRaises(VerificationError):self.fixture.dispatch('sendTransaction',['x'])

    def test_min_context_slot_is_only_a_lower_bound(self):
        f=self.fixture
        with self.assertRaises(VerificationError):f.dispatch('getMultipleAccounts',[[process.CLOCK],{'encoding':'base64','commitment':'finalized','minContextSlot':f.slot+1}])

class ProcessConfigTests(unittest.TestCase):
    def test_normal_binary_is_executed_at_original_project_path(self):
        parent=process.ROOT/'integration/artifacts/local/r01-solana-rebuild'
        with tempfile.TemporaryDirectory(dir=parent,prefix='binary-pin-test-') as directory:
            original=Path(sys.executable).resolve();command_path,record=process.pin_binary(original,Path(directory))
            self.assertEqual(command_path,original)
            self.assertEqual(record['sha256'],process.digest(original.read_bytes()))

    def test_configs_select_normal_rpc_parser_and_normal_router(self):
        f=process.FixedBankRPC('cpmm');cfg=process.make_configs(f,Path('/tmp/unused-r01-config'), 'http://127.0.0.1:9000',9001,9002,'r01-local-auth-token-with-32-characters')
        self.assertEqual(cfg['parser']['service']['mode'],'rpc');self.assertEqual(cfg['parser']['service']['name'],'chain-parser-rust')
        self.assertEqual(cfg['router']['journal']['source_auth_tokens'],{'http://127.0.0.1:9001':'r01-local-auth-token-with-32-characters'});self.assertEqual(cfg['router']['service']['name'],'dex-router');self.assertEqual(cfg['parser']['chains'][0]['genesis_hash'],process.GENESIS)
        self.assertEqual(cfg['parser']['chains'][0]['key'],'solana:mainnet-beta');self.assertEqual(cfg['router']['journal']['source_urls'],['http://127.0.0.1:9001'])

    def test_freshness_policy_is_explicit_without_altering_clock(self):
        f=process.FixedBankRPC('orca');before=copy.deepcopy(f.accounts[process.CLOCK]);cfg=process.make_configs(f,Path('/tmp/unused-r01-config'),'http://127.0.0.1:9000',9001,9002,'r01-local-auth-token-with-32-characters')
        self.assertEqual(cfg['parser']['startup']['max_baseline_age_ms'],315360000000)
        self.assertEqual(cfg['router']['routing']['max_state_age_ms'],315360000000);self.assertEqual(f.accounts[process.CLOCK],before)

    def test_upgradeable_token_owner_is_preserved_and_elf_pin_is_explicit(self):
        f=process.FixedBankRPC('cpmm');p=process.pool_config(f);identity=f.token_identity()
        self.assertEqual(identity['owner'],process.LOADER);self.assertEqual(p['raydium_cpmm']['expected_token_program_code_sha256'],identity['payload_sha256'])
        self.assertNotEqual(identity['payload_sha256'],identity['program_account_sha256'])

def specimen(fixture):
    native=json.loads((process.BASE/'solana-rebuild'/(fixture.label+'-native-execution.json')).read_text())
    gross=native['cases'][1]['net_recipient_raw_delta'];minimum=gross*9950//10000
    edge={'protocol':process.SPECS[fixture.label][1],'pool_id':fixture.manifest['pool']['pool'],'zero_for_one':True,'to':fixture.manifest['pool']['mint_b']}
    path={'amount_in':'1000000','amount_out':str(gross),'edges':[edge],'tokens':[process.WSOL,fixture.manifest['pool']['mint_b']]}
    route={'path':path,'amount_in':'1000000','amount_out':str(gross)}
    plan={'context':{'chain_key':process.CHAIN,'caller':native['payer'],'recipient':native['recipient'],'sell_token':process.WSOL,'buy_token':fixture.manifest['pool']['mint_b'],'slippage_bps':50,'deadline_ms':2000000000000},'snapshot':{'block':{'chain_key':process.CHAIN,'number_or_slot':fixture.slot,'hash':fixture.block['blockhash']}},'allocation':{'sell_amount':'1000000','gross_buy_amount':str(gross),'net_buy_amount':str(gross),'refund':'0','routes':[route]},'min_buy_amount':str(minimum),'economic_policy_hash':'unit-test-policy'}
    plan['plan_hash']=process.plan_hash(plan)
    quote={'quoteId':'unit-only-not-a-normal-process-quote','planHash':plan['plan_hash'],'snapshotRef':plan['snapshot'],'grossBuyAmount':str(gross),'minBuyAmount':str(minimum),'netBuyAmountEstimate':None,'costs':{'unavailableReason':'CHAIN_FEE_MODEL_NOT_IMPLEMENTED'},'consumedIn':'1000000','refund':'0','routes':[route]}
    return quote,{'quote':copy.deepcopy(quote),'execution_plan':plan,'expired':False,'snapshot_valid':True},native

class UpgradeConfigTests(unittest.TestCase):
    def test_actual_upgrade_mode_is_explicit_for_each_protocol(self):
        for label in ['cpmm','clmm','orca']:
            with self.subTest(protocol=label):
                fixture=process.FixedBankRPC(label);identity=fixture.token_identity();config=process.pool_config(fixture)
                nested=config[process.SPECS[label][2]]
                self.assertEqual(nested['token_program_upgrade'],{'programdata':identity['programdata'],'deployed_slot':identity['upgrade_slot']})
                self.assertEqual(nested['expected_token_program_code_sha256'],identity['payload_sha256'])

    def test_declared_token_programdata_owner_is_not_trusted_by_generator(self):
        fixture=process.FixedBankRPC('cpmm');pd=fixture.token_identity()['programdata']
        fixture.accounts[pd]['owner']=process.TOKEN
        with self.assertRaises(VerificationError):fixture.token_identity()

    def test_immutable_mode_does_not_invent_upgrade_configuration(self):
        fixture=process.FixedBankRPC('cpmm')
        with patch.object(fixture,'token_identity',return_value={'programdata':None,'payload_sha256':'0'*64}):
            config=process.pool_config(fixture)
        self.assertNotIn('token_program_upgrade',config['raydium_cpmm'])

class ClmmWindowTests(unittest.TestCase):
    def test_default_preserves_full_captured_config(self):
        f=process.FixedBankRPC('clmm')
        self.assertEqual(f.clmm_window,'full-captured')
        self.assertEqual(process.pool_config(f)['raydium_clmm']['tick_array_starts'],
                         sorted(t['start'] for t in f.manifest['extras']['tick_arrays']))

    def test_nearest_three_uses_negative_floor_and_original_superset(self):
        full=process.FixedBankRPC('clmm');f=process.FixedBankRPC('clmm',clmm_window='nearest-three')
        before=copy.deepcopy(f.accounts);manifest=copy.deepcopy(f.manifest)
        declaration=process.tick_window_declaration(f)
        self.assertEqual(declaration['declared_starts'],[-24180,-24120,-24060])
        self.assertEqual(declaration['tick_lower_inclusive'],-24180)
        self.assertEqual(declaration['tick_upper_exclusive'],-24000)
        self.assertEqual(declaration['input_amount'],1000000)
        self.assertEqual(len(declaration['captured_arrays']),7)
        self.assertEqual(len(declaration['declared_arrays']),3)
        self.assertEqual(process.pool_config(f)['raydium_clmm']['tick_array_starts'],declaration['declared_starts'])
        self.assertEqual(f.accounts,before);self.assertEqual(f.accounts,full.accounts);self.assertEqual(f.manifest,manifest)
        self.assertNotEqual(declaration['declaration_sha256'],process.tick_window_declaration(full)['declaration_sha256'])
        self.assertFalse(declaration['whole_pool_or_all_inputs_claimed'])

    def test_missing_selected_array_cannot_be_synthesized_or_widened(self):
        f=process.FixedBankRPC('clmm',clmm_window='nearest-three')
        selected=next(t for t in f.manifest['extras']['tick_arrays'] if t['start']==-24180)
        del f.accounts[selected['address']]
        with self.assertRaises(VerificationError):process.pool_config(f)

    def test_duplicate_captured_start_is_rejected(self):
        f=process.FixedBankRPC('clmm',clmm_window='nearest-three')
        f.manifest['extras']['tick_arrays'].append(copy.deepcopy(f.manifest['extras']['tick_arrays'][0]))
        with self.assertRaises(VerificationError):process.tick_window_declaration(f)

    def test_unknown_window_rejected(self):
        with self.assertRaises(VerificationError):process.FixedBankRPC('clmm',clmm_window='automatic')

    def test_clmm_option_does_not_change_other_protocols(self):
        for label in ['cpmm','orca']:
            full=process.FixedBankRPC(label);f=process.FixedBankRPC(label,clmm_window='nearest-three')
            self.assertEqual(process.pool_config(f),process.pool_config(full))
            self.assertEqual(process.tick_window_declaration(f)['effective_clmm_window'],'not-applicable')

    def test_actual_scope_hash_is_not_replaced_by_window_declaration_hash(self):
        f=process.FixedBankRPC('clmm',clmm_window='nearest-three')
        d=process.tick_window_declaration(f)
        scope=process.digest(json.dumps([f.manifest['pool']['pool']],separators=(',',':')).encode())
        baseline={'reference':{'scope_hash':scope},'pools':[{'pool_id':f.manifest['pool']['pool'],'raydium_clmm':{'tick_arrays':[
            {'start_tick_index':t['start'],'address':t['address']} for t in d['declared_arrays']]}}]}
        observed=process.observe_window_scope(f,baseline,{'snapshotRef':{'scope_hash':scope}})
        self.assertEqual(observed['parser_scope_hash'],scope);self.assertEqual(observed['quote_scope_hash'],scope)
        self.assertFalse(observed['production_scope_hash_includes_tick_window'])
        self.assertEqual(observed['declaration_sha256'],d['declaration_sha256'])
        baseline['pools'][0]['raydium_clmm']['tick_arrays'].pop()
        with self.assertRaises(VerificationError):process.observe_window_scope(f,baseline)

    def test_wrong_quote_scope_is_rejected_even_if_window_matches(self):
        f=process.FixedBankRPC('cpmm');scope=process.digest(json.dumps([f.manifest['pool']['pool']],separators=(',',':')).encode())
        baseline={'reference':{'scope_hash':scope},'pools':[{'pool_id':f.manifest['pool']['pool']}]}
        with self.assertRaises(VerificationError):process.observe_window_scope(f,baseline,{'snapshotRef':{'scope_hash':'0'*64}})

class DifferentialTests(unittest.TestCase):
    def test_one_atom_pool_gross_difference_is_failure(self):
        q={'grossBuyAmount':'100','netBuyAmountEstimate':None,'consumedIn':'1000000','refund':'0'}
        r=process.compare_quote(q,{'pool_gross':101,'recipient_token_delta':101});self.assertFalse(r['r01_pool_gross_verified'])

    def test_after_chain_fees_unavailable_does_not_block_r01_pool_math(self):
        q={'grossBuyAmount':'100','netBuyAmountEstimate':None,'costs':{'unavailableReason':'CHAIN_FEE_MODEL_NOT_IMPLEMENTED'},'consumedIn':'1000000','refund':'0'}
        r=process.compare_quote(q,{'pool_gross':100,'recipient_token_delta':100})
        self.assertTrue(r['r01_pool_gross_verified']);self.assertIsNone(r['quantities']['after_chain_fees']['value'])
        self.assertEqual(r['quantities']['after_chain_fees']['quantity_basis'],'after_chain_fees_unavailable')
        self.assertNotIn('full_gross_and_net_differential_verified',r)

    def test_gas_net_is_not_compared_with_recipient_token_delta(self):
        r=process.compare_quote({'grossBuyAmount':'100','netBuyAmountEstimate':'99','consumedIn':'1000000','refund':'0'},{'pool_gross':100,'recipient_token_delta':100})
        self.assertTrue(r['r01_pool_gross_verified']);self.assertEqual(r['quantities']['after_chain_fees']['value'],99)
        self.assertIsNone(r['quantities']['after_chain_fees']['reference_value'])

    def test_pool_gross_and_recipient_delta_use_distinct_quantities(self):
        r=process.compare_quote({'grossBuyAmount':'100','netBuyAmountEstimate':None,'consumedIn':'1000000','refund':'0'},{'pool_gross':100,'recipient_token_delta':97})
        self.assertTrue(r['r01_pool_gross_verified']);self.assertEqual(r['quantities']['recipient_token_delta']['reference_value'],97)
        self.assertIsNone(r['quantities']['recipient_token_delta']['router_selected_replay_value'])

    def test_matching_quote_requires_full_input_consumption(self):
        with self.assertRaises(VerificationError):process.compare_quote({'grossBuyAmount':'100','netBuyAmountEstimate':None,'consumedIn':'999999','refund':'1'},{'pool_gross':100,'recipient_token_delta':100})

class BinaryFreezeTests(unittest.TestCase):
    def test_three_cases_must_use_the_same_normal_binary_pair(self):
        case={'binaries':{'parser':{'sha256':'a'*64},'router':{'sha256':'b'*64}},'binary_source_changed_during_case':{'parser':False,'router':False}}
        self.assertTrue(process.same_binary_freeze([case,copy.deepcopy(case),copy.deepcopy(case)]))
        other=copy.deepcopy(case);other['binaries']['parser']['sha256']='c'*64
        self.assertFalse(process.same_binary_freeze([case,other]))

    def test_missing_or_inflight_changed_binary_cannot_pass(self):
        self.assertFalse(process.same_binary_freeze([{}]))
        case={'binaries':{'parser':{'sha256':'a'*64},'router':{'sha256':'b'*64}},'binary_source_changed_during_case':{'parser':True,'router':False}}
        self.assertFalse(process.same_binary_freeze([case]))

class RouterPlanTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):cls.fixture=process.FixedBankRPC('cpmm')

    def test_router_status_uses_snapshot_ref_and_ready(self):
        q,e,n=specimen(self.fixture);reference=e['execution_plan']['snapshot']
        self.assertTrue(process.router_scope_ready({'http_status':200,'body':{'ready':True,'snapshot_ref':reference}},reference))
        self.assertFalse(process.router_scope_ready({'http_status':200,'body':{'ready':False,'snapshot_ref':reference}},reference))
        self.assertFalse(process.router_scope_ready({'http_status':200,'body':{'ready':True,'snapshot_ref':{}}},reference))

    def test_single_pool_selected_amount_and_minimum_are_bound(self):
        q,e,n=specimen(self.fixture);s=process.selected_plan_quantities(q,e,self.fixture,n)
        self.assertEqual(s['amount_in'],1000000);self.assertEqual(s['minimum_amount_out'],int(q['minBuyAmount']))
        self.assertFalse(s['production_instruction_provided'])
        self.assertEqual(s['optimizer_after_route_activation_cost']['quantity_basis'],'after_configured_route_activation_cost')
        self.assertFalse(s['optimizer_after_route_activation_cost']['used_as_chain_fee_net'])

    def test_minimum_cannot_be_changed_to_one(self):
        q,e,n=specimen(self.fixture);e['execution_plan']['min_buy_amount']='1'
        with self.assertRaises(VerificationError):process.selected_plan_quantities(q,e,self.fixture,n)

    def test_rehashed_wrong_slippage_floor_is_rejected(self):
        q,e,n=specimen(self.fixture);p=e['execution_plan'];p['min_buy_amount']=str(int(p['min_buy_amount'])+1);p['plan_hash']=process.plan_hash(p);q['planHash']=p['plan_hash'];q['minBuyAmount']=p['min_buy_amount'];e['quote']=copy.deepcopy(q)
        with self.assertRaises(VerificationError):process.selected_plan_quantities(q,e,self.fixture,n)

    def test_other_bank_is_not_silently_rebound(self):
        q,e,n=specimen(self.fixture);p=e['execution_plan'];p['snapshot']['block']['hash']='other';p['plan_hash']=process.plan_hash(p);q['planHash']=p['plan_hash'];e['quote']=copy.deepcopy(q)
        with self.assertRaises(VerificationError):process.selected_plan_quantities(q,e,self.fixture,n)

    def test_split_or_multi_hop_requires_separate_support(self):
        q,e,n=specimen(self.fixture);p=e['execution_plan'];p['allocation']['routes'].append(copy.deepcopy(p['allocation']['routes'][0]));p['plan_hash']=process.plan_hash(p);q['routes']=p['allocation']['routes'];q['planHash']=p['plan_hash'];e['quote']=copy.deepcopy(q)
        with self.assertRaises(VerificationError):process.selected_plan_quantities(q,e,self.fixture,n)

    def test_pool_selection_and_recipient_cannot_be_swapped(self):
        q,e,n=specimen(self.fixture);p=e['execution_plan'];p['context']['recipient']=n['payer'];p['plan_hash']=process.plan_hash(p);q['planHash']=p['plan_hash'];e['quote']=copy.deepcopy(q)
        with self.assertRaises(VerificationError):process.selected_plan_quantities(q,e,self.fixture,n)

class RecordedNormalPlanTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        path=process.ROOT/'integration/artifacts/local/r01-solana-process-token-upgrade-v2/cpmm/result.json'
        if not path.exists():raise unittest.SkipTest('actual normal CPMM process observation required')
        cls.record=json.loads(path.read_text())

    def test_actual_cached_plan_and_native_quantity_replay_are_independently_rechecked(self):
        result=process.verify_recorded_case(self.record)
        self.assertTrue(result['r01_pool_gross_verified']);self.assertTrue(result['selected_plan_recipient_token_delta_verified'])
        self.assertIsNone(result['after_chain_fees']['value']);self.assertFalse(result['normal_process_reexecuted'])

    def test_success_flags_cannot_replace_missing_real_quote(self):
        record=copy.deepcopy(self.record);record['normal_quote_response']['http_status']=503
        record['r01_pool_gross_verified']=True;record['selected_plan_recipient_token_delta_verified']=True
        with self.assertRaises(VerificationError):process.verify_recorded_case(record)

    def test_tampered_selected_instruction_is_rejected(self):
        record=copy.deepcopy(self.record)
        record['selected_plan_quantity_replay']['execution']['cases'][1]['instruction_data_base64']='AA=='
        with self.assertRaises(VerificationError):process.verify_recorded_case(record)

class RecordedWindowTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        path=process.ROOT/'integration/artifacts/local/r01-solana-process-bounded-window-v4/clmm/result.json'
        if not path.exists():raise unittest.SkipTest('actual v4 normal CLMM window observation required')
        cls.record=json.loads(path.read_text())

    def test_actual_bounded_plan_and_all_original_bytes_replay(self):
        result=process.verify_recorded_case(self.record)
        self.assertTrue(result['r01_pool_gross_verified']);self.assertTrue(result['selected_plan_recipient_token_delta_verified'])
        self.assertEqual(result['tick_window']['declared_starts'],[-24180,-24120,-24060])
        self.assertFalse(result['normal_process_reexecuted'])

    def test_bounded_quote_cannot_be_relabelled_as_full_scope(self):
        record=copy.deepcopy(self.record);record['tick_window']['requested_clmm_window']='full-captured'
        with self.assertRaises(VerificationError):process.verify_recorded_case(record)

    def test_rehashed_full_declaration_cannot_replace_actual_decoded_three(self):
        record=copy.deepcopy(self.record);record['tick_window']=process.tick_window_declaration(process.FixedBankRPC('clmm'))
        with self.assertRaises(VerificationError):process.verify_recorded_case(record)

    def test_window_hash_cannot_replace_observed_production_scope_hash(self):
        record=copy.deepcopy(self.record)
        record['window_scope_observation']['parser_scope_hash']=record['tick_window']['declaration_sha256']
        with self.assertRaises(VerificationError):process.verify_recorded_case(record)

if __name__=='__main__':unittest.main()
