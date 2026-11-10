#!/usr/bin/env python3
"""Compare the full native-Linux SDK ELF against bound public ProgramData."""
import base64,hashlib,json,sys
from pathlib import Path
stage=Path(__file__).resolve().parent
root=next(p for p in stage.parents if (p/'integration/tools/acceptance/r01_public_catalog_solana_rebuild.py').is_file())
sys.path.insert(0,str(root/'integration/tools/acceptance'))
from r01_public_catalog_solana_rebuild import bind_programdata,compare_artifact,require,inspect_elf
out=stage/'output'
def sha(p):return hashlib.sha256(Path(p).read_bytes()).hexdigest()
def ref(p):return {'path':str(p.relative_to(root)),'sha256':sha(p)}
record=json.loads((out/'clmm-linux-native-build.json').read_text());probe=json.loads((out/'linux-compiler-probe.json').read_text())
require(record['native_Linux_host'] is True and record['source_bytes_unchanged'] is True,'native-build-or-source-binding')
require(record['Cargo_lock_sha256_before']==record['Cargo_lock_sha256_after']=='9370048a70726a60e281003ff798d425ce81b84ce42343d3ffdeb91def2a7d8a','lock-binding')
require(record['exit_code']==0,'actual-native-build-failed')
require(all(p['readonly_enforced'] for p in probe['readonly_probes']),'readonly-bindings')
manifest=json.loads((stage/'build-input-verification.json').read_text());expected={p['name']:p['sha256'] for p in manifest['inputs']};actual={}
for line in (out/'guest-input-hashes.txt').read_text().splitlines():
 value,path=line.split(None,1);actual[Path(path).name]=value
required_inputs={'source.tar.gz','platform.tar.bz2','vendor.tar','sdk.tar','cargo-build-sbf','criterion.tar.bz2'}
require(set(actual)==required_inputs and all(actual[name]==expected[name] for name in required_inputs),'guest-input-bytes-not-host-pins')
trusted_source=json.loads((stage/'clmm-source-verification.json').read_text())
trusted_files={p['path']:p['sha256'] for p in trusted_source['files']}
require(record['source_tree_before']==record['source_tree_after']==trusted_files,'full-source-tree-not-official-archive-bytes')
row=next(p for p in json.loads((root/'integration/test/source/deployment-catalog/catalog.json').read_text())['deployments'] if p['id']=='solana-raydium-clmm')
observation_path=root/row['runtime']['observation']['path'];require(sha(observation_path)==row['runtime']['observation']['sha256'],'public-ProgramData-observation-pin')
observation=json.loads(observation_path.read_text());binding=bind_programdata(row,observation)
payload=base64.b64decode(observation['response']['result']['value'][1]['data'][0],validate=True)[45:]
artifacts=[p for p in record['artifacts'] if p['kind']=='SDK-standard-output'];require(len(artifacts)==1,'one-complete-SDK-output-required')
artifact=out/'deploy'/Path(artifacts[0]['path']).name;require(sha(artifact)==artifacts[0]['sha256'],'retained-native-artifact-pin')
built=artifact.read_bytes();comparison=compare_artifact(payload,built)
previous=root/'integration/artifacts/local/r01-solana-rebuild/output-clmm-linux-libs/raydium_clmm.so';previous_bytes=previous.read_bytes();old_comparison=compare_artifact(payload,previous_bytes)
section_differences=[]
for name in sorted(comparison['observed_elf']['sections'].keys()|comparison['built_elf']['sections'].keys()):
 a=comparison['observed_elf']['sections'].get(name);b=comparison['built_elf']['sections'].get(name)
 if a is None or b is None:section_differences.append({'name':name,'state':'section-missing'});continue
 av=payload[a['offset']:a['offset']+a['bytes']];bv=built[b['offset']:b['offset']+b['bytes']]
 count=sum(x!=y for x,y in zip(av,bv))+abs(len(av)-len(bv))
 rowdiff={'name':name,'observed_bytes':len(av),'built_bytes':len(bv),'different_bytes':count,'complete_equal':av==bv}
 if name=='.text' and len(av)==len(bv) and len(av)%8==0:
  rowdiff['different_8byte_instruction_slots']=sum(av[i:i+8]!=bv[i:i+8] for i in range(0,len(av),8))
  rowdiff['first_different_instruction_examples']=[{'file_offset':a['offset']+i,'observed_hex':av[i:i+8].hex(),'native_build_hex':bv[i:i+8].hex()} for i in range(0,len(av),8) if av[i:i+8]!=bv[i:i+8]][:8]
 section_differences.append(rowdiff)
result={'schema_version':1,'source_commit':record['source_commit'],'programdata_binding':binding,'native_build_record':ref(out/'clmm-linux-native-build.json'),'Linux_compiler_probe':ref(out/'linux-compiler-probe.json'),'artifact':ref(artifact),'strict_full_ELF_comparison':comparison,'per_section_byte_differences':section_differences,'previous_mixed_host_artifact':ref(previous),'previous_mixed_host_comparison':old_comparison,'native_artifact_identical_to_previous_mixed_host_linux_sysroot':built==previous_bytes,'native_host_switch_resolved_exact_mismatch':comparison['matched'],'source_deployment_exact_byte_match':comparison['matched'],'source_or_lock_modified':False,'extra_ELF_normalization_or_trimming':False,'historical_original_bank_verified':False,'remaining_compiler_features_or_publisher_recipe_causality_proven':False}
(stage/'clmm-native-full-comparison.json').write_text(json.dumps(result,indent=2)+'\n')
print(json.dumps({k:result[k] for k in ['source_commit','native_artifact_identical_to_previous_mixed_host_linux_sysroot','native_host_switch_resolved_exact_mismatch','source_deployment_exact_byte_match']},indent=2));print(json.dumps({k:comparison[k] for k in ['matched','reason','mismatch_domains','first_difference','payload_sha256','artifact_sha256','artifact_bytes','tail_all_zero']},indent=2))
