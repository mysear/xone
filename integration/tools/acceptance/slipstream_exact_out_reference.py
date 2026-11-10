#!/usr/bin/env python3
"""Authenticate the fixed Slipstream negative-output reference and every ABI read."""
import json
import subprocess
from pathlib import Path
from weighted_evidence import ROOT, OUT, require, sha
from pancake_bsc4_acceptance import bound, runtime_match
DEFAULT=ROOT/'integration/test/contracts/slipstream-exact-out.json'


def validate(path=DEFAULT):
    f=json.loads(Path(path).read_bytes())
    require(f.get('schema_version')==1 and f.get('quote_kind')=='EXACT_OUT','SLIPSTREAM_XO_REFERENCE_KIND')
    require(f['generator_sha256']==sha(ROOT/'integration/tools/ci/slipstream-exact-out-state.py'),'SLIPSTREAM_XO_NORMALIZER_CHANGED')
    ref=f['original_reference'];bound(ref['path'],ref['sha256'])
    require(ref['sha256']==f['source_reference_sha256'],'SLIPSTREAM_XO_REFERENCE_IDENTITY')
    original_path=ROOT/ref['path'];d=json.loads(original_path.read_bytes())
    require(d.get('passed') is True and d['quote_kind']=='EXACT_OUT' and d['generator_sha256']==sha(ROOT/'integration/tools/ci/slipstream-exact-out-reference.cjs'),'SLIPSTREAM_XO_GENERATOR_CHANGED')
    for entry in d['runtime']['loaded_dependencies']:
        p=bound(entry['path'],entry['sha256']);require((ROOT/p['path']).stat().st_size==entry['bytes'],'SLIPSTREAM_XO_DEPENDENCY_SIZE')
    base=ROOT/'integration/tools/reference/slipstream-extended'
    require(sha(base/'source-metadata.json')==d['pinned_source_manifest_sha256'],'SLIPSTREAM_XO_METADATA_CHANGED')
    metadata=json.loads((base/'source-metadata.json').read_bytes())
    require(metadata['commit']==d['source_commit']==f['source_commit'],'SLIPSTREAM_XO_SOURCE_COMMIT')
    for name,digest in metadata['files'].items():bound(str((base/name).relative_to(ROOT)),digest)
    sealed=ROOT/'integration/test/source/deployment-catalog/metadata-exact/base-aerodrome-slipstream/exact-standard-input.json'
    require(sha(sealed)==d['retained_source_input_sha256'] and sha(base/'voter-verified-source.json')==d['voter_source_sha256'],'SLIPSTREAM_XO_RETAINED_SOURCE_CHANGED')
    artifacts=json.loads((OUT/'core-slipstream-context-runtime-artifacts.json').read_bytes())
    require(artifacts['source_input_sha256']==d['retained_source_input_sha256'] and artifacts['module_manifest_sha256']==d['pinned_source_manifest_sha256'],'SLIPSTREAM_XO_ARTIFACT_SOURCE_BINDING')
    require(f['deployment']==d['deployment'] and f['code_templates']==d['code_templates'],'SLIPSTREAM_XO_SHIPPED_IDENTITY')
    for role,artifact in [('implementation','pool'),('factory','factory'),('swap_custom','custom_swap'),('unstaked_custom','custom_unstaked'),('swap_dynamic','dynamic_swap')]:runtime_match(d['deployment'][role+'_code'],artifacts['contracts'][artifact])
    voter=json.loads((base/'voter-verified-source.json').read_bytes())
    require(d['deployment']['voter_code'].lower()==voter['runtimeBytecode']['onchainBytecode'].lower(),'SLIPSTREAM_XO_VOTER_RUNTIME_CHANGED')
    require(len(d['cases'])==len(f['cases'])==108 and len(d['sequences'])==4 and sum(len(s['steps'])for s in d['sequences'])==12,'SLIPSTREAM_XO_CORPUS_SCOPE')
    complete=partial=0
    for index,(raw,case)in enumerate(zip(d['cases'],f['cases'])):
        require(case['index']==index and case['original']==raw,'SLIPSTREAM_XO_CASE_IDENTITY')
        require(raw['amount_in']=='-'+raw['target_output'] and int(raw['target_output'])>0 and case['target_output']==raw['target_output'],'SLIPSTREAM_XO_NEGATIVE_SPECIFIED_OUTPUT')
        before,after,e=raw['before'],raw['expected']['after'],raw['expected'];side=0 if raw['zero_for_one'] else 1;other=1-side
        require(not e.get('reverted') and e['receipt']['status']==1 and 0<int(e['amount_out'])<=int(raw['target_output']) and e['payer_delta']==e['consumed_in'],'SLIPSTREAM_XO_ACTUAL_FUNDS')
        require(int(after['raw_balance'+str(side)])==int(before['raw_balance'+str(side)])+int(e['consumed_in']) and int(after['raw_balance'+str(other)])==int(before['raw_balance'+str(other)])-int(e['amount_out']),'SLIPSTREAM_XO_POOL_BALANCE_CONSERVATION')
        full=e['amount_out']==raw['target_output'];complete+=full;partial+=not full
        require(case['expected']['partial']==(not full) and not case['expected']['reverted'],'SLIPSTREAM_XO_PARTIAL_CLASSIFICATION')
    require((complete,partial)==(90,18),'SLIPSTREAM_XO_EXECUTION_COUNTS')
    for raw,seq in zip(d['sequences'],f['sequences']):
        require(raw['initial_case_index']==seq['initial_case_index'],'SLIPSTREAM_XO_SEQUENCE_BANK')
        for step,observed in zip(raw['steps'],seq['steps']):
            require(step['amount_in']=='-'+step['target_output'] and step['amount_out']==step['target_output'] and step['payer_delta']==step['consumed_in'] and observed['original']==step,'SLIPSTREAM_XO_SEQUENCE_FULL_OUTPUT')
    for command,reason in [(['node',str(ROOT/'integration/tools/ci/slipstream-exact-out-getters.cjs'),str(original_path)],'SLIPSTREAM_XO_RAW_ABI_READ_CHANGED'),(['python3',str(ROOT/'integration/tools/ci/slipstream-exact-out-state.py'),'--verify',str(Path(path).resolve())],'SLIPSTREAM_XO_RAW_PROJECTION_CHANGED')]:
        p=subprocess.run(command,capture_output=True,text=True,timeout=60);require(p.returncode==0,reason+':'+p.stderr[-350:])
    return dict(schema_version=1,passed=True,reference=bound(str(Path(path).relative_to(ROOT))),original=bound(ref['path'],ref['sha256']),full_original_swaps=90,partial_swaps_rejected=18,original_sequence_steps=12,raw_abi_banks=228,raw_abi_reads=12960,authority='fixed unchanged original Slipstream negative-output swaps, origin-specific module fees, real caller-contract token debits and recipient credits under controlled initialized banks; no public bank qualification',all_requested_features_complete=False,production_ready=False)


if __name__=='__main__':print(json.dumps(validate(),indent=2))
