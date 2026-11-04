"""Deterministic corpus specification; all numerical expectations come from the native original program."""
from copy import deepcopy
import hashlib
from pathlib import Path
import json
import subprocess

U64 = 2**64 - 1
U128 = 2**128 - 1

class Oracle:
    def __init__(self, executable):
        self.process = subprocess.Popen([str(executable)], stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True, bufsize=1)
    def ask(self, **request):
        self.process.stdin.write(json.dumps(request, separators=(',', ':')) + '\n')
        self.process.stdin.flush()
        line = self.process.stdout.readline()
        assert line, f'native oracle stopped: {self.process.poll()}'
        response = json.loads(line)
        assert 'fatal' not in response, response
        return response
    def close(self):
        self.process.stdin.close()
        assert self.process.wait(timeout=60) == 0


def build(executable):
    oracle = Oracle(executable)
    try:
        base = oracle.ask(action='base')
        fixture = oracle.ask(action='fixture')
        vectors = oracle.ask(action='tickmath')
        cases = []
        def case(name, p, amount, direction, tags=()):
            response = oracle.ask(action='quote', pool=p, input=str(amount), zero_for_one=direction)
            row = dict(name=name, pool=deepcopy(p), input=str(amount), zero_for_one=direction, **response, tags=list(tags))
            cases.append(row)
            return row
        def arrays(p, starts, ticks=None, layout='fixed'):
            s = p['orca_whirlpool']
            spacing = s['tick_spacing']
            if ticks is None:
                ticks = [deepcopy(t) for a in s['tick_arrays'] for t in a['ticks']]
            s['tick_arrays'] = []
            for start in starts:
                contained = [deepcopy(t) for t in ticks if start <= t['tick'] < start + 88 * spacing]
                contained.sort(key=lambda t: t['tick'])
                bitmap = sum(1 << ((t['tick'] - start)//spacing) for t in contained)
                address = oracle.ask(action='array_address', pool=p['pool_id'], start=start)['address']
                s['tick_arrays'].append(dict(address=address, start_tick_index=start, layout=layout, initialized_bitmap=str(bitmap), ticks=contained))
            return p
        def at_tick(tick, spacing=64, liquidity='1000000000000', count=7, empty=True):
            # Identity PDAs depend on spacing. Keep the base spacing for admitted int-bound tests;
            # raw helper-domain spacing cases are explicitly marked below.
            p = deepcopy(base)
            s = p['orca_whirlpool']
            assert spacing == s['tick_spacing']
            s['tick_current_index'] = tick
            s['sqrt_price_x64'] = oracle.ask(action='price', tick=tick)['sqrt_price_x64']
            s['liquidity'] = liquidity
            span = spacing * 88
            first = (tick // span) * span
            min_start = (-443636 // span)*span
            max_start = (443636 // span)*span
            starts = list(range(max(min_start, min(first - (count//2)*span, max_start-(count-1)*span)), max_start+1, span))[:count]
            return arrays(p, starts, [] if empty else None)

        amounts = [0, 1, 2, 10, 1000, 10**6, 10**9, 5*10**9, 10**10, 25*10**9, 10**11, 10**12, 10**15, U64, U64+1]
        for layout in ['fixed', 'dynamic']:
            for direction in [True, False]:
                for fee in [0, 3000, 60000, 65535]:
                    for protocol in [0, 2500]:
                        p = deepcopy(base)
                        p['fee_numerator'] = p['orca_whirlpool']['fee_rate'] = fee
                        p['orca_whirlpool']['protocol_fee_rate'] = protocol
                        for a in p['orca_whirlpool']['tick_arrays']:
                            a['layout'] = layout
                        for amount in amounts:
                            case(f'matrix/{layout}/{"a-b" if direction else "b-a"}/fee-{fee}/protocol-{protocol}/in-{amount}', p, amount, direction,
                                 ['matrix', 'fee-u16max-rejected' if fee == 65535 else 'admitted-fee'])

        for direction in [True, False]:
            d = 'a-b' if direction else 'b-a'
            for layout in ['fixed', 'dynamic']:
                for tick in [-443636, -443635, -440001, -100001, -5633, -5632, -1, 0, 1, 5567, 5568, 5631, 5632, 100001, 440001, 443635, 443636]:
                    p = at_tick(tick)
                    for a in p['orca_whirlpool']['tick_arrays']: a['layout'] = layout
                    for amount in [1, 10**6, 10**10, U64]:
                        case(f'empty/{layout}/{d}/tick-{tick}/in-{amount}', p, amount, direction, ['empty-arrays', 'seven-arrays', 'tick-boundary', 'int-bounds'])
            p = deepcopy(base)
            p['orca_whirlpool']['liquidity'] = '0'
            for amount in [1, 10**9, U64]:
                case(f'zero-liquidity/{d}/in-{amount}', p, amount, direction, ['liquidity-zero', 'crossing'])
            # Zero liquidity crosses a tick without consuming input, then resumes.
            # A genuine zero-liquidity gap similarly consumes nothing between boundaries.
            for layout in ['fixed', 'dynamic']:
                for mode in ['initial-zero', 'gap']:
                    p = deepcopy(base)
                    ticks = []
                    p['orca_whirlpool']['liquidity'] = '0' if mode == 'initial-zero' else '1000000000000'
                    for tick, net in ([(-64,-10**12),(64,10**12)] if mode == 'initial-zero' else [(-128,-10**12),(-64,10**12),(64,-10**12),(128,10**12)]):
                        ticks.append(dict(tick=tick, liquidity_gross='1000000000000', liquidity_net=str(net), fee_growth_outside0='37', fee_growth_outside1='53'))
                    arrays(p, [-5632,0,5632], ticks, layout)
                    for amount in [10**6,5*10**9,25*10**9]:
                        case(f'zero-traversal/{layout}/{d}/{mode}/in-{amount}', p, amount, direction, ['liquidity-zero', 'zero-liquidity-gap' if mode=='gap' else 'zero-liquidity-recovery', 'crossing'])
            # Immediate crossing at price tick zero makes even a u128 active liquidity
            # boundary observable with u64 input. The MIN signed negation uses original
            # release wrapping, not unbounded Python arithmetic.
            for net in [-(2**127), -(2**127-1), 2**127-1]:
                p = deepcopy(base)
                p['orca_whirlpool']['liquidity'] = str(2**127+10**12)
                if not direction:
                    p['orca_whirlpool']['tick_current_index'] = -1  # legitimate left-cross tick shift
                t = dict(tick=0, liquidity_gross=str(abs(net)), liquidity_net=str(net), fee_growth_outside0=str(U128-1), fee_growth_outside1=str(U128-3))
                arrays(p, [-5632,0,5632], [t])
                for amount in [1,10**6,5*10**9]:
                    case(f'signed-net/{d}/net-{net}/in-{amount}', p, amount, direction, ['signed-i128-crossing','int-bounds','crossing'])
            p = deepcopy(base)
            p['orca_whirlpool']['liquidity'] = str(U128)
            for amount in [1, 10**9, U64]:
                case(f'max-liquidity/{d}/in-{amount}', p, amount, direction, ['u128-liquidity', 'int-bounds'])
            for liquid in [1, 100, 2**64, 2**96]:
                p = deepcopy(base)
                p['orca_whirlpool']['liquidity'] = str(liquid)
                arrays(p, [-16896,-11264,-5632,0,5632,11264,16896], [])
                for amount in [1, 1000, 10**9, U64]:
                    case(f'liquidity/{d}/L-{liquid}/in-{amount}', p, amount, direction, ['liquidity-bounds', 'dust', 'seven-arrays'])
            for field in ['fee_growth_global0', 'fee_growth_global1']:
                p = deepcopy(base)
                p['orca_whirlpool'][field] = str(U128 - 1)
                for amount in [10**6, 5*10**9, 25*10**9]:
                    case(f'fee-growth-wrap/{d}/{field}/in-{amount}', p, amount, direction, ['fee-growth-wrap', 'crossing'])
            for field in ['protocol_fee_owed0', 'protocol_fee_owed1']:
                p = deepcopy(base)
                p['orca_whirlpool'][field] = U64 - 1
                case(f'fee-owed-wrap/{d}/{field}', p, 5*10**9, direction, ['u64-owed-wrap', 'crossing'])
            p = deepcopy(base)
            for a in p['orca_whirlpool']['tick_arrays']:
                for t in a['ticks']:
                    t['fee_growth_outside0'] = str(U128 - 7)
                    t['fee_growth_outside1'] = str(U128 - 11)
            case(f'crossed-outside-wrap/{d}', p, 25*10**9, direction, ['outside-fee-wrap', 'crossing'])
            p = deepcopy(base)
            p['orca_whirlpool'][f'raw_vault{0 if direction else 1}'] = U64
            case(f'raw-vault-input-overflow/{d}', p, 10**6, direction, ['raw-vault-overflow'])
            p['orca_whirlpool'][f'raw_vault{0 if direction else 1}'] = U64 - 10**6
            case(f'raw-vault-exact-u64-max/{d}', p, 10**6, direction, ['raw-vault-u64-max'])
            p = deepcopy(base)
            p['orca_whirlpool'][f'raw_vault{1 if direction else 0}'] = 0
            case(f'raw-vault-output-underflow/{d}', p, 10**6, direction, ['raw-vault-underflow'])
            p = deepcopy(base)
            p['orca_whirlpool']['protocol_fee_rate'] = 2501
            case(f'protocol-fee-over-bound/{d}', p, 10**6, direction, ['protocol-bound-reject'])
            p = deepcopy(base)
            p['orca_whirlpool']['reward_last_updated_timestamp'] = p['orca_whirlpool']['bank_unix_timestamp']+1
            case(f'reward-future/{d}', p, 10**6, direction, ['timestamp-reject'])

            # Exact-in original manager permits partial fills at the global limit. The profile
            # rejects them atomically; these positive-output cases test consumed != allocated.
            tick = -433000 if direction else 433000
            p = at_tick(tick, liquidity='10000000000')
            p['orca_whirlpool']['raw_vault0'] = U64
            p['orca_whirlpool']['raw_vault1'] = U64
            case(f'global-limit-partial/{d}', p, U64, direction, ['partial-fill', 'global-limit', 'int-bounds'])

            # Seven complete arrays still supply only three to the manager. Making the fourth
            # array nonempty must not turn an out-of-sequence rejection into a fill.
            p = deepcopy(base)
            arrays(p, [-16896,-11264,-5632,0,5632,11264,16896], [])
            case(f'three-primary-cap/{d}/empty', p, 10**13, direction, ['seven-arrays', 'three-primary-cap'])
            fourth = -16896 if direction else 16896
            t = dict(tick=fourth, liquidity_gross='1000000000000', liquidity_net='0', fee_growth_outside0='0', fee_growth_outside1='0')
            arrays(p, [-16896,-11264,-5632,0,5632,11264,16896], [t])
            case(f'three-primary-cap/{d}/fourth-initialized', p, 10**13, direction, ['seven-arrays', 'three-primary-cap'])

        sequences = []
        patterns = [[True]*8, [False]*8, [True,False]*4, [False,True]*4,
                    [True,True,False,True,False,False,True,False], [False,False,True,False,True,True,False,True],
                    [True,True,True,False,False,True,False,False], [False,False,False,True,True,False,True,True]]
        for i, pattern in enumerate(patterns):
            p = deepcopy(base)
            if i % 2:
                for a in p['orca_whirlpool']['tick_arrays']: a['layout'] = 'dynamic'
            initial = deepcopy(p)
            steps = []
            for j, direction in enumerate(pattern):
                amount = 10**9 if i < 4 else (5*10**9 if j % 2 == 0 else 2*10**9)
                response = oracle.ask(action='quote', pool=p, input=str(amount), zero_for_one=direction)
                assert not response['expected'].get('reverted'), (i,j,response)
                steps.append(dict(name=f'sequence-{i}/step-{j}', pool=deepcopy(p), input=str(amount), zero_for_one=direction, **response))
                p = deepcopy(response['expected']['next_pool'])
            sequences.append(dict(name=f'shared-pool-{i}', pool=initial, steps=steps, expected_final_pool=p, allocation_mode='same shared pool; consumed in order'))
        # Regression: a left cross can leave tick=-1 at the first array's low edge.
        # A B->A return must use the original shifted selection, without requiring -5632.
        for layout in ['fixed', 'dynamic']:
            p = deepcopy(base)
            p['orca_whirlpool']['tick_current_index'] = 1
            p['orca_whirlpool']['sqrt_price_x64'] = oracle.ask(action='price', tick=1)['sqrt_price_x64']
            arrays(p, [0,5632,11264], layout=layout)
            first = case(f'boundary-return/{layout}/left-to-window-edge', p, 50146692, True, ['boundary-return','low-edge-minus-one'])
            assert first['expected']['amount_out'] == '49998750'
            edge = deepcopy(first['expected']['next_pool'])
            assert edge['orca_whirlpool']['tick_current_index'] == -1
            assert edge['orca_whirlpool']['sqrt_price_x64'] == '18446744073709551616'
            returned = case(f'boundary-return/{layout}/right-from-low-edge', edge, 10000, False, ['boundary-return','shifted-first-array'])
            assert returned['expected']['amount_out'] == '9969'
            assert returned['raw_reference']['selected_tick_array_starts'] == [0,5632,11264]
            padded = deepcopy(edge)
            # This unneeded proof must not affect any manager/poststate result.
            arrays(padded, [-5632,0,5632,11264], layout=layout)
            padded_return = case(f'boundary-return/{layout}/right-with-unused-left-proof', padded, 10000, False, ['boundary-return','unused-left-proof'])
            trimmed = deepcopy(padded_return['expected']['next_pool'])
            trimmed['orca_whirlpool']['tick_arrays'].pop(0)
            assert trimmed == returned['expected']['next_pool']
            assert padded_return['raw_reference'] == returned['raw_reference']
            rejected = case(f'boundary-return/{layout}/left-missing-directional-proof', edge, 10000, True, ['boundary-return','directional-proof-rejection'])
            assert rejected['expected'] == {'reverted':True} and rejected['revert_reason'].startswith('original builder:')
            steps = [deepcopy(first),deepcopy(returned)]
            current = deepcopy(returned['expected']['next_pool'])
            for j, direction in enumerate([True,False]*3, start=2):
                response = oracle.ask(action='quote', pool=current, input='1000', zero_for_one=direction)
                assert not response['expected'].get('reverted'), (layout,j,response)
                steps.append(dict(name=f'boundary-return/{layout}/step-{j}', pool=deepcopy(current),input='1000',zero_for_one=direction,**response))
                current = deepcopy(response['expected']['next_pool'])
            sequences.append(dict(name=f'boundary-return-{layout}',pool=deepcopy(p),steps=steps,expected_final_pool=current,allocation_mode='shared window low-edge left crossing, shifted right return, then mixed direction'))
        return dict(schema_version=1, base_pool=base, cases=cases, sequences=sequences, reference={
            'repository':'orca-so/whirlpools','revision':'cc683ec92acb5e7606caa3d8e514d9383e9c7c11','program_id':base['orca_whirlpool']['program_id'],
            'execution':'unchanged original no-entrypoint program crate; native manager::swap + original account loaders/SparseSwapTickSequenceBuilder + Whirlpool.update_after_swap',
            'production_math_imports':False,'adaptive_fee_info':None,'fee_rate_max':60000,'protocol_fee_rate_max':2500,
            'full_exact_input_required':True,'zero_output_rejected':True,'primary_array_cap':3,'provided_array_count_range':[3,7],
            'standard_token_wrapper':'checked u64 input-vault deposit and output-vault withdrawal; no Token program execution; discard all mutations on error/partial/zero output',
            'overflow_semantics':'upstream release overflow-checks=false; original update_after_swap u64 += wrapping; original explicit u128 fee wrapping',
            'max_steps':'corpus uses 8192 only; original manager has no adapter-specific budget argument',
            'synthetic_proof_identity':True,'sbf_transaction_executed':False,'token_program_executed':False,'validator_executed':False,
            'limitations':['No SBF/validator/CPI/token/signature or instruction-account-constraint execution','Synthetic code hashes identify fixture byte strings, not deployed mainnet bytecode','Fee u16::MAX is rejected by the pinned original MAX_FEE_RATE=60000','Extra supplied arrays are fallback candidates, not additional arrays traversed in one swap','Oracle is uninitialized system-owned empty; all rewards uninitialized and control flags zero','Configured smaller max_steps budgets are outside this oracle corpus']
        }), fixture, vectors
    finally:
        oracle.close()


def verify(corpus):
    cases = corpus['cases']
    assert len(cases) >= 300
    assert len({c['name'] for c in cases}) == len(cases)
    successes = []
    reasons = {}
    crossed = 0
    growth_wrap = owed_wrap = outside_wrap = 0
    changed = {'raw_vault0','raw_vault1','sqrt_price_x64','tick_current_index','liquidity','protocol_fee_owed0','protocol_fee_owed1','fee_growth_global0','fee_growth_global1','reward_last_updated_timestamp','tick_arrays'}
    for c in cases + [s for seq in corpus['sequences'] for s in seq['steps']]:
        p, e = c['pool'], c['expected']
        assert p['protocol'] == 'orca-whirlpools-static' and p['adapter_version'] == 'orca-whirlpools/cc683ec9-spl-static-1'
        assert p['chain_key'] == 'solana:localnet' and p['reserve0'] == p['reserve1'] == '0'
        s = p['orca_whirlpool']
        assert p['fee_numerator'] == s['fee_rate']
        assert isinstance(c['input'], str)
        for a in s['tick_arrays']:
            assert len(a['ticks']) == int(a['initialized_bitmap']).bit_count()
        if e.get('reverted'):
            assert e == {'reverted':True}
            reason = c['revert_reason']
            reasons[reason] = reasons.get(reason, 0)+1
            continue
        successes.append(c)
        n = e['next_pool']['orca_whirlpool']
        assert set(e['next_pool']) == set(p) and set(n) == set(s)
        for k in set(p)-{'orca_whirlpool'}: assert e['next_pool'][k] == p[k], k
        for k in set(s)-changed: assert n[k] == s[k], k
        i,o = (0,1) if c['zero_for_one'] else (1,0)
        assert n[f'raw_vault{i}'] == s[f'raw_vault{i}'] + int(c['input'])
        assert n[f'raw_vault{o}'] == s[f'raw_vault{o}'] - int(e['amount_out'])
        assert int(e['amount_out']) > 0
        assert c['raw_reference']['amount_consumed'] == c['input']
        selected = c['raw_reference']['selected_tick_array_starts']
        assert 1 <= len(selected) <= 3
        spacing = s['tick_spacing']
        span = 88*spacing
        initial = (s['tick_current_index']//span)*span
        if not c['zero_for_one'] and s['tick_current_index']+spacing >= initial+span:
            initial += span
        assert selected == [initial+(-1 if c['zero_for_one'] else 1)*span*j for j in range(len(selected))]
        assert n[f'protocol_fee_owed{i}'] == (s[f'protocol_fee_owed{i}']+c['raw_reference']['next_protocol_fee'])%(2**64)
        assert n[f'protocol_fee_owed{o}'] == s[f'protocol_fee_owed{o}']
        assert n[f'fee_growth_global{o}'] == s[f'fee_growth_global{o}']
        assert n['reward_last_updated_timestamp'] == s['bank_unix_timestamp']
        crossed += bool(c['raw_reference']['crossed_ticks'])
        growth_wrap += int(n[f'fee_growth_global{i}']) < int(s[f'fee_growth_global{i}'])
        owed_wrap += n[f'protocol_fee_owed{i}'] < s[f'protocol_fee_owed{i}']
        for a,b in zip(s['tick_arrays'],n['tick_arrays']):
            assert {k:v for k,v in a.items() if k!='ticks'} == {k:v for k,v in b.items() if k!='ticks'}
            for t,u in zip(a['ticks'],b['ticks']):
                assert {k:v for k,v in t.items() if not k.startswith('fee_growth_outside')} == {k:v for k,v in u.items() if not k.startswith('fee_growth_outside')}
                outside_wrap += int(u['fee_growth_outside0']) < int(t['fee_growth_outside0']) or int(u['fee_growth_outside1']) < int(t['fee_growth_outside1'])
        for field in ['raw_vault0','raw_vault1','protocol_fee_owed0','protocol_fee_owed1','bank_slot','bank_epoch','bank_unix_timestamp','deployed_slot','reward_last_updated_timestamp']:
            assert type(n[field]) is int and 0 <= n[field] <= U64
        for field in ['sqrt_price_x64','liquidity','fee_growth_global0','fee_growth_global1']:
            assert isinstance(n[field],str) and 0 <= int(n[field]) <= U128
    assert len(corpus['sequences']) >= 8
    for seq in corpus['sequences']:
        p = seq['pool']
        assert len(seq['steps']) >= 8
        for step in seq['steps']:
            assert step['pool'] == p
            assert not step['expected'].get('reverted')
            p = step['expected']['next_pool']
        assert p == seq['expected_final_pool']
    for direction in [True,False]:
        for fee in [0,3000,60000]:
            for proto in [0,2500]:
                assert any(c['zero_for_one']==direction and c['pool']['fee_numerator']==fee and c['pool']['orca_whirlpool']['protocol_fee_rate']==proto for c in successes)
        assert any('partial fill' in c.get('revert_reason','') and c['zero_for_one']==direction for c in cases)
        assert any(c['zero_for_one']==direction and 'raw-vault-exact-u64-max' in c['name'] and not c['expected'].get('reverted') for c in cases)
        cap = [c for c in cases if c['name'].startswith(f'three-primary-cap/{"a-b" if direction else "b-a"}')]
        assert len(cap)==2 and all(c['expected'].get('reverted') for c in cap)
    assert any(c['pool']['orca_whirlpool']['liquidity'] == '0' for c in successes), 'successful zero-liquidity traversal coverage missing'
    assert any('zero-liquidity-gap' in c.get('tags', []) for c in successes), 'zero-liquidity gap coverage missing'
    assert any('signed-i128-crossing' in c.get('tags', []) for c in successes), 'signed tick crossing boundary coverage missing'
    boundary_cases = [c for c in cases if 'boundary-return' in c.get('tags', [])]
    boundary_sequences = [s for s in corpus['sequences'] if s['name'].startswith('boundary-return-')]
    assert len(boundary_cases) == 8 and len(boundary_sequences) == 2
    for sequence in boundary_sequences:
        first, returned = sequence['steps'][:2]
        assert first['input'] == '50146692' and first['expected']['amount_out'] == '49998750'
        edge = first['expected']['next_pool']['orca_whirlpool']
        assert edge['tick_current_index'] == -1 and edge['sqrt_price_x64'] == '18446744073709551616'
        assert [a['start_tick_index'] for a in edge['tick_arrays']] == [0,5632,11264]
        assert returned['input'] == '10000' and returned['expected']['amount_out'] == '9969'
        assert returned['raw_reference']['selected_tick_array_starts'] == [0,5632,11264]
    baseline = json.loads(Path(__file__).with_name('boundary-return-baseline.json').read_text())
    assert len(baseline['cases']) == 872 and len(baseline['sequences']) == 8
    def canonical_hash(value):
        return hashlib.sha256(json.dumps(value,sort_keys=True,separators=(',',':')).encode()).hexdigest()
    for key in ['cases','sequences']:
        current = {row['name']:canonical_hash(row) for row in corpus[key]}
        for name, digest in baseline[key].items():
            assert current[name] == digest, f'preexisting {key} entry changed: {name}'
    assert corpus['base_pool'] == baseline['base_pool']
    assert growth_wrap and owed_wrap and outside_wrap and crossed
    return dict(cases=len(cases), successful_cases=sum(not c['expected'].get('reverted',False) for c in cases),reverted_cases=sum(c['expected'].get('reverted',False) for c in cases),
                sequences=len(corpus['sequences']),sequence_steps=sum(len(s['steps']) for s in corpus['sequences']),
                boundary_return_cases=len(boundary_cases),boundary_return_sequences=len(boundary_sequences),preserved_prior_cases=len(baseline['cases']),preserved_prior_sequences=len(baseline['sequences']),
                full_poststate_accounting_checks=len(successes),successful_crossing_swaps=crossed,fee_growth_wraps=growth_wrap,protocol_fee_owed_wraps=owed_wrap,crossed_tick_outside_wraps=outside_wrap,
                revert_reason_counts=reasons)
