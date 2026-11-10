"""Native-bank boundaries: no saved claims may substitute for replay."""
import base64
import copy
from pathlib import Path
import struct
import json
from unittest.mock import patch
import sys
import unittest
sys.path.insert(0,str(Path(__file__).resolve().parents[1]))
import r01_public_catalog_solana_bank as bank
from r01_public_catalog_solana_rebuild import VerificationError,b58encode


def token(amount=100,owner=bytes([2])*32,mint=bytes([1])*32):
    raw=bytearray(165);raw[:32]=mint;raw[32:64]=owner;struct.pack_into('<Q',raw,64,amount);raw[108]=1
    return {'owner':bank.TOKEN,'executable':False,'data':[base64.b64encode(raw).decode(),'base64']}


class TokenAccountTests(unittest.TestCase):
    def test_raw_balance_and_owner_are_decoded(self):
        a=bank.decode_token_account(token())
        self.assertEqual(a['amount'],100);self.assertEqual(a['owner'],b58encode(bytes([2])*32))

    def test_foreign_account_owner_is_rejected(self):
        a=token();a['owner']='11111111111111111111111111111111'
        with self.assertRaises(VerificationError):bank.decode_token_account(a)

    def test_frozen_account_is_rejected(self):
        a=token();raw=bytearray(base64.b64decode(a['data'][0]));raw[108]=2;a['data'][0]=base64.b64encode(raw).decode()
        with self.assertRaises(VerificationError):bank.decode_token_account(a)

    def test_truncated_account_is_rejected(self):
        a=token();a['data'][0]=base64.b64encode(bytes(72)).decode()
        with self.assertRaises(VerificationError):bank.decode_token_account(a)

    def test_native_rpc_write_is_never_allowed(self):
        with self.assertRaises(VerificationError):bank.rpc_read('https://api.mainnet-beta.solana.com','sendTransaction',[])

    def test_empty_capture_is_not_executable(self):
        with self.assertRaises(VerificationError):bank.validate_capture({'passed':True,'historical_original_bank_verified':True})

    def test_cli_relative_file_reference_is_resolved(self):
        p=Path(__file__).resolve().relative_to(bank.ROOT)
        self.assertEqual(bank.reference(p)['path'],str(p))

    def test_recorded_http_body_must_equal_parsed_response(self):
        record={'raw_response_base64':base64.b64encode(b'{"result":1}').decode(),'raw_response_sha256':bank.digest(b'{"result":1}'),'response':{'result':2}}
        with self.assertRaises(VerificationError):bank.verify_rpc_record(record)

    def test_recorded_http_body_hash_is_checked(self):
        raw=b'{"result":1}'
        record={'raw_response_base64':base64.b64encode(raw).decode(),'raw_response_sha256':'0'*64,'response':{'result':1}}
        with self.assertRaises(VerificationError):bank.verify_rpc_record(record)

    def test_native_runtime_pin_cannot_be_forged(self):
        lock_path=bank.BASE/'solana-rebuild/runtime-lock.json'
        if not lock_path.exists():self.skipTest('local runtime acquisition required')
        lock=json.loads(lock_path.read_text());lock['files'][0]['sha256']='0'*64
        with self.assertRaises(VerificationError):bank.verify_runtime_lock(lock)

    def test_raw_clock_encoding(self):
        data=struct.pack('<QqQQq',123,1000,2,3,1010)
        self.assertEqual(bank.decode_clock(data),{'slot':123,'epoch_start_timestamp':1000,'epoch':2,'leader_schedule_epoch':3,'unix_timestamp':1010})
        with self.assertRaises(VerificationError):bank.decode_clock(data[:32])


class OriginalTokenBindingTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.manifest=json.loads((bank.BASE/'solana-rebuild/cpmm-sourcebank-correct/sourcebank.json').read_text())
        cls.mapping=bank.validate_capture(cls.manifest)[0];cls.slot=cls.manifest['binding']['observed_context_slot']
        cls.pd=bank.b58encode(bank.account_bytes(cls.mapping[bank.TOKEN])[4:])

    def test_actual_token_payload_is_full_original_ELF(self):
        raw=bank.program_payload(bank.TOKEN,self.mapping,self.slot)
        self.assertEqual(bank.digest(raw),'8190d3f7ceb6cb7a7a8d8924bff89f9f611e15ce1f806f2b6237f3311a98f697')

    def test_wrong_programdata_owner_is_rejected(self):
        mapping=copy.deepcopy(self.mapping);mapping[self.pd]['owner']=bank.TOKEN
        with self.assertRaises(VerificationError):bank.program_payload(bank.TOKEN,mapping,self.slot)

    def test_executable_programdata_is_rejected(self):
        mapping=copy.deepcopy(self.mapping);mapping[self.pd]['executable']=True
        with self.assertRaises(VerificationError):bank.program_payload(bank.TOKEN,mapping,self.slot)

    def test_upgrade_after_bank_is_rejected(self):
        mapping=copy.deepcopy(self.mapping);raw=bytearray(bank.account_bytes(mapping[self.pd]));raw[4:12]=(self.slot+1).to_bytes(8,'little');mapping[self.pd]['data'][0]=base64.b64encode(raw).decode()
        with self.assertRaises(VerificationError):bank.program_payload(bank.TOKEN,mapping,self.slot)

    def test_forged_program_pointer_is_rejected(self):
        mapping=copy.deepcopy(self.mapping);raw=bytearray(bank.account_bytes(mapping[bank.TOKEN]));raw[4:]=bytes(32);mapping[bank.TOKEN]['data'][0]=base64.b64encode(raw).decode()
        with self.assertRaises(VerificationError):bank.program_payload(bank.TOKEN,mapping,self.slot)

