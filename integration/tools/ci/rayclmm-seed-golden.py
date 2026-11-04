#!/usr/bin/env python3
"""Regenerate customizable-v2 evidence from unchanged native source, without service builds."""
import argparse
import base64
from collections import Counter
import copy
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import struct
import subprocess
import sys

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[3]
REFERENCE = ROOT / 'integration/tools/reference/raydium-clmm'
ARTIFACTS = ROOT / 'integration/artifacts/local'
RUNTIME = Path('/tmp/xone-raydium-clmm/target2')
PROFILE = 'raydium-clmm-custom-seed'
ADAPTER = 'raydium-clmm/6d7a0f82-spl-custom-seed-1'
REPORT = ARTIFACTS / 'rayclmm-seed-native-source.json'
SEEDS = (1, 255, 256, 65535)


def sha(data):
    return hashlib.sha256(data).hexdigest()


def load(name):
    spec = importlib.util.spec_from_file_location(name.replace('-', '_'), Path(__file__).with_name(name + '.py'))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


bitmap = load('rayclmm-bitmap-golden')
# Only reuse the independent reference's integer/accounting audit, never business math.
bitmap.PROFILE, bitmap.ADAPTER = PROFILE, ADAPTER


def execution_evidence():
    """Explicit execution boundary of this native reference, not an SBF bank."""
    return {
        'sbf_transaction_executed': False,
        'token_program_executed': False,
        'anchor_instruction_executed': False,
    }


def audit_execution_flags(report):
    # `is False` also rejects numeric zero, which compares equal to False.
    for field in execution_evidence():
        assert report.get(field) is False, 'execution flag must be explicit boolean false: ' + field


def audit_pool(p, strict=False):
    bitmap.audit_pool(p, strict)
    s = p['raydium_clmm']
    assert type(s['seed_index']) is int and 1 <= s['seed_index'] <= 65535
    assert s['program_id'] == 'CAMMCzo5YL8w4VFF8KVHrK22GGUsp5VTaW7grrKgrWqK'
    assert bitmap.key_bytes(p['token0']) < bitmap.key_bytes(p['token1'])
    for a in [p['pool_id'], p['token0'], p['token1'], s['amm_config'], s['observation'], s['vault0'], s['vault1']]:
        assert len(bitmap.key_bytes(a)) == 32


def audit_result(p, amount, direction, result):
    audit_pool(p)
    counts = bitmap.audit_result(p, amount, direction, result)
    if result.get('reverted'):
        return counts
    q = result['next_pool']
    audit_pool(q, True)
    a, b = p['raydium_clmm'], q['raydium_clmm']
    mutable = {'sqrt_price_x64','tick_current','liquidity','fee_growth_global0','fee_growth_global1',
               'protocol_fees0','protocol_fees1','fund_fees0','fund_fees1','recent_epoch',
               'raw_vault0','raw_vault1','tick_arrays','observation_initialized','observation_index','observations'}
    assert {k:v for k,v in a.items() if k not in mutable} == {k:v for k,v in b.items() if k not in mutable}
    assert len(a['tick_arrays']) == len(b['tick_arrays'])
    for before, after in zip(a['tick_arrays'], b['tick_arrays']):
        assert before['address'] == after['address'] and before['start_tick_index'] == after['start_tick_index']
        assert [(t['tick'],t['liquidity_gross'],t['liquidity_net']) for t in before['ticks']] == [(t['tick'],t['liquidity_gross'],t['liquidity_net']) for t in after['ticks']]
    counts['seed_preserved_results'] += 1
    return counts


