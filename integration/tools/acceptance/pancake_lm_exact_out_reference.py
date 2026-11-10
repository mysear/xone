#!/usr/bin/env python3
"""Authenticate original negative-amount observations without inverse quote math."""
import json
import subprocess
from pathlib import Path
from weighted_evidence import ROOT, OUT, require, sha
from pancake_bsc4_acceptance import bound, runtime_match
DEFAULT=ROOT/'integration/test/contracts/pancake-lm-exact-out.json'


def validate(path=DEFAULT):
    f=json.loads(Path(path).read_bytes())
    require(f.get('schema_version')==1 and f.get('quote_kind')=='EXACT_OUT','LM_XO_REFERENCE_KIND')
    require(f['generator_sha256']==sha(ROOT/'integration/tools/ci/pancake-lm-exact-out-state.cjs'),'LM_XO_NORMALIZER_CHANGED')
    ref=f['original_reference'];bound(ref['path'],ref['sha256'])
    require(ref['sha256']==f['source_reference_sha256'],'LM_XO_REFERENCE_IDENTITY')
    d=json.loads((ROOT/ref['path']).read_bytes())
    require(d.get('passed') is True and d['quote_kind']=='EXACT_OUT'
            and d['generator_sha256']==sha(ROOT/'integration/tools/ci/pancake-lm-exact-out-reference.cjs'),'LM_XO_GENERATOR_CHANGED')
    for entry in d['runtime']['loaded_dependencies']:
        p=bound(entry['path'],entry['sha256']);require((ROOT/p['path']).stat().st_size==entry['bytes'],'LM_XO_DEPENDENCY_SIZE')
    metadata_path=ROOT/'integration/tools/reference/pancake-v3-lm/source-metadata.json'
    require(sha(metadata_path)==d['source_metadata_sha256'],'LM_XO_SOURCE_METADATA_CHANGED')
    metadata=json.loads(metadata_path.read_bytes())
    require(d['source_commit']==metadata['commit']==f['source_commit'],'LM_XO_SOURCE_COMMIT')
    for entry in metadata['files']+metadata['dependencies']['openzeppelin']['files']:
        bound('integration/tools/reference/pancake-v3-lm/'+entry['path'],entry['sha256'])
    require(f['deployment']==d['deployment'] and f['artifacts']==d['artifacts'],'LM_XO_SHIPPED_IDENTITY')
    for role,artifact in d['artifacts'].items():runtime_match(d['deployment'][role]['code'],artifact)
    require(len(d['cases'])==100 and len(d['invalid_bank_cases'])==4 and len(f['cases'])==104
            and len(d['sequences'])==4 and sum(len(s['steps'])for s in d['sequences'])==12,'LM_XO_CORPUS_SCOPE')
    complete=partial=reverted=0
    all_cases=d['cases']+d['invalid_bank_cases']
    for index,(raw,case)in enumerate(zip(all_cases,f['cases'])):
        require(raw['index']==case['index']==index and case['original']==raw,'LM_XO_CASE_IDENTITY')
        require(raw['amount_in']=='-'+raw['desired_output'] and int(raw['desired_output'])>0
                and case['target_output']==raw['desired_output'],'LM_XO_NEGATIVE_SPECIFIED_OUTPUT')
        before,after,e=raw['before'],raw['expected']['after'],raw['expected']
        zero=raw['zero_for_one'];side=0 if zero else 1;other=1-side
        if e['reverted']:
            require(before==after and e['recipient_deltas']==['0','0'] and e['payer_deltas']==['0','0'],'LM_XO_REVERT_ATOMICITY')
            reverted+=1
        else:
            require(int(e['consumed_in'])>=0 and 0<=int(e['amount_out'])<=int(raw['desired_output'])
                    and e['recipient_deltas'][other]==e['amount_out'] and e['recipient_deltas'][side]=='0'
                    and e['payer_deltas'][side]==e['consumed_in'] and e['payer_deltas'][other]=='0','LM_XO_REAL_FUNDS_DELTA')
            require(int(after['raw'+str(side)])==int(before['raw'+str(side)])+int(e['consumed_in'])
                    and int(after['raw'+str(other)])==int(before['raw'+str(other)])-int(e['amount_out']),'LM_XO_POOL_BALANCE_CONSERVATION')
            if e['amount_out']==raw['desired_output']:complete+=1
            else:partial+=1
        require(case['expected']['partial']==(not e['reverted'] and e['amount_out']!=raw['desired_output'])
                and case['expected']['reverted']==e['reverted'],'LM_XO_PARTIAL_CLASSIFICATION')
        bank=f['source_banks'][case['source_bank_id']]
        require(bank['original_bank']==raw['source_before'] and bank['pool']==case['pool'],'LM_XO_RAW_SOURCE_BANK')
        for before_after in ['source_before','source_after']:
            require(len(raw[before_after]['reads'])>=40,'LM_XO_RAW_GETTER_SCOPE')
    require((complete,partial,reverted)==(80,20,4),'LM_XO_EXECUTION_COUNTS')
    for raw,seq in zip(d['sequences'],f['sequences']):
        for i,(step,observed)in enumerate(zip(raw['steps'],seq['steps'])):
            require(not step['expected']['reverted'] and step['expected']['amount_out']==step['desired_output']
                    and observed['target_output']==step['desired_output'] and observed['original']==step,'LM_XO_SEQUENCE_FULL_OUTPUT')
            bank=f['source_banks'][seq['source_bank_ids'][i]]
            require(bank['original_bank']==step['source_before'],'LM_XO_SEQUENCE_SOURCE_IDENTITY')
    projected=subprocess.run(['node',str(ROOT/'integration/tools/ci/pancake-lm-exact-out-state.cjs'),'--verify',str(Path(path).resolve())],capture_output=True,text=True,timeout=30)
    require(projected.returncode==0,'LM_XO_RAW_PROJECTION_CHANGED:'+projected.stderr[-300:])
    return dict(schema_version=1,passed=True,reference=bound(str(Path(path).relative_to(ROOT))),
        original=bound(ref['path'],ref['sha256']),full_original_swaps=80,partial_swaps_rejected=20,
        invalid_banks_rejected=4,original_sequence_steps=12,
        authority='unchanged pinned 98684794 Pool/LM/MasterChef negative amountSpecified, controlled banks and standard callback Tokens; no public bank qualification',
        all_requested_features_complete=False,production_ready=False)


if __name__=='__main__':print(json.dumps(validate(),indent=2))
