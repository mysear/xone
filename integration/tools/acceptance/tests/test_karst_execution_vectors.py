import copy
import json
import os
from pathlib import Path
import sys
import unittest
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from karst_execution_vectors import audit, cases, CAP


class OriginalKarstExecution(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.v=json.loads(Path(os.environ['XONE_KARST_EXECUTION_VECTORS']).read_bytes())
        cls.inputs=cls.v['inputs'];cls.rows=cls.v['cases']

    def test_full_original_signed_funding_clz_and_cap_coverage(self):
        audit(self.inputs,self.rows)
        self.assertEqual(len(self.rows),468)
        self.assertEqual(sum(r['accepted'] for r in self.rows),401)
        self.assertEqual(sum('clz_zeros' in s for s in self.inputs),257)
        self.assertEqual(sum(s.get('gas_boundary',0)>CAP for s in self.inputs),4)
        self.assertFalse(self.v['current_vm_admitted'])
        self.assertEqual(self.v['original_op_commit'],'60360273aada79572fe2212497de578ad2fa249b')
        self.assertEqual(self.v['original_revm_version'],'43.0.3')

    def test_removed_reordered_or_consistently_replaced_inputs_are_rejected(self):
        for what in ['drop','order','replace']:
            inputs=list(self.inputs);rows=list(self.rows)
            if what=='drop':rows.pop()
            elif what=='order':rows[0],rows[1]=rows[1],rows[0]
            else:
                inputs[0]=copy.deepcopy(inputs[0]);inputs[0]['id']='substitute';rows[0]=dict(rows[0],id='substitute')
            with self.assertRaises(ValueError,msg=what):audit(inputs,rows)

    def test_clz_output_gas_cap_and_funding_results_cannot_be_substituted(self):
        for what in ['clz-output','clz-gas','cap','funding','unknown']:
            index=next(i for i,s in enumerate(self.inputs) if s['id']==('clz-256' if what.startswith('clz') else 'cap-0-16777217' if what=='cap' else self.inputs[2]['id']))
            rows=list(self.rows);rows[index]=copy.deepcopy(rows[index]);r=rows[index]
            if what=='clz-output':r['output']='00'*32
            elif what=='clz-gas':r['gas_used']+=1
            elif what in ['cap','funding']:r['accepted']=not r['accepted']
            else:r['unknown']=True
            with self.assertRaises(ValueError,msg=what):audit(self.inputs,rows)

    def test_fee_roles_rollback_code_nonce_and_source_before_cannot_be_substituted(self):
        for what in ['balance','before','storage','code','nonce','operator']:
            index=next(i for i,r in enumerate(self.rows) if r['accepted'] and r['failed'])
            rows=list(self.rows);rows[index]=copy.deepcopy(rows[index]);r=rows[index];owner=self.inputs[index]['caller']
            if what=='balance':r['accounts'][owner]['balance']=str(int(r['accounts'][owner]['balance'])+1)
            elif what=='before':r['before'][owner]['balance']='0'
            elif what=='storage':r['accounts']['0x1000000000000000000000000000000000000001']['storage']['0x'+'00'*32]='0x'+'01'*32
            elif what=='code':r['accounts'][owner]['code']='0x00'
            elif what=='nonce':r['accounts'][owner]['nonce']+=1
            else:r['operator_fee']=str(int(r['operator_fee'])+1)
            with self.assertRaises(ValueError,msg=what):audit(self.inputs,rows)