def audit_identities(data):
    assert data['profile'] == PROFILE and data['adapter_version'] == ADAPTER
    assert data['config_index_encoding'] == 'u16 BE' and data['array_start_encoding'] == 'i32 BE'
    vectors = data['vectors']
    assert {v['seed_index'] for v in vectors} == {0,1,2,255,256,257,32768,65534,65535}
    assert len({v['pool_id'] for v in vectors}) == len(vectors)
    assert {'PoolState::set_seed_index','PoolState::seeds','PoolState::key'}.issubset(data['source_functions'])
    deps = {}
    for v in vectors:
        seed = v['seed_index']
        assert v['seed_le'] == list(struct.pack('<H', seed)) and v['seed_be'] == list(struct.pack('>H', seed))
        assert 0 <= v['pool_bump'] <= 255 and 0 <= v['wrong_be_pool_bump'] <= 255
        assert v['amm_config'] == data['amm_config'] and v['token0'] == data['token0'] and v['token1'] == data['token1']
        assert v['be_differs'] == (seed not in (0,257,65535))
        if seed not in (0,257,65535):
            assert v['pool_id'] != v['wrong_be_pool_id']
        if seed in (257,65535):
            assert v['pool_id'] == v['wrong_be_pool_id']
        for field in ['vault0','vault1','observation','bitmap_extension']:
            d = v['dependencies'][field]
            assert v[field] == d['address'] and 0 <= d['bump'] <= 255
            assert d['address'] not in deps.setdefault(field, set())
            deps[field].add(d['address'])
        starts = [a['start_tick_index'] for a in v['dependencies']['tick_arrays']]
        assert len(starts) == len(set(starts)) and {0,30720,-30780,61440,-61440}.issubset(starts)
        assert all(0 <= a['bump'] <= 255 for a in v['dependencies']['tick_arrays'])
    # BE(1) is LE(256), and BE(256) is LE(1), a concrete wrong-pool control.
    by_seed = {v['seed_index']:v for v in vectors}
    assert by_seed[1]['wrong_be_pool_id'] == by_seed[256]['pool_id']
    assert by_seed[256]['wrong_be_pool_id'] == by_seed[1]['pool_id']
    return len(vectors)


