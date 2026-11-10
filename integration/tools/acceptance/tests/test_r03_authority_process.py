import copy
import json
from pathlib import Path
import sys
from tempfile import TemporaryDirectory
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import r03_authority_process as runner


class AuthorityArtifactTests(unittest.TestCase):
    def setUp(self):
        self.temp = TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        old_root = runner.ROOT
        self.addCleanup(setattr, runner, 'ROOT', old_root)
        runner.ROOT = Path(self.temp.name).resolve()
        self.binary = runner.ROOT / 'chain-parser-rust/target/debug/deps/normal_yellowstone-test'
        self.binary.parent.mkdir(parents=True)
        self.binary.write_bytes(b'test fixture executable identity')
        self.record = {'reason':'compiler-artifact', 'target':{'name':'normal_yellowstone','kind':['test']},
                       'profile':{'test':True}, 'features':['external-authority-tests'],
                       'executable':str(self.binary)}

    def test_selects_exact_compiled_integration_target_among_other_output(self):
        other = copy.deepcopy(self.record)
        other['target']['name'] = 'another-test'
        text = 'compiler progress\n' + json.dumps(other) + '\n' + json.dumps(self.record)
        self.assertEqual(runner.executable_from_artifacts(text), self.binary)

    def test_missing_duplicate_or_non_feature_artifacts_fail_closed(self):
        invalid = copy.deepcopy(self.record)
        invalid['features'] = []
        for text in ['', json.dumps(self.record)+'\n'+json.dumps(self.record), json.dumps(invalid)]:
            with self.subTest(text=text), self.assertRaises(RuntimeError):
                runner.executable_from_artifacts(text)

    def test_non_test_artifact_cannot_satisfy_authority_scenario(self):
        for field, value in [('kind',['bin']),('test',False)]:
            record = copy.deepcopy(self.record)
            record['target' if field == 'kind' else 'profile'][field] = value
            with self.subTest(field=field), self.assertRaises(RuntimeError):
                runner.executable_from_artifacts(json.dumps(record))

    def test_missing_or_outside_executable_fails(self):
        for path in [self.binary.with_name('missing'), runner.ROOT/'other-project/executable']:
            invalid = copy.deepcopy(self.record)
            invalid['executable'] = str(path)
            with self.subTest(path=path), self.assertRaises(RuntimeError):
                runner.executable_from_artifacts(json.dumps(invalid))


if __name__ == '__main__':
    unittest.main()
