import copy
import json
import os
from pathlib import Path
import sys
import unittest
sys.path.insert(0,str(Path(__file__).resolve().parents[1]))
from op_execution_vectors import audit, cases, original_inputs, require


class OriginalExecutionReference(unittest.TestCase):
    def test_original_archive_authenticates_entire_client(self):
        refs=original_inputs();self.assertGreater(len(refs),1000)
        self.assertTrue(any(p.endswith('/core/state_transition.go')for p in refs))
        self.assertTrue(any(p.endswith('/core/vm/interpreter.go')for p in refs))

    def test_signed_matrix_covers_seven_forks_both_chains_and_access_lists(self):
        rows=cases();self.assertEqual(len(rows),448)
        self.assertEqual({r['rule'] for r in rows},{'regolith','canyon','ecotone','fjord','granite','holocene','isthmus'})
        for rule in {r['rule'] for r in rows}:
            subset=[r for r in rows if r['rule']==rule]
            self.assertEqual({r['chain'] for r in subset},{10,8453})
            self.assertEqual({r['access'] for r in subset},{True,False})
            self.assertEqual({r['outcome'] for r in subset},{'success','refund','revert','halt'})
        self.assertTrue(all(r['timestamp']>=1686789347 for r in rows if r['chain']==8453))

    def test_require_survives_optimization(self):
        with self.assertRaises(ValueError):require(False,'rejected')

    def test_independent_audit_rejects_source_funding_and_state_tampering(self):
        path=os.environ.get('XONE_OP_EXECUTION_VECTORS');self.assertIsNotNone(path)
        rows=json.loads(Path(path).read_bytes())['cases'];audit(rows)
        for field in ['before','after']:
            changed=copy.deepcopy(rows);p=changed[0]['payer']
            changed[0]['actual'][field][p]=str(int(changed[0]['actual'][field][p])+1)
            with self.assertRaises(ValueError):audit(changed)
        for field in ['l1_fee','maximum_funding','operator_maximum']:
            changed=copy.deepcopy(rows);changed[0][field]=str(int(changed[0][field])+1)
            with self.assertRaises(ValueError):audit(changed)
        for field in ['nonce_after','storage_after','gas_used']:
            changed=copy.deepcopy(rows);changed[0]['actual'][field]= '0x1'if field=='storage_after'else 2
            with self.assertRaises(ValueError):audit(changed)
        changed=copy.deepcopy(rows);changed[0]['below_funding']['accepted']=True
        with self.assertRaises(ValueError):audit(changed)
        with self.assertRaises(ValueError):audit(rows[:-1])
