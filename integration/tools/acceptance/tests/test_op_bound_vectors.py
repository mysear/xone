import copy
import json
import os
from pathlib import Path
import sys
import unittest
sys.path.insert(0,str(Path(__file__).resolve().parents[1]))
from op_bound_vectors import audit, envelope, require


class OriginalBoundReference(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.vector=Path(os.environ['XONE_OP_BOUND_VECTORS'])
        cls.inputs=json.loads((cls.vector.parent/'inputs.json').read_bytes())
        cls.rows=json.loads(cls.vector.read_bytes())['cases']

    def test_complete_original_bound_matrix_and_conservation(self):
        audit(self.inputs,self.rows)
        self.assertEqual(len({r['id'] for r in self.rows}),448)
        self.assertTrue(all(len(r['accounts'])>=14 for r in self.rows))

    def test_reference_audit_rejects_identity_fee_and_amount_tampering(self):
        for field,value in [('id','other'),('transaction_hash','0x00'),('gas_used',0),('nonce_after',0),('operator_fee','1'),('l1_fee','0'),('spent','1'),('bought','1')]:
            rows=list(self.rows);rows[0]=dict(rows[0],**{field:value})
            with self.assertRaises(ValueError,msg=field):audit(self.inputs,rows)
        with self.assertRaises(ValueError):audit(self.inputs,self.rows[:-1])

    def test_reference_audit_rejects_accounts_storage_code_nonce_and_burn(self):
        for field in ['balance','nonce','code','storage','missing_account']:
            rows=list(self.rows);r=copy.deepcopy(rows[0]);rows[0]=r
            address=self.inputs[0]['caller'];a=r['accounts'][address]
            if field=='balance':a['balance']=str(int(a['balance'])+1)
            elif field=='nonce':a['nonce']+=1
            elif field=='code':a['code']='0x00'
            elif field=='storage':
                address=next(a for a,v in r['accounts'].items() if v['storage']);r['accounts'][address]['storage'].clear()
            else:del r['accounts'][address]
            with self.assertRaises(ValueError,msg=field):audit(self.inputs,rows)
        rows=list(self.rows);r=copy.deepcopy(rows[0]);rows[0]=r;r['accounts']['0x4200000000000000000000000000000000000019']['balance']='111'
        with self.assertRaises(ValueError):audit(self.inputs,rows)

    def test_reference_rlp_and_require_fail_under_optimization(self):
        for wire in ['','02','c080','ff'*10]:
            with self.assertRaises((ValueError,IndexError)):envelope(wire)
        with self.assertRaises(ValueError):require(False,'rejected')
