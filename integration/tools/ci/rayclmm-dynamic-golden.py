#!/usr/bin/env python3
"""Reproduce pinned original-native dynamic quotes, raw accounts and audit evidence.

No services, main Cargo workspace, business math, upstream edits or wall clock reads.
Use --timestamp explicitly when refreshing the held native Clock snapshot.
"""
import argparse
import base64
import copy
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import struct
import subprocess
import sys
from collections import Counter

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[3]
REFERENCE = ROOT / 'integration/tools/reference/raydium-clmm'
ARTIFACTS = ROOT / 'integration/artifacts/local'
RUNTIME = Path('/tmp/xone-raydium-clmm/target2')
REPORT = ARTIFACTS / 'rayclmm-dynamic-native-source.json'
PROFILE = 'raydium-clmm-dynamic'
ADAPTER = 'raydium-clmm/6d7a0f82-spl-dynamic-1'
CLOCK = 'SysvarC1ock11111111111111111111111111111111'
ANCHOR = 1793245486
FIELDS = ['filter_period','decay_period','reduction_factor','dynamic_fee_control','max_volatility_accumulator',
          'tick_spacing_index_reference','volatility_reference','volatility_accumulator','last_update_timestamp']
PARAMETERS = FIELDS[:5]


def sha(data):
    return hashlib.sha256(data).hexdigest()


def load(name):
    spec = importlib.util.spec_from_file_location(name.replace('-', '_'), Path(__file__).with_name(name + '.py'))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


bitmap = load('rayclmm-bitmap-golden')
bitmap.PROFILE, bitmap.ADAPTER = PROFILE, ADAPTER


def execution_evidence():
    return {'sbf_transaction_executed':False,'token_program_executed':False,'anchor_instruction_executed':False}


def audit_execution_flags(report):
    for key in execution_evidence():
        assert report.get(key) is False, 'explicit boolean false required: ' + key


def audit_pool(p):
    bitmap.audit_pool(p, True)
    s = p['raydium_clmm']; f = s['dynamic_fee']; spacing = s['tick_spacing']
    assert set(f) == set(FIELDS)
    assert 1 <= spacing <= 1000
    for field in FIELDS:
        value = f[field]; assert type(value) is int, field
        if field == 'tick_spacing_index_reference':
            assert -443636 // spacing <= value <= 443636 // spacing
        else:
            assert 0 <= value <= 2**32-1, field
    assert 1 <= f['filter_period'] < f['decay_period'] <= 65535
    assert 1 <= f['reduction_factor'] <= 9999 and 1 <= f['dynamic_fee_control'] <= 99999
    assert f['max_volatility_accumulator'] * spacing <= 2**32-1
    assert f['volatility_reference'] <= f['max_volatility_accumulator']
    assert f['volatility_accumulator'] <= f['max_volatility_accumulator']
    assert type(s.get('seed_index',0)) is int and 0 <= s.get('seed_index',0) <= 65535
    assert s['open_time'] < s['bank_unix_timestamp'] <= 2**32-1
    assert 1 <= s['max_steps'] <= 8192
    return f


def spacing_admission_bound(s):
    a = s['tick_arrays']
    return abs(a[-1]['start_tick_index'] - a[0]['start_tick_index']) // s['tick_spacing'] + 60 + sum(len(x['ticks']) for x in a) + 2


