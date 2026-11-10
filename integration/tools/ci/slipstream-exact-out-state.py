#!/usr/bin/env python3
"""Project fixed original negative-output banks; --verify never rewrites evidence."""
from pathlib import Path
import json,hashlib,sys
R=Path(__file__).resolve().parents[3];P=R/'integration/artifacts/local/core-slipstream-exact-out-original-20261110.json';N=json.loads(P.read_text());D=N['deployment'];T=N['code_templates'];ZERO='0x'+'0'*40
sha=lambda code:hashlib.sha256(bytes.fromhex(code[2:])).hexdigest()
def module(kind,address,unstaked,raw):
 if kind=='none':return dict(kind='none',address=ZERO,code_sha256='',template_sha256='',factory=ZERO,custom_fee=0)
 name='custom_unstaked'if unstaked else'custom_swap'if kind=='custom'else'dynamic_swap'
 key='unstaked_custom_code'if unstaked else'swap_custom_code'if kind=='custom'else'swap_dynamic_code'
 out=dict(kind=kind,address=address.lower(),code_sha256=sha(D[key]),template_sha256=T[name]['normalized_execution_body_sha256'],factory=D['factory'],custom_fee=int(raw['unstaked_custom_fee']if unstaked else raw.get('custom_fee',0)))
 if kind=='dynamic':
  a=raw['dynamic_fee_config'];out['dynamic']=dict(base_fee=int(a[0]),fee_cap=int(a[1]),scaling_factor=int(a[2]),initial_fee_enabled=a[3],initial_fee=int(a[4]),default_scaling_factor=int(raw['default_scaling_factor']),default_fee_cap=int(raw['default_fee_cap']),seconds_ago=int(raw['seconds_ago']))
 return out
def state(raw,derived=False):
 m=raw['module_state'];slot=raw['slot0'];kind=m['kind'];origin=D['sender'];swap_fee=int(m['view_swap_fee']);ufee=int(m['view_unstaked_fee'])
 core=dict(implementation=D['implementation'],implementation_code_sha256=sha(D['implementation_code']),factory_code_sha256=sha(D['factory_code']),sqrt_price_x96=slot[0],tick=slot[1],liquidity=raw['liquidity'],tick_spacing=50,factory=D['factory'],code_sha256=sha(D['pool_code']),max_steps=128,tick_words=raw['tick_words'],ticks=[dict(tick=t['tick'],liquidity_gross=t['values'][0],liquidity_net=t['values'][1])for t in raw['ticks']])
 out=dict(core=core,implementation_template_sha256=T['implementation']['normalized_execution_body_sha256'],factory_template_sha256=T['factory']['normalized_execution_body_sha256'],token0_code_sha256=sha(D['token0_code']),token1_code_sha256=sha(D['token1_code']),factory_voter=m['factory_voter'].lower(),voter_code_sha256=sha(D['voter_code']),voter_gauge=m['gauge'].lower(),voter_gauge_alive=m['gauge_alive'],factory_default_fee=int(m['factory_default_fee']),factory_default_unstaked_fee=int(m['factory_default_unstaked_fee']),factory_registered=m['factory_registered'],swap_fee_module=module(kind,m['swap_fee_module'],False,m),unstaked_fee_module=module('none'if m['unstaked_fee_module']==ZERO else'custom',m['unstaked_fee_module'],True,m),origins=[dict(origin=origin,discount=int(m['origin_discount'])if kind=='dynamic'else 0,view_swap_fee=swap_fee,view_unstaked_fee=ufee)],view_origin=origin,view_swap_fee=swap_fee,view_unstaked_fee=ufee,raw_balance0=raw['raw_balance0'],raw_balance1=raw['raw_balance1'])
 for target,source in [('fee_growth_global0','feeGrowthGlobal0X128'),('fee_growth_global1','feeGrowthGlobal1X128'),('reward_growth_global','rewardGrowthGlobalX128'),('reward_rate','rewardRate'),('reward_reserve','rewardReserve'),('period_finish','periodFinish'),('rollover','rollover'),('staked_liquidity','stakedLiquidity'),('max_liquidity_per_tick','maxLiquidityPerTick')]:out[target]=raw[source]
 out.update(gauge_fees0=raw['gaugeFees'][0],gauge_fees1=raw['gaugeFees'][1],last_updated=raw['lastUpdated'],gauge=raw['gauge'].lower(),nft=raw['nft'].lower(),factory_registry=raw['factoryRegistry'].lower(),observation_index=slot[2],observation_cardinality=slot[3],observation_cardinality_next=slot[4],unlocked=slot[5])
 out['ticks']=[]
 for t in raw['ticks']:
  v=t['values'];out['ticks'].append(dict(tick=t['tick'],liquidity_gross=v[0],liquidity_net=v[1],staked_liquidity_net=v[2],fee_growth_outside0=v[3],fee_growth_outside1=v[4],reward_growth_outside=v[5],tick_cumulative_outside=v[6],seconds_per_liquidity_outside_x128=v[7],seconds_outside=v[8],initialized=v[9]))
 out['observations']=[dict(block_timestamp=v['values'][0],tick_cumulative=v['values'][1],seconds_per_liquidity_cumulative_x128=v['values'][2],initialized=v['values'][3])for v in raw['observations']]
 out.update(block_hash='0x'+'c7'*32,block_number=100,timestamp=N['fixed_timestamp'],max_observations=16,derived=derived)
 return out
