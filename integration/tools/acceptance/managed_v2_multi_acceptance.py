#!/usr/bin/env python3
"""T34.8/.9 frozen local catalog/shared Vault acceptance, with closed evidence.

Run only after verify.py and an independent source review of this exact closure.
Unit tests import validators; importing this module never starts infrastructure.
"""
from collections import Counter
from pathlib import Path
import hashlib
import json
import os
import re
import subprocess
import time

import managed_v2_multi_smoke as multi
from weighted_evidence import BINARIES, checked_runtime, runtime_fingerprint, snapshot_inputs, write_result

ROOT = Path(__file__).resolve().parents[3]
OUT = ROOT / 'integration/artifacts/local'
TARGET = OUT / 'managed-v2-multi-acceptance.json'
REVIEW = OUT / 'managed-v2-multi-review.json'
SCOPE = 'implemented-local-managed-v2-current-catalog-and-shared-vault'
REVIEW_SCOPE = 'managed-v2-current-catalog-and-shared-vault-local-scope'
BANK_SHA256 = multi.projection.BANK_SHA256
ACTORS = ('router1', 'router2')
REMAINING = [
    'BPT as a routing underlying',
    'Rates and mixed-pool full capability combinations',
    'Collection propagation across the complete capability surface',
    'Independent real-chain trust and execution gas reference',
    'Production performance and operational readiness',
    'Public executor, VERIFIED and build admission',
    'The complete protocol/version/feature universe remains unimplemented',
]


def require(value, message):
    if not value:
        raise ValueError(message)


def obj(value, label):
    require(type(value) is dict, label + ' must be an object')
    return value


def rows(value, label):
    require(type(value) is list, label + ' must be an array')
    for row in value:
        obj(row, label + ' row')
    return value


def positive_int(value):
    return type(value) is int and value > 0


def digest(value):
    require(type(value) is str and re.fullmatch('[0-9a-f]{64}', value), 'missing/invalid SHA256')
    return value


def sha(data):
    return hashlib.sha256(data).hexdigest()


def canonical_sha(value):
    return sha(json.dumps(value, sort_keys=True, separators=(',', ':'), ensure_ascii=False).encode())


def amount(value, label, zero=False):
    require(type(value) is str and re.fullmatch('0|[1-9][0-9]*', value) is not None,
            'missing/invalid amount: ' + label)
    n = int(value)
    require((n >= 0 if zero else n > 0) and n < 2**256, 'amount out of range: ' + label)
    return n


def nonpromotion(value):
    if type(value) is dict:
        for key, item in value.items():
            if key in ('verified', 'production_ready', 'all_requested_features_complete', 'independent_gas_reference'):
                require(item is False, 'local evidence promoted: ' + key)
            nonpromotion(item)
    elif type(value) is list:
        for item in value:
            nonpromotion(item)


def runtime_shape(runtime):
    obj(runtime, 'runtime')
    digest(runtime.get('source_tree_sha256'))
    binaries = obj(runtime.get('binaries'), 'runtime binaries')
    require(set(binaries) == set(BINARIES), 'exact five binaries required')
    for value in binaries.values():
        digest(value)


def validate_verification(verification, runtime):
    runtime_shape(runtime)
    obj(verification, 'verification')
    require(verification.get('passed') is True and verification.get('source_unchanged_during_verification') is True
            and verification.get('verification_tool_unchanged') is True, 'full checked source verification required')
    require(verification.get('source_tree_sha256') == runtime['source_tree_sha256']
            and verification.get('checked_binary_sha256') == runtime['binaries'], 'verification runtime/five bins differ')
    commands = rows(verification.get('commands'), 'verification commands')
    require(commands and all(type(c.get('exit_code')) is int and c['exit_code'] == 0 for c in commands),
            'verification contains incomplete/failed commands')
    expected = [(p, ['make', 'check']) for p in ('chain-parser-go', 'chain-parser-rust', 'dex-router', 'common/protoc')]
    expected += [(p, ['make', 'debug-build']) for p in ('chain-parser-go', 'chain-parser-rust')]
    expected += [('integration', ['python3', 'tools/ci/' + tool]) for tool in ('contracts.py', 'boundary.py')]
    require(len(commands) == len(expected), 'full verify.py command inventory required')
    for project, command in expected:
        require(sum(c.get('project') == project and c.get('command') == command for c in commands) == 1,
                'missing/duplicated verification command: ' + project + ' ' + ' '.join(command))


def validate_review(review, runtime, inputs, verification_sha256):
    runtime_shape(runtime)
    digest(verification_sha256)
    obj(inputs, 'inputs')
    require(inputs and all(digest(v) for v in inputs.values()), 'empty input closure')
    obj(review, 'review')
    require(review.get('passed') is True and review.get('authority') == 'independent-agent-source-review'
            and review.get('scope') == REVIEW_SCOPE, 'independent multi-scope source review required')
    require(review.get('runtime') == runtime and review.get('input_hashes') == inputs
            and review.get('verification_sha256') == verification_sha256, 'review does not bind this exact closure')
    reviewers = set()
    for name in ('spec', 'quality'):
        row = obj(review.get(name), name + ' review')
        require(row.get('passed') is True and row.get('unresolved') == [], 'unresolved/failed review: ' + name)
        ident = row.get('reviewer')
        require(type(ident) is str and ident.strip() == ident and bool(ident) and ident not in reviewers,
                'distinct identified spec/quality reviewers required')
        reviewers.add(ident)
    nonpromotion(review)


