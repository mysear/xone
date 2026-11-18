import copy
import json
import os
from pathlib import Path
import sys
import unittest
sys.path.insert(0,str(Path(__file__).resolve().parents[1]))
from karst_api_vectors import audit


class OriginalKarstNormalAPI(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        base=Path(os.environ['XONE_KARST_API_REFERENCE']);cls.v=json.loads((base/'bound-vectors.json').read_bytes());cls.sources=json.loads((base/'karst-api-sources.json').read_bytes());cls.inputs=cls.v['inputs'];cls.rows=cls.v['cases']

    def test_complete_original_router_modes_and_fee_models(self):
        audit(self.inputs,self.rows,self.sources)
        self.assertEqual(len(self.rows),4)
        self.assertEqual({s['id'].rsplit('-',2)[1] for s in self.inputs},{'0','2'})
        self.assertEqual({s['id'].rsplit('-',1)[1] for s in self.inputs},{'in','out'})

    def test_reordered_rebound_or_missing_inputs_are_rejected(self):
        for change in ['drop','reorder','replace']:
            inputs=list(self.inputs);rows=list(self.rows)
            if change=='drop':rows.pop()
            elif change=='reorder':rows[0],rows[1]=rows[1],rows[0]
            else:
                inputs[0]=dict(inputs[0],id='replacement');rows[0]=dict(rows[0],id='replacement')
            with self.assertRaises(ValueError,msg=change):audit(inputs,rows,self.sources)

    def test_source_fees_nonce_code_and_storage_coverage_are_bound(self):
        for change in ['before','balance','operator','nonce','code','storage','unknown']:
            rows=copy.deepcopy(self.rows);r=rows[0];caller=self.inputs[0]['caller'].lower()
            if change=='before':r['before'][caller]['balance']='0'
            elif change=='balance':r['accounts'][caller]['balance']=str(int(r['accounts'][caller]['balance'])+1)
            elif change=='operator':r['operator_fee']=str(int(r['operator_fee'])+1)
            elif change=='nonce':r['accounts'][caller]['nonce']+=1
            elif change=='code':r['accounts'][caller]['code']='0x00'
            elif change=='storage':r['accounts'][self.inputs[0]['token_in']]['storage'].popitem()
            else:r['unknown']=True
            with self.assertRaises(ValueError,msg=change):audit(self.inputs,rows,self.sources)

    def test_signed_amount_and_result_substitutions_are_rejected(self):
        for change in ['spent','bought','success','gas']:
            index=next(i for i,s in enumerate(self.inputs) if s['id'].endswith('-in' if change=='spent' else '-out'))
            rows=copy.deepcopy(self.rows);r=rows[index]
            if change in ['spent','bought']:r[change]=str(int(r[change])+1)
            elif change=='success':r['failed']=True
            else:r['gas_used']=30_000_000
            with self.assertRaises(ValueError,msg=change):audit(self.inputs,rows,self.sources)
