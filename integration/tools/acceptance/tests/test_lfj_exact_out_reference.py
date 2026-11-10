from pathlib import Path
import copy,json,subprocess,sys,tempfile,unittest
sys.path.insert(0,str(Path(__file__).resolve().parents[1]))
import lfj_exact_out_reference as subject

class LfjMinimumTargetReference(unittest.TestCase):
    @classmethod
    def setUpClass(cls):cls.fixture=json.loads(subject.DEFAULT.read_bytes())
    def mutate(self,change):
        value=copy.deepcopy(self.fixture);change(value)
        with tempfile.TemporaryDirectory(prefix='lfj-target-guard-',dir=subject.OUT)as directory:
            path=Path(directory)/'reference.json';path.write_text(json.dumps(value));return subject.validate(path)
    def test_original_targets_keep_real_surplus_and_input_minus_one_controls(self):
        v=subject.validate();self.assertEqual((v['full_original_targets'],v['with_output_surplus'],v['incomplete_targets_rejected'],v['sequence_steps']),(216,60,72,18))
    def test_kind_generator_and_original_digest_changes_are_rejected(self):
        for change in [lambda f:f.__setitem__('quote_kind','EXACT_IN'),lambda f:f.__setitem__('generator_sha256','00'*32),lambda f:f['original_reference'].__setitem__('sha256','00'*32)]:
            with self.assertRaises(ValueError):self.mutate(change)
    def test_target_input_and_actual_output_cannot_be_relabelled(self):
        for change in [lambda f:f['cases'][0].__setitem__('target_output','2'),lambda f:f['cases'][0].__setitem__('required_input','1'),lambda f:f['cases'][0].__setitem__('actual_output','0'),lambda f:f['cases'][0]['expected_pool']['lfj'].__setitem__('raw_reserve0','1')]:
            with self.assertRaises(ValueError):self.mutate(change)
    def test_runtime_source_banks_and_sequences_are_bound(self):
        for change in [lambda f:f['deployment'].__setitem__('implementation_code','0x6000'),lambda f:f['source_banks'][0]['storage_reads'].__setitem__('0x3','0x02'),lambda f:f['sequences'][0]['steps'][0]['original']['expected'].__setitem__('payer_token_delta','1')]:
            with self.assertRaises(ValueError):self.mutate(change)
    def test_raw_getter_mutations_missing_reads_duplicates_and_anchor_changes_fail(self):
        original=json.loads((subject.ROOT/self.fixture['original_reference']['path']).read_bytes())
        changes=[lambda n:n['cases'][0]['getter_reads'][0].__setitem__('result','0x'+'00'*32),lambda n:n['cases'][0]['getter_reads'].pop(),lambda n:n['cases'][0]['getter_reads'].append(copy.deepcopy(n['cases'][0]['getter_reads'][0])),lambda n:n['cases'][0]['getter_reads'][0].__setitem__('block_number',0)]
        with tempfile.TemporaryDirectory(prefix='lfj-raw-guard-',dir=subject.OUT)as directory:
            path=Path(directory)/'original.json'
            for change in changes:
                value=copy.deepcopy(original);change(value);path.write_text(json.dumps(value));r=subprocess.run(['node',str(subject.ROOT/'integration/tools/ci/lfj-v21-exact-out-getters.cjs'),str(path)],capture_output=True,text=True,timeout=30);self.assertNotEqual(r.returncode,0);self.assertIn('LFJ_TARGET_GETTER_',r.stderr)

if __name__=='__main__':unittest.main()