def hash_block(header):
    return dict(chain_key='evm:31337', number_or_slot=int(header['number'], 16), hash=header['hash'],
                parent_hash=header['parentHash'], timestamp_ms=int(header['timestamp'], 16)*1000)


def validate_reference(reference, header):
    reference = obj(reference, 'source reference')
    require(reference.get('block') == hash_block(header), 'source reference does not bind complete native H')
    for key in ('canonical_epoch', 'journal_seq', 'schema_version', 'online_generation', 'writer_epoch', 'owner_assignment_version'):
        require(positive_int(reference.get(key)), 'missing positive reference field: ' + key)
    require(reference.get('schema_version') == 1 and reference.get('owner_service') == 'chain-parser-go', 'source owner/schema differs')
    digest(reference.get('scope_hash'))
    digest(reference.get('adapter_set_hash'))
    return reference


def indicative(quote):
    q = obj(quote, 'actual API quote')
    v = obj(q.get('verification'), 'quote verification')
    require(v.get('level') == 'INDICATIVE' and v.get('success') is False, 'API quote overstated verification')
    amount(q.get('consumedIn'), 'API consumedIn')
    amount(q.get('grossBuyAmount'), 'API grossBuyAmount')
    nonpromotion(q)
    return q


def count_scopeids(pools):
    ids = [obj(p, 'pool').get('pool_id') for p in pools]
    require(all(type(p) is str and re.fullmatch('0x[0-9a-f]{40}', p) for p in ids)
            and len(set(ids)) == len(ids), 'physical scope ids incomplete/duplicated')
    return ids


def actors(rows_, label):
    require(Counter(r.get('actor') for r in rows_) == Counter(ACTORS), label + ' requires both actors once')


def load_facts(scope):
    require(scope in multi.SCOPES, 'unknown multi scope')
    data = multi.REFERENCE[scope].read_bytes()
    if scope == 'catalog':
        require(sha(data) == multi.catalog.CATALOG_SHA256, 'catalog raw reference differs from qualified pin')
    # The native --check wrapper authenticates shared raw in this frozen gate.
    # Its mint-funded capacity corpus can change; bind its actual bytes below.
    prefix_data = (ROOT / 'integration/test/contracts/managed-v2-aum.json').read_bytes()
    require(sha(prefix_data) == BANK_SHA256, 'old27 original bank differs')
    prefix = json.loads(prefix_data, object_pairs_hook=multi.projection._pairs)
    raw = json.loads(data, object_pairs_hook=multi.projection._pairs)
    return raw, prefix, multi.ReplayChain(scope, raw, prefix)


def validate_wire(wires, frames):
    seen = set()
    for wire in rows(wires, 'C++ wire'):
        ident = wire.get('source_bank')
        require(ident in frames, 'wire unknown source bank')
        frame = frames[ident]
        ref = validate_reference(wire.get('reference'), frame['header'])
        require(wire.get('pool_ids') == count_scopeids(frame['pools']) and wire.get('managed_fields') == 39
                and wire.get('allcomplete_bootstrap') is True and positive_int(wire.get('bytes')), 'incomplete/poisoned C++ all-pool bootstrap')
        digest(wire.get('protobuf_sha256'))
        replicas = obj(wire.get('replicas'), 'wire replicas')
        require(set(replicas) == set(ACTORS), 'wire replica inventory differs')
        for actor in ACTORS:
            state = obj(replicas[actor], 'replica')
            require(state.get('ready') is True and state.get('snapshot_ref') == ref
                    and type(state.get('scope_pool_count')) is int and state['scope_pool_count'] == len(frame['pools']),
                    'replica source/scope/readiness differs')
        seen.add((ident, canonical_sha(ref)))
    require(set(frames) <= {ident for ident, _ in seen}, 'C++ wire missing full native bank')
    return seen


def validate_vm_state(state, bank, raw, original=False):
    state = obj(state, 'actual VM state')
    header = obj(state.get('header'), 'VM header')
    require(state.get('header_sha256') == canonical_sha(header), 'VM header digest differs')
    require(type(header.get('hash')) is str and re.fullmatch('0x[0-9a-f]{64}', header['hash']), 'invalid VM header')
    if original:
        require(header == bank['header'], 'VM source header differs from complete raw header')
    pools = rows(state.get('pools'), 'actual VM pools')
    require(len(pools) == len(bank['pools']), 'VM full-pool getter inventory differs')
    calls = {}
    for actual, native in zip(pools, bank['pools']):
        require(actual.get('scenario') == native['scenario'] and actual.get('pool') == native['pool']['pool_id'], 'VM physical pool differs')
        getters = rows(actual.get('getters'), 'actual getters')
        require(len(getters) == len(native['getters']), 'missing original getter records')
        for g, expected in zip(getters, native['getters']):
            require(all(g.get(k) == expected[k] for k in ('to', 'data', 'method', 'args')), 'VM original getter identity differs')
            require(g.get('block_hash') == header['hash'] and g.get('block_tag') == header['number'], 'VM getter anchor differs')
            result = g.get('result')
            require(type(result) is str and re.fullmatch('0x(?:[0-9a-fA-F]{2})*', result), 'missing actual getter result')
            if original:
                require(g == expected, 'actual source getter comparison differs from original raw record')
            key = (g['to'].lower(), g['data'].lower())
            require(key not in calls or calls[key] == result, 'shared RPC getter forked by pool')
            calls[key] = result
    require(type(state.get('unique_getter_calls')) is int and state['unique_getter_calls'] == len(calls), 'actual unique getter call count differs')
    require(state.get('code_checks') == [dict(address=i['address'], code_sha256=i['code_sha256']) for i in raw['identities']],
            'VM original physical runtime identities differ')
    return state


