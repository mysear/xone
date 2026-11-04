#!/usr/bin/env python3
"""Build only the independent native oracle; audit/regenerate full poststate goldens."""
import argparse
from collections import Counter
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import tomllib

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[3]
REFERENCE = ROOT / 'integration/tools/reference/raydium-clmm'
OUTPUT = ROOT / 'integration/test/contracts/rayclmm-quotes.json'
COPY = ROOT / 'dex-router/test/contracts/rayclmm-quotes.json'
FIXTURE = ROOT / 'integration/test/solana/rayclmm-base.json'
ARTIFACTS = ROOT / 'integration/artifacts/local'
# Explicitly outside the shared workspace: no service builds or infrastructure.
RUNTIME = Path('/tmp/xone-raydium-clmm/target2')

def sha(data):
    return hashlib.sha256(data).hexdigest()

def load_fetch():
    spec = importlib.util.spec_from_file_location('rayclmm_fetch', Path(__file__).with_name('rayclmm-reference-fetch.py'))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module

def audit(corpus):
    assert len(corpus['cases']) >= 300 and len(corpus['sequences']) >= 8
    names = [x['name'] for x in corpus['cases']]
    assert len(names) == len(set(names))
    counts = Counter()
    def typed(pool):
        s = pool['raydium_clmm']
        for field in ['sqrt_price_x64', 'liquidity', 'fee_growth_global0', 'fee_growth_global1']:
            assert type(s[field]) is str and 0 <= int(s[field]) < 2**128
        for field in ['raw_vault0', 'raw_vault1', 'protocol_fees0', 'protocol_fees1', 'fund_fees0', 'fund_fees1', 'bank_slot', 'bank_unix_timestamp', 'bank_epoch', 'open_time', 'recent_epoch', 'observation_recent_epoch']:
            assert type(s[field]) is int and 0 <= s[field] < 2**64
        assert len(s['bitmap']) == 16 and all(type(n) is int and 0 <= n < 2**64 for n in s['bitmap'])
        assert len(s['observations']) == 100
        for a in s['tick_arrays']:
            for t in a['ticks']:
                assert type(t['liquidity_net']) is str and -(2**127) <= int(t['liquidity_net']) < 2**127
                for field in ['liquidity_gross','fee_growth_outside0','fee_growth_outside1']:
                    assert type(t[field]) is str and 0 <= int(t[field]) < 2**128
        assert pool['reserve0'] == pool['reserve1'] == '0'
        assert pool['protocol'] == 'raydium-clmm-static'
        assert pool['adapter_version'] == 'raydium-clmm/6d7a0f82-spl-static-1'
    def result(p, amount, d, expected):
        typed(p)
        if expected.get('reverted'):
            assert expected == {'reverted': True}
            return
        assert set(expected) == {'amount_out', 'next_pool'}
        assert type(expected['amount_out']) is str and 0 < int(expected['amount_out']) < 2**64
        typed(expected['next_pool'])
        before, after = p['raydium_clmm'], expected['next_pool']['raydium_clmm']
        i, o = (0, 1) if d else (1, 0)
        assert after[f'raw_vault{i}'] == before[f'raw_vault{i}'] + int(amount)
        assert after[f'raw_vault{o}'] == before[f'raw_vault{o}'] - int(expected['amount_out'])
        assert after['recent_epoch'] == before['bank_epoch']
        assert after['observation_recent_epoch'] == before['observation_recent_epoch']
        assert after['bitmap'] == before['bitmap']
        assert after[f'protocol_fees{o}'] == before[f'protocol_fees{o}']
        assert after[f'fund_fees{o}'] == before[f'fund_fees{o}']
        for field in ['protocol_fees0','protocol_fees1','fund_fees0','fund_fees1']:
            assert after[field] >= before[field]
        counts['successful_results'] += 1
        if int(after['fee_growth_global0']) < int(before['fee_growth_global0']) or int(after['fee_growth_global1']) < int(before['fee_growth_global1']):
            counts['growth_wrap_results'] += 1
        touched = sum(a['recent_epoch'] != b['recent_epoch'] for a,b in zip(after['tick_arrays'], before['tick_arrays']))
        if touched >= 2:
            counts['cross_array_results'] += 1
        if after['observation_index'] != before['observation_index']:
            counts['observation_ring_updates'] += 1
    for case in corpus['cases']:
        assert set(case) == {'name','pool','input','zero_for_one','expected'}
        assert type(case['input']) is str
        result(case['pool'], case['input'], case['zero_for_one'], case['expected'])
        counts['cases'] += 1
        counts['reverted_cases' if case['expected'].get('reverted') else 'successful_cases'] += 1
        counts['zero_for_one' if case['zero_for_one'] else 'one_for_zero'] += 1
    for sequence in corpus['sequences']:
        current = sequence['pool']
        for step in sequence['steps']:
            result(current,step['input'],step['zero_for_one'],step['expected'])
            assert not step['expected'].get('reverted')
            current = step['expected']['next_pool']
            counts['sequence_steps'] += 1
        assert current == sequence['expected_final_pool']
        counts['sequences'] += 1
    for prefix in ['zero-fee','static-fee','high-fee','protocol-all','fund-all','lp-only','empty-bitmap','zero-liquidity','growth-wrap','u128-max','u64-max','raw-input-overflow','raw-output-underflow','protocol-counter-overflow','fund-counter-overflow','tick-boundary','partial-window-exhaustion','observation-wrap','bitmap-low-edge','bitmap-high-edge','min-tick','max-tick','cross-liquidity-delta']:
        assert any(n.startswith(prefix) for n in names), prefix
    assert counts['growth_wrap_results'] and counts['cross_array_results'] and counts['observation_ring_updates']
    return dict(sorted(counts.items()))

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--offline',action='store_true')
    parser.add_argument('--check',action='store_true')
    parser.add_argument('--report-only',action='store_true',help='Verify all existing golden bytes and refresh only generation evidence')
    args=parser.parse_args()
    sources=load_fetch().verify()
    toolchain=Path.home()/'.rustup/toolchains/1.90.0-x86_64-apple-darwin/bin'
    cargo,rustc,rustdoc=[str(toolchain/x) for x in ['cargo','rustc','rustdoc']]
    env=dict(os.environ,RUSTC=rustc,RUSTDOC=rustdoc,CARGO_TARGET_DIR=str(RUNTIME))
    commands=[];transcript=[]
    def run(command):
        commands.append(command)
        completed=subprocess.run(command,cwd=REFERENCE,env=env,stdout=subprocess.PIPE,stderr=subprocess.PIPE)
        transcript.append('$ '+' '.join(command)+'\n'+completed.stderr.decode()+completed.stdout.decode())
        if completed.returncode:
            print(transcript[-1],file=sys.stderr)
            raise SystemExit(completed.returncode)
        return completed.stdout
    compiler=run([rustc,'-Vv']).decode().strip()
    assert compiler.startswith('rustc 1.90.0 ')
    cargo_version=run([cargo,'-V']).decode().strip()
    flags=['--locked','--config','./cargo-source.toml']+(['--offline'] if args.offline else [])
    run([cargo,'test',*flags])
    run([cargo,'build',*flags])
    binary=str(RUNTIME/'debug/raydium-clmm-reference')
    generated=run([binary,'generate'])
    assert generated==run([binary,'generate']), 'nondeterministic corpus'
    fixture=run([binary,'fixture'])
    vectors=run([binary,'tick-vectors'])
    assert vectors==run([binary,'tick-vectors']), 'nondeterministic tick digest'
    corpus=json.loads(generated);base=json.loads(fixture);ticks=json.loads(vectors)
    counts=audit(corpus)
    assert base['pool']==corpus['base_pool'] and base['metadata']['synthetic_proof_identity']
    assert len(ticks['vectors'])>=1000 and ticks['exhaustive']['count']==887273
    assert len(base['rpc_accounts'])==18
    assert base['pool_config']['accounts']==base['rpc_request_accounts'][:7]
    assert len(base['rpc_response']['result']['value'])==18
    lock=tomllib.loads((REFERENCE/'Cargo.lock').read_text())
    dependencies=[{'name':p['name'],'version':p['version'],'source':p.get('source','path'),'checksum':p.get('checksum')} for p in lock['package']]
    assert {p['name'] for p in dependencies if p['source']=='path'}=={'raydium-clmm','raydium-clmm-reference'}
    uint=[p for p in dependencies if p['name']=='uint']
    assert len(uint)==1 and uint[0]['source']=='git+https://github.com/raydium-io/parity-common#43a0303b9c2a23a00508816bd6d5a2e9381f143c'
    harness=sorted(list((REFERENCE/'src').glob('*.rs'))+list((REFERENCE/'tests').glob('*.rs'))+[REFERENCE/'Cargo.toml',REFERENCE/'cargo-source.toml',Path(__file__).resolve(),Path(__file__).with_name('rayclmm-reference-fetch.py').resolve()])
    report={'schema_version':1,'revision':sources['revision'],'archives':{a['archive']:a['archive_sha256'] for a in sources['archives']},'unchanged_source_files_verified':sum(len(a['files']) for a in sources['archives']),'source_adaptations':[],'dependency_source_replacement':'Locked git uint is served from byte-identical archived source with generated Cargo checksum metadata','compiler':compiler,'cargo':cargo_version,'cargo_lock_sha256':sha((REFERENCE/'Cargo.lock').read_bytes()),'original_cargo_lock_sha256':sha((REFERENCE/'upstream/Cargo.lock').read_bytes()),'source_manifest_sha256':sha((REFERENCE/'source-metadata.json').read_bytes()),'golden_sha256':sha(generated),'fixture_sha256':sha(fixture),'tick_vectors_sha256':sha(vectors),'harness_files':{str(p.relative_to(ROOT)):sha(p.read_bytes()) for p in harness},'counts':counts,'tick_vectors':len(ticks['vectors']),'exhaustive_tick_digest':ticks['exhaustive'],'deterministic_regeneration':True,'golden_copies_identical':True,'business_math_imported':False,'native_swap_internal_executed':True,'native_clock_syscall_executed':True,'observation_poststate_executed':True,'sbf_transaction_executed':False,'token_program_executed':False,'synthetic_proof_identity':True,'tests':'cargo test --locked passed (native integration tests and doctests)','dependencies':dependencies,'commands':commands,'environment':{'RUSTC':rustc,'RUSTDOC':rustdoc,'CARGO_TARGET_DIR':str(RUNTIME)},'coverage_limits':['Original swap_internal executes; Anchor instruction/accounts validation, SBF bank, CPI, token program, signatures, owner/executable checks, user balances and WSOL backing are not executed by this oracle','Raw account fixture is emitted from original structs for actual RPC decoder/service verification by the main worker; this script starts no services','Explicit profile bound 0 <= bank timestamp <= u32::MAX, stricter than original modulo cast','No extension account supplied; missing proof arrays or required extension cause native rejection','Conservative max_steps preflight counts all initialized ticks plus final partial step; not an instruction compute budget meter','Some counter/width failure probes intentionally use boundary states not reachable from the base fixture','Synthetic proof code is not deployed bytecode and its hash makes no deployed source identity claim']}
    targets=[(OUTPUT,generated),(COPY,generated),(FIXTURE,fixture),(REFERENCE/'base-fixture.json',fixture),(REFERENCE/'tick-vectors.json',vectors)]
    targets += [(ROOT/project/'test/contracts/rayclmm-tickmath.json',vectors) for project in ['integration','chain-parser-go','chain-parser-rust','dex-router']]
    targets += [(ROOT/'chain-parser-rust/test/fixtures/solana/rayclmm-base.json',fixture)]
    if args.check:
        for path,data in targets:
            assert path.read_bytes()==data,f'stale output: {path}'
        saved=json.loads((ARTIFACTS/'rayclmm-golden-generation.json').read_text())
        for field in ['archives','cargo_lock_sha256','original_cargo_lock_sha256','source_manifest_sha256','golden_sha256','fixture_sha256','tick_vectors_sha256','harness_files','counts','exhaustive_tick_digest']:
            assert saved[field]==report[field],f'stale report field: {field}'
    else:
        for path,data in targets:
            if args.report_only:
                assert path.read_bytes()==data,f'report-only would change output: {path}'
            else:
                path.parent.mkdir(parents=True,exist_ok=True);path.write_bytes(data)
        ARTIFACTS.mkdir(parents=True,exist_ok=True)
        (ARTIFACTS/'rayclmm-golden-generation.json').write_text(json.dumps(report,indent=2)+'\n')
        # Keep diagnostic transcript compact; fixture/golden payloads already have hashed outputs.
        transcript=[t for t in transcript if not any(t.startswith('$ '+binary+' '+cmd) for cmd in ['generate','fixture','tick-vectors'])]
        transcript.append(json.dumps({'counts':counts,'golden_sha256':sha(generated),'fixture_sha256':sha(fixture),'tick_vectors_sha256':sha(vectors)},indent=2))
        (ARTIFACTS/'rayclmm-golden-generation.log').write_text('\n'.join(transcript)+'\n')
    print(json.dumps({'verified':True,'check_only':args.check,**counts,'tick_vectors':len(ticks['vectors']),'golden_sha256':sha(generated)},indent=2))

if __name__=='__main__':
    main()