def pool(s):return dict(chain_key='evm:31337',pool_id=D['pool'],protocol='aerodrome-slipstream-context',token0=D['token0'],token1=D['token1'],reserve0='0',reserve1='0',fee_numerator=0,fee_denominator=1000000,adapter_version='aerodrome-slipstream/f8717faa-module-1',token_behavior='standard',slipstream_context=s)
def execution_pool(raw,before):
 value=pool(state(raw,True));value['slipstream_context']['core']['tick_words']=json.loads(json.dumps(before['slipstream_context']['core']['tick_words']));return value
C=[]
for index,c in enumerate(N['cases']):
 expected=c['expected'];full=not expected.get('reverted') and expected['amount_out']==c['target_output']
 C.append(dict(index=index,mode=c['mode'],stake=c['stake'],target_output=c['target_output'],zero_for_one=c['zero_for_one'],pool=pool(state(c['before'])),expected=dict(reverted=bool(expected.get('reverted')),partial=not expected.get('reverted') and not full,consumed_in=expected.get('consumed_in','0'),amount_out=expected.get('amount_out','0'),expected_pool=execution_pool(expected['after'],pool(state(c['before']))) if full else None),original=c,getter_reads=c['before_getter_reads'],original_before_header=c['original_before_header']))
for case in C:
 parent=case['pool'];v=parent['slipstream_context']
 module_cfg=lambda item:dict(kind=item['kind'],address=item['address'],expected_code_sha256=item['code_sha256'])
 config=dict(id=parent['pool_id'],protocol=parent['protocol'],token0=parent['token0'],token1=parent['token1'],fee_numerator=0,fee_denominator=1000000,program_id='',accounts=[],slipstream_context=dict(expected_code_sha256=v['core']['code_sha256'],expected_implementation=v['core']['implementation'],expected_implementation_code_sha256=v['core']['implementation_code_sha256'],expected_factory=v['core']['factory'],expected_factory_code_sha256=v['core']['factory_code_sha256'],expected_voter=v['factory_voter'],expected_voter_code_sha256=v['voter_code_sha256'],expected_token0_code_sha256=v['token0_code_sha256'],expected_token1_code_sha256=v['token1_code_sha256'],token_policy='standard',origins=[o['origin']for o in v['origins']],swap_fee_module=module_cfg(v['swap_fee_module']),unstaked_fee_module=module_cfg(v['unstaked_fee_module']),tick_word_radius=1,max_initialized_ticks=64,max_steps=128,max_observations=16))
 case['config']=config
S=[]
for row in N['sequences']:
 initial=C[row['initial_case_index']]
 S.append(dict(initial_case_index=row['initial_case_index'],mode=initial['mode'],pool=initial['pool'],steps=[dict(target_output=s['target_output'],zero_for_one=initial['zero_for_one'],consumed_in=s['consumed_in'],amount_out=s['amount_out'],fee=s['fee'],expected_pool=execution_pool(s['after'],initial['pool']),canonical_pool=pool(state(s['after'],False)),getter_reads=s['after_getter_reads'],original_after_header=s['original_after_header'],original=s)for s in row['steps']]))
banks=[dict(id=c['index'],original_case_index=c['index'],source_step=-1,canonical_pool=c['pool'],getter_reads=c['getter_reads'],original_header=c['original_before_header'])for c in C]
for sequence in S:
 for index,step in enumerate(sequence['steps']):
  banks.append(dict(id=len(banks),original_case_index=sequence['initial_case_index'],source_step=index,canonical_pool=step['canonical_pool'],getter_reads=step['getter_reads'],original_header=step['original_after_header']))
value=dict(schema_version=1,quote_kind='EXACT_OUT',generator_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),authority='typed projection of unchanged original negative-output calls; controlled banks, synthetic header markers and execution windows explicit',original_reference=dict(path=str(P.relative_to(R)),sha256=hashlib.sha256(P.read_bytes()).hexdigest()),source_reference_sha256=hashlib.sha256(P.read_bytes()).hexdigest(),source_commit=N['source_commit'],cases=C,sequences=S,source_banks=banks,deployment=D,code_templates=T,all_requested_features_complete=False,production_ready=False)
text=json.dumps(value,indent=2)+'\n'
if len(sys.argv)==3 and sys.argv[1]=='--verify':
 if Path(sys.argv[2]).read_bytes()!=text.encode():raise ValueError('SLIPSTREAM_XO_RAW_PROJECTION_CHANGED')
 print('Exact original projection verified without writes')
elif sys.argv[1:] in [[],['--integration-only']]:
 (R/'integration/test/contracts/slipstream-exact-out.json').write_text(text)
 if not sys.argv[1:]:(R/'dex-router/test/contracts/slipstream-exact-out.json').write_text(text)
 print('Exact-output cases',len(C),'banks',len(banks),'sequences',len(S))
else:raise ValueError('SLIPSTREAM_XO_NORMALIZER_ARGS')