def audit(corpus, identities):
    audit_identities(identities)
    by_seed = {v['seed_index']:v for v in identities['vectors']}
    counts = Counter()
    assert corpus['reference']['profile'] == PROFILE and corpus['reference']['business_math_imported'] is False
    assert corpus['base_pool']['raydium_clmm']['seed_index'] == 1
    assert {p['raydium_clmm']['seed_index'] for p in corpus['seed_pools']} == set(SEEDS)
    for p in corpus['seed_pools']:
        assert p['raydium_clmm']['amm_config'] == corpus['base_pool']['raydium_clmm']['amm_config']
        assert (p['token0'],p['token1']) == (corpus['base_pool']['token0'],corpus['base_pool']['token1'])
    names = [c['name'] for c in corpus['cases']]
    assert len(names) >= 160 and len(names) == len(set(names))
    cells, successes, vectors = Counter(), Counter(), set()
    for c in corpus['cases']:
        assert set(c) == {'name','pool','input','zero_for_one','expected'}
        p = c['pool']; s = p['raydium_clmm']; seed = s['seed_index']; v = by_seed[seed]
        assert p['pool_id'] == v['pool_id'] and s['pool_bump'] == v['pool_bump']
        for field in ['vault0','vault1','observation']:
            assert s[field] == v[field]
        counts.update(audit_result(p,c['input'],c['zero_for_one'],c['expected']))
        cell = (seed,s.get('collect_fee_on',0),c['zero_for_one'])
        cells[cell] += 1
        counts['cases'] += 1
        if c['expected'].get('reverted'):
            counts['reverted_cases'] += 1
        else:
            counts['successful_cases'] += 1
            successes[cell] += 1
    assert set(cells) == {(s,m,d) for s in SEEDS for m in (0,1,2) for d in (False,True)}
    assert set(successes) == set(cells)
    by_name = {c['name']:c for c in corpus['cases']}
    bank_cases = [c for c in corpus['cases'] if c['name'].startswith('seed1-next-bank-exact-fixture-')]
    assert len(bank_cases) == 4
    assert {(c['input'],c['zero_for_one']) for c in bank_cases} == {(a,d) for a in ('1000000','2000000000') for d in (False,True)}
    assert all(c['pool'] == corpus['next_bank_pool'] and not c['expected'].get('reverted') for c in bank_cases)
    counts['exact_next_bank_vectors'] = len(bank_cases)
    for v in corpus['independent_vectors']:
        c = by_name[v['name']]
        assert v['expected'] == c['expected'] and not c['expected'].get('reverted')
        assert v['seed_index'] == c['pool']['raydium_clmm']['seed_index']
        assert v['collect_fee_on'] == c['pool']['raydium_clmm'].get('collect_fee_on',0)
        assert v['input'] == c['input'] and v['zero_for_one'] == c['zero_for_one']
        assert c['pool']['raydium_clmm']['tick_current'] == v['center_index'] * 60
        vectors.add((v['seed_index'],v['collect_fee_on'],v['center_index'],v['zero_for_one'],v['input']))
    assert vectors == {(s,m,i,d,a) for s in SEEDS for m in (0,1,2) for i in (512,-512) for d in (False,True) for a in ('1000000','2000000000')}
    shapes = set()
    for seq in corpus['sequences']:
        current = seq['pool']; directions = []
        assert len(seq['steps']) >= 2
        for step in seq['steps']:
            assert not step['expected'].get('reverted')
            counts.update(audit_result(current,step['input'],step['zero_for_one'],step['expected']))
            current = step['expected']['next_pool']; directions.append(step['zero_for_one']); counts['sequence_steps'] += 1
        assert current == seq['expected_final_pool']
        shapes.add((current['raydium_clmm']['seed_index'],current['raydium_clmm'].get('collect_fee_on',0),tuple(directions)))
        counts['sequences'] += 1
    assert shapes == {(s,m,d) for s in SEEDS for m in (0,1,2) for d in ((False,False),(True,True),(False,True),(True,False))}
    for c in corpus['independence_cases']:
        current = {p['pool_id']:p for p in c['pools']}
        assert len(current) == 2
        assert len({p['raydium_clmm']['seed_index'] for p in current.values()}) == 2
        assert len({(p['token0'],p['token1'],p['raydium_clmm']['amm_config']) for p in current.values()}) == 1
        for step in c['steps']:
            key = step['pool_id']
            counts.update(audit_result(current[key],step['input'],step['zero_for_one'],step['expected']))
            current[key] = step['expected']['next_pool']; counts['independence_steps'] += 1
        assert current == {p['pool_id']:p for p in c['expected_final_pools']}
        counts['independence_cases'] += 1
    assert counts['independence_cases'] >= 3
    probes = corpus['error_probes']
    assert len(probes) >= 14 and len({p['name'] for p in probes}) == len(probes)
    assert all(p['expected'] == {'reverted':True} and p['reason_contains'] in p['native_reason'] for p in probes)
    for label in ['seed-width-0','seed-width-65536','new-profile-old-pool-PDA','wrong-BE-seed1','wrong-BE-seed255','wrong-BE-seed256']:
        assert any(p['name'] == label for p in probes)
    for label in ['index512','index-512','index1024','index-1024','index7168','index-7168','conservative-max-steps',
                  'missing-extension-proof','u64-max','u128-max','growth-wrap','raw-input-overflow','raw-output-underflow',
                  'protocol-counter-overflow','fund-counter-overflow','selected-missing-proof-array','retained-outside-domain-bits']:
        assert any(label in name for name in names), label
    for field in ['cross_base_extension_results','cross_bitmap_group_results','growth_wrap_results','observation_ring_updates']:
        assert counts[field] > 0, field
    counts['identity_error_probes'] = len(probes)
    counts['independent_base_vectors'] = len(vectors)
    for (s,m,d),n in sorted(cells.items()):
        counts[f'seed{s}_mode{m}_dir{int(d)}_cases'] = n
        counts[f'seed{s}_mode{m}_dir{int(d)}_successful_cases'] = successes[(s,m,d)]
    return dict(sorted(counts.items()))


