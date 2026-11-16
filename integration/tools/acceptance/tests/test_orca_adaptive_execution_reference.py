from pathlib import Path
import base64,copy,hashlib,json,os,sys,tempfile,unittest
sys.path.insert(0,str(Path(__file__).resolve().parents[1]))
from orca_adaptive_execution_reference import ROOT,verify_document,canonical,snapshot_records
class OrcaAdaptiveExecutionEvidence(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        folder=Path(os.environ.get('XONE_ORCA_ADAPTIVE_EXECUTION_DIR',ROOT/'integration/artifacts/local/orca-adaptive-execution-api-preflight/api'))
        cls.doc=json.loads((folder/'orca-adaptive-10-preparation.json').read_bytes());cls.bank=json.loads((ROOT/'integration/test/contracts/orca-adaptive-execution-bank.json').read_bytes())['cases'][10]
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
class CapturedProgramClosure(unittest.TestCase):
    def records(self,second_owner='loader',second_lamports=1):
        self.tmp=tempfile.TemporaryDirectory();self.addCleanup(self.tmp.cleanup);folder=Path(self.tmp.name);expected={}
        program=dict(owner='loader',lamports=1,executable=False,data=[base64.b64encode(bytes(50000)).decode(),'base64'])
        for index in range(2):
            a=copy.deepcopy(program)
            if index:a.update(owner=second_owner,lamports=second_lamports)
            ids=['program','user'];values=[a,dict(owner='system',lamports=index,executable=False,data=['','base64'])]
            record=dict(chainKey='solana:controlled',genesisHash='controlled',request=dict(params=[ids]),response=dict(result=dict(context=dict(slot=100),value=values)))
            (folder/f'getMultipleAccounts-{index}.json').write_text(json.dumps(record))
            capture=dict(schemaVersion=1,chainKey=record['chainKey'],genesisHash=record['genesisHash'],observedSlot=100,accounts=dict(zip(ids,values)));expected[hashlib.sha256(canonical(capture)).hexdigest()]=capture
        return snapshot_records(folder),expected
    def test_identical_program_bytes_share_storage_without_changing_snapshots(self):
        observed,expected=self.records();self.assertEqual({k:v[0]for k,v in observed.items()},expected);values=[v[0]['accounts']['program']for v in observed.values()];self.assertIs(values[0],values[1])
    def test_program_owner_difference_is_never_collapsed(self):
        observed,expected=self.records(second_owner='other-loader');self.assertEqual({k:v[0]for k,v in observed.items()},expected);values=[v[0]['accounts']['program']for v in observed.values()];self.assertIsNot(values[0],values[1])
    def test_program_lamport_difference_is_never_collapsed(self):
        observed,expected=self.records(second_lamports=2);self.assertEqual({k:v[0]for k,v in observed.items()},expected);values=[v[0]['accounts']['program']for v in observed.values()];self.assertIsNot(values[0],values[1])
if __name__=='__main__':unittest.main()
