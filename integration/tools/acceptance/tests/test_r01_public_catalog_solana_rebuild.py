"""Counterexamples for the R01 Solana deployment and bank boundary."""
import base64
import copy
import hashlib
import importlib.util
import json
from pathlib import Path
import struct
import sys
import unittest

TOOLS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(TOOLS))
import r01_public_catalog_solana_rebuild as subject

ROOT = TOOLS.parents[2]
BASE = ROOT / 'integration/test/source/deployment-catalog'


def elf(text=b'01234567', rodata=b'hello\x00'):
    names = b'\x00.text\x00.rodata\x00.shstrtab\x00'
    shoff = 64 + len(text) + len(rodata) + len(names)
    result = bytearray(shoff + 4 * 64)
    result[:16] = b'\x7fELF\x02\x01\x01' + bytes(9)
    struct.pack_into('<HHIQQQIHHHHHH', result, 16, 3, 263, 1, 64, 0, shoff, 0, 64, 56, 0, 64, 4, 3)
    result[64:shoff] = text + rodata + names
    for i, (name, kind, offset, size) in enumerate([(0, 0, 0, 0), (1, 1, 64, len(text)), (7, 1, 64+len(text), len(rodata)), (15, 3, 64+len(text)+len(rodata), len(names))]):
        struct.pack_into('<IIQQQQIIQQ', result, shoff+i*64, name, kind, 0, 0, offset, size, 0, 0, 1, 0)
    return bytes(result)


class ArtifactComparisonTests(unittest.TestCase):
    def test_full_prefix_and_zero_padding_are_accepted(self):
        built = elf()
        result = subject.compare_artifact(built + bytes(40), built)
        self.assertTrue(result['matched'])
        self.assertEqual(result['allocation_padding_bytes'], 40)

    def test_header_difference_is_never_skipped(self):
        built = bytearray(elf())
        built[24] ^= 8
        result = subject.compare_artifact(elf()+bytes(40), bytes(built))
        self.assertFalse(result['matched'])
        self.assertEqual(result['first_difference'], 24)

    def test_nonzero_tail_is_never_trimmed(self):
        result = subject.compare_artifact(elf()+bytes(20)+b'x', elf())
        self.assertFalse(result['matched'])
        self.assertEqual(result['reason'], 'nonzero-allocation-tail')

    def test_equal_text_does_not_authorize_different_metadata(self):
        result = subject.compare_artifact(elf(rodata=b'/home/runner\x00'), elf(rodata=b'/Users/runner\x00'))
        self.assertFalse(result['matched'])
        self.assertTrue(result['sections']['.text']['equal'])
        self.assertFalse(result['sections']['.rodata']['equal'])

    def test_mismatch_domains_isolate_metadata_without_granting_match(self):
        result=subject.compare_artifact(elf(rodata=b'/home/runner\x00'),elf(rodata=b'/Users/runner\x00'))
        self.assertFalse(result['matched'])
        self.assertIn('.rodata',result['mismatch_domains'])
        self.assertNotIn('.text',result['mismatch_domains'])
        self.assertIn('elf-header',result['mismatch_domains'])

    def test_truncated_section_table_fails(self):
        with self.assertRaises(subject.VerificationError):
            subject.inspect_elf(elf()[:-1])

    def test_declared_section_outside_file_fails(self):
        bad = bytearray(elf())
        shoff = struct.unpack_from('<Q', bad, 40)[0]
        struct.pack_into('<Q', bad, shoff+64+32, len(bad)+1)
        with self.assertRaises(subject.VerificationError):
            subject.inspect_elf(bytes(bad))

    def test_non_sbf_elf_fails(self):
        bad = bytearray(elf()); struct.pack_into('<H',bad,18,62)
        with self.assertRaises(subject.VerificationError):
            subject.inspect_elf(bytes(bad))


class DeploymentBindingTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        rows = json.loads((BASE/'catalog.json').read_text())['deployments']
        cls.row = next(r for r in rows if r['id']=='solana-raydium-cpmm')
        cls.observation = json.loads((ROOT/cls.row['runtime']['observation']['path']).read_text())

    def test_actual_programdata_and_pda_bind(self):
        result = subject.bind_programdata(self.row, self.observation)
        self.assertEqual(result['payload_sha256'],self.row['runtime']['sha256'])
        self.assertEqual(result['upgrade_slot'],463522946)

    def test_wrong_programdata_address_fails(self):
        row=copy.deepcopy(self.row);row['upgrade']['programdata']=row['address']
        with self.assertRaises(subject.VerificationError):subject.bind_programdata(row,self.observation)

    def test_wrong_program_address_fails(self):
        row=copy.deepcopy(self.row);row['address']='11111111111111111111111111111111'
        with self.assertRaises(subject.VerificationError):subject.bind_programdata(row,self.observation)

    def test_changed_upgrade_slot_fails(self):
        row=copy.deepcopy(self.row);row['upgrade']['slot']+=1
        with self.assertRaises(subject.VerificationError):subject.bind_programdata(row,self.observation)

    def test_mincontextslot_is_not_an_exact_bank(self):
        obs=copy.deepcopy(self.observation);obs['response']['result']['context']['slot']+=1
        with self.assertRaises(subject.VerificationError):subject.bind_programdata(self.row,obs)

    def test_executable_programdata_fails(self):
        obs=copy.deepcopy(self.observation);obs['response']['result']['value'][1]['executable']=True
        with self.assertRaises(subject.VerificationError):subject.bind_programdata(self.row,obs)

    def test_foreign_owner_fails(self):
        obs=copy.deepcopy(self.observation);obs['response']['result']['value'][1]['owner']='11111111111111111111111111111111'
        with self.assertRaises(subject.VerificationError):subject.bind_programdata(self.row,obs)

    def test_payload_byte_mutation_fails(self):
        obs=copy.deepcopy(self.observation);v=obs['response']['result']['value'][1];d=bytearray(base64.b64decode(v['data'][0]));d[80]^=1;v['data'][0]=base64.b64encode(d).decode()
        with self.assertRaises(subject.VerificationError):subject.bind_programdata(self.row,obs)

    def test_observation_address_order_cannot_be_swapped(self):
        obs=copy.deepcopy(self.observation);obs['params'][0].reverse()
        with self.assertRaises(subject.VerificationError):subject.bind_programdata(self.row,obs)


class BankBoundaryTests(unittest.TestCase):
    def test_passed_flag_and_bank_hash_are_not_execution(self):
        result=subject.assess_bank({'passed':True,'native_bank':True,'bank_hash':'abc','net_received':100})
        self.assertFalse(result['historical_original_bank_verified'])
        self.assertFalse(result['net_recipient_verified'])

    def test_new_bank_cannot_be_promoted_to_historical_with_slot(self):
        result=subject.assess_bank({'bank_kind':'local-new-bank','slot':466345524,'source_bank_slot':466345524,'historical_original_bank_verified':True})
        self.assertFalse(result['historical_original_bank_verified'])

    def test_rpc_context_and_getblock_are_not_state_proof(self):
        result=subject.assess_bank({'bank_kind':'rpc-account-context','blockhash':'x','context_slot':466345524,'historical_original_bank_verified':True})
        self.assertFalse(result['historical_original_bank_verified'])

    def test_token_delta_uses_raw_amounts_and_recipient_binding(self):
        before={'mint':'mint','owner':'recipient','amount':123}
        after={'mint':'mint','owner':'recipient','amount':130}
        self.assertEqual(subject.token_delta(before,after,'mint','recipient'),7)
        after['owner']='other'
        with self.assertRaises(subject.VerificationError):subject.token_delta(before,after,'mint','recipient')

    def test_bool_amount_does_not_pass_as_integer(self):
        with self.assertRaises(subject.VerificationError):subject.token_delta({'mint':'m','owner':'r','amount':False},{'mint':'m','owner':'r','amount':7},'m','r')

if __name__=='__main__':unittest.main()
