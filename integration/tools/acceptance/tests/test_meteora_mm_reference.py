from pathlib import Path
import copy,hashlib,json,sys,unittest
sys.path.insert(0,str(Path(__file__).resolve().parents[1]))
import meteora_mm_reference as reference
class TestMeteoraReference(unittest.TestCase):
    @classmethod
    def setUpClass(cls):cls.original=json.loads(reference.ORIGINAL.read_bytes());cls.projected=json.loads(reference.PROJECTED.read_bytes())
    def validate(self,o,p):
        digest=hashlib.sha256(json.dumps(o,sort_keys=True).encode()).hexdigest();p['source_sha256']=digest
        return reference.verify(o,p,digest,verify_files=False)
    def test_valid_original_and_projected_records(self):self.assertTrue(self.validate(copy.deepcopy(self.original),copy.deepcopy(self.projected))['passed'])
    def test_rehashed_output_amount_and_bank_coverage_changes_fail(self):
        for mutation in ['input','output','case-drop','array-drop','array-duplicate','oracle-byte','native-verdict','sequence-drop','failed-bank']:
            with self.subTest(mutation=mutation):
                o,p=copy.deepcopy(self.original),copy.deepcopy(self.projected);c=o['cases'][3];r=p['cases'][3]['steps'][0]
                if mutation=='input':r['input']='999'
                elif mutation=='output':c['recipient_output_delta']='999';c['sequence'][0]['recipient_output_delta']='999';r['output']='999'
                elif mutation=='case-drop':o['cases'].pop();p['cases'].pop()
                elif mutation=='array-drop':r['pool']['meteora']['bin_arrays'].pop()
                elif mutation=='array-duplicate':r['pool']['meteora']['bin_arrays'].append(copy.deepcopy(r['pool']['meteora']['bin_arrays'][0]))
                elif mutation=='oracle-byte':r['after']['meteora']['oracle_data']='00'+r['after']['meteora']['oracle_data'][2:]
                elif mutation=='native-verdict':c['sequence'][0]['native']['err']={'fake':1}
                elif mutation=='sequence-drop':c['sequence'].pop();p['cases'][3]['steps'].pop()
                elif mutation=='failed-bank':o['cases'][0]['after'][reference.PAIR]['lamports']+=1
                with self.assertRaises(ValueError):self.validate(o,p)
class TestMeteoraEdgeReference(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        import meteora_mm_edge_reference as edge
        cls.edge=edge;cls.original=json.loads(edge.ORIGINAL.read_bytes());cls.projected=json.loads(edge.PROJECTED.read_bytes())
    def validate(self,o,p):
        digest=hashlib.sha256(json.dumps(o,sort_keys=True).encode()).hexdigest();p['source_sha256']=digest
        return self.edge.verify(o,p,digest)
    def test_valid_additional_original_boundary_banks(self):self.assertEqual(self.validate(copy.deepcopy(self.original),copy.deepcopy(self.projected))['original_swaps'],23)
    def test_rehashed_edge_oracle_counters_and_real_transfers_fail(self):
        import base64
        for mutation in ['oracle-counter','actual-output','duplicate-case','program-code']:
            with self.subTest(mutation=mutation):
                o,p=copy.deepcopy(self.original),copy.deepcopy(self.projected);n=o['records'][3]['native'];row=p['cases'][3]
                if mutation=='oracle-counter':
                    oracle='Fnkg415DEx72GSPooKUWTbPS9wzKucQe4qnvFrrvcZK2';raw=bytearray(base64.b64decode(n['after'][oracle]['data_base64']));raw[8]^=1;n['after'][oracle]['data_base64']=base64.b64encode(raw).decode();row['after']['meteora']['oracle_data']=raw.hex()
                elif mutation=='actual-output':n['recipient_output_delta']='999';row['output']='999'
                elif mutation=='duplicate-case':o['records'][4]=copy.deepcopy(o['records'][3]);p['cases'][4]=copy.deepcopy(p['cases'][3])
                else:n['program_sha256']='00'*32
                with self.assertRaises(ValueError):self.validate(o,p)
if __name__=='__main__':unittest.main()
