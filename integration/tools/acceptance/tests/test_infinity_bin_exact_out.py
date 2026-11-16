import copy,json,sys,unittest
from pathlib import Path
sys.path.insert(0,str(Path(__file__).resolve().parents[1]))
from verify_infinity_bin_exact_out import ROOT,validate
class BinOutputEvidence(unittest.TestCase):
    @classmethod
    def setUpClass(cls):cls.original=json.loads((ROOT/'integration/test/contracts/infinity-bin-exact-out.json').read_bytes())
    def rejected(self,mutate,reason):
        c=copy.deepcopy(self.original);mutate(c)
        with self.assertRaisesRegex(ValueError,reason):validate(c)
    def test_target_cannot_change(self):self.rejected(lambda c:c['cases'][0].update(target='123'),'NATIVE_TARGET_AND_INPUT')
    def test_protocol_fee_cannot_be_removed(self):
        def mutate(c):next(x for x in c['cases']if x['passed']and int(x['native']['protocol_amount'])>0)['native']['protocol_amount']='0'
        self.rejected(mutate,'BIN_CASH_CONSERVATION')
    def test_tree_cannot_omit_members(self):self.rejected(lambda c:c['cases'][0]['native']['post']['tree'].update(level0='0x0'),'TREE_ROOT')
    def test_swap_cannot_mint_shares(self):self.rejected(lambda c:c['cases'][0]['native']['post']['raw_bins'][0].update(total_shares='1'),'SWAP_SHARES')
    def test_app_projection_cannot_hide_input(self):self.rejected(lambda c:c['cases'][0]['after']['infinity_bin'].update(app_reserve0='0'),'FULL_PROJECTION')
    def test_revert_cannot_modify_pool(self):
        def mutate(c):next(x for x in c['cases']if not x['passed'])['native']['post']['active_id']+=1
        self.rejected(mutate,'NATIVE_ROLLBACK')
    def test_continuation_cannot_reset(self):self.rejected(lambda c:c['sequences'][0]['steps'][1].update(pool=copy.deepcopy(c['sequences'][0]['steps'][0]['pool'])),'SEQUENCE_STATE')
    def test_grid_cannot_claim_unexecuted_input(self):self.rejected(lambda c:c['grids'][0].update(bestRequiredInput='1'),'GRID_MINIMUM')
    def test_detached_dust_cannot_be_erased(self):
        def mutate(c):next(x for x in c['cases']if 'burn'in x)['pool']['infinity_bin']['detached_active']=None
        self.rejected(mutate,'NATIVE_PRESTATE')
    def test_cases_cannot_be_omitted(self):self.rejected(lambda c:c['cases'].pop(),'CASE_COUNT')
if __name__=='__main__':unittest.main()
