#!/usr/bin/env python3
"""Replay normal Rust/PG/Kafka/dual-Router ExactOut API plans in the original SBF."""
import copy
import hashlib
import json
import secrets
import subprocess
import tempfile
import time
from pathlib import Path

from meteora_exact_out_reference import ORIGINAL, PROJECTED, original
from meteora_mm_smoke import Chain, FAULTS, ROOT, OUT, digest, helper_hashes
from infrastructure import Infrastructure, port
from configuration import config_for, save_config, service_command, sample_for
from kafka_smoke import wait
from reorg_smoke import await_condition
from smoke import http, quote, wait_scope
from weighted_evidence import source_hash, write_result

TARGET = OUT / 'core-meteora-exact-out-normal-process.json'
CLOCK = 'SysvarC1ock11111111111111111111111111111111'
PROJECTOR = ROOT / 'dex-router/target/debug/examples/meteora-exact-out-plan-projector'
CHAIN = 'solana:localnet'


def require(ok, reason):
    if not ok:
        raise ValueError(reason)


def bank_cases(evidence):
    mm = json.loads((ROOT / 'integration/test/contracts/meteora-mm.json').read_bytes())['decoder']
    ids = set(mm['config']['accounts'])
    clock = mm['cases'][0]['accounts'][CLOCK]
    return {'decoder': dict(config=mm['config'], shared_accounts=mm['shared_accounts'], cases=[
        {'accounts': {CLOCK: clock, **{k: dict(owner=a['owner'], executable=a['executable'],
                                              lamports=a['lamports'], data=[a['data_base64'], 'base64'])
                                      for k, a in e['native']['before'].items() if k in ids}}}
        for e in evidence['records']])}