def audit_result(p, amount, direction, result):
    f = audit_pool(p)
    counts = bitmap.audit_result(p, amount, direction, result)
    if result.get('reverted'):
        return counts
    q = result['next_pool']; g = audit_pool(q)
    s, t = p['raydium_clmm'], q['raydium_clmm']
    mutable = {'sqrt_price_x64','tick_current','liquidity','fee_growth_global0','fee_growth_global1',
               'protocol_fees0','protocol_fees1','fund_fees0','fund_fees1','recent_epoch','raw_vault0','raw_vault1',
               'tick_arrays','observation_initialized','observation_index','observations','dynamic_fee'}
    assert {k:v for k,v in s.items() if k not in mutable} == {k:v for k,v in t.items() if k not in mutable}
    assert all(f[k] == g[k] for k in PARAMETERS)
    # Audit storage/reference semantics, not a replacement implementation of swap math.
    elapsed = max(0, s['bank_unix_timestamp'] - f['last_update_timestamp'])
    if elapsed < f['filter_period']:
        for field in ['tick_spacing_index_reference','volatility_reference','last_update_timestamp']:
            assert f[field] == g[field], field
    else:
        assert g['last_update_timestamp'] == s['bank_unix_timestamp']
        assert g['tick_spacing_index_reference'] == s['tick_current'] // s['tick_spacing']
        assert g['volatility_reference'] == (f['volatility_accumulator'] * f['reduction_factor'] // 10000 if elapsed < f['decay_period'] else 0)
    assert len(s['tick_arrays']) == len(t['tick_arrays'])
    for a,b in zip(s['tick_arrays'], t['tick_arrays']):
        assert a['address'] == b['address'] and a['start_tick_index'] == b['start_tick_index']
        assert [(x['tick'],x['liquidity_gross'],x['liquidity_net']) for x in a['ticks']] == [(x['tick'],x['liquidity_gross'],x['liquidity_net']) for x in b['ticks']]
    counts['dynamic_full_pool_results'] += 1
    counts['accumulator_cap_results'] += int(g['volatility_accumulator'] == g['max_volatility_accumulator'])
    counts['filter_held_results' if elapsed < f['filter_period'] else 'decay_results' if elapsed < f['decay_period'] else 'reset_results'] += 1
    return counts


def audit_time_probe(probe):
    p, later = probe['pool'], probe['later_pool']
    s, t = p['raydium_clmm'], later['raydium_clmm']
    assert t['bank_unix_timestamp'] == s['bank_unix_timestamp'] + 3600
    original = copy.deepcopy(p); original['raydium_clmm']['bank_unix_timestamp'] += 3600
    assert original == later
    for x in (s,t):
        f = x['dynamic_fee']
        assert f['last_update_timestamp'] == 0
        assert x['bank_unix_timestamp'] - f['last_update_timestamp'] >= f['decay_period']
    audit_result(p,probe['input'],probe['zero_for_one'],probe['expected'])
    audit_result(later,probe['input'],probe['zero_for_one'],probe['later_expected'])
    assert probe['expected']['amount_out'] == probe['later_expected']['amount_out']
    before, after = copy.deepcopy(probe['expected']['next_pool']), probe['later_expected']['next_pool']
    assert after['raydium_clmm']['dynamic_fee']['last_update_timestamp'] - before['raydium_clmm']['dynamic_fee']['last_update_timestamp'] == 3600
    # Observation tick cumulatives must remain identical; only written timestamps differ.
    a, b = before['raydium_clmm']['observations'], after['raydium_clmm']['observations']
    assert [x['tick_cumulative'] for x in a] == [x['tick_cumulative'] for x in b]
    for field in ['bank_unix_timestamp','observations']:
        before['raydium_clmm'][field] = after['raydium_clmm'][field]
    before['raydium_clmm']['dynamic_fee']['last_update_timestamp'] = after['raydium_clmm']['dynamic_fee']['last_update_timestamp']
    assert before == after


def audit(corpus):
    audit_execution_flags(corpus['reference'])
    assert corpus['reference']['business_math_imported'] is False
    assert len(corpus['cases']) >= 300 and len(corpus['sequences']) >= 8
    assert len({c['name'] for c in corpus['cases']}) == len(corpus['cases'])
    counts, cells = Counter(), set()
    names = [c['name'] for c in corpus['cases']]
    for c in corpus['cases']:
        assert set(c) == {'name','pool','input','zero_for_one','expected'}
        s = c['pool']['raydium_clmm']
        assert spacing_admission_bound(s) <= s['max_steps'], 'admission-only probe in mathematical cases'
        counts.update(audit_result(c['pool'],c['input'],c['zero_for_one'],c['expected']))
        counts['cases'] += 1
        counts['reverted_cases' if c['expected'].get('reverted') else 'successful_cases'] += 1
        if not c['expected'].get('reverted'):
            cells.add((s.get('seed_index',0),s.get('collect_fee_on',0),c['zero_for_one']))
    assert cells == {(seed,mode,d) for seed in [0,1,255,256,65535] for mode in [0,1,2] for d in [False,True]}
    for label in ['time-filter-decay-0-','time-filter-decay-9-','time-filter-decay-10-','time-filter-decay-11-',
                  'time-filter-decay-119-','time-filter-decay-120-','time-filter-decay-121-','future-last-timestamp',
                  'spacing1000-','spacing3-','ceil-first-unit','ceil-control1-','maxacc0-','maxacc4294967295-',
                  'groupcross-index1024','groupcross-index-1024','zeroL','growth-wrap','raw-input-overflow',
                  'raw-output-underflow','protocol-counter-overflow','fund-counter-overflow','u64-max-partial',
                  'u128-max','observation-wrap','selected-missing-proof-array','partial-window-exhaustion','basefee-cap']:
        assert any(label in name for name in names), label
    shapes = set()
    for seq in corpus['sequences']:
        p = seq['pool']; directions = []
        for step in seq['steps']:
            assert not step['expected'].get('reverted')
            counts.update(audit_result(p,step['input'],step['zero_for_one'],step['expected']))
            p = step['expected']['next_pool']; directions.append(step['zero_for_one']); counts['sequence_steps'] += 1
        assert p == seq['expected_final_pool']
        shapes.add((p['raydium_clmm'].get('collect_fee_on',0),tuple(directions))); counts['sequences'] += 1
    assert shapes == {(m,ds) for m in [0,1,2] for ds in [(False,False),(True,True),(True,False),(False,True)]}
    assert len(corpus['admission_probes']) >= 2
    for probe in corpus['admission_probes']:
        s = probe['pool']['raydium_clmm']
        assert probe['expected'] == {'reverted':True}
        assert 'dynamic spacing admission' in probe['native_reason']
        assert spacing_admission_bound(s) > s['max_steps']
        assert not probe['relaxed_expected'].get('reverted')
        audit_result(probe['relaxed_pool'],probe['input'],probe['zero_for_one'],probe['relaxed_expected'])
        counts['admission_probes'] += 1
    assert len(corpus['time_invariance_probes']) == 8
    for probe in corpus['time_invariance_probes']:
        audit_time_probe(probe); counts['time_invariance_probes'] += 1
    for field in ['input_fee_results','output_fee_results','cross_array_results','cross_base_extension_results',
                  'cross_bitmap_group_results','accumulator_cap_results','filter_held_results','decay_results','reset_results',
                  'growth_wrap_results','observation_ring_updates']:
        assert counts[field] > 0, field
    counts['seed_fee_direction_cells'] = len(cells)
    return dict(sorted(counts.items()))


def raw(f, address):
    a = f['rpc_accounts'][address]
    data = base64.b64decode(a['data_base64'],validate=True)
    assert a['data'] == [a['data_base64'],'base64'] and a['space'] == len(data)
    return data


def replace_raw(f, address, data):
    a = f['rpc_accounts'][address]; text = base64.b64encode(data).decode()
    a['data_base64'] = text; a['data'] = [text,'base64']; a['space'] = len(data)
    for i,key in enumerate(f['rpc_request_accounts']):
        if key == address: f['rpc_response']['result']['value'][i] = copy.deepcopy(a)


def audit_raw(f):
    p = f['pool']; audit_pool(p); s = p['raydium_clmm']; fee = s['dynamic_fee']
    assert f['pool_config']['protocol'] == PROFILE and f['pool_config']['id'] == p['pool_id']
    assert len(f['rpc_request_accounts']) == 19 and len(set(f['rpc_request_accounts'])) == 19
    assert f['pool_config']['accounts'] == f['rpc_request_accounts'][:7]
    assert set(f['rpc_request_accounts']) == set(f['rpc_accounts'])
    assert f['rpc_response']['result']['value'] == [f['rpc_accounts'][a] for a in f['rpc_request_accounts']]
    data = raw(f,p['pool_id'])
    assert len(data) == 1544
    assert data[391:393] == struct.pack('<H',s.get('seed_index',0)) and data[393:397] == bytes(4)
    assert data[1096:1130] == struct.pack('<HHHIIiIIQ',*(fee[k] for k in FIELDS))
    assert data[1130:1544] == bytes(414)
    clock = raw(f,CLOCK)
    assert len(clock) == 40 and struct.unpack_from('<Q',clock)[0] == s['bank_slot']
    assert struct.unpack_from('<Q',clock,16)[0] == s['bank_epoch']
    assert struct.unpack_from('<q',clock,32)[0] == s['bank_unix_timestamp']
    assert f['rpc_response']['result']['context']['slot'] == s['bank_slot']
    assert len(raw(f,s['bitmap_extension']['address'])) == 1832
    assert fee['last_update_timestamp'] == 0
    assert s['bank_unix_timestamp'] - fee['last_update_timestamp'] >= fee['decay_period']


def audit_fixture(fixture, corpus):
    audit_raw(fixture)
    next_fixture = fixture['metadata']['next_bank_fixture']; audit_raw(next_fixture)
    assert fixture['pool'] == corpus['base_pool'] and next_fixture['pool'] == corpus['next_bank_pool']
    assert fixture['pool_config'] == next_fixture['pool_config']
    assert fixture['rpc_request_accounts'] == next_fixture['rpc_request_accounts']
    assert fixture['pool']['pool_id'] == next_fixture['pool']['pool_id']
    a,b = fixture['pool']['raydium_clmm'],next_fixture['pool']['raydium_clmm']
    assert a.get('seed_index',0) == b.get('seed_index',0) == 1
    for field in ['dynamic_fee_control','max_volatility_accumulator','volatility_reference','volatility_accumulator']:
        assert a['dynamic_fee'][field] != b['dynamic_fee'][field]
    cases = {c['name']:c for c in corpus['cases']}
    for key,p in [('base_vectors',fixture['pool']),('next_bank_vectors',next_fixture['pool'])]:
        vectors = fixture['metadata'][key]
        assert len(vectors) == 4
        assert {(v['input'],v['zero_for_one']) for v in vectors} == {(a,d) for a in ['1000000','2000000000'] for d in [False,True]}
        for v in vectors:
            c = cases[v['name']]
            assert c['pool'] == p and c['input'] == v['input'] and c['zero_for_one'] == v['zero_for_one']
            assert c['expected'] == v['expected'] and not v['expected'].get('reverted')
    changed = []
    for a,b in zip(fixture['metadata']['base_vectors'],fixture['metadata']['next_bank_vectors']):
        assert (a['input'],a['zero_for_one']) == (b['input'],b['zero_for_one'])
        if a['expected']['amount_out'] != b['expected']['amount_out']: changed.append((a['input'],a['zero_for_one']))
    assert ('2000000000',False) in changed and ('2000000000',True) in changed
    policy = fixture['metadata']['time_policy']
    assert policy['config_max_age_seconds'] == 180
    assert policy['base_last_update_timestamp'] == policy['next_bank_last_update_timestamp'] == 0
    assert policy['native_time_anchor'] == corpus['reference']['native_time_anchor']
    assert policy['native_time_invariance_probes'] == corpus['time_invariance_probes']
    return {'raw_fixture_variants':2,'raw_account_count_per_bank':19,'exact_fixture_vectors':8,'next_bank_changed_amount_vectors':len(changed)}


# Snapshotted before dynamic implementation: all old corpus, identities, tick/index,
# raw fixture copies and lockfiles are byte protected, reports are allowed to refresh.
PROTECTED = {
    "chain-parser-go/test/contracts/rayclmm-bitmap-frames.json": "1bbaa9fb048fade88b2a28e25d41ac17dd9c78b6a856643ef8da48601cde26b4",
    "chain-parser-go/test/contracts/rayclmm-bitmap-indices.json": "24bbd337f791f217e6b3476d2284acaebeb53de77f26baaf99556ba9dadaea29",
    "chain-parser-go/test/contracts/rayclmm-frames.json": "d7154fb8eba1ec78657eb6db3669ce417150c0de1f8e01754334755c77fb296b",
    "chain-parser-go/test/contracts/rayclmm-seed-frames.json": "ca00a5865acd2986fd76a9038e15047563aad6a2ad4b5ed04012f6af2279417b",
    "chain-parser-go/test/contracts/rayclmm-seed-identities.json": "2d028f3247eb8390fd71322066993c27434d687135cf87d7a7239876295a7dea",
    "chain-parser-go/test/contracts/rayclmm-state.json": "d1c7442133de4be5d027c7a9bccb26585671dfa7484b5ca4a4407c2fc288519d",
    "chain-parser-go/test/contracts/rayclmm-tickmath.json": "06632193a2fc8733494bbc8a6dfcdc56f267220f7289883fa56b18ada8c7b171",
    "chain-parser-go/test/contracts/rayclmm-token-fee-frames.json": "09d2bd2948d258c085199c2656784b6580681b34163745d98f562c481c0c7613",
    "chain-parser-rust/test/contracts/rayclmm-bitmap-frames.json": "1bbaa9fb048fade88b2a28e25d41ac17dd9c78b6a856643ef8da48601cde26b4",
    "chain-parser-rust/test/contracts/rayclmm-bitmap-indices.json": "24bbd337f791f217e6b3476d2284acaebeb53de77f26baaf99556ba9dadaea29",
    "chain-parser-rust/test/contracts/rayclmm-frames.json": "d7154fb8eba1ec78657eb6db3669ce417150c0de1f8e01754334755c77fb296b",
    "chain-parser-rust/test/contracts/rayclmm-seed-frames.json": "ca00a5865acd2986fd76a9038e15047563aad6a2ad4b5ed04012f6af2279417b",
    "chain-parser-rust/test/contracts/rayclmm-seed-identities.json": "2d028f3247eb8390fd71322066993c27434d687135cf87d7a7239876295a7dea",
    "chain-parser-rust/test/contracts/rayclmm-state.json": "d1c7442133de4be5d027c7a9bccb26585671dfa7484b5ca4a4407c2fc288519d",
    "chain-parser-rust/test/contracts/rayclmm-tickmath.json": "06632193a2fc8733494bbc8a6dfcdc56f267220f7289883fa56b18ada8c7b171",
    "chain-parser-rust/test/contracts/rayclmm-token-fee-frames.json": "09d2bd2948d258c085199c2656784b6580681b34163745d98f562c481c0c7613",
    "chain-parser-rust/test/fixtures/solana/rayclmm-base.json": "f7d198d2398f19f5bbe52d3e2c85615d66c63dfa18aec326bf3da4f8127769a7",
    "chain-parser-rust/test/fixtures/solana/rayclmm-bitmap-base.json": "9ffc9ca1874243819b296dbd968e9407ecd1fd685c807606eca555628d5c164e",
    "chain-parser-rust/test/fixtures/solana/rayclmm-seed-base.json": "83af05c872cf1a9e08888c01ba3144df6d3434c5d3e79bc9bbd198f35eabd78e",
    "dex-router/test/contracts/rayclmm-bitmap-frames.json": "1bbaa9fb048fade88b2a28e25d41ac17dd9c78b6a856643ef8da48601cde26b4",
    "dex-router/test/contracts/rayclmm-bitmap-indices.json": "24bbd337f791f217e6b3476d2284acaebeb53de77f26baaf99556ba9dadaea29",
    "dex-router/test/contracts/rayclmm-bitmap-quotes.json": "350d4d447af4e07acb22786480cd2aa69ddcb85073a0ecfd80427079f20a851a",
    "dex-router/test/contracts/rayclmm-frames.json": "d7154fb8eba1ec78657eb6db3669ce417150c0de1f8e01754334755c77fb296b",
    "dex-router/test/contracts/rayclmm-quotes.json": "ef1a0be7c80d6f7f5e815b75a10223a9a7be225d279bc74e057664b5af4f6f31",
    "dex-router/test/contracts/rayclmm-seed-frames.json": "ca00a5865acd2986fd76a9038e15047563aad6a2ad4b5ed04012f6af2279417b",
    "dex-router/test/contracts/rayclmm-seed-identities.json": "2d028f3247eb8390fd71322066993c27434d687135cf87d7a7239876295a7dea",
    "dex-router/test/contracts/rayclmm-seed-quotes.json": "803db96e7f599bfd95851862a6c95d3a8d03f2601a76f6f610e6d6b43c68476b",
    "dex-router/test/contracts/rayclmm-state.json": "d1c7442133de4be5d027c7a9bccb26585671dfa7484b5ca4a4407c2fc288519d",
    "dex-router/test/contracts/rayclmm-tickmath.json": "06632193a2fc8733494bbc8a6dfcdc56f267220f7289883fa56b18ada8c7b171",
    "dex-router/test/contracts/rayclmm-token-fee-frames.json": "09d2bd2948d258c085199c2656784b6580681b34163745d98f562c481c0c7613",
    "dex-router/test/contracts/rayclmm-token-fee-quotes.json": "6d0f2da485cbc86edc67e3f19343e97245f1422556c0932e94b9efc5931b74b6",
    "integration/test/contracts/rayclmm-bitmap-frames.json": "1bbaa9fb048fade88b2a28e25d41ac17dd9c78b6a856643ef8da48601cde26b4",
    "integration/test/contracts/rayclmm-bitmap-indices.json": "24bbd337f791f217e6b3476d2284acaebeb53de77f26baaf99556ba9dadaea29",
    "integration/test/contracts/rayclmm-bitmap-quotes.json": "350d4d447af4e07acb22786480cd2aa69ddcb85073a0ecfd80427079f20a851a",
    "integration/test/contracts/rayclmm-frames.json": "d7154fb8eba1ec78657eb6db3669ce417150c0de1f8e01754334755c77fb296b",
    "integration/test/contracts/rayclmm-quotes.json": "ef1a0be7c80d6f7f5e815b75a10223a9a7be225d279bc74e057664b5af4f6f31",
    "integration/test/contracts/rayclmm-seed-frames.json": "ca00a5865acd2986fd76a9038e15047563aad6a2ad4b5ed04012f6af2279417b",
    "integration/test/contracts/rayclmm-seed-identities.json": "2d028f3247eb8390fd71322066993c27434d687135cf87d7a7239876295a7dea",
    "integration/test/contracts/rayclmm-seed-quotes.json": "803db96e7f599bfd95851862a6c95d3a8d03f2601a76f6f610e6d6b43c68476b",
    "integration/test/contracts/rayclmm-state.json": "d1c7442133de4be5d027c7a9bccb26585671dfa7484b5ca4a4407c2fc288519d",
    "integration/test/contracts/rayclmm-tickmath.json": "06632193a2fc8733494bbc8a6dfcdc56f267220f7289883fa56b18ada8c7b171",
    "integration/test/contracts/rayclmm-token-fee-frames.json": "09d2bd2948d258c085199c2656784b6580681b34163745d98f562c481c0c7613",
    "integration/test/contracts/rayclmm-token-fee-quotes.json": "6d0f2da485cbc86edc67e3f19343e97245f1422556c0932e94b9efc5931b74b6",
    "integration/test/solana/rayclmm-base.json": "f7d198d2398f19f5bbe52d3e2c85615d66c63dfa18aec326bf3da4f8127769a7",
    "integration/test/solana/rayclmm-bitmap-base.json": "9ffc9ca1874243819b296dbd968e9407ecd1fd685c807606eca555628d5c164e",
    "integration/test/solana/rayclmm-seed-base.json": "83af05c872cf1a9e08888c01ba3144df6d3434c5d3e79bc9bbd198f35eabd78e",
    "integration/tools/reference/raydium-clmm/Cargo.lock": "a33eacfecdbc020fc81b773345f62ce7e24eaae4a264cfa1ebab7cdbab66e9e2",
    "integration/tools/reference/raydium-clmm/base-fixture.json": "f7d198d2398f19f5bbe52d3e2c85615d66c63dfa18aec326bf3da4f8127769a7",
    "integration/tools/reference/raydium-clmm/bitmap-base-fixture.json": "9ffc9ca1874243819b296dbd968e9407ecd1fd685c807606eca555628d5c164e",
    "integration/tools/reference/raydium-clmm/seed-base-fixture.json": "83af05c872cf1a9e08888c01ba3144df6d3434c5d3e79bc9bbd198f35eabd78e",
    "integration/tools/reference/raydium-clmm/source-metadata.json": "cd2277e3af6a5a91cb3a6ae76141ce80ac3f222f6fe7ca9c6e452dd55078996c",
    "integration/tools/reference/raydium-clmm/tick-vectors.json": "06632193a2fc8733494bbc8a6dfcdc56f267220f7289883fa56b18ada8c7b171",
    "integration/tools/reference/raydium-clmm/upstream/Cargo.lock": "9370048a70726a60e281003ff798d425ce81b84ce42343d3ffdeb91def2a7d8a"
}


def protected_outputs():
    for name,digest in PROTECTED.items():
        assert sha((ROOT/name).read_bytes()) == digest, 'protected old bytes changed: ' + name
    return dict(PROTECTED)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--offline',action='store_true')
    parser.add_argument('--check',action='store_true')
    parser.add_argument('--report-only',action='store_true')
    parser.add_argument('--timestamp',type=int,default=ANCHOR,help='Explicit held native Clock unix snapshot; never silently replaced by wall clock')
    args = parser.parse_args()
    assert 120 < args.timestamp <= 2**32-3602
    sources = load('rayclmm-reference-fetch').verify()
    assert sum(len(a['files']) for a in sources['archives']) == 225
    dependencies = load('rayclmm-token-fee-golden').source_checks()
    preserved = protected_outputs()
    toolchain = Path.home()/'.rustup/toolchains/1.90.0-x86_64-apple-darwin/bin'
    cargo,rustc,rustdoc = [str(toolchain/n) for n in ['cargo','rustc','rustdoc']]
    env = dict(os.environ,RUSTC=rustc,RUSTDOC=rustdoc,CARGO_TARGET_DIR=str(RUNTIME),PYTHONDONTWRITEBYTECODE='1')
    commands, transcript = [], []
    def run(cmd,payload=False,data=None):
        commands.append(cmd)
        r = subprocess.run(cmd,cwd=REFERENCE,env=env,input=data,stdout=subprocess.PIPE,stderr=subprocess.PIPE)
        transcript.append('$ '+' '.join(cmd)+'\n'+r.stderr.decode()+('stdout SHA-256: '+sha(r.stdout)+'\n' if payload else r.stdout.decode()))
        if r.returncode:
            print(transcript[-1],file=sys.stderr); raise SystemExit(r.returncode)
        return r.stdout
    compiler = run([rustc,'-Vv']).decode().strip(); assert compiler.startswith('rustc 1.90.0 ')
    flags = ['--locked','--config','./cargo-source.toml']+(['--offline'] if args.offline else [])
    test_output = run([cargo,'test',*flags]); run([cargo,'build',*flags])
    native_test_count = sum(int(n) for n in re.findall(rb'test result: ok\. (\d+) passed;',test_output))
    assert native_test_count >= 31, native_test_count
    binary = str(RUNTIME/'debug/raydium-clmm-reference')
    data = {}
    for command in ['generate-dynamic','dynamic-fixture']:
        cmd = [binary,command,'--timestamp',str(args.timestamp)]
        data[command] = run(cmd,True)
        assert data[command] == run(cmd,True), 'nondeterministic '+command
    corpus,fixture = [json.loads(data[c]) for c in ['generate-dynamic','dynamic-fixture']]
    counts = audit(corpus); counts.update(audit_fixture(fixture,corpus)); counts['native_tests'] = native_test_count
    # Replay every math case, each admission failure/control and both times of every
    # metamorphic probe through the same public JSON CLI used by differential callers.
    requests = [(c,c['expected']) for c in corpus['cases']]
    for probe in corpus['admission_probes']:
        requests += [(probe,probe['expected']),({**probe,'pool':probe['relaxed_pool']},probe['relaxed_expected'])]
    for probe in corpus['time_invariance_probes']:
        requests += [(probe,probe['expected']),({**probe,'pool':probe['later_pool']},probe['later_expected'])]
    for request,expected in requests:
        actual = json.loads(run([binary,'quote'],True,json.dumps({k:request[k] for k in ['pool','input','zero_for_one']}).encode()))
        if expected.get('reverted'):
            assert actual.get('reverted') is True
            if 'reason_contains' in request: assert request['reason_contains'] in actual['reason']
        else: assert actual == expected, request.get('name')
    counts['json_cli_replays'] = len(requests)
    spec = importlib.util.spec_from_file_location('dynamic_audit_tests',REFERENCE/'tests/dynamic_runner.py')
    test_module = importlib.util.module_from_spec(spec); spec.loader.exec_module(test_module)
    test_count = test_module.run_tests(sys.modules[__name__],corpus,fixture)
    # Verify all prior command outputs from the current binary, not only saved files.
    old = {'generate':'integration/test/contracts/rayclmm-quotes.json',
           'generate-token-fee':'integration/test/contracts/rayclmm-token-fee-quotes.json',
           'generate-bitmap':'integration/test/contracts/rayclmm-bitmap-quotes.json',
           'generate-seed':'integration/test/contracts/rayclmm-seed-quotes.json',
           'fixture':'integration/test/solana/rayclmm-base.json',
           'bitmap-fixture':'integration/test/solana/rayclmm-bitmap-base.json',
           'seed-fixture':'integration/test/solana/rayclmm-seed-base.json',
           'seed-identities':'integration/test/contracts/rayclmm-seed-identities.json',
           'tick-vectors':'integration/test/contracts/rayclmm-tickmath.json',
           'bitmap-vectors':'integration/test/contracts/rayclmm-bitmap-indices.json'}
    for cmd,name in old.items(): assert sha(run([binary,cmd],True)) == preserved[name], 'old native output changed: '+cmd
    prior = {}
    for name in ['rayclmm-golden-generation.json','rayclmm-token-fee-golden-generation.json','rayclmm-bitmap-golden-generation.json','rayclmm-seed-native-source.json']:
        path = ARTIFACTS/name; report = json.loads(path.read_bytes())
        for f,digest in report['harness_files'].items():
            assert sha((ROOT/f).read_bytes()) == digest, 'refresh ordered static/token-fee/bitmap/seed reports: '+f
        prior[str(path.relative_to(ROOT))] = sha(path.read_bytes())
    outputs = [(ROOT/project/'test/contracts/rayclmm-dynamic-quotes.json',data['generate-dynamic']) for project in ['integration','dex-router']]
    outputs += [(p,data['dynamic-fixture']) for p in [ROOT/'integration/test/solana/rayclmm-dynamic-base.json',ROOT/'chain-parser-rust/test/fixtures/solana/rayclmm-dynamic-base.json',REFERENCE/'dynamic-base-fixture.json']]
    harness = sorted(set(list((REFERENCE/'src').glob('*.rs'))+list((REFERENCE/'tests').glob('*.rs'))+list((REFERENCE/'tests').glob('*.py'))+[REFERENCE/'README.md',REFERENCE/'Cargo.toml',REFERENCE/'cargo-source.toml',Path(__file__).resolve()]))
    report = {'schema_version':1,'revision':sources['revision'],'profile':PROFILE,'adapter_version':ADAPTER,
              'unchanged_source_files_verified':225,'source_adaptations':[],'compiler':compiler,
              'archives':{a['archive']:a['archive_sha256'] for a in sources['archives']},
              'cargo_lock_sha256':sha((REFERENCE/'Cargo.lock').read_bytes()),'original_cargo_lock_sha256':sha((REFERENCE/'upstream/Cargo.lock').read_bytes()),
              'source_manifest_sha256':sha((REFERENCE/'source-metadata.json').read_bytes()),'dependencies':dependencies,
              'harness_files':{str(p.relative_to(ROOT)):sha(p.read_bytes()) for p in harness},'prior_report_hashes':prior,
              'counts':counts,'golden_sha256':sha(data['generate-dynamic']),'fixture_sha256':sha(data['dynamic-fixture']),
              'outputs':{str(p.relative_to(ROOT)):sha(b) for p,b in outputs},'preserved_hashes':preserved,
              'deterministic_regeneration':True,'python_audit_mutation_tests':test_count,
              'native_swap_internal_executed':True,'native_swap_state_dynamic_fee_executed':True,'native_dynamic_fee_info_executed':True,
              'native_extension_account_supplied':True,'native_clock_syscall_executed':True,'full_pool_poststates':True,
              'business_math_imported':False,**execution_evidence(),
              'dynamic_raw_account_offset':[1096,1176],'dynamic_field_data_offset':[1096,1130],'zero_padding_offset':[1130,1544],
              'time_policy':fixture['metadata']['time_policy'],
              'budget_probe_controls':[{k:p[k] for k in ['name','input','zero_for_one','native_reason','domain']} for p in corpus['admission_probes']],
              'source_semantics':{'dynamic_fee_info':'upstream/programs/amm/src/states/pool_fee.rs: packed 80 bytes; validate_params, saturating reference/filter/decay update and capped volatility accumulator',
                                  'swap_state':'upstream/programs/amm/src/instructions/swap.rs: SwapState::new, get_spacing_bounded_price, update_dynamic_fee_index, get_total_fee_rate, compute_dynamic_fee_rate and swap_internal execute unchanged',
                                  'pool_storage':'upstream/programs/amm/src/states/pool.rs: original packed DynamicFeeInfo at 1096..1176; fee/seed/padding remain their original layout'},
              'limits':['Pinned source audit sample; synthetic program proofs are not deployed-bytecode identity.',
                        'Native RefMut/Clock/core executes. Anchor instruction/account validation, SBF bank, CPI, token program transfers, signatures, executable/owner checks, user balances and WSOL backing are not executed.',
                        'Dynamic classic SPL; collect_fee_on 0/1/2, seed0..65535, mandatory complete extension; no orders, rewards or Token2022; u32 bank time; full exact-input only.',
                        'Dynamic admission counts full proof-window spacing intervals, initialized ticks and final steps. Low-budget probes are separately labeled admission_probes and excluded from mathematical equality cases; no runtime budget meter is claimed.',
                        'Boundary overflow states can be unreachable from the base. Source errors/panics and partial fills reject without committing mutations.',
                        'Raw banks preserve pool identity and account window, and change actual dynamic control/max accumulator/reference inputs. Both reset from last_update_timestamp=0 beyond decay.',
                        'Held native timestamp is explicit. Actual current process Clock is permitted only beyond decay; native T/T+3600 probes establish exact amount invariance. Full poststate expectations retain their actual native bank timestamps.',
                        'No services, infrastructure or main Cargo builds run. Main worker owns process acceptance.'],
              'commands':commands,'environment':{'RUSTC':rustc,'RUSTDOC':rustdoc,'CARGO_TARGET_DIR':str(RUNTIME)}}
    audit_execution_flags(report)
    if args.check:
        for path,b in outputs: assert path.read_bytes() == b, 'stale output: '+str(path)
        saved = json.loads(REPORT.read_bytes()); audit_execution_flags(saved)
        for field in report:
            if field != 'commands': assert saved[field] == report[field], 'stale dynamic report: '+field
    else:
        for path,b in outputs:
            if args.report_only: assert path.read_bytes() == b, 'report-only would change output: '+str(path)
            else: path.parent.mkdir(parents=True,exist_ok=True); path.write_bytes(b)
        REPORT.write_text(json.dumps(report,indent=2)+'\n')
        (ARTIFACTS/'rayclmm-dynamic-reference-build.log').write_text('\n'.join(transcript)+'\n')
    protected_outputs()
    print(json.dumps({'verified':True,'check_only':args.check,**counts,'golden_sha256':report['golden_sha256'],'fixture_sha256':report['fixture_sha256']},indent=2))


if __name__ == '__main__':
    main()
