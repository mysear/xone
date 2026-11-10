"""Exact recovered source binding, including metadata and strict history identity."""
import copy
import json
from pathlib import Path
import sys
import unittest
from unittest.mock import patch

TOOLS=Path(__file__).resolve().parents[1]
sys.path.insert(0,str(TOOLS))
import r01_public_catalog as catalog
import r01_public_catalog_history as history
ROOT=TOOLS.parents[2]
BASE=ROOT/'integration/test/source/deployment-catalog'

class ExactSourceBindingTests(unittest.TestCase):
    def test_exact_source_manifest_is_bound_to_catalog_runtime_and_full_input(self):
        row=next(r for r in catalog.load_json(BASE/'catalog.json')['deployments'] if r['id']=='base-aerodrome-v2')
        manifest=BASE/'metadata-exact/exact-source-manifest.json'
        report=BASE/'metadata-exact/exact-source-report-complete.json'
        row['exact_source_compilation']={'entry_id':row['id'],'input_manifest':{'path':str(manifest.relative_to(ROOT)),'sha256':catalog.sha256(manifest.read_bytes())},'report':{'path':str(report.relative_to(ROOT)),'sha256':catalog.sha256(report.read_bytes())}}
        entry,result=catalog.preflight_exact_source(ROOT,row)
        self.assertTrue(result['metadata_exact'])
        for field in ['runtime','anchor','address','id']:
            bad=copy.deepcopy(row)
            if field=='runtime':bad[field]['sha256']='00'*32
            elif field=='anchor':bad[field]['height']+=1
            elif field=='address':bad[field]='0x'+'11'*20
            else:bad[field]='different-deployment'
            with self.subTest(field=field),self.assertRaises(catalog.CatalogError):catalog.preflight_exact_source(ROOT,bad)
        altered=copy.deepcopy(result);altered['metadata_exact']=False
        loaded=catalog.json_blob
        def changed(root,ref):
            value=loaded(root,ref)
            if ref==row['exact_source_compilation']['report']:
                value['entries']=[altered if e['id']==row['id'] else e for e in value['entries']]
            return value
        with patch.object(catalog,'json_blob',side_effect=changed),self.assertRaises(catalog.CatalogError):catalog.preflight_exact_source(ROOT,row)

    def test_exact_compiler_rebuild_rejects_rehashed_source_settings_and_getter_substitution(self):
        import subprocess
        script=r"""
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto');
const h=require(process.argv[1]),m=require(process.argv[2]),root=process.argv[3];
const entry=m.entries.find(e=>e.id==='base-aerodrome-v2');
const tmp=fs.mkdtempSync(path.join(root,'integration/artifacts/local/r01-public-exact-test-'));
const sha=b=>crypto.createHash('sha256').update(b).digest('hex');
function changed(ref,mutate,name){let x=JSON.parse(fs.readFileSync(path.join(root,ref.path)));mutate(x);let b=Buffer.from(JSON.stringify(x));let p=path.join(tmp,name+'.json');fs.writeFileSync(p,b);return {path:path.relative(root,p),sha256:sha(b)}}
try {
 for(const kind of ['source','settings','compiler','getter','metadata']){
  const bad=structuredClone(entry);
  if(kind==='source')bad.input=changed(bad.input,x=>{Object.values(x.sources)[0].content+='\n// changed exact source bytes\n'},kind);
  if(kind==='settings')bad.input=changed(bad.input,x=>{x.settings.optimizer.runs+=1},kind);
  if(kind==='compiler')bad.compiler.version='0.8.20+commit.a1b79de6';
  if(kind==='getter')bad.implementation_dependencies[0].getter_selector='0x00000000';
  if(kind==='metadata')bad.artifact=changed(bad.artifact,x=>{x.metadata+=' '},kind);
  let failed=false;try{h.verifyEntry(bad)}catch(e){failed=true}
  if(!failed)throw Error('EXACT_TAMPER_ACCEPTED:'+kind);
 }
 console.log('5 exact compiler/metadata/source/getter substitutions rejected');
}finally{fs.rmSync(tmp,{recursive:true,force:true})}
"""
        p=subprocess.run(['node','-e',script,str(TOOLS/'r01_public_catalog_exact_sources.cjs'),str(BASE/'metadata-exact/exact-source-manifest.json'),str(ROOT)],capture_output=True,text=True,timeout=120)
        self.assertEqual(p.returncode,0,p.stderr)