def validate_transaction(tx):
    tx = obj(tx, 'actual transaction evidence')
    for key in ('transaction', 'receipt', 'header'):
        value = obj(tx.get(key), 'actual ' + key)
        require(tx.get(key + '_sha256') == canonical_sha(value), 'actual transaction/receipt/header digest differs')
    t, r, h = tx['transaction'], tx['receipt'], tx['header']
    require(type(tx.get('signed_transaction')) is str and re.fullmatch('0x(?:[0-9a-fA-F]{2})+', tx['signed_transaction']), 'signed VM transaction absent')
    require(type(t.get('hash')) is str and re.fullmatch('0x[0-9a-f]{64}', t['hash']) and r.get('transactionHash') == t['hash']
            and t.get('blockHash') == r.get('blockHash') == h.get('hash')
            and t.get('blockNumber') == r.get('blockNumber') == h.get('number')
            and r.get('status') in ('0x0', '0x1') and (tx.get('phase') != 'selected-plan' or r['status'] == '0x1'), 'actual transaction receipt/header binding failed')
    return tx


def wallet_delta(before, after, input_, output):
    before, after = obj(before, 'wallets before'), obj(after, 'wallets after')
    for name in ('sender_sell', 'sender_buy', 'recipient_buy'):
        amount(before.get(name), 'before wallet ' + name, zero=True)
        amount(after.get(name), 'after wallet ' + name, zero=True)
    require(int(before['sender_sell']) - int(after['sender_sell']) == amount(input_, 'actual wallet debit')
            and int(after['recipient_buy']) - int(before['recipient_buy']) == amount(output, 'actual wallet credit'), 'actual ERC20 wallet differences do not match API plan')


def validate_swap_evidence(tx, step, pool, request, raw):
    """Bind ABI calldata and actual Vault/ERC20 logs; no pricing math."""
    native = next(c for c in raw['cases'] if c['operation'] == 'swap')['execution']
    selector = native['transaction']['input'][:10]
    word = lambda value: format(value, '064x')
    address_word = lambda value: value[2:].rjust(64, '0')
    words = [word(224), address_word(request['caller']), word(0), address_word(request['recipient']),
             word(0), word(0), word(2**256-1), pool['multi_asset']['pool_id'][2:], word(0),
             address_word(step['asset_in']), address_word(step['asset_out']),
             word(amount(step.get('actual_amount_in'), 'executed swap input')), word(192), word(0)]
    require(tx['transaction'].get('input') == selector + ''.join(words), 'actual signed Vault swap calldata differs from selected API plan')
    logs = rows(tx['receipt'].get('logs'), 'actual swap receipt logs')
    native_swap = [log for log in native['receipt']['logs'] if len(log['topics']) == 4
                   and log['address'] == native['transaction']['to']]
    require(len(native_swap) == 1, 'original Vault Swap topic not unique')
    topic = native_swap[0]['topics'][0]
    vault = pool['multi_asset']['vault']
    swaps = [log for log in logs if log.get('address') == vault and log.get('topics', [])[:1] == [topic]]
    require(len(swaps) == 1 and swaps[0].get('topics') == [topic, pool['multi_asset']['pool_id'],
            '0x' + address_word(step['asset_in']), '0x' + address_word(step['asset_out'])]
            and swaps[0].get('data') == '0x' + word(amount(step['actual_amount_in'], 'Swap debit'))
            + word(amount(step['actual_amount_out'], 'Swap credit')), 'actual Vault Swap event differs from selected plan/wallet amounts')
    # The original token Transfer topic is shared by the two ordinary ERC20s.
    transfer_topic = '0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef'
    for token, sender, recipient, quantity in ((step['asset_in'], request['caller'], vault, step['actual_amount_in']),
                                               (step['asset_out'], vault, request['recipient'], step['actual_amount_out'])):
        transfers = [log for log in logs if log.get('address') == token and log.get('topics') ==
                     [transfer_topic, '0x' + address_word(sender), '0x' + address_word(recipient)]]
        require(len(transfers) == 1 and transfers[0].get('data') == '0x' + word(amount(quantity, 'ERC20 transfer')),
                'actual ERC20 transfer does not bind caller/Vault/recipient and selected amount')
    for log in logs:
        require(log.get('transactionHash') == tx['transaction']['hash'] and log.get('blockHash') == tx['header']['hash']
                and log.get('blockNumber') == tx['header']['number'] and log.get('removed') is False,
                'actual event receipt/source binding differs')