def run():
    reference = json.loads(PROJECTED.read_bytes())
    evidence = json.loads(ORIGINAL.read_bytes())
    chain = Chain(bank_cases(evidence))
    # Exercise both directions, original rounding dust, liquidity/Oracle boundaries and fee policy.
    cases = [0, 2, 6, 7, 8, 19, 20, 44, 80, 120, 128, 133, 138, 140, 142, 149, 151, 154, 160, 162, 163]
    require(all(reference['cases'][i]['steps'][0]['passed'] for i in cases), 'PROCESS_CASE_SELECTION')
    chain.index = cases[0]
    processes, logs, records = {}, {}, []
    source, tag = source_hash(), secrets.token_hex(6)
    names = dict(go='chain-parser-go', rust='chain-parser-rust', router1='dex-router', router2='dex-router')
    inputs = {str(p.relative_to(ROOT)): digest(p) for p in [PROJECTED, ORIGINAL, ROOT / 'integration/test/contracts/meteora-mm.json']}
    helpers = dict(helper_hashes(), **{str(p.relative_to(ROOT)): digest(p) for p in [Path(__file__), Path(__file__).with_name('meteora_mm_smoke.py'), Path(__file__).with_name('meteora_exact_out_reference.py'), Path(original.__file__)]})
    binaries = {path: digest(ROOT / path) for path in ['chain-parser-go/bin/fixture-parser-go', 'chain-parser-rust/target/debug/chain-parser-rust', 'dex-router/target/debug/dex-router', str(PROJECTOR.relative_to(ROOT))]}
    report = dict(schema_version=1, passed=False, source_tree_sha256=source, inputs=inputs, helpers=helpers, binaries=binaries,
                  authority='CONTROLLED_RETAINED_SBF_WITH_SYNTHETIC_FINALIZED_RPC_AND_ACTUAL_PG_KAFKA', checks=[], production_ready=False, all_requested_features_complete=False)
    write_result(TARGET, report)
    servers = [chain.provider(), chain.provider()]
    try:
        with Infrastructure() as infra, tempfile.TemporaryDirectory(prefix='xone-meteora-xo-') as folder:
            root = Path(folder)
            token = secrets.token_urlsafe(32)
            ports = {alias: port() for alias in names}
            configs = {}

            def base(alias):
                return f'http://127.0.0.1:{ports[alias]}'

            for alias, name in names.items():
                c = config_for(name, root / alias, token, ports[alias])
                c['history']['enabled'] = False
                c['journal'].update(transport='kafka', source_urls=[base('go'), base('rust')], poll_interval_ms=150, request_timeout_ms=15000)
                c['journal']['kafka'].update(brokers=[infra.broker], topics={'evm:31337': 'canonical.evm-31337.v1', CHAIN: 'canonical.solana-localnet.v1'}, replica_id=alias, transaction_timeout_ms=4000)
                c['control'].update(backend='disabled' if name == 'dex-router' else 'postgres', dsn_file='' if name == 'dex-router' else str(infra.dsnfile), instance_id=alias, lease_ttl_ms=30000, renewal_ms=1000, min_commit_remaining_ms=7000)
                c['startup']['max_baseline_age_ms'] = 315360000000
                if alias == 'rust':
                    c['chains'][0].update(genesis_hash='localnet', confirmation_tag='finalized', rpc_urls=[f'http://127.0.0.1:{s.server_port}' for s in servers], pools=[copy.deepcopy(chain.f['decoder']['config'])])
                elif alias == 'go':
                    sample_for('evm:31337', root / alias / 'fixtures', int(time.time() * 1000))
                else:
                    c['routing'].update(max_state_age_ms=315360000000, quote_ttl_ms=60000, max_splits=1, split_parts=4)
                    c['api']['quote_timeout_ms'] = 30000
                configs[alias] = save_config(root / alias / 'config', c)

            def start(alias):
                logs[alias] = (OUT / f'meteora-xo-{tag}-{alias}.log').open('ab')
                processes[alias] = subprocess.Popen(service_command(names[alias], configs[alias], root / alias / 'fixtures' if alias == 'go' else None), cwd=ROOT / names[alias], stdout=logs[alias], stderr=subprocess.STDOUT)

            def stop(alias):
                p = processes.pop(alias)
                p.terminate()
                try:
                    p.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    p.kill()
                    p.wait()
                logs.pop(alias).close()

            def status(alias):
                return http(base(alias) + f'/internal/chains/{CHAIN}/status', token)

            def current(alias):
                return status(alias)[1].get('snapshot_ref', {}).get('block', {}).get('number_or_slot') == chain.head

            def request():
                s = reference['cases'][chain.index]['steps'][0]
                n = evidence['records'][chain.index]['native']
                p, z = s['pool'], s['zero_for_one']
                return dict(chainKey=CHAIN, kind='EXACT_OUT', sellToken=p['token0' if z else 'token1'], buyToken=p['token1' if z else 'token0'], buyAmount=s['target'], maxSellAmount=str(original.BUDGET), caller=n['payer'], recipient=n['recipient'], slippageBps=50, verification='FAST')

            def check(alias, capture=True):
                req = request()
                code, q = http(base(alias) + '/v1/quote-exact-out', token, req)
                s = reference['cases'][chain.index]['steps'][0]
                require(code == 200, f'QUOTE_FAILED:{chain.index}:{q}')
                require(q['requiredSellAmount'] == s['required_input'] and q['grossBuyAmount'] == q['minBuyAmount'] == s['target'] and q['outputSurplus'] == q['refund'] == '0', 'EXACT_OUT_QUOTE_AMOUNTS')
                require(q['verification']['level'] == 'INDICATIVE' and q['netSellAmountEstimate'] is None, 'AUTHORITY_BOUNDARY')
                if capture:
                    code, e = http(base(alias) + '/internal/quotes/' + q['quoteId'] + '/evidence', token)
                    require(code == 200, 'PLAN_EVIDENCE')
                    code, snapshot = http(base('rust') + f'/internal/chains/{CHAIN}/baseline', token)
                    require(code == 200 and snapshot['reference'] == q['snapshotRef'], 'PLAN_SOURCE_BANK')
                    records.append(dict(case_index=chain.index, snapshot=snapshot, request=req, quote=q, execution_plan=e['execution_plan']))
                return q

            try:
                for alias in ['go', 'rust']:
                    start(alias)
                    wait(base(alias) + '/health/ready')
                for alias in ['router1', 'router2']:
                    start(alias)
                    wait(base(alias) + '/health/live')
                    wait_scope(base(alias), token, CHAIN)
                for index in cases:
                    chain.advance(index)
                    for alias in ['router1', 'router2']:
                        await_condition(lambda a=alias: current(a), f'Meteora case {index} not published')
                        check(alias)
                report['checks'].append('exact-credit-and-required-input-on-both-router-processes')
                chain.advance(8)
                for alias in ['router1', 'router2']:
                    await_condition(lambda a=alias: current(a), 'Meteora control scope not published')
                    req = request()
                    q = check(alias, False)
                    req['maxSellAmount'] = str(int(q['requiredSellAmount']) - 1)
                    require(http(base(alias) + '/v1/quote-exact-out', token, req)[0] != 200, 'API_UNDERFUNDED_ACCEPTED')
                report['checks'].append('minimum-input-minus-one-fails-at-api')
                for fault in FAULTS:
                    old = {a: check(a, False) for a in ['router1', 'router2']}
                    chain.advance(bad=fault)
                    for alias in old:
                        await_condition(lambda a=alias: status(a)[0] == 503, 'Meteora fault not revoked:' + fault)
                        require(http(base(alias) + '/v1/quote-exact-out', token, request())[0] != 200, 'STALE_EXACT_OUT_QUOTE')
                        quote(base(alias), token, 'evm:31337', 'B')
                    chain.advance()
                    for alias in old:
                        await_condition(lambda a=alias: current(a), 'Meteora fault not recovered:' + fault)
                        require(check(alias, False)['onlineGeneration'] > old[alias]['onlineGeneration'], 'RECOVERY_GENERATION')
                    report['checks'].append('fault-withdraw-and-recovery:' + fault)
                stop('router1')
                start('router1')
                wait(base('router1') + '/health/live')
                wait_scope(base('router1'), token, CHAIN)
                check('router1')
                report['checks'].append('router-checkpoint-tail-restart')
            finally:
                for alias in list(processes):
                    stop(alias)
    finally:
        for server in servers:
            server.shutdown()
            server.server_close()

    captured = OUT / f'meteora-xo-plans-{tag}.json'
    modeled = OUT / f'meteora-xo-projection-{tag}.json'
    native_path = OUT / f'meteora-xo-sbf-replay-{tag}.json'
    captured.write_text(json.dumps(dict(schema_version=1, records=records)) + '\n')
    subprocess.run([str(PROJECTOR), str(captured), str(modeled)], check=True)
    projected = json.loads(modeled.read_bytes())
    require(projected['passed'] and len(projected['records']) == len(records) == len(cases) * 2 + 1, 'PROCESS_REPLAY_COVERAGE')
    replay = []
    for record in projected['records']:
        require(len(record['steps']) == 1, 'SINGLE_ORIGINAL_TEMPLATE')
        step = record['steps'][0]
        case = evidence['records'][record['case_index']]
        zero, target, limit = step['zero_for_one'], int(step['amount_out']), int(record['max_sell_amount'])
        n = original.execute(case['mode'], zero, target, budget=limit)['native']
        bad = original.execute(case['mode'], zero, target, budget=int(step['amount_in']) - 1)['native']
        require(n['passed'] and n['payer_input_delta'] == step['amount_in'] and n['recipient_output_delta'] == step['amount_out'], 'ORIGINAL_PLAN_TRANSFERS')
        require(not bad['passed'] and bad['before'] == n['before'] == bad['after'], 'ORIGINAL_PLAN_HARD_LIMIT')
        for name in ['before', 'after']:
            expected = original.projection.pool(n[name], n['clock'], n['program_sha256'])['meteora']
            actual = step[name]['meteora']
            for field in ['pair_data', 'oracle_data', 'bin_arrays', 'raw_vault0', 'raw_vault1']:
                require(actual[field] == expected[field], f'ORIGINAL_LEDGER:{name}:{field}')
        replay.append(dict(case_index=record['case_index'], plan_hash=record['plan_hash'], native=n, input_minus_one=bad))
    native_path.write_text(json.dumps(dict(schema_version=1, records=replay), indent=2) + '\n')
    require(source_hash() == source and all(digest(ROOT / p) == h for p, h in (inputs | helpers | binaries).items()), 'VALIDATION_INPUT_CHANGED')
    artifacts = {name: dict(path=str(path.relative_to(ROOT)), sha256=digest(path)) for name, path in [('captured_plans', captured), ('model_projection', modeled), ('original_capture', native_path)]}
    report.update(passed=True, source_tree_sha256_after=source, api_records=len(records), original_sbf_swaps=len(replay), input_limit_controls=len(replay), source_faults=len(FAULTS), account_batches=chain.reads, block_reads=chain.blocks, artifacts=artifacts)
    write_result(TARGET, report)
    return report


if __name__ == '__main__':
    try:
        r = run()
        print(json.dumps({k: r[k] for k in ['passed', 'api_records', 'original_sbf_swaps', 'input_limit_controls', 'source_faults']}))
    except Exception as e:
        report = json.loads(TARGET.read_bytes()) if TARGET.exists() else {}
        report.update(passed=False, error=str(e), error_type=type(e).__name__)
        write_result(TARGET, report)
        raise
