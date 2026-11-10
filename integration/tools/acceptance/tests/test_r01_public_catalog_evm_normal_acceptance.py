"""Six declared normal cases cannot disappear, duplicate, or change H silently."""
from pathlib import Path
import sys
import unittest
TOOLS=Path(__file__).resolve().parents[1]
sys.path.insert(0,str(TOOLS))
import r01_public_catalog_evm_normal_acceptance as stage

class DeclaredNormalScopeTests(unittest.TestCase):
    def test_missing_duplicate_and_unlisted_cases_are_rejected(self):
        rows=stage.load_json(stage.BASE/'stage-scope.json')['required'];entries=[{'id':r['id']} for r in rows]
        self.assertEqual(stage.scope_errors(rows,entries),[])
        self.assertTrue(stage.scope_errors(rows,entries[:-1]))
        self.assertTrue(stage.scope_errors(rows,entries+[entries[0]]))
        self.assertTrue(stage.scope_errors(rows,entries+[{'id':'lfj-liquidity-book'}]))

    def test_old_sealed_bytes_remain_unchanged(self):
        self.assertGreater(stage.sealed_check(),2000)

if __name__=='__main__':unittest.main()