def raw(f, address):
    a = f['rpc_accounts'][address]
    b = base64.b64decode(a['data_base64'], validate=True)
    assert a['space'] == len(b) and a['executable'] is False
    return b


def audit_fixture(f, corpus, identities):
    assert f['pool'] == corpus['base_pool']
    metadata = f['metadata']; bank = metadata['next_bank_fixture']
    assert f['pool_config'] == bank['pool_config'] and f['rpc_request_accounts'] == bank['rpc_request_accounts']
    assert f['pool']['pool_id'] == bank['pool']['pool_id']
    assert bank['pool'] == corpus['next_bank_pool']
    assert bank['pool']['raydium_clmm']['seed_index'] == 1
    assert bank['pool']['raydium_clmm']['collect_fee_on'] == 1
    assert metadata['seed_offset'] == [391,393]
    by_seed = {v['seed_index']:v for v in identities['vectors']}
    fixtures = [f,bank,metadata['alternate_negative_base'],metadata['alternate_fixture'],*metadata['seed_fixtures']]
    assert {q['pool']['raydium_clmm']['seed_index'] for q in metadata['seed_fixtures']} >= {256,65535}
    for q in fixtures:
        p=q['pool'];s=p['raydium_clmm'];e=s['bitmap_extension'];v=by_seed[s['seed_index']]
        audit_pool(p,True)
        assert p['pool_id'] == v['pool_id'] and s['pool_bump'] == v['pool_bump']
        config=q['pool_config']
        assert config['id'] == p['pool_id'] and config['protocol'] == PROFILE
        assert config['accounts'] == q['rpc_request_accounts'][:7]
        assert 'seed_index' not in config['raydium_clmm']
        assert len(q['rpc_request_accounts']) == 19 and len(set(q['rpc_request_accounts'])) == 19
        assert [q['rpc_accounts'][a] for a in q['rpc_request_accounts']] == q['rpc_response']['result']['value']
        assert q['rpc_response']['result']['context']['slot'] == s['bank_slot']
        b=raw(q,p['pool_id'])
        assert len(b) == 1544 and b[:8] == hashlib.sha256(b'account:PoolState').digest()[:8]
        assert b[8] == v['pool_bump'] and b[9:41] == bitmap.key_bytes(s['amm_config'])
        assert b[73:105] == bitmap.key_bytes(p['token0']) and b[105:137] == bitmap.key_bytes(p['token1'])
        assert b[137:169] == bitmap.key_bytes(s['vault0']) and b[169:201] == bitmap.key_bytes(s['vault1'])
        assert b[201:233] == bitmap.key_bytes(s['observation'])
        assert b[390] == s.get('collect_fee_on',0) and b[391:393] == struct.pack('<H',s['seed_index']) and b[393:397] == bytes(4)
        # Actual classic SPL owner/authority field binds each vault to its seeded pool.
        for side in (0,1):
            vb=raw(q,s[f'vault{side}'])
            assert len(vb) == 165 and vb[:32] == bitmap.key_bytes(p[f'token{side}']) and vb[32:64] == bitmap.key_bytes(p['pool_id'])
            assert struct.unpack_from('<Q',vb,64)[0] == s[f'raw_vault{side}']
            assert q['rpc_accounts'][s[f'vault{side}']]['owner'] == 'TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA'
        ob=raw(q,s['observation']);assert ob[:8] == hashlib.sha256(b'account:ObservationState').digest()[:8]
        # Observation packs initialized/u64 epoch/u16 index before the pool pubkey.
        assert ob[19:51] == bitmap.key_bytes(p['pool_id'])
        for a in s['tick_arrays']:
            ab=raw(q,a['address']);assert ab[:8] == hashlib.sha256(b'account:TickArrayState').digest()[:8]
            assert ab[8:40] == bitmap.key_bytes(p['pool_id']) and struct.unpack_from('<i',ab,40)[0] == a['start_tick_index']
        eb=raw(q,e['address'])
        assert len(eb) == 1832 and eb[:8] == hashlib.sha256(b'account:TickArrayBitmapExtension').digest()[:8]
        assert eb[8:40] == bitmap.key_bytes(p['pool_id']) and list(struct.unpack('<224Q',eb[40:])) == e['positive']+e['negative']
        assert q['rpc_request_accounts'][-1] == e['address']
    a,b=f['pool']['raydium_clmm'],bank['pool']['raydium_clmm']
    assert a['tick_current'] == b['tick_current'] == 30720
    assert a['sqrt_price_x64'] == b['sqrt_price_x64'] and a['liquidity'] == b['liquidity']
    assert a['bitmap_extension'] != b['bitmap_extension'] and not b['tick_arrays'][-1]['ticks']
    for label,p in [('base_vectors',f['pool']),('next_bank_vectors',bank['pool'])]:
        vectors=metadata[label]
        assert {(v['zero_for_one'],v['input']) for v in vectors} == {(d,a) for d in (False,True) for a in ('1000000','2000000000')}
        for v in vectors:
            assert not v['expected'].get('reverted');audit_result(p,v['input'],v['zero_for_one'],v['expected'])
            if label == 'next_bank_vectors':
                exact = [c for c in corpus['cases'] if c['name'].startswith('seed1-next-bank-exact-fixture-') and c['input'] == v['input'] and c['zero_for_one'] == v['zero_for_one']]
                assert len(exact) == 1 and exact[0]['pool'] == p and exact[0]['expected'] == v['expected']
    probe=metadata['next_bank_scope_probe']
    assert probe['input'] == '30000000000' and probe['zero_for_one'] is False
    assert probe['before_expected']['amount_out'] == '1377715832' and probe['after_expected'] == {'reverted':True}
    assert probe['mode_only_expected']['next_pool']['raydium_clmm']['seed_index'] == 1
    assert probe['mode_only_expected']['next_pool']['raydium_clmm']['collect_fee_on'] == 1
    return len(fixtures)


