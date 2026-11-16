from pathlib import Path
import copy,hashlib,json,os,sys,unittest
sys.path.insert(0,str(Path(__file__).resolve().parents[1]))
from orca_execution_reference import ROOT,verify_document,canonical
class OrcaExecutionEvidence(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        folder=Path(os.environ.get('XONE_ORCA_EXECUTION_DIR',ROOT/'integration/artifacts/local/orca-execution-preflight4/api'))
        cls.doc=json.loads((folder/'orca-15-preparation.json').read_bytes());cls.bank=json.loads((ROOT/'integration/test/contracts/orca-execution-bank.json').read_bytes())['cases'][15]
    def rejected(self,change,reason,rehash=False):
        d=copy.deepcopy(self.doc);change(d)
        if rehash:
            d['preparation']['preparationHash']=hashlib.sha256(canonical({k:v for k,v in d['preparation'].items()if k!='preparationHash'})).hexdigest()
            d['independent']['preparationHash']=d['preparation']['preparationHash']
        with self.assertRaisesRegex(ValueError,reason):verify_document(d,self.bank)
    def test_original_record(self):verify_document(self.doc,self.bank)
    def test_compute_limit_cannot_change(self):self.rejected(lambda d:d['preparation'].update(computeUnitLimit=200000),'COMPUTE_BUDGET')
    def test_signed_cannot_be_unsigned(self):self.rejected(lambda d:d['preparation'].update(signed=True),'UNSIGNED_BOUNDARY')
    def test_preparation_cannot_change(self):self.rejected(lambda d:d['preparation'].update(quotedInput='999'),'PREPARATION_HASH')
    def test_quote_cannot_be_substituted(self):self.rejected(lambda d:d['quote'].update(quoteId='other'),'QUOTE_BINDING')
    def test_native_expected_input_cannot_change(self):self.rejected(lambda d:d['preparation'].update(quotedInput='999'),'QUOTED_NATIVE_AMOUNTS',True)
    def test_rpc_success_is_not_independent_evidence(self):self.rejected(lambda d:d['independent'].update(accountSource='RPC_SIMULATED'),'INDEPENDENT_SCOPE')
    def test_changed_source_bank_is_rejected(self):self.rejected(lambda d:d['independent'].update(sourceBankUnchanged=False),'INDEPENDENT_SCOPE')
    def test_production_claim_is_rejected(self):self.rejected(lambda d:d['independent'].update(independentProductionVerification=True),'INDEPENDENT_SCOPE')
    def test_evidence_for_other_wire_is_rejected(self):self.rejected(lambda d:d['independent'].update(transactionSha256='0'*64),'ACTUAL_WIRE_EVIDENCE')
    def test_actual_credit_cannot_change(self):self.rejected(lambda d:d['independent'].update(actualOutput='999'),'EVIDENCE_AMOUNTS')
    def test_post_accounts_cannot_change(self):self.rejected(lambda d:d['independent']['postAccounts'][0].update(lamports=str(int(d['independent']['postAccounts'][0]['lamports'])+1)),'POST_ACCOUNT_HASH')
    def test_zero_lamport_post_account_is_still_protected(self):self.rejected(lambda d:next(a for a in d['independent']['postAccounts']if a['lamports']=='0').update(lamports='1'),'POST_ACCOUNT_HASH')
    def test_snapshot_cannot_be_historical_proof(self):self.rejected(lambda d:d['preparation']['accountSnapshot'].update(historicalBankVerified=True),'CAPTURE_SCOPE',True)
    def test_network_fee_cannot_change(self):self.rejected(lambda d:d['preparation'].update(feeLamports='0'),'NETWORK_FEE',True)
    def test_unchecked_evidence_field_is_committed(self):self.rejected(lambda d:d['independent'].update(unitsConsumed=1),'EVIDENCE_HASH')
if __name__=='__main__':unittest.main()
