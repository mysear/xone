#!/usr/bin/env python3
"""Regenerate pinned original native bitmap evidence; build only the independent oracle."""
import argparse
import base64
from collections import Counter
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import struct
import subprocess
import sys
import tomllib

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[3]
REFERENCE = ROOT / 'integration/tools/reference/raydium-clmm'
ARTIFACTS = ROOT / 'integration/artifacts/local'
RUNTIME = Path('/tmp/xone-raydium-clmm/target2')
PROFILE = 'raydium-clmm-bitmap'
ADAPTER = 'raydium-clmm/6d7a0f82-spl-bitmap-1'
REPORT = ARTIFACTS / 'rayclmm-bitmap-golden-generation.json'
OLD_HASHES = {'rayclmm-quotes.json': 'ef1a0be7c80d6f7f5e815b75a10223a9a7be225d279bc74e057664b5af4f6f31',
              'rayclmm-token-fee-quotes.json': '6d0f2da485cbc86edc67e3f19343e97245f1422556c0932e94b9efc5931b74b6',
              'rayclmm-tickmath.json': '06632193a2fc8733494bbc8a6dfcdc56f267220f7289883fa56b18ada8c7b171'}
OLD_FIXTURE_HASH = 'f7d198d2398f19f5bbe52d3e2c85615d66c63dfa18aec326bf3da4f8127769a7'


def sha(data):
    return hashlib.sha256(data).hexdigest()


def load(name):
    spec = importlib.util.spec_from_file_location(name.replace('-', '_'), Path(__file__).with_name(name + '.py'))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def uint(value, bits, string=False):
    if string:
        assert type(value) is str and str(int(value)) == value
    else:
        assert type(value) is int
    assert 0 <= int(value) < 2**bits, (bits, value)


def audit_pool(p, strict):
    assert p['protocol'] == PROFILE and p['adapter_version'] == ADAPTER
    s = p['raydium_clmm']
    mode = s.get('collect_fee_on', 0)
    assert type(mode) is int and mode in (0, 1, 2)
    for field in ['sqrt_price_x64', 'liquidity', 'fee_growth_global0', 'fee_growth_global1']:
        uint(s[field], 128, True)
    for field in ['raw_vault0', 'raw_vault1', 'protocol_fees0', 'protocol_fees1', 'fund_fees0', 'fund_fees1',
                  'bank_slot', 'bank_unix_timestamp', 'bank_epoch', 'open_time', 'recent_epoch', 'observation_recent_epoch']:
        uint(s[field], 64)
    for field in ['trade_fee_rate', 'protocol_fee_rate', 'fund_fee_rate', 'tick_spacing', 'max_steps',
                  'pool_bump', 'mint0_decimals', 'mint1_decimals', 'observation_index']:
        uint(s[field], 32)
    assert len(s['bitmap']) == 16 and len(s['observations']) == 100
    for word in s['bitmap']:
        uint(word, 64)
    e = s.get('bitmap_extension')
    if strict:
        assert e is not None and len(e['positive']) == len(e['negative']) == 112
    if e is not None:
        assert set(e) == {'address', 'positive', 'negative'}
        for word in e['positive'] + e['negative']:
            uint(word, 64)
    for array in s['tick_arrays']:
        for tick in array['ticks']:
            for field in ['liquidity_gross', 'fee_growth_outside0', 'fee_growth_outside1']:
                uint(tick[field], 128, True)
            net = tick['liquidity_net']
            assert type(net) is str and str(int(net)) == net and -(2**127) <= int(net) < 2**127
    for o in s['observations']:
        uint(o['block_timestamp'], 32)
        assert type(o['tick_cumulative']) is int and -(2**63) <= o['tick_cumulative'] < 2**63