@unittest.skipUnless(bank.DEPS.is_dir(), 'workspace-local solders runtime must be acquired for native replay')
class ActualDeployedSBFReplayTests(unittest.TestCase):
    def test_saved_execution_is_verified_by_a_fresh_native_replay(self):
        base=bank.BASE/'solana-rebuild'
        manifest=json.loads((base/'cpmm-sourcebank-correct/sourcebank.json').read_text())
        evidence=json.loads((base/'cpmm-native-execution.json').read_text())
        self.assertTrue(bank.verify_saved_execution(manifest,evidence)['verified'])
        evidence['cases'][1]['net_recipient_raw_delta']+=1
        with self.assertRaises(VerificationError):bank.verify_saved_execution(manifest,evidence)

    def test_selected_minimum_is_encoded_and_quantities_are_separate(self):
        base=bank.BASE/'solana-rebuild';manifest=json.loads((base/'cpmm-sourcebank-correct/sourcebank.json').read_text())
        reference=json.loads((base/'cpmm-native-execution.json').read_text());minimum=reference['cases'][1]['net_recipient_raw_delta']*9950//10000
        replay=bank.run_native(manifest,minimum_amount_out=minimum)
        data=base64.b64decode(replay['cases'][1]['instruction_data_base64'])
        self.assertEqual(int.from_bytes(data[16:24],'little'),minimum)
        quantities=bank.execution_quantities(manifest,replay)
        self.assertEqual(quantities['pool_gross'],337524356);self.assertEqual(quantities['recipient_token_delta'],337524356)
        self.assertIsNone(quantities['after_chain_fees'])

    def test_three_selected_quantity_ABI_replays_preserve_reference_token_deltas(self):
        base=bank.BASE/'solana-rebuild'
        for label,folder in [('cpmm','cpmm-sourcebank-correct'),('clmm','clmm-sourcebank'),('orca','orca-sourcebank')]:
            with self.subTest(program=label):
                manifest=json.loads((base/folder/'sourcebank.json').read_text());reference=json.loads((base/(label+'-native-execution.json')).read_text());pool=manifest['pool'];row=manifest['catalog_row'];block=bank.load_reference(manifest['block'])['response']['result']
                selection={'plan_hash':'unit-only-selection-not-normal-Router-evidence','pool':pool['pool'],'program':row['address'],'programdata':row['upgrade']['programdata'],'upgrade_slot':row['upgrade']['slot'],'full_payload_sha256':row['runtime']['sha256'],'sourcebank_slot':reference['clock']['slot'],'blockhash':block['blockhash'],'caller':reference['payer'],'recipient':reference['recipient'],'mint_in':pool['mint_a'],'mint_out':pool['mint_b'],'zero_for_one':True,'amount_in':1000000,'minimum_amount_out':reference['cases'][1]['net_recipient_raw_delta']*9950//10000,'production_instruction_provided':False}
                replay=bank.replay_selected_quantities(manifest,selection)
                self.assertTrue(replay['selected_quantities_native_execution_succeeded'])
                self.assertFalse(replay['production_router_instruction_execution_verified'])
                self.assertIn('caller-supplied',replay['selection_origin_authority'])
                self.assertEqual(replay['quantities']['recipient_token_delta'],reference['cases'][1]['net_recipient_raw_delta'])
                self.assertEqual(replay['execution']['clock'],reference['clock'])
                bad=copy.deepcopy(selection);bad['sourcebank_slot']+=1
                with self.assertRaises(VerificationError):bank.replay_selected_quantities(manifest,bad)

    def test_three_actual_programs_replay_offline_with_distinct_net_recipient(self):
        cases=[('cpmm','cpmm-sourcebank-correct',337524356),('clmm','clmm-sourcebank',90091),('orca','orca-sourcebank',90094)]
        for label,folder,expected in cases:
            with self.subTest(program=label):
                manifest=json.loads((bank.BASE/'solana-rebuild'/folder/'sourcebank.json').read_text())
                result=bank.run_native(manifest)
                self.assertTrue(result['controlled_net_recipient_verified'])
                self.assertTrue(result['original_token_cpi_observed'])
                self.assertTrue(result['recipient_is_distinct_from_payer'])
                self.assertFalse(result['historical_original_bank_verified'])
                self.assertEqual(result['cases'][0]['net_recipient_raw_delta'],0)
                self.assertTrue(result['cases'][0]['all_tracked_accounts_unchanged'])
                self.assertEqual(result['cases'][1]['net_recipient_raw_delta'],expected)
                self.assertEqual(result['loaded_programs'][0]['payload_sha256'],manifest['catalog_row']['runtime']['sha256'])

if __name__=='__main__':unittest.main()
