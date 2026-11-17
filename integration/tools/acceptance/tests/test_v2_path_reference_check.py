import copy
import importlib.util
import json
from pathlib import Path
import unittest

path=Path(__file__).resolve().parents[1]/'v2_path_reference_check.py'
spec=importlib.util.spec_from_file_location('v2_path_reference_check',path)
reference=importlib.util.module_from_spec(spec);spec.loader.exec_module(reference)

class PathReference(unittest.TestCase):
    @classmethod
    def setUpClass(cls):cls.original=json.loads(reference.FIXTURE.read_bytes())
    def test_original_receipt_bank_inventory_is_complete(self):
        self.assertGreater(len(reference.check(self.original)),0)
    def test_missing_duplicate_or_unexpected_cases_reject(self):
        for mode in range(3):
            value=copy.deepcopy(self.original)
            if mode==0:value['cases'].pop()
            elif mode==1:value['cases'][-1]=value['cases'][0]
            else:value['cases'][0]['id']='unknown'
            with self.assertRaises(ValueError):reference.check(value)
    def test_source_identity_storage_boundary_and_authority_tampering_reject(self):
        for mode in range(6):
            value=copy.deepcopy(self.original);case=value['cases'][0]
            if mode==0:case['fixture']['execution_environment']['parent_hash']='changed'
            elif mode==1:case['expected']['post_accounts'][0]['storage'].append({'slot':'0x12','value':'0x01'})
            elif mode==2:case['executor']['code_sha256']='0'*64
            elif mode==3:case['expected']['actual_output']='0'
            elif mode==4:value['production_ready']=True
            else:value['inputs']['../escape']='0'*64
            with self.assertRaises(ValueError):reference.check(value)

if __name__=='__main__':unittest.main()