def validate_shared_actual(actual, row, bank, raw, prefix, reference_sha256):
    a = obj(actual, 'independent actual VM replay')
    require(a.get('passed') is True and a.get('vm_started') is True and a.get('schema_version') == 1
            and a.get('authority') == 'independent-local-original-bytecode-selected-shared-vault-plan-reference', 'independent original selected-plan VM evidence required')
    for field in ('verified', 'production_ready', 'all_requested_features_complete'):
        require(a.get(field) is False, 'VM missing explicit nonpromotion: ' + field)
    nonpromotion(a)
    require(a.get('original_bank_sha256') == BANK_SHA256 and a.get('shared_reference_sha256') == reference_sha256, 'VM raw reference binding differs')
    source = obj(a.get('source'), 'actual VM source')
    require(source.get('bank') == bank['id'] and source.get('block') == row['quote']['snapshotRef']['block'], 'VM source/request/quote binding differs')
    validate_vm_state(source.get('state'), bank, raw, original=True)
    expected_calls = len({(g['to'].lower(), g['data'].lower()) for p in bank['pools'] for g in p['getters']})
    require(source.get('getter_records_checked') == bank['getter_count'] == 152
            and source.get('unique_getter_calls') == expected_calls == 114, 'complete actual source getter comparison/call counts differ')
    q, request = row['quote'], row['request']
    require(a.get('actual_amount_in') == request['sellAmount'] == q['consumedIn']
            and a.get('actual_amount_out') == q['grossBuyAmount'], 'VM aggregate amounts not bound to actual API request')
    wallet_delta(a.get('wallets_before'), a.get('wallets_after'), a.get('actual_amount_in'), a.get('actual_amount_out'))
    transactions = rows(a.get('transactions'), 'VM transactions')
    for tx in transactions:
        validate_transaction(tx)
    history = [tx for tx in transactions if tx.get('phase') != 'selected-plan']
    selected = [tx for tx in transactions if tx.get('phase') == 'selected-plan']
    originals = [o['value'] for o in prefix['objects'].values() if o['kind'] == 'transaction' and int(o['value']['receipt']['blockNumber'], 16) <= 28]
    originals.sort(key=lambda t: int(t['receipt']['blockNumber'], 16))
    originals += [t for t in raw['transactions'] if int(t['execution_header']['number'], 16) <= int(bank['header']['number'], 16)]
    require(len(history) == len(originals) == a.get('native_transaction_count') and a.get('prefix_transaction_count') == 28,
            'VM complete original history transaction counts differ')
    for tx, original in zip(history, originals):
        height = int(original['receipt']['blockNumber'], 16)
        require(tx.get('phase') == ('original-prefix' if height <= 28 else 'shared-history')
                and all(tx.get(k) == original[k] for k in ('signed_transaction', 'transaction', 'receipt')), 'VM original signed history differs')
    routes = rows(a.get('routes'), 'VM routes')
    require(type(a.get('selected_route_count')) is int and a['selected_route_count'] == len(routes) == len(q['routes'])
            and a.get('plan_transaction_count') == len(selected) == sum(len(r['path']['edges']) for r in q['routes']), 'selected route/transaction inventory differs')
    cursor, selected_index = source['state'], 0
    for index, (route, quoted) in enumerate(zip(routes, q['routes'])):
        require(route.get('index') == index and route.get('before') == cursor
                and route.get('quoted_amount_in') == route.get('actual_amount_in') == quoted['amount_in']
                and route.get('quoted_amount_out') == route.get('actual_amount_out') == quoted['amount_out'], 'actual selected route differs from quoted route')
        wallet_delta(route.get('wallets_before'), route.get('wallets_after'), route.get('actual_amount_in'), route.get('actual_amount_out'))
        steps = rows(route.get('steps'), 'VM route steps')
        require(len(steps) == len(quoted['path']['edges']), 'VM selected path edge inventory differs')
        for step, edge in zip(steps, quoted['path']['edges']):
            pool = next(p['pool'] for p in bank['pools'] if p['pool']['pool_id'] == edge['pool_id'])
            require(step.get('pool') == edge['pool_id'] and step.get('pool_id') == pool['multi_asset']['pool_id']
                    and step.get('asset_in') == request['sellToken'] and step.get('asset_out') == request['buyToken']
                    and step.get('sender') == request['caller'] and step.get('recipient') == request['recipient']
                    and step.get('actual_amount_in') == quoted['amount_in'] and step.get('actual_amount_out') == quoted['amount_out']
                    and step.get('before') == cursor and step.get('transaction') == selected[selected_index], 'VM step/request/selected transaction binding differs')
            wallet_delta(step.get('wallets_before'), step.get('wallets_after'), step.get('actual_amount_in'), step.get('actual_amount_out'))
            tx = selected[selected_index]
            require(tx['transaction'].get('from') == request['caller'] and tx['transaction'].get('to') == pool['multi_asset']['vault'], 'selected transaction caller/Vault differs')
            validate_swap_evidence(tx, step, pool, request, raw)
            cursor = validate_vm_state(step.get('after'), bank, raw)
            require(cursor['header'] == tx['header'] and int(cursor['header']['number'], 16) == int(step['before']['header']['number'], 16) + 1
                    and cursor['header']['parentHash'] == step['before']['header']['hash']
                    and cursor['header']['timestamp'] == bank['header']['timestamp'], 'selected transaction/state chronology differs')
            selected_index += 1
        require(route.get('after') == cursor, 'VM route lost full post-state')
    require(a.get('final_state') == cursor, 'VM final complete state differs')


