#!/usr/bin/env python3
"""Original target-input projection; output surplus is explicit and never trimmed."""
from pathlib import Path
import hashlib
import json
import sys
R=Path(__file__).resolve().parents[3]
SOURCE=R/'integration/artifacts/local/core-lfj-v21-exact-out-original-20261110.json'
N=json.loads(SOURCE.read_bytes());D=N['deployment']
sha=lambda code:hashlib.sha256(bytes.fromhex(code[2:])).hexdigest()

def pool(raw):
    state=dict(raw)
    state.update(code_sha256=sha(D['pool_code']),implementation=D['implementation'],implementation_code_sha256=sha(D['implementation_code']),factory=D['factory'],factory_code_sha256=sha(D['factory_code']),token0_code_sha256=sha(D['token0_code']),token1_code_sha256=sha(D['token1_code']),block_hash='0x'+'b7'*32,block_number=100,max_bins=16,max_steps=64,timestamp=int(raw['timestamp']))
    return dict(chain_key='evm:31337',pool_id=D['pool'],protocol='lfj-lb-v21',token0=D['token0'],token1=D['token1'],reserve0='0',reserve1='0',fee_numerator=0,fee_denominator=1000000000000000000,adapter_version='lfj-lb/v2.1.0-fee-oracle-1',token_behavior='standard',lfj=state)

def source_pool(raw):
    value=pool(raw);state=value['lfj'];current=(int(state['parameters'])>>216)&65535
    if current:
        by_index={sample['index']:sample for sample in state['oracle_samples']}
        length=int(by_index[current]['data'])&65535
        order=[current]
        following=current%length+1
        if following!=current:order.append(following)
        state['oracle_samples']=[by_index[index] for index in order]
    return value

def case(index,row):
    expected=row['expected'];full=expected['swapped'];strict=full and expected['output_surplus']=='0'
    return dict(index=index,pool=pool(row['before']),target_output=row['target_output'],zero_for_one=row['zero_for_one'],required_input=row['original_input_view']['required_input'],full_target=full,strictly_representable=strict,output_surplus=expected.get('output_surplus'),actual_output=expected.get('actual_output'),actual_pool=pool(expected['after']) if full else None,expected_pool=pool(expected['after']) if full else None,original=row)

C=[case(i,row)for i,row in enumerate(N['cases'])]
for c in C:
    v=c['pool']['lfj'];parent=c['pool']
    c['config']=dict(id=parent['pool_id'],protocol=parent['protocol'],token0=parent['token0'],token1=parent['token1'],fee_numerator=0,fee_denominator=parent['fee_denominator'],program_id='',accounts=[],lfj={key:v[key.removeprefix('expected_')] for key in ['expected_code_sha256','expected_implementation','expected_implementation_code_sha256','expected_factory','expected_factory_code_sha256','expected_token0_code_sha256','expected_token1_code_sha256']})
    c['config']['lfj'].update(bin_step=v['bin_step'],token_policy='standard',max_bins=16,max_steps=64)

S=[dict(initial_case_index=s['initial_case_index'],pool=C[s['initial_case_index']]['pool'],steps=[case(i,row)for i,row in enumerate(s['steps'])])for s in N['sequences']]
banks=[dict(id=c['index'],pool=c['pool'],original_case_index=c['index'],source_step=-1,getter_reads=c['original']['getter_reads'],storage_reads=c['original']['storage_reads'],original_header=c['original']['original_before_block'])for c in C]
for seq in S:
    seq['source_bank_ids']=[seq['initial_case_index']]
    for i,c in enumerate(seq['steps']):
        e=c['original']['expected'];bank_id=len(banks);seq['source_bank_ids'].append(bank_id);banks.append(dict(id=bank_id,pool=source_pool(e['after']),original_case_index=seq['initial_case_index'],source_step=i,getter_reads=e['getter_reads'],storage_reads=e['storage_reads'],original_header=e['original_after_block']))
value=dict(schema_version=1,quote_kind='EXACT_OUT_MINIMUM_TARGET',generator_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),original_reference=dict(path=str(SOURCE.relative_to(R)),sha256=hashlib.sha256(SOURCE.read_bytes()).hexdigest()),authority='projection of original getSwapIn plus actual prefunded Pair swaps; actual recipient output and minimum target remain distinct; no public execution or Avalanche bank claim',deployment=D,cases=C,sequences=S,source_banks=banks,exact_out_admission=False,all_requested_features_complete=False,production_ready=False)
text=json.dumps(value,indent=2)+'\n'
if len(sys.argv)==3 and sys.argv[1]=='--verify':
    if Path(sys.argv[2]).read_bytes()!=text.encode():raise ValueError('LFJ_TARGET_DRAFT_PROJECTION_CHANGED')
    print('Original target draft verified without writes')
elif len(sys.argv)==1:
    for target in [R/'integration/test/contracts/lfj-exact-out.json',R/'dex-router/test/contracts/lfj-exact-out.json']:
        target.write_text(text)
    print(json.dumps(dict(cases=len(C),full_targets=sum(c['full_target']for c in C),with_surplus=sum(c['full_target'] and not c['strictly_representable']for c in C),partial_rejected=sum(not c['full_target']for c in C),banks=len(banks))))
else:raise ValueError('LFJ_TARGET_PROJECTION_ARGS')
