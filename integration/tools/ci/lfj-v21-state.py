#!/usr/bin/env python3
"""Project controlled original LFJ banks into separate typed test inputs."""
from pathlib import Path
import json,hashlib
R=Path(__file__).resolve().parents[3];source=R/'integration/artifacts/local/core-lfj-v21-original-reference.json';native=json.loads(source.read_text());deployment=native['deployment']
sha=lambda code:hashlib.sha256(bytes.fromhex(code[2:])).hexdigest()
# Explicit controlled envelope; no actual Avalanche source or deployment claim.
block_hash='0x'+'b7'*32;parent_hash='0x'+'b6'*32
cases=[]
for index,row in enumerate(native['cases']):
 state=dict(row['before']);state.update(code_sha256=sha(deployment['pool_code']),implementation=deployment['implementation'],implementation_code_sha256=sha(deployment['implementation_code']),factory=deployment['factory'],factory_code_sha256=sha(deployment['factory_code']),token0_code_sha256=sha(deployment['token0_code']),token1_code_sha256=sha(deployment['token1_code']),block_hash=block_hash,block_number=100,max_bins=16,max_steps=64)
 state['timestamp']=int(state['timestamp'])
 pool={'chain_key':'evm:31337','pool_id':deployment['pool'],'protocol':'lfj-lb-v21','token0':deployment['token0'],'token1':deployment['token1'],'reserve0':'0','reserve1':'0','fee_numerator':0,'fee_denominator':1000000000000000000,'adapter_version':'lfj-lb/v2.1.0-fee-oracle-1','token_behavior':'standard','lfj':state}
 config={'id':pool['pool_id'],'protocol':pool['protocol'],'token0':pool['token0'],'token1':pool['token1'],'fee_numerator':0,'fee_denominator':pool['fee_denominator'],'program_id':'','accounts':[],'lfj':{'expected_code_sha256':state['code_sha256'],'expected_implementation':state['implementation'],'expected_implementation_code_sha256':state['implementation_code_sha256'],'expected_factory':state['factory'],'expected_factory_code_sha256':state['factory_code_sha256'],'expected_token0_code_sha256':state['token0_code_sha256'],'expected_token1_code_sha256':state['token1_code_sha256'],'bin_step':state['bin_step'],'token_policy':'standard','max_bins':16,'max_steps':64}}
 cases.append({'index':index,'pool':pool,'config':config,'input':row['input'],'zero_for_one':row['zero_for_one'],'expected':row['expected'],'original_view':row['original_view'],'original_before_block':row['original_before_block'],'getter_reads':row['getter_reads'],'storage_reads':row['storage_reads'],'allocation_points':row.get('allocation_points',[])})
report={'schema_version':1,'authority':'controlled envelope over unchanged original mutable fields; no source-header or historical-chain equivalence','original_reference':{'path':str(source.relative_to(R)),'sha256':hashlib.sha256(source.read_bytes()).hexdigest()},'synthetic_chain_key':'evm:31337','block_hash':block_hash,'parent_hash':parent_hash,'block_number':100,'timestamp':cases[0]['pool']['lfj']['timestamp'],'deployment':deployment,'cases':cases,'sequences':native.get('sequences',[]),'actual_Avalanche_source_qualified':False,'production_ready':False}
text=json.dumps(report,indent=2)+'\n'
(R/'integration/artifacts/local/core-lfj-v21-normalized.json').write_text(text)
for service in ['integration','chain-parser-go','chain-parser-rust','dex-router']:
 (R/service/'test/contracts/lfj-v21-state.json').write_text(text)
print('normalized original cases',len(cases))