def validate_process(report, runtime, inputs, *, gate_started_at_unix_ms, gate_completed_at_unix_ms=None):
    """Validate one fresh scope against exact raw facts; never invoke a VM here."""
    obj(report, 'process report')
    runtime_shape(runtime)
    scope = report.get('scope')
    require(scope in multi.SCOPES, 'unknown process scope')
    require(report.get('full_service_run') is True and report.get('passed') is True and report.get('state') == 'PASSED', 'full service scope has not passed')
    for key in ('verified', 'production_ready', 'all_requested_features_complete', 'independent_gas_reference'):
        require(report.get(key) is False, 'process nonpromotion missing: ' + key)
    nonpromotion(report)
    start, end = report.get('started_at_unix_ms'), report.get('completed_at_unix_ms')
    upper = int(time.time()*1000) if gate_completed_at_unix_ms is None else gate_completed_at_unix_ms
    require(positive_int(start) and positive_int(end) and positive_int(gate_started_at_unix_ms)
            and gate_started_at_unix_ms <= start <= end <= upper, 'process is stale or has invalid gate chronology')
    require(all(report.get(k) == runtime for k in ('source_binding', 'source_binding_started', 'source_binding_completed'))
            and all(report.get(k) == inputs for k in ('input_hashes', 'input_hashes_started', 'input_hashes_completed'))
            and report.get('inputs_unchanged') is True, 'process source/five-binary/input closure differs')
    raw, prefix, chain = load_facts(scope)
    require(report.get('exact_reference_sha256') == chain.reference_sha256 and report.get('original_bank_sha256') == BANK_SHA256, 'process reference raw SHA differs')
    frames = {f['id']: f for f in chain.frames}
    observed = rows(report.get('source_frames'), 'source frames')
    require(report.get('source_frame_count') == len(observed) == len(frames)
            and Counter(f.get('source_bank') for f in observed) == Counter(frames.keys()), 'complete once-only native source frame inventory required')
    for frame in observed:
        expected = frames[frame['source_bank']]
        require(frame.get('header') == expected['header'] and frame.get('pools') == expected['pools']
                and frame.get('raw_getter_count') == raw['banks'][expected['id']]['getter_count']
                and frame.get('unique_getter_count') == len(expected['getters']), 'raw complete source bank/getters differ')
        validate_reference(frame.get('reference'), expected['header'])
    wires = validate_wire(report.get('cpp_wire'), frames)
    for frame in observed:
        require((frame['source_bank'], canonical_sha(frame['reference'])) in wires, 'source frame has no exact C++ replica bootstrap')
    expected_inventory = [dict(name=c['name'], operation=c['operation'], source_bank=multi.source_id(scope, c),
        after_bank=c['expected_after_bank4pools' if scope == 'catalog' else 'expected_after_bank2pools']) for c in chain.cases]
    require(report.get('native_case_inventory') == expected_inventory, 'native case/collection inventory differs')
    cases = {c['name']: c for c in chain.cases if c['operation'] == 'swap'}
    first = next(iter(cases.values()))
    plans = rows(report.get('positive_plans'), 'positive plans')
    expected_plans = Counter((a, name, 'source') for a in ACTORS for name in cases)
    expected_plans.update(('router1', first['name'], 'fault-recovery:' + f) for f in multi.faults_for(scope))
    expected_plans.update((a, first['name'], 'restart') for a in ACTORS)
    capacity = [p for p in plans if p.get('phase') == 'capacity-split']
    regular = [p for p in plans if p.get('phase') != 'capacity-split']
    require(len(regular) == 14 and Counter((p.get('actor'), p.get('case'), p.get('phase')) for p in regular) == expected_plans,
            'all fourteen once-only source/fault/restart plans required')
    if scope == 'shared':
        actors(capacity, 'actual API capacity split')
        require(len(plans) == 16, 'shared scope requires fourteen native/recovery/restart plus two capacity plans')
    else:
        require(not capacity and len(plans) == 14, 'unexpected catalog capacity plans')
    for row in plans:
        require(row.get('passed') is True and row.get('independent_gas_reference') is False, 'positive plan incomplete/promoted')
        is_capacity = row.get('phase') == 'capacity-split'
        require(type(row.get('case')) is str and bool(row['case']), 'positive plan case identity absent')
        case = first if is_capacity else cases[row['case']]
        expected_request = multi.native_request(case)
        if is_capacity:
            expected_request['sellAmount'] = '5000000000000000000000'
        ident = multi.source_id(scope, case)
        frame = frames[ident]
        require(row.get('source_bank') == ident and row.get('source_hash') == frame['header']['hash']
                and row.get('request') == expected_request, 'positive request/native source bank binding differs')
        q = indicative(row.get('quote'))
        request = row['request']
        ref = validate_reference(q.get('snapshotRef'), frame['header'])
        require((ident, canonical_sha(ref)) in wires and q.get('scopeHash') == ref['scope_hash'], 'quote source ref lacks exact complete wire/replica evidence')
        require(q.get('consumedIn') == request['sellAmount'] and q.get('refund') == '0'
                and q.get('sellAmount') == request['sellAmount'] and q.get('chainKey') == request['chainKey'], 'quote/request exact-in binding differs')
        ids = count_scopeids(frame['pools'])
        quality = obj(q.get('quality'), 'quote quality')
        require(quality.get('candidateScope') == ids and quality.get('excludedPools') == {}, 'complete pure Managed candidate/exclusion inventory differs')
        selected = rows(q.get('routes'), 'API selected routes')
        require(type(row.get('route_count')) is int and row['route_count'] == len(selected)
                and 0 < len(selected) <= (8 if scope == 'shared' else 1), 'invalid API selected route count')
        if is_capacity:
            require(row['route_count'] == 2 and len(selected) == 2
                    and {r.get('path', {}).get('edges', [{}])[0].get('pool_id') for r in selected} == set(ids),
                    'capacity order must actually select the two distinct physical API routes')
        in_sum = out_sum = 0
        for route in selected:
            in_sum += amount(route.get('amount_in'), 'route input')
            out_sum += amount(route.get('amount_out'), 'route output')
            path = obj(route.get('path'), 'selected path')
            edges = rows(path.get('edges'), 'selected edges')
            require(len(edges) == 1 and edges[0].get('pool_id') in ids
                    and edges[0].get('protocol') == 'balancer-v2-managed-aum'
                    and path.get('tokens') == [request['sellToken'], request['buyToken']]
                    and path.get('amount_in') == route['amount_in'] and path.get('amount_out') == route['amount_out'], 'API selected path differs from physical bank/request')
        require(in_sum == amount(q['consumedIn'], 'consumed') and out_sum == amount(q['grossBuyAmount'], 'gross'), 'API selected route amounts do not conserve request/gross')
        if scope == 'catalog':
            native = obj(row.get('native_expected'), 'native expected amounts')
            for key in ('amount_in', 'amount_out', 'trader_input_delta', 'trader_output_delta'):
                amount(native.get(key), 'native ' + key)
            require(native == case['expected_native'] and q['grossBuyAmount'] == native['amount_out'] and q['consumedIn'] == native['amount_in'], 'catalog original native wallet amounts differ')
            selected_pool = next(p['pool_id'] for p in frame['pools'] if p['multi_asset']['pool_id'] == native['swap_event']['pool_id'])
            require(all(r['path']['edges'][0]['pool_id'] == selected_pool for r in selected), 'catalog selected a different physical native pool')
        else:
            validate_shared_actual(row.get('independent_actual'), row, raw['banks'][ident], raw, prefix, chain.reference_sha256)
    faults = rows(report.get('faults'), 'faults')
    require(report.get('fault_count') == len(faults) == 4 and Counter(obj(f.get('evidence'), 'fault evidence').get('fault') for f in faults)
            == Counter(multi.faults_for(scope)), 'four exact native source fault recoveries required')
    ancestor, next_frame = chain.frames[:2]
    for fault in faults:
        e = fault['evidence']
        name = e['fault']
        chain.select(1, name)
        expected = chain.fault_evidence
        require(e.get('source_bank') == next_frame['id'] and e.get('source_hash') == next_frame['header']['hash']
                and all(e.get(k) == expected[k] for k in ('method', 'body', 'before')) and e.get('before') != e.get('after'), 'fault is not an actual different-next-H RPC mutation')
        if name == 'future-head':
            require(type(e.get('after')) is str and re.fullmatch('0x[0-9a-f]+', e['after'])
                    and int(e['after'], 16) > int(e['before'], 16), 'future head did not alter original timestamp')
        else:
            require(e.get('after') == expected['after'], 'fault RPC payload differs from native fault')
        old = validate_reference(fault.get('ancestor_reference'), ancestor['header'])
        recovered = validate_reference(fault.get('recovered_reference'), next_frame['header'])
        require((next_frame['id'], canonical_sha(recovered)) in wires, 'recovered next bank lacks complete C++ replica evidence')
        require(old['block']['hash'] != e['source_hash'] and fault.get('recovered') is True
                and positive_int(fault.get('actual_fault_rpc_hits')) and old['online_generation'] < recovered['online_generation'], 'fault did not withdraw actual next bank/recover a newer generation')
        recovery = next(p for p in plans if p['phase'] == 'fault-recovery:' + name)
        require(recovery['quote']['snapshotRef']['online_generation'] > recovered['online_generation'],
                'return-to-ancestor recovery quote lacks a subsequent generation')
        rejections = rows(fault.get('quote_rejections'), 'withdrawal quote rejections')
        actors(rejections, 'withdrawal')
        # The producer records actual CHAIN_NOT_READY as HTTP 503, as well as 4xx.
        require(all(type(r.get('status')) is int and 400 <= r['status'] < 600 for r in rejections), 'withdrawn source served an API quote')
        unrelated = rows(fault.get('unrelated_solana'), 'unrelated actual Solana quotes')
        actors(unrelated, 'unrelated Solana')
        for item in unrelated:
            q = indicative(item.get('quote'))
            require(item.get('request_chain') == q.get('chainKey') == 'solana:localnet'
                    and q.get('consumedIn') == '10003' and ('status' not in item or item['status'] == 200), 'unrelated Solana actual quote failed')
    reverts = rows(report.get('reverts'), 'VERIFIED/build refusals')
    actors(reverts, 'VERIFIED/build')
    require(all(r.get('verified_status') == 422 and r.get('build_status') == 422
                and 'INDEPENDENT_SIMULATION_UNAVAILABLE' in json.dumps(r.get('verified_response'))
                and type(r.get('build_response')) is dict and bool(r['build_response']) for r in reverts), 'VERIFIED/public build unexpectedly admitted or evidence absent')
    checks = report.get('checks')
    require(type(checks) is list and all(type(c) is str for c in checks), 'missing service checks')
    expected_checks = {'online-go-rpc-rust-debug-solana-and-two-router-processes', 'all-native-full-banks-and-eight-two-replica-positive-plans',
        'router2-restart-complete-native-bank-restore', 'verified-and-build-rejected-on-both-replicas',
        'real-api-selected-plans-independent-vm-full-getters-and-wallet-differences' if scope == 'shared' else
        'four-native-swaps-and-four-30day-collections-complete-observed-after'}
    expected_checks.update('actual-next-bank-withdrawal-and-new-generation:' + f for f in multi.faults_for(scope))
    require(expected_checks <= set(checks), 'service obligations missing')