class SBFRecordedBuildTests(unittest.TestCase):
    def test_actual_builds_are_compared_in_full_and_mismatch_never_qualifies(self):
        import base64
        row=next(r for r in catalog.load_json(BASE/'catalog.json')['deployments'] if r['id']=='solana-raydium-cpmm')
        observation=catalog.json_blob(ROOT,row['runtime']['observation'])
        payload=base64.b64decode(observation['response']['result']['value'][1]['data'][0])[45:]
        def ref(path):return {'path':str(path.relative_to(ROOT)),'sha256':catalog.sha256(path.read_bytes())}
        row['sbf_build_attempts']=[ref(BASE/'solana-verifiable'/name) for name in ['cpmm-actual-sbf-comparison.json','cpmm-modern-actual-sbf-comparison.json']]
        results=catalog.validate_sbf_build_attempts(ROOT,row,payload)
        self.assertEqual(len(results),2)
        self.assertTrue(all(not r['executable_match'] for r in results))
        # A CLI execution has its own CatalogError class; imports must not turn
        # an expected negative build comparison into malformed evidence.
        import runpy
        cli=runpy.run_path(str(TOOLS/'r01_public_catalog.py'),run_name='r01_cli_probe')
        self.assertEqual(len(cli['validate_sbf_build_attempts'](ROOT,row,payload)),2)
        altered=bytearray(payload);altered[30]^=1
        with self.assertRaises(catalog.CatalogError):catalog.validate_sbf_build_attempts(ROOT,row,bytes(altered))

    def test_clmm_and_orca_actual_builds_remain_explicit_negative_evidence(self):
        import base64
        for protocol in ['raydium-clmm','orca-whirlpools']:
            row=next(r for r in catalog.load_json(BASE/'catalog.json')['deployments'] if r['protocol']==protocol)
            observation=catalog.json_blob(ROOT,row['runtime']['observation'])
            payload=base64.b64decode(observation['response']['result']['value'][1]['data'][0])[45:]
            p=BASE/'solana-verifiable'/(protocol+'-actual-sbf-comparison.json')
            row['sbf_build_attempts']=[{'path':str(p.relative_to(ROOT)),'sha256':catalog.sha256(p.read_bytes())}]
            with self.subTest(protocol=protocol):
                result=catalog.validate_sbf_build_attempts(ROOT,row,payload)
                self.assertFalse(result[0]['executable_match'])

class HistoryIdentityTests(unittest.TestCase):
    def setUp(self):
        self.catalog=catalog.load_json(BASE/'history-catalog-qualified.json')
        self.scope=catalog.load_json(BASE/'history-scope-qualified.json')
        self.replayed={'source_compiler_binding':catalog.load_json(BASE/'history/ethereum-fresh-04/source-execution-binding-report.json')}

    def test_real_scoped_rows_and_omission_duplicate_and_version_tamper(self):
        good=history.validate_history_catalog(self.catalog,self.scope,self.replayed)
        self.assertTrue(good['qualified'],good)
        for mutation in ['missing','duplicate','version','runtime','chain','hash','height','execution','proof']:
            bad=copy.deepcopy(self.catalog);scope=copy.deepcopy(self.scope)
            if mutation=='missing':bad['entries'].pop()
            elif mutation=='duplicate':bad['entries'].append(copy.deepcopy(bad['entries'][0]))
            elif mutation=='version':
                bad['entries'][1]['version']='runtime-sha256:'+'00'*32
                scope['required'][1]['version']=bad['entries'][1]['version']
            elif mutation=='runtime':bad['entries'][0]['runtime_sha256']='00'*32
            elif mutation=='chain':bad['entries'][0]['chain']='base';scope['required'][0]['chain']='base'
            elif mutation in ['hash','height']:
                bad['entries'][1]['anchor'][mutation]='0x'+'11'*32 if mutation=='hash' else 1
                scope['required'][1]['anchor']=copy.deepcopy(bad['entries'][1]['anchor'])
            elif mutation=='execution':bad['entries'][1]['execution_case']['case_index']=1
            else:bad['entries'][0]['proof']['all_accounts']['sha256']='00'*32
            with self.subTest(mutation=mutation):self.assertFalse(history.validate_history_catalog(bad,scope,self.replayed)['qualified'])

    def test_unreported_balance_storage_code_override_is_rejected(self):
        directory=BASE/'history/ethereum-fresh-04'
        loaded=history.load_json
        for kind in ['balance_overrides','storage_overrides','code_overrides']:
            def altered(path):
                value=loaded(path)
                if Path(path).name=='fork-execution.json':value['overrides'][kind]=[{'address':'0x'+'11'*20,'value':'0x1'}]
                return value
            with self.subTest(kind=kind),patch.object(history,'load_json',side_effect=altered),self.assertRaises(catalog.CatalogError):history.environment_differences(directory,{'TIMESTAMP':4})
        with self.assertRaises(catalog.CatalogError):history.environment_differences(directory,{'BLOCKHASH':1})

if __name__=='__main__':unittest.main()