def protected_outputs():
    # Hard pins established before seed implementation. Reports may refresh; old data may not.
    pins = {
        'rayclmm-quotes.json':'ef1a0be7c80d6f7f5e815b75a10223a9a7be225d279bc74e057664b5af4f6f31',
        'rayclmm-token-fee-quotes.json':'6d0f2da485cbc86edc67e3f19343e97245f1422556c0932e94b9efc5931b74b6',
        'rayclmm-bitmap-quotes.json':'350d4d447af4e07acb22786480cd2aa69ddcb85073a0ecfd80427079f20a851a',
        'rayclmm-tickmath.json':'06632193a2fc8733494bbc8a6dfcdc56f267220f7289883fa56b18ada8c7b171',
        'rayclmm-bitmap-indices.json':'24bbd337f791f217e6b3476d2284acaebeb53de77f26baaf99556ba9dadaea29',
    }
    preserved={}
    for name,digest in pins.items():
        projects=('integration','dex-router') if 'quotes' in name else ('integration','chain-parser-go','chain-parser-rust','dex-router')
        for project in projects:
            p=ROOT/project/'test/contracts'/name
            assert sha(p.read_bytes()) == digest, p
            preserved[str(p.relative_to(ROOT))]=digest
    for name,digest,reference_name in [
        ('rayclmm-base.json','f7d198d2398f19f5bbe52d3e2c85615d66c63dfa18aec326bf3da4f8127769a7','base-fixture.json'),
        ('rayclmm-bitmap-base.json','9ffc9ca1874243819b296dbd968e9407ecd1fd685c807606eca555628d5c164e','bitmap-base-fixture.json'),
    ]:
        for p in [ROOT/'integration/test/solana'/name,ROOT/'chain-parser-rust/test/fixtures/solana'/name,REFERENCE/reference_name]:
            assert sha(p.read_bytes()) == digest,p
            preserved[str(p.relative_to(ROOT))]=digest
    assert sha((REFERENCE/'tick-vectors.json').read_bytes()) == pins['rayclmm-tickmath.json']
    preserved[str((REFERENCE/'tick-vectors.json').relative_to(ROOT))]=pins['rayclmm-tickmath.json']
    return preserved


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--offline',action='store_true')
    parser.add_argument('--check',action='store_true')
    parser.add_argument('--report-only',action='store_true',help='Verify existing data bytes and refresh evidence only')
    args=parser.parse_args()
    sources=load('rayclmm-reference-fetch').verify()
    assert sum(len(a['files']) for a in sources['archives']) == 225
    dependencies=load('rayclmm-token-fee-golden').source_checks()
    preserved=protected_outputs()
    toolchain=Path.home()/'.rustup/toolchains/1.90.0-x86_64-apple-darwin/bin'
    cargo,rustc,rustdoc=[str(toolchain/n) for n in ('cargo','rustc','rustdoc')]
    env=dict(os.environ,RUSTC=rustc,RUSTDOC=rustdoc,CARGO_TARGET_DIR=str(RUNTIME),PYTHONDONTWRITEBYTECODE='1')
    commands=[];transcript=[]
    def run(cmd,payload=False,data=None):
        commands.append(cmd)
        c=subprocess.run(cmd,cwd=REFERENCE,env=env,input=data,stdout=subprocess.PIPE,stderr=subprocess.PIPE)
        transcript.append('$ '+' '.join(cmd)+'\n'+c.stderr.decode()+('stdout SHA-256: '+sha(c.stdout)+'\n' if payload else c.stdout.decode()))
        if c.returncode:
            print(transcript[-1],file=sys.stderr);raise SystemExit(c.returncode)
        return c.stdout
    compiler=run([rustc,'-Vv']).decode().strip();assert compiler.startswith('rustc 1.90.0 ')
    flags=['--locked','--config','./cargo-source.toml']+(['--offline'] if args.offline else [])
    run([cargo,'test',*flags]);run([cargo,'build',*flags])
    binary=str(RUNTIME/'debug/raydium-clmm-reference')
    data={}
    for command in ['generate-seed','seed-fixture','seed-identities']:
        data[command]=run([binary,command],True)
        assert data[command] == run([binary,command],True), 'nondeterministic '+command
    corpus,fixture,identities=[json.loads(data[c]) for c in ['generate-seed','seed-fixture','seed-identities']]
    counts=audit(corpus,identities)
    counts['identity_vectors']=audit_identities(identities)
    counts['raw_fixture_variants']=audit_fixture(fixture,corpus,identities)
    # Replay error cases, including invalid u32 seed widths, through the JSON CLI.
    for probe in corpus['error_probes']:
        result=json.loads(run([binary,'quote'],True,json.dumps({k:probe[k] for k in ('pool','input','zero_for_one')}).encode()))
        assert result['reverted'] is True and probe['reason_contains'] in result['reason']
    budget=[]
    for c in corpus['cases']:
        if 'conservative-max-steps' not in c['name']:
            continue
        p=copy.deepcopy(c['pool']);p['raydium_clmm']['max_steps']=8192
        q=json.loads(run([binary,'quote'],True,json.dumps({'pool':p,'input':c['input'],'zero_for_one':c['zero_for_one']}).encode()))
        assert not q.get('reverted'), c['name']
        touched=sum(a['recent_epoch']==p['raydium_clmm']['bank_epoch'] for a in q['next_pool']['raydium_clmm']['tick_arrays'])
        # Only actual multi-step inputs qualify as max_steps parity failures.
        assert touched >= 2, (c['name'],touched)
        budget.append({'name':c['name'],'input':c['input'],'zero_for_one':c['zero_for_one'],'native_success_with_full_budget':True,'touched_arrays':touched,'tick_before':p['raydium_clmm']['tick_current'],'tick_after':q['next_pool']['raydium_clmm']['tick_current']})
    assert len(budget) == 6
    counts['actual_multistep_budget_probes']=len(budget)
    old_commands={'generate':'integration/test/contracts/rayclmm-quotes.json','generate-token-fee':'integration/test/contracts/rayclmm-token-fee-quotes.json',
                  'generate-bitmap':'integration/test/contracts/rayclmm-bitmap-quotes.json','fixture':'integration/test/solana/rayclmm-base.json',
                  'bitmap-fixture':'integration/test/solana/rayclmm-bitmap-base.json','bitmap-vectors':'integration/test/contracts/rayclmm-bitmap-indices.json','tick-vectors':'integration/test/contracts/rayclmm-tickmath.json'}
    for command,name in old_commands.items():
        assert sha(run([binary,command],True)) == preserved[name], 'old native output changed: '+command
    outputs=[(ROOT/project/'test/contracts/rayclmm-seed-quotes.json',data['generate-seed']) for project in ['integration','dex-router']]
    outputs += [(ROOT/project/'test/contracts/rayclmm-seed-identities.json',data['seed-identities']) for project in ['integration','chain-parser-go','chain-parser-rust','dex-router']]
    outputs += [(p,data['seed-fixture']) for p in [ROOT/'integration/test/solana/rayclmm-seed-base.json',ROOT/'chain-parser-rust/test/fixtures/solana/rayclmm-seed-base.json',REFERENCE/'seed-base-fixture.json']]
    # Audit self-tests consume in-memory generated evidence before output files exist.
    test_path=REFERENCE/'tests/seed_runner.py'
    spec=importlib.util.spec_from_file_location('seed_audit_tests',test_path)
    test_module=importlib.util.module_from_spec(spec);spec.loader.exec_module(test_module)
    test_count=test_module.run_tests(sys.modules[__name__],corpus,fixture,identities)
    # Enforce ordered fresh report chain: static -> token fee -> bitmap -> seed.
    prior={}
    for name in ['rayclmm-golden-generation.json','rayclmm-token-fee-golden-generation.json','rayclmm-bitmap-golden-generation.json']:
        path=ARTIFACTS/name;r=json.loads(path.read_bytes())
        for f,digest in r['harness_files'].items():
            assert sha((ROOT/f).read_bytes()) == digest, 'refresh ordered report chain: '+f
        prior[str(path.relative_to(ROOT))]=sha(path.read_bytes())
    harness=sorted(set(list((REFERENCE/'src').glob('*.rs'))+list((REFERENCE/'tests').glob('*.rs'))+list((REFERENCE/'tests').glob('*.py'))+[REFERENCE/'README.md',REFERENCE/'Cargo.toml',REFERENCE/'cargo-source.toml',Path(__file__).resolve()]))
    report={'schema_version':1,'revision':sources['revision'],'profile':PROFILE,'adapter_version':ADAPTER,
        'unchanged_source_files_verified':225,'source_adaptations':[], 'compiler':compiler,
        'archives':{a['archive']:a['archive_sha256'] for a in sources['archives']},
        'cargo_lock_sha256':sha((REFERENCE/'Cargo.lock').read_bytes()),'original_cargo_lock_sha256':sha((REFERENCE/'upstream/Cargo.lock').read_bytes()),
        'source_manifest_sha256':sha((REFERENCE/'source-metadata.json').read_bytes()),'dependencies':dependencies,
        'harness_files':{str(p.relative_to(ROOT)):sha(p.read_bytes()) for p in harness},'prior_report_hashes':prior,
        'counts':counts,'golden_sha256':sha(data['generate-seed']),'fixture_sha256':sha(data['seed-fixture']),'identities_sha256':sha(data['seed-identities']),
        'outputs':{str(p.relative_to(ROOT)):sha(b) for p,b in outputs},'preserved_hashes':preserved,
        'deterministic_regeneration':True,'python_audit_mutation_tests':test_count,
        'native_set_seed_index_seeds_key_executed':True,'native_swap_internal_executed':True,'native_extension_account_supplied':True,
        'native_clock_syscall_executed':True,'full_pool_poststates':True,'business_math_imported':False,
        **execution_evidence(),
        'budget_probe_controls':budget,'next_bank_scope_probe':fixture['metadata']['next_bank_scope_probe'],
        'seed_raw_account_offset':[391,393],'zero_padding_offset':[393,397],
        'source_semantics':{
            'create_customizable_pool_v2':{'path':'upstream/programs/amm/src/instructions/create_customizable_pool_v2.rs','lines':[10,23,30,136,162,177,212],
                'evidence':'u16 instruction argument; positive seed required; canonical pool PDA appends seed_index.to_le_bytes(); vault/observation/extension seeds bind pool; initialize then set_seed_index'},
            'pool_signer':{'path':'upstream/programs/amm/src/states/pool.rs','lines':[115,117,119,172,187,246],
                'evidence':'fee_on then seed_index [u8;2] then four padding bytes; set_seed_index uses u16 LE; seeds omit nonempty seed at zero; key uses create_program_address with those seeds'},
            'spl_vault_authority':{'path':'upstream/programs/amm/src/util/token.rs','lines':[127,132,134,613,652,657],
                'evidence':'initialize_account3 authority is pool_state; classic SPL outbound transfer uses pool_state authority and original PoolState.seeds signer'},
            'tick_array':{'path':'upstream/programs/amm/src/states/tick_array.rs','lines':[43,46,48],
                'evidence':'original key derives tick_array + pool_id + start_tick_index.to_be_bytes()'},
            'extension':{'path':'upstream/programs/amm/src/states/tickarray_bitmap_extension.rs','lines':[47],
                'evidence':'original key derives pool_tick_array_bitmap_extension + pool_id'},
        },
        'limits':['Pinned source audit sample; synthetic code proofs are not deployed bytecode.',
                  'Native core/RefMut/Clock executes. Anchor instruction/account validation, SBF, CPI, SPL program transfers, signatures, executable/owner checks, user balances and WSOL backing are not executed by this oracle.',
                  'Static collect_fee_on 0/1/2; classic SPL; default dynamic fee; no orders, rewards or Token2022; seed1..65535; complete mandatory bitmap extension.',
                  'Conservative initialized-tick-count + 1 budget preflight is stricter admission than actual execution and is not a runtime compute meter. Budget failures use inputs proven to touch multiple arrays with native full budget.',
                  'Boundary counter/width probes can be unreachable from the baseline. Full-input exact-in only; missing selected proof rejects.',
                  'Seed256/65535 raw fixtures are separate pools; next bank changes seed1 fee0 to fee1 and clears extension/ticks without changing pool identity.',
                  'No services, infrastructure, or main Cargo builds run. Service integration is owned by the main worker.'],
        'commands':commands,'environment':{'RUSTC':rustc,'RUSTDOC':rustdoc,'CARGO_TARGET_DIR':str(RUNTIME)}}
    audit_execution_flags(report)
    if args.check:
        for p,b in outputs:
            assert p.read_bytes() == b, 'stale output: '+str(p)
        saved=json.loads(REPORT.read_bytes())
        audit_execution_flags(saved)
        for field in report:
            if field not in ['commands']:
                assert saved[field] == report[field], 'stale seed report: '+field
    else:
        for p,b in outputs:
            if args.report_only:
                assert p.read_bytes() == b, 'report-only would change output: ' + str(p)
            else:
                p.parent.mkdir(parents=True,exist_ok=True);p.write_bytes(b)
        REPORT.write_text(json.dumps(report,indent=2)+'\n')
        (ARTIFACTS/'rayclmm-seed-reference-build.log').write_text('\n'.join(transcript)+'\n')
    print(json.dumps({'verified':True,'check_only':args.check,**counts,'golden_sha256':report['golden_sha256'],'fixture_sha256':report['fixture_sha256'],'identities_sha256':report['identities_sha256']},indent=2))


if __name__ == '__main__':
    main()