def stage_commands():
    return [
        ('reference-guards', ['make', 'managed-v2-aum-reference-guards']),
        ('old27-exact', ['python3', 'tools/ci/managed-v2-aum-check.py', '--check']),
        ('old-paused', ['node', 'tools/ci/managed-v2-aum-paused-vault-golden.cjs', '--check']),
        ('old-recovery', ['node', 'tools/ci/managed-v2-aum-recovery-golden.cjs', '--check']),
        ('old-projection', ['python3', 'tools/ci/managed-v2-aum-service-golden.py', '--check']),
        ('catalog-native', ['node', 'tools/ci/managed-v2-aum-catalog-golden.cjs', '--check']),
        ('catalog-projection', ['python3', 'tools/acceptance/managed_v2_catalog.py', '--input', 'test/contracts/managed-v2-aum-catalog.json', '--check', 'test/contracts/managed-v2-aum-catalog-pools.json']),
        ('shared-native', ['node', 'tools/ci/managed-v2-shared-vault-golden.cjs', '--check']),
        ('wire39', ['python3', 'tools/ci/managed-v2-aum-wire-golden.py', '--check']),
        ('wire39-self-test', ['python3', 'tools/ci/managed-v2-aum-wire-golden.py', '--self-test']),
        ('helpers', ['make', 'managed-v2-multi-helper-test']),
        ('multi-differential', ['make', 'managed-v2-multi-differential-test']),
        ('single-regression', ['python3', 'tools/acceptance/managed_v2_smoke.py']),
        ('multi-service', ['python3', 'tools/acceptance/managed_v2_multi_smoke.py', '--scope', 'both']),
    ]


