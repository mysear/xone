import copy
import json
import os
from pathlib import Path
import sys
import unittest

sys.path.insert(0,str(Path(__file__).resolve().parents[1]))
from op_chain_cost_vectors import audit, cases, original_inputs, require


class OriginalTransitionReference(unittest.TestCase):
    def test_original_archive_authenticates_whole_client(self):
        refs=original_inputs()
        self.assertGreater(len(refs),1000)
        self.assertTrue(any(p.endswith('/core/state_transition.go')for p in refs))
        self.assertTrue(any(p.endswith('/core/vm/interpreter.go')for p in refs))

    def test_signed_scenario_matrix_keeps_rules_outcomes_and_aliases(self):
        rows=cases()
        self.assertEqual(len(rows),320)
        self.assertEqual({r['rule']for r in rows},{'regolith','ecotone','fjord','isthmus','jovian'})
        self.assertEqual({r['kind']for r in rows},{'legacy','access','dynamic'})
        self.assertEqual({r['beneficiary']for r in rows},{'separate','payer','base','l1','operator'})

    def test_require_survives_optimization(self):
        with self.assertRaises(ValueError):require(False,'rejected')

    def test_independent_account_audit_rejects_tampered_vm_results(self):
        path=os.environ.get('XONE_OP_CHAIN_COST_VECTORS')
        self.assertIsNotNone(path,'current original transition results required')
        rows=json.loads(Path(path).read_bytes())['cases']
        self.assertEqual(audit(rows)['below_intrinsic'],0)
        for field in ['before','after']:
            changed=copy.deepcopy(rows)
            payer=changed[0]['payer']
            changed[0]['actual'][field][payer]=str(int(changed[0]['actual'][field][payer])+1)
            with self.assertRaises(ValueError):audit(changed)
        for field in ['l1_fee','maximum_funding','operator_maximum','effective_gas_price']:
            changed=copy.deepcopy(rows);changed[0][field]=str(int(changed[0][field])+1)
            with self.assertRaises(ValueError):audit(changed)
        changed=copy.deepcopy(rows);changed[0]['actual']['operator_fee']='1'
        with self.assertRaises(ValueError):audit(changed)
        changed=copy.deepcopy(rows);changed[0]['actual']['gas_used']+=1
        with self.assertRaises(ValueError):audit(changed)
        with self.assertRaises(ValueError):audit(rows[:-1])
