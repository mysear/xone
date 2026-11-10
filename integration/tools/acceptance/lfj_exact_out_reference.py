#!/usr/bin/env python3
"""Bind original target views to actual payer/recipient deltas and raw banks."""
from pathlib import Path
import json
import subprocess
import sys
from weighted_evidence import ROOT,OUT,require,sha
from pancake_bsc4_acceptance import bound
DEFAULT=ROOT/'integration/test/contracts/lfj-exact-out.json'

def validate(path=DEFAULT):
    f=json.loads(Path(path).read_bytes());require(f['schema_version']==1 and f['quote_kind']=='EXACT_OUT_MINIMUM_TARGET','LFJ_XO_REFERENCE_KIND')
    require(f['generator_sha256']==sha(ROOT/'integration/tools/ci/lfj-v21-exact-out-state.py'),'LFJ_XO_NORMALIZER_CHANGED')
    ref=f['original_reference'];bound(ref['path'],ref['sha256']);raw_path=ROOT/ref['path'];n=json.loads(raw_path.read_bytes())
    require(n['passed'] is True and n['quote_kind']=='ORIGINAL_GET_SWAP_IN_MINIMUM_TARGET' and n['generator_sha256']==sha(ROOT/'integration/tools/ci/lfj-v21-exact-out-reference.cjs'),'LFJ_XO_GENERATOR_CHANGED')
    base=ROOT/'integration/tools/reference/lfj-v21';meta=json.loads((base/'source-metadata.json').read_bytes());require(sha(base/'source-metadata.json')==n['source_metadata_sha256'] and meta['tag']==n['source_tag'],'LFJ_XO_ORIGINAL_SOURCE_METADATA')
    for name,digest in meta['files'].items():bound(str((base/name).relative_to(ROOT)),digest)
    for name,entry in meta['external_files'].items():bound(str((base/name).relative_to(ROOT)),entry['sha256'])
    require('0x'+sha(ROOT/'integration/.runtime/lfj-solc'/meta['compiler']['path'])==meta['compiler']['sha256'],'LFJ_XO_ORIGINAL_COMPILER')
    for entry in n['runtime']['loaded_dependencies']:
        b=bound(entry['path'],entry['sha256']);require((ROOT/b['path']).stat().st_size==entry['bytes'],'LFJ_XO_DEPENDENCY_SIZE')
    require(f['deployment']==n['deployment'] and len(f['cases'])==len(n['cases'])==288 and len(n['sequences'])==6 and sum(len(s['steps'])for s in n['sequences'])==18,'LFJ_XO_SCOPE')
    full=extra=partial=0
    for index,(c,raw)in enumerate(zip(f['cases'],n['cases'])):
        require(c['index']==index and c['original']==raw and c['target_output']==raw['target_output'] and c['required_input']==raw['original_input_view']['required_input'],'LFJ_XO_CASE_IDENTITY')
        e=raw['expected'];target=int(raw['target_output']);require(target>0,'LFJ_XO_TARGET_POSITIVE')
        require(c['full_target']==e['swapped'] and e['incomplete_target']==(int(raw['original_input_view']['amount_out_left'])!=0),'LFJ_XO_PARTIAL_CLASSIFICATION')
        if not e['swapped']:partial+=1;continue
        full+=1;received=int(e['actual_output']);cost=int(raw['original_input_view']['required_input']);surplus=received-target
        require(surplus>=0 and str(surplus)==e['output_surplus']==c['output_surplus'] and str(received)==c['actual_output'] and str(cost)==e['payer_token_delta'],'LFJ_XO_REAL_FUNDS')
        require(e['funding_receipt']['status']==e['receipt']['status']==1 and raw['original_forward_view']['amount_in_left']=='0' and raw['original_forward_view']['amount_out']==e['actual_output'],'LFJ_XO_REAL_PAIR_EXECUTION')
        control=raw['minimum_input_control'];require(int(control['input'])==cost-1 and int(control['amount_out'])<target and control['below_target'] is True,'LFJ_XO_MINIMUM_INPUT_CONTROL')
        side=0 if raw['zero_for_one'] else 1;other=1-side
        require(int(e['after']['raw_reserve'+str(side)])==int(raw['before']['raw_reserve'+str(side)])+cost and int(e['after']['raw_reserve'+str(other)])==int(raw['before']['raw_reserve'+str(other)])-received,'LFJ_XO_ACTUAL_POOL_CONSERVATION')
        extra+=surplus!=0
    require((full,extra,partial)==(216,60,72),'LFJ_XO_OUTCOME_COUNTS')
    for original,seq in zip(n['sequences'],f['sequences']):
        require(seq['initial_case_index']==original['initial_case_index'] and len(seq['steps'])==len(original['steps'])==3,'LFJ_XO_SEQUENCE_SCOPE')
        for observed,row in zip(seq['steps'],original['steps']):require(observed['original']==row and row['expected']['swapped'] is True,'LFJ_XO_SEQUENCE_EXECUTED')
    for cmd,reason in [(['node',str(ROOT/'integration/tools/ci/lfj-v21-exact-out-getters.cjs'),str(raw_path)],'LFJ_XO_RAW_ABI_CHANGED'),([sys.executable,str(ROOT/'integration/tools/ci/lfj-v21-exact-out-state.py'),'--verify',str(Path(path).resolve())],'LFJ_XO_PROJECTION_CHANGED')]:
        result=subprocess.run(cmd,capture_output=True,text=True,timeout=60);require(result.returncode==0,reason+':'+result.stderr[-350:])
    return dict(schema_version=1,passed=True,reference=bound(str(Path(path).relative_to(ROOT))),original=bound(ref['path'],ref['sha256']),full_original_targets=216,with_output_surplus=60,incomplete_targets_rejected=72,sequence_steps=18,original_pair_swaps=228,raw_abi_banks=540,raw_abi_reads=17280,authority='unchanged original getSwapIn/getSwapOut/Pair, controlled clone/factory sentinel and separately prefunded Token transfers; no original Router/atomic executor or public Avalanche bank qualification',all_requested_features_complete=False,production_ready=False)

if __name__=='__main__':print(json.dumps(validate(),indent=2))