def frozen(runtime, inputs, verification_sha256=None):
    require(runtime_fingerprint() == runtime and snapshot_inputs() == inputs, 'source/runtime/all inputs changed during frozen gate')
    if verification_sha256 is not None:
        require(sha((OUT / 'verification.json').read_bytes()) == verification_sha256, 'verification receipt changed during gate')


def run_stage(name, command, report, runtime, inputs, env, target=TARGET):
    """Atomically persist intent, exit and log SHA, including interruptions/drift."""
    target = Path(target)
    log = target.parent / ('managed-v2-multi-final-' + name + '.log')
    stage = dict(stage=name, command=command, state='RUNNING', started_at_unix_ms=int(time.time()*1000), log=str(log))
    report['commands'].append(stage)
    write_result(target, report)
    try:
        frozen(runtime, inputs, report.get('verification_sha256'))
        with log.open('wb') as stream:
            process = subprocess.run(command, cwd=ROOT / 'integration', env=env, stdout=stream, stderr=subprocess.STDOUT)
        stage['exit_code'] = process.returncode
        require(process.returncode == 0, name + ' failed; see ' + str(log))
        frozen(runtime, inputs, report.get('verification_sha256'))
        stage['state'] = 'PASSED'
    except BaseException as error:
        stage.update(state='FAILED', error=str(error), error_type=type(error).__name__)
        report.update(state='FAILED', passed=False, error=str(error), error_type=type(error).__name__, completed_at_unix_ms=int(time.time()*1000))
        raise
    finally:
        stage.update(completed_at_unix_ms=int(time.time()*1000), log_sha256=sha(log.read_bytes()) if log.exists() else None)
        write_result(target, report)
    return stage