def audit_result(p, amount, direction, result):
    audit_pool(p, False)
    uint(amount, 64, True)
    assert type(direction) is bool
    if 'reverted' in result:
        assert result == {'reverted': True} and result['reverted'] is True
        return Counter()
    assert set(result) == {'amount_out', 'next_pool'}
    uint(result['amount_out'], 64, True)
    assert int(amount) > 0 and int(result['amount_out']) > 0
    q = result['next_pool']
    audit_pool(q, True)
    assert {k:v for k,v in p.items() if k != 'raydium_clmm'} == {k:v for k,v in q.items() if k != 'raydium_clmm'}
    s, t = p['raydium_clmm'], q['raydium_clmm']
    mode = s.get('collect_fee_on', 0)
    assert t.get('collect_fee_on', 0) == mode
    input_token, output_token = (0,1) if direction else (1,0)
    fee_token = input_token if mode == 0 else mode - 1
    assert t[f'raw_vault{input_token}'] == s[f'raw_vault{input_token}'] + int(amount)
    assert t[f'raw_vault{output_token}'] == s[f'raw_vault{output_token}'] - int(result['amount_out'])
    assert s['bitmap'] == t['bitmap'] and s['bitmap_extension'] == t['bitmap_extension']
    assert t['recent_epoch'] == s['bank_epoch']
    for field in [f'protocol_fees{1-fee_token}', f'fund_fees{1-fee_token}', f'fee_growth_global{1-fee_token}']:
        assert s[field] == t[field], field
    for field in ['protocol_fees0','protocol_fees1','fund_fees0','fund_fees1']:
        assert t[field] >= s[field]
    counts = Counter(successful_results=1)
    counts['input_fee_results' if fee_token == input_token else 'output_fee_results'] += 1
    counts['growth_wrap_results'] += int(int(t[f'fee_growth_global{fee_token}']) < int(s[f'fee_growth_global{fee_token}']))
    counts['observation_ring_updates'] += int(t['observation_index'] != s['observation_index'])
    before = s['tick_current'] // (60*s['tick_spacing'])
    after = t['tick_current'] // (60*t['tick_spacing'])
    counts['cross_array_results'] += int(before != after)
    counts['cross_base_extension_results'] += int((-512 <= before <= 511) != (-512 <= after <= 511))
    # Count crossings of the actual source groups, with base treated as one group.
    def group(index):
        return ('base',0) if -512 <= index <= 511 else ('positive', (index-512)//512) if index >=512 else ('negative',(-index-513)//512)
    counts['cross_bitmap_group_results'] += int(group(before) != group(after))
    return counts


def audit(corpus):
    assert len(corpus['cases']) >= 160 and len(corpus['sequences']) >= 8
    assert corpus['reference']['business_math_imported'] is False
    counts, cells, successes = Counter(), Counter(), Counter()
    names = [c['name'] for c in corpus['cases']]
    assert len(names) == len(set(names))
    vector_names = set(corpus['independent_vectors'])
    assert len(vector_names) == len(corpus['independent_vectors']) == 24
    assert vector_names.issubset(names)
    vectors = set()
    for c in corpus['cases']:
        assert set(c) == {'name','pool','input','zero_for_one','expected'}
        p, amount, direction, result = [c[k] for k in ['pool','input','zero_for_one','expected']]
        counts.update(audit_result(p, amount, direction, result))
        mode = p['raydium_clmm'].get('collect_fee_on',0)
        cells[(mode,direction)] += 1
        counts['cases'] += 1
        counts['reverted_cases' if result.get('reverted') else 'successful_cases'] += 1
        if not result.get('reverted'):
            successes[(mode,direction)] += 1
        if c['name'] in vector_names:
            assert not result.get('reverted')
            side = 'negative' if p['raydium_clmm']['tick_current'] < 0 else 'positive'
            vectors.add((side,mode,direction,amount))
    assert vectors == {(side,m,d,a) for side in ['positive','negative'] for m in (0,1,2) for d in (False,True) for a in ['1000000','2000000000']}
    assert max(cells.values()) - min(cells.values()) <= 2
    assert len(successes) == 6 and min(successes.values()) > 0
    seq_shapes = set()
    seq_names = [s['name'] for s in corpus['sequences']]
    assert len(seq_names) == len(set(seq_names))
    for seq in corpus['sequences']:
        current = seq['pool']
        directions=[]
        assert len(seq['steps']) >= 2
        mode=current['raydium_clmm'].get('collect_fee_on',0)
        for step in seq['steps']:
            assert not step['expected'].get('reverted')
            counts.update(audit_result(current,step['input'],step['zero_for_one'],step['expected']))
            current=step['expected']['next_pool']
            directions.append(step['zero_for_one'])
            counts['sequence_steps'] += 1
        assert current == seq['expected_final_pool']
        seq_shapes.add((mode,tuple(directions)))
        counts['sequences'] += 1
    assert seq_shapes == {(m,shape) for m in (0,1,2) for shape in [(True,True),(False,False),(True,False),(False,True)]}
    for label in ['index512','index-512','index1024','index-1024','index7168','index-7168','empty-bitmap',
                  'missing-extension-proof','selected-missing-proof-array','ticklimits','u128-max','u64-max',
                  'raw-input-overflow','raw-output-underflow','protocol-counter-overflow','fund-counter-overflow',
                  'growth-wrap','ceil-high-fee','conservative-max-steps','retained-outside-domain-bits']:
        assert any(label in name for name in names), label
    for field in ['cross_base_extension_results','cross_bitmap_group_results','growth_wrap_results','observation_ring_updates']:
        assert counts[field]>0, field
    counts['independent_base_vectors']=len(vectors)
    for (mode,direction),n in cells.items():
        counts[f'mode{mode}_{"zero_for_one" if direction else "one_for_zero"}_cases']=n
        counts[f'mode{mode}_{"zero_for_one" if direction else "one_for_zero"}_successful_cases']=successes[(mode,direction)]
    return dict(sorted(counts.items()))


def key_bytes(s):
    alphabet='123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz'
    n=0
    for c in s:
        n=n*58+alphabet.index(c)
    return n.to_bytes(32,'big')


def audit_fixture(fixture, corpus, old):
    p=fixture['pool']
    assert p == corpus['base_pool']
    assert fixture['pool_config']['protocol'] == PROFILE
    assert fixture['pool_config']['accounts'] == fixture['rpc_request_accounts'][:7]
    assert len(fixture['rpc_request_accounts']) == 19
    assert [fixture['rpc_accounts'][a] for a in fixture['rpc_request_accounts']] == fixture['rpc_response']['result']['value']
    assert fixture['rpc_response']['result']['context']['slot'] == p['raydium_clmm']['bank_slot']
    # Preserve identity, config, fees, vaults and liquidity of the original fixture.
    for key,value in old['pool'].items():
        if key not in ['protocol','adapter_version','raydium_clmm']:
            assert p[key] == value, key
    moved={'bitmap_extension','bitmap','tick_arrays','tick_current','sqrt_price_x64'}
    assert {k:v for k,v in p['raydium_clmm'].items() if k not in moved} == {k:v for k,v in old['pool']['raydium_clmm'].items() if k not in moved}
    for key,value in old['rpc_accounts'].items():
        if key != p['pool_id'] and key in fixture['rpc_accounts']:
            assert fixture['rpc_accounts'][key] == value, key
    for f in [fixture,fixture['metadata']['alternate_negative_base'],fixture['metadata']['alternate_fixture'],fixture['metadata']['next_bank_fixture']]:
        s=f['pool']['raydium_clmm'];e=s['bitmap_extension']
        audit_pool(f['pool'],True)
        account=f['rpc_accounts'][e['address']]
        assert account['space']==1832 and account['owner']==s['program_id'] and account['executable'] is False
        raw=base64.b64decode(account['data_base64'])
        assert len(raw)==1832
        assert raw[:8] == hashlib.sha256(b'account:TickArrayBitmapExtension').digest()[:8]
        assert raw[8:40] == key_bytes(f['pool']['pool_id'])
        assert list(struct.unpack('<224Q',raw[40:])) == e['positive']+e['negative']
        assert f['rpc_request_accounts'][-1] == e['address']
        assert f['rpc_response']['result']['value'][-1] == account
    bank=fixture['metadata']['next_bank_fixture']
    assert bank['pool'] == corpus['next_bank_pool']
    assert bank['rpc_request_accounts']==fixture['rpc_request_accounts']
    assert bank['pool']['raydium_clmm']['collect_fee_on']==1
    before=p['raydium_clmm']['bitmap_extension'];after=bank['pool']['raydium_clmm']['bitmap_extension']
    assert sum(w.bit_count() for w in before['positive']+before['negative']) == sum(w.bit_count() for w in after['positive']+after['negative'])+1
    assert bank['pool']['raydium_clmm']['tick_arrays'][-1]['ticks']==[]
    for v in fixture['metadata']['next_bank_vectors']:
        assert not v['expected'].get('reverted')
        match=[c for c in corpus['cases'] if c['pool']==bank['pool'] and c['input']==v['input'] and c['zero_for_one']==v['zero_for_one']]
        assert len(match)==1 and match[0]['expected']==v['expected']
    probe=fixture['metadata']['next_bank_scope_probe']
    assert probe['input']=='30000000000' and probe['zero_for_one'] is False
    assert probe['before_expected']['amount_out']=='1377715832'
    assert probe['mode_only_expected']['next_pool']['raydium_clmm']['tick_current'] >= 30840
    assert probe['after_expected']=={'reverted':True}
    for before_pool,result in [(p,probe['before_expected']),(bank['pool'],probe['after_expected'])]:
        matching=[c for c in corpus['cases'] if c['pool']==before_pool and c['input']==probe['input'] and c['zero_for_one'] is False]
        assert len(matching)==1 and matching[0]['expected']==result
    candidates=fixture['metadata']['next_bank_scope_probe_candidates']
    assert [c['input'] for c in candidates]==['20000000000','25000000000','30000000000','35000000000']
    assert [c['after_reverted'] for c in candidates]==[False,True,True,True]
    assert fixture['metadata']['alternate_fixture']['pool']==next(s['expected']['next_pool'] for c in corpus['cases'] if c['pool']==p and c['input']=='1000000' and c['zero_for_one'] for s in [c])


def audit_indices(indices):
    vectors=indices['vectors']
    assert 60<=len(vectors)<=100
    assert len({v['index'] for v in vectors})==len(vectors)
    assert {-7394,-1025,-1024,-513,512,1023,1024,7393}.issubset({v['index'] for v in vectors})
    for v in vectors:
        assert v['tick_spacing']==1 and v['start_tick_index']==v['index']*60
        assert v['expected_initialized'] is True
        assert len(v['positive'])==len(v['negative'])==112
        for w in v['positive']+v['negative']:
            uint(w,64)
        assert sum(w.bit_count() for w in v['positive']+v['negative'])==1
    for side in ['positive','negative']:
        for group in range(14):
            assert any(any(v[side][group*8:group*8+8]) for v in vectors)
    assert all(v['rejected'] is True for v in indices['invalid_source_starts'])
    return len(vectors)


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--offline',action='store_true')
    parser.add_argument('--check',action='store_true')
    parser.add_argument('--report-only',action='store_true',help='Verify existing bytes and refresh generation evidence only')
    args=parser.parse_args()
    sources=load('rayclmm-reference-fetch').verify()
    assert sum(len(a['files']) for a in sources['archives'])==225
    dependencies=load('rayclmm-token-fee-golden').source_checks()
    preserved={}
    for name,digest in OLD_HASHES.items():
        projects=['integration','dex-router'] if name!='rayclmm-tickmath.json' else ['integration','chain-parser-go','chain-parser-rust','dex-router']
        for project in projects:
            path=ROOT/project/'test/contracts'/name
            assert sha(path.read_bytes())==digest, path
            preserved[str(path.relative_to(ROOT))]=digest
    for path in [ROOT/'integration/test/solana/rayclmm-base.json',ROOT/'chain-parser-rust/test/fixtures/solana/rayclmm-base.json',REFERENCE/'base-fixture.json']:
        assert sha(path.read_bytes())==OLD_FIXTURE_HASH,path
        preserved[str(path.relative_to(ROOT))]=OLD_FIXTURE_HASH
    toolchain=Path.home()/'.rustup/toolchains/1.90.0-x86_64-apple-darwin/bin'
    cargo,rustc,rustdoc=[str(toolchain/name) for name in ['cargo','rustc','rustdoc']]
    env=dict(os.environ,RUSTC=rustc,RUSTDOC=rustdoc,CARGO_TARGET_DIR=str(RUNTIME))
    commands=[];transcript=[]
    def run(command,payload=False,data=None):
        commands.append(command)
        c=subprocess.run(command,cwd=REFERENCE,env=env,input=data,stdout=subprocess.PIPE,stderr=subprocess.PIPE)
        transcript.append('$ '+' '.join(command)+'\n'+c.stderr.decode()+('stdout SHA-256: '+sha(c.stdout)+'\n' if payload else c.stdout.decode()))
        if c.returncode:
            print(transcript[-1],file=sys.stderr)
            raise SystemExit(c.returncode)
        return c.stdout
    compiler=run([rustc,'-Vv']).decode().strip()
    assert compiler.startswith('rustc 1.90.0 ')
    cargo_version=run([cargo,'-V']).decode().strip()
    flags=['--locked','--config','./cargo-source.toml']+(['--offline'] if args.offline else [])
    run([sys.executable,str(REFERENCE/'tests/bitmap_runner.py')])
    run([cargo,'test',*flags])
    run([cargo,'build',*flags])
    binary=str(RUNTIME/'debug/raydium-clmm-reference')
    data={}
    for command in ['generate-bitmap','bitmap-fixture','bitmap-vectors']:
        data[command]=run([binary,command],True)
        assert data[command]==run([binary,command],True),command+' nondeterministic'
    for command,name in [('generate','rayclmm-quotes.json'),('generate-token-fee','rayclmm-token-fee-quotes.json'),('tick-vectors','rayclmm-tickmath.json')]:
        assert sha(run([binary,command],True))==OLD_HASHES[name], 'old native generation changed: '+name
    assert sha(run([binary,'fixture'],True))==OLD_FIXTURE_HASH
    corpus,fixture,indices=[json.loads(data[c]) for c in ['generate-bitmap','bitmap-fixture','bitmap-vectors']]
    counts=audit(corpus)
    counts['source_index_vectors']=audit_indices(indices)
    counts['invalid_source_index_vectors']=len(indices['invalid_source_starts'])
    audit_fixture(fixture,corpus,json.loads((REFERENCE/'base-fixture.json').read_bytes()))
    # Re-serialize full post-pool through the public raw account adapter.
    after=fixture['metadata']['alternate_fixture']['pool']
    rebuilt=json.loads(run([binary,'fixture-from-pool'],True,json.dumps(after).encode()))
    assert rebuilt==fixture['metadata']['alternate_fixture']
    outputs=[]
    outputs += [(ROOT/project/'test/contracts/rayclmm-bitmap-quotes.json',data['generate-bitmap']) for project in ['integration','dex-router']]
    outputs += [(ROOT/project/'test/contracts/rayclmm-bitmap-indices.json',data['bitmap-vectors']) for project in ['integration','chain-parser-go','chain-parser-rust','dex-router']]
    outputs += [(path,data['bitmap-fixture']) for path in [ROOT/'integration/test/solana/rayclmm-bitmap-base.json',ROOT/'chain-parser-rust/test/fixtures/solana/rayclmm-bitmap-base.json',REFERENCE/'bitmap-base-fixture.json']]
    harness=sorted(list((REFERENCE/'src').glob('*.rs'))+list((REFERENCE/'tests').glob('*.rs'))+list((REFERENCE/'tests').glob('*.py'))+[REFERENCE/'README.md',REFERENCE/'Cargo.toml',REFERENCE/'cargo-source.toml',Path(__file__).resolve()])
    report={'schema_version':1,'revision':sources['revision'],'profile':PROFILE,'adapter_version':ADAPTER,
            'unchanged_source_files_verified':225,'source_adaptations':[],
            'archives':{a['archive']:a['archive_sha256'] for a in sources['archives']},'compiler':compiler,'cargo':cargo_version,
            'cargo_lock_sha256':sha((REFERENCE/'Cargo.lock').read_bytes()),'original_cargo_lock_sha256':sha((REFERENCE/'upstream/Cargo.lock').read_bytes()),
            'source_manifest_sha256':sha((REFERENCE/'source-metadata.json').read_bytes()),'dependencies':dependencies,
            'harness_files':{str(p.relative_to(ROOT)):sha(p.read_bytes()) for p in harness},
            'counts':counts,'golden_sha256':sha(data['generate-bitmap']),'fixture_sha256':sha(data['bitmap-fixture']),
            'indices_sha256':sha(data['bitmap-vectors']),'preserved_hashes':preserved,
            'outputs':{str(p.relative_to(ROOT)):sha(b) for p,b in outputs},
            'native_swap_internal_executed':True,'native_extension_account_supplied':True,'native_extension_poststate_extracted':True,
            'source_flip_generated_indices':True,'python_audit_mutation_tests':4,
            'next_bank_scope_probe':fixture['metadata']['next_bank_scope_probe'],
            'next_bank_scope_probe_candidates':fixture['metadata']['next_bank_scope_probe_candidates'],'business_math_imported':False,'deterministic_regeneration':True,
            'sbf_transaction_executed':False,'token_program_executed':False,'synthetic_proof_identity':True,
            'limits':{'extension_account_bytes':1832,'positive_words':112,'negative_words':112,'tick_spacing':1,
                      'source_tick_min':-443636,'source_tick_max':443636,'source_array_index_min':-7394,'source_array_index_max':7393,
                      'logical_array_index_min':-7680,'logical_array_index_max':7679,'proof_arrays_min':3,'proof_arrays_max':7,
                      'max_steps_upper_bound':8192,'conservative_admission':'all initialized proof ticks plus one final partial step <= max_steps'},
            'coverage_limits':['Original swap_internal and original extension AccountLoad Ref/RefMut execute natively; signature is Option<&AccountInfo>. No upstream patches.',
                               'Anchor entrypoint validation, SBF bank/runtime, CPI, token program, signatures, owner/executable checks, user balances and WSOL backing are not executed.',
                               'Synthetic loader/token code proofs make no deployed bytecode identity claim. No services or infrastructure are started.',
                               'Static collect_fee_on 0/1/2 only; no dynamic fee, custom seed, reward, order, Token2022 or production deployment acceptance.',
                               'Original search selects arrays; selected missing proof rejects. Logical out-of-domain bits are retained, not flipped as valid source starts.',
                               'max_steps preflight is a conservative admission bound, not an instruction compute meter. Some width/overflow probes are intentionally unreachable base states.'],
            'commands':commands,'environment':{'RUSTC':rustc,'RUSTDOC':rustdoc,'CARGO_TARGET_DIR':str(RUNTIME)}}
    if args.check:
        for path,b in outputs:
            assert path.read_bytes()==b,'stale output: '+str(path)
        saved=json.loads(REPORT.read_bytes())
        for field in ['archives','cargo_lock_sha256','original_cargo_lock_sha256','source_manifest_sha256','harness_files','counts','golden_sha256','fixture_sha256','indices_sha256','outputs','preserved_hashes','limits']:
            assert saved[field]==report[field], 'stale report: '+field
    else:
        for path,b in outputs:
            if args.report_only:
                assert path.read_bytes()==b, 'report-only would change output: '+str(path)
            else:
                path.parent.mkdir(parents=True,exist_ok=True)
                path.write_bytes(b)
        REPORT.write_text(json.dumps(report,indent=2)+'\n')
        (ARTIFACTS/'rayclmm-bitmap-golden-generation.log').write_text('\n'.join(transcript)+'\n')
    print(json.dumps({'verified':True,'check_only':args.check,**counts,'golden_sha256':report['golden_sha256'],'fixture_sha256':report['fixture_sha256'],'indices_sha256':report['indices_sha256']},indent=2))


if __name__=='__main__':
    main()
