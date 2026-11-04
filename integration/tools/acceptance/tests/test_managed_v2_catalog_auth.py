#!/usr/bin/env python3
"""Mutate only a new tmp reference copy; current ROOT qualification runs separately."""
import json,os,pathlib,shutil,subprocess,tempfile,unittest
SOURCE=pathlib.Path(os.environ.get('XONE_SOURCE_ROOT',pathlib.Path(__file__).resolve().parents[4])).resolve()
NODE=os.environ.get('XONE_NODE','/usr/local/bin/node')
GEN='integration/tools/ci/managed-v2-aum-catalog-golden.cjs'
CHECK='integration/tools/ci/managed-v2-aum-catalog-check.cjs'
PRODUCER_SOURCE=pathlib.Path(os.environ.get('XONE_CATALOG_PRODUCER',SOURCE/GEN)).resolve()
CHECKER_SOURCE=pathlib.Path(os.environ.get('XONE_CATALOG_CHECKER',SOURCE/CHECK)).resolve()
RAW=pathlib.Path(os.environ.get('XONE_CATALOG_RAW',SOURCE/'integration/test/contracts/managed-v2-aum-catalog.json')).resolve()
T=REF=WORK=None

def setup_copy():
    global T,REF,WORK
    WORK=tempfile.TemporaryDirectory(prefix='managed-v2-catalog-auth-',dir=os.environ.get('XONE_CATALOG_TEST_TMP'))
    T=pathlib.Path(WORK.name);REF=T/'integration/tools/reference/balancer-managed-v2-aum'
    shutil.copytree(SOURCE/'integration/tools/reference/balancer-managed-v2-aum',REF,dirs_exist_ok=True)
    for rel in ['integration/test/contracts/managed-v2-aum.json','integration/test/contracts/managed-v2-aum-pools.json',
                'integration/tools/ci/managed-v2-aum-golden.cjs','integration/tools/ci/managed-v2-aum-check.py',
                'integration/tools/ci/managed-v2-aum-guards.py','integration/tools/ci/managed-v2-aum-paused-vault-golden.cjs']:
        p=T/rel;p.parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(SOURCE/rel,p)
    pins=json.loads((REF/'runtime-pins.json').read_text())
    for row in pins['files']:
        p=T/row['path'];p.parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(SOURCE/row['path'],p)
    p=T/'integration/.runtime/stable-reference/package.json';shutil.copyfile(SOURCE/'integration/.runtime/stable-reference/package.json',p)
    for source,rel in [(PRODUCER_SOURCE,GEN),(CHECKER_SOURCE,CHECK)]:shutil.copyfile(source,T/rel)

def invoke(js=None, check=False):
    env=dict(os.environ,XONE_SOURCE_ROOT=str(T));env.pop('XONE_CATALOG_PRODUCER',None)
    if check:command=[NODE,str(T/CHECK),str(RAW)]
    else:command=[NODE,'-e',"const P=require(process.argv[1]);"+js,str(T/GEN)]
    return subprocess.run(command,env=env,capture_output=True,text=True)

def inventory():
    p=invoke('console.log(JSON.stringify(P.preservation()))');
    if p.returncode:raise RuntimeError(p.stderr[-4000:])
    return json.loads(p.stdout)

class Boundary(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        setup_copy();cls.base=inventory()
    def test_exact_old_inputs_manifest_compiler_licenses_and_new_tools(self):
        paths={r['path'] for r in self.base};prefix='integration/tools/reference/balancer-managed-v2-aum/'
        original=json.loads((SOURCE/'integration/test/contracts/managed-v2-aum.json').read_text())['reference']['inputs']
        self.assertTrue({r['path'] for r in original['files']} <= paths)
        self.assertTrue({prefix+'file-manifest.json',prefix+'source-pins.json',prefix+'compiler/soljson-v0.7.1.js',prefix+'licenses/provenance.json',GEN,CHECK} <= paths)
        self.assertFalse({prefix+n for n in ['generation.json','verified.json','output-0.pending','output-1.pending','output-2.pending']} & paths)
    def test_verified_add_delete_passed_flip_same_fixed_bank_qualification(self):
        p=REF/'verified.json';saved=p.read_bytes() if p.exists() else None
        try:
            for value in [None,{'passed':False}, {'passed':True,'unrelated_receipt':'different output'}]:
                if value is None:p.unlink(missing_ok=True)
                else:p.write_text(json.dumps(value)+'\n')
                self.assertEqual(inventory(),self.base)
                result=invoke(check=True);self.assertEqual(result.returncode,0,result.stderr[-4000:])
        finally:
            if saved is None:p.unlink(missing_ok=True)
            else:p.write_bytes(saved)
    def test_all_fixed_receipts_pending_are_outputs_only(self):
        for name in ['generation.json','output-0.pending','output-1.pending','output-2.pending']:
            with self.subTest(name=name):
                p=REF/name;saved=p.read_bytes() if p.exists() else None
                try:
                    p.unlink(missing_ok=True);self.assertEqual(inventory(),self.base)
                    p.write_text('{"passed":true}\n');self.assertEqual(inventory(),self.base)
                finally:
                    if saved is None:p.unlink(missing_ok=True)
                    else:p.write_bytes(saved)
    def test_unknown_files_even_nested_known_basename_rejected(self):
        for name in ['unknown-input.json','unknown.pending','licenses/verified.json','licenses/generation.json','licenses/file-manifest.json']:
            with self.subTest(name=name):
                p=REF/name;self.assertFalse(p.exists());p.write_text('{"passed":true}\n')
                try:self.assertNotEqual(invoke('P.preservation()').returncode,0)
                finally:p.unlink()
    def test_missing_original_source_and_manifest_rejected(self):
        for name in ['tasks/helpers/time.ts','file-manifest.json']:
            with self.subTest(name=name):
                p=REF/name;saved=p.read_bytes();p.unlink()
                try:self.assertNotEqual(invoke('P.preservation()').returncode,0)
                finally:p.write_bytes(saved)
    def test_changed_source_pin_compiler_license_sha_rejected(self):
        for name in ['source-pins.json','compiler/soljson-v0.7.1.js','licenses/provenance.json']:
            with self.subTest(name=name):
                p=REF/name;saved=p.read_bytes();p.write_bytes(saved+b'\n')
                try:self.assertNotEqual(invoke('P.preservation()').returncode,0)
                finally:p.write_bytes(saved)
    def test_manifest_member_remove_sha_change_and_manifest_bytes_rejected(self):
        p=REF/'file-manifest.json';saved=p.read_bytes()
        try:
            for mutation in ['remove-member','change-member-sha','whitespace-only']:
                with self.subTest(mutation=mutation):
                    m=json.loads(saved)
                    if mutation=='remove-member':m['files'].pop()
                    elif mutation=='change-member-sha':m['files'][0]['sha256']='0'*64
                    p.write_bytes(saved+b'\n' if mutation=='whitespace-only' else (json.dumps(m)+'\n').encode())
                    self.assertNotEqual(invoke('P.preservation()').returncode,0)
        finally:p.write_bytes(saved)
    def test_checker_dependency_digest_is_bound_to_same_raw(self):
        p=T/CHECK;saved=p.read_bytes()
        try:
            p.write_bytes(saved+b'\n');result=invoke(check=True)
            self.assertNotEqual(result.returncode,0)
        finally:p.write_bytes(saved)
    @classmethod
    def tearDownClass(cls):
        try:
            if inventory()!=cls.base:raise AssertionError('negative test copy not restored')
        finally:WORK.cleanup()

if __name__=='__main__':unittest.main(verbosity=2)