def run():
    report = dict(schema_version=1, scope=SCOPE, state='RUNNING', passed=False, verified=False, production_ready=False,
        all_requested_features_complete=False, independent_gas_reference=False, commands=[], started_at_unix_ms=int(time.time()*1000),
        original_bank_sha256=BANK_SHA256, remaining=list(REMAINING))
    write_result(TARGET, report)
    try:
        verification, runtime = checked_runtime()
        validate_verification(verification, runtime)
        inputs = snapshot_inputs()
        own = str(Path(__file__).resolve().relative_to(ROOT))
        require(inputs.get(own) == sha(Path(__file__).read_bytes()), 'acceptance checker absent from input closure')
        verification_sha256 = sha((OUT / 'verification.json').read_bytes())
        review_bytes = REVIEW.read_bytes()
        validate_review(json.loads(review_bytes), runtime, inputs, verification_sha256)
        report.update(runtime=runtime, source_tree_sha256=runtime['source_tree_sha256'], input_hashes=inputs,
                      verification_sha256=verification_sha256, checked_binary_sha256=runtime['binaries'])
        write_result(TARGET, report)
        env = os.environ.copy()
        env['PATH'] = os.pathsep.join((str(ROOT / 'integration/.runtime/protoc-bin'), str(Path.home() / '.cargo/bin'), '/usr/local/bin', env.get('PATH', '')))
        env['PYTHONDONTWRITEBYTECODE'] = '1'
        env['XONE_NODE'] = '/usr/local/bin/node'
        for name, command in stage_commands():
            run_stage(name, command, report, runtime, inputs, env)
        # The old validator is used only for its original single-profile regression.
        import managed_v2_acceptance as legacy
        old_path = OUT / 'managed-v2-aum-three-service-smoke.json'
        old_bytes = old_path.read_bytes()
        old = json.loads(old_bytes)
        legacy.validate_process(old, runtime, inputs)
        single_stage = next(s for s in report['commands'] if s['stage'] == 'single-regression')
        require(single_stage['started_at_unix_ms'] <= old['started_at_unix_ms'] <= old['completed_at_unix_ms'] <= single_stage['completed_at_unix_ms'], 'old single-profile process regression is stale')
        multi_stage = next(s for s in report['commands'] if s['stage'] == 'multi-service')
        reports = {}
        for scope in multi.SCOPES:
            data = multi.REPORTS[scope].read_bytes()
            value = json.loads(data)
            require(value.get('scope') == scope, 'process report scope/path differs')
            validate_process(value, runtime, inputs, gate_started_at_unix_ms=multi_stage['started_at_unix_ms'],
                             gate_completed_at_unix_ms=multi_stage['completed_at_unix_ms'])
            reports[scope] = dict(path=str(multi.REPORTS[scope]), sha256=sha(data))
        reference_path = ROOT / 'integration/tools/reference/balancer-managed-v2-aum/verified.json'
        reference_bytes = reference_path.read_bytes()
        original = json.loads(reference_bytes)
        require(original.get('passed') is True and original.get('fixture_sha256') == BANK_SHA256
                and original.get('regeneration') == 'exact-temporary-byte-comparison', 'exact old27 original replay receipt missing')
        frozen(runtime, inputs, verification_sha256)
        require(REVIEW.read_bytes() == review_bytes, 'independent review changed during gate')
        validate_review(json.loads(REVIEW.read_bytes()), runtime, inputs, verification_sha256)
        require(old_path.read_bytes() == old_bytes and reference_path.read_bytes() == reference_bytes, 'single/reference report bytes changed')
        for binding in reports.values():
            require(sha(Path(binding['path']).read_bytes()) == binding['sha256'], 'process report bytes changed')
        for stage in report['commands']:
            require(stage['state'] == 'PASSED' and stage['log_sha256'] == sha(Path(stage['log']).read_bytes()), 'stage log changed/incomplete')
        report.update(passed=True, state='PASSED', full_service_run=True, source_unchanged_during_verification=True, inputs_unchanged=True,
            input_hashes_completed=inputs, runtime_completed=runtime, process_reports=reports, previous_single_process_report_sha256=sha(old_bytes),
            review_sha256=sha(review_bytes), original_reference_sha256=sha(reference_bytes),
            reference_files={str(p.relative_to(ROOT)):sha(p.read_bytes()) for p in [ROOT / 'integration/test/contracts/managed-v2-aum.json', *multi.REFERENCE.values()]},
            source_frames=dict(catalog=10, shared=7), scope_pool_counts=dict(catalog=4, shared=2), positive_plans_per_scope=dict(catalog=14, shared=16),
            completed_at_unix_ms=int(time.time()*1000))
        write_result(TARGET, report)
        return report
    except BaseException as error:
        report.update(passed=False, state='FAILED', error=str(error), error_type=type(error).__name__, completed_at_unix_ms=int(time.time()*1000))
        write_result(TARGET, report)
        raise


if __name__ == '__main__':
    result = run()
    print(json.dumps(dict(passed=result['passed'], scope=result['scope'], verified=False,
                          production_ready=False, all_requested_features_complete=False)))
