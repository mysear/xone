"""Prefill exact RPC inputs for an observed signed-block window.

Headers establish only a request plan here. Normal Go header, BLS, receipt and
MPT verification remains responsible for every publication.
"""
import copy
import json
import math
import re
import time

from signed_rpc_capture import strict_json


MAX_PENDING = 8
MAX_RESPONSE_BYTES = 16 << 20
MAX_HEIGHT = (1 << 64) - 1
HASH = re.compile(r'0x[0-9a-fA-F]{64}')
CAPACITY_ERRORS = frozenset({'CAPTURE_PENDING_BUDGET', 'CAPTURE_BYTES_BUDGET'})
SELECTOR_ARITY = {'eth_getCode': 2, 'eth_call': 2, 'eth_getStorageAt': 3, 'eth_getProof': 3}


def _hash(value):
    return isinstance(value, str) and HASH.fullmatch(value) is not None


def _selector(value, old_number, old_hash, number, block_hash):
    if value == hex(old_number):
        return hex(number)
    if (isinstance(value, dict) and set(value) <= {'blockHash', 'requireCanonical'}
            and ('requireCanonical' not in value or type(value['requireCanonical']) is bool)
            and _hash(value.get('blockHash')) and value['blockHash'].lower() == old_hash):
        selected = copy.deepcopy(value)
        selected['blockHash'] = block_hash
        return selected
    return None


def _template(row, old_number, old_hash, number, block_hash):
    method, original = row['method'], row['params']
    if not isinstance(original, list):
        return None
    params = copy.deepcopy(original)
    if method == 'eth_getBlockByNumber' and len(params) == 2 and type(params[1]) is bool:
        if params[0] != hex(old_number):
            return None
        params[0] = hex(number)
    elif method == 'eth_getBlockReceipts' and len(params) == 1:
        if not _hash(params[0]) or params[0].lower() != old_hash:
            return None
        params[0] = block_hash
    elif method in SELECTOR_ARITY and len(params) == SELECTOR_ARITY[method]:
        selected = _selector(params[-1], old_number, old_hash, number, block_hash)
        if selected is None:
            return None
        params[-1] = selected
    else:
        return None
    return row['source_index'], method, params


class _Planner:
    def __init__(self, capture, deadline, expires_at):
        self.capture, self.deadline = capture, deadline
        self.expires_at = expires_at
        self.planned, self.completed = set(), set()
        self.cached = set()
        stats = self.check()
        self.providers = stats.get('provider_ids')
        if (not isinstance(self.providers, list) or len(self.providers) != 3
                or any(not isinstance(p, str) or not re.fullmatch(r'rpc-[0-9a-f]{24}', p)
                       for p in self.providers)):
            raise ValueError('SIGNED_WINDOW_SOURCE_SCOPE')

    def remaining(self):
        return min(self.deadline - time.monotonic(), self.expires_at - time.time())

    def check(self):
        if self.remaining() <= 0:
            raise ValueError('SIGNED_WINDOW_TIMEOUT')
        stats = self.capture.stats()
        if type(stats.get('pending')) is not int or stats['pending'] < 0:
            raise ValueError('SIGNED_WINDOW_CAPTURE_STATS')
        if stats.get('closed'):
            raise ValueError('CAPTURE_CLOSED')
        if stats.get('storage_failed'):
            raise ValueError('CAPTURE_STORAGE_FAILED')
        return stats

    def key(self, job):
        source, method, params = job
        return (self.providers[source], method, json.dumps(params, sort_keys=True, separators=(',', ':')))

    def request(self, job):
        self.check()
        source, method, params = job
        raw = self.capture.response(source, json.dumps(
            dict(jsonrpc='2.0', id=1, method=method, params=params), separators=(',', ':')).encode())
        self.check()
        if not isinstance(raw, bytes) or len(raw) > MAX_RESPONSE_BYTES:
            raise ValueError('SIGNED_WINDOW_RESPONSE_BUDGET')
        response = strict_json(raw)
        if (not isinstance(response, dict) or response.get('jsonrpc') != '2.0'
                or type(response.get('id')) is not int or response['id'] != 1
                or ('result' in response) == ('error' in response)):
            raise ValueError('SIGNED_WINDOW_RPC_ENVELOPE')
        if 'error' in response:
            error = response['error']
            code = error.get('message') if isinstance(error, dict) else None
            if code == 'CAPTURE_PENDING':
                return 'pending', None
            if isinstance(code, str) and code in CAPACITY_ERRORS:
                return 'capacity', code
            if isinstance(code, str) and re.fullmatch(r'CAPTURE_[A-Z0-9_]+|UNRECORDED', code):
                raise ValueError(code)
            rpc_code = error.get('code') if isinstance(error, dict) else None
            if type(rpc_code) is int:
                raise ValueError('SIGNED_WINDOW_RPC_ERROR_' + str(rpc_code))
            raise ValueError('SIGNED_WINDOW_RPC_ERROR')
        if response['result'] is None:
            raise ValueError('SIGNED_WINDOW_RPC_NULL')
        return 'complete', response['result']

    def run(self, requests, on_result=None):
        requests = iter(requests)
        pending, retry, exhausted = {}, None, False
        empty_rounds = 0
        while not exhausted or pending or retry is not None:
            self.check()
            completed_before = len(self.completed)
            for key, job in list(pending.items()):
                state, result = self.request(job)
                if state == 'complete':
                    if on_result:
                        on_result(job, result)
                    self.completed.add(key)
                    del pending[key]
                elif state == 'capacity':
                    raise ValueError('SIGNED_WINDOW_PENDING_STATE')
            while len(pending) < MAX_PENDING and not exhausted:
                if retry is None:
                    try:
                        job = next(requests)
                    except StopIteration:
                        exhausted = True
                        break
                else:
                    job, retry = retry, None
                key = self.key(job)
                if key in self.completed or key in pending:
                    continue
                pending_before = self.check()['pending']
                if pending_before >= MAX_PENDING and key not in self.cached:
                    retry = job
                    break
                self.planned.add(key)
                state, result = self.request(job)
                if state == 'complete':
                    if on_result:
                        on_result(job, result)
                    self.completed.add(key)
                elif state == 'pending':
                    pending[key] = job
                else:
                    if not self.check()['pending'] and not pending_before:
                        raise ValueError(result)
                    retry = job
                    break
            if exhausted and not pending and retry is None:
                return
            stats = self.check()
            if stats['pending']:
                empty_rounds = 0
                remaining = self.remaining()
                if remaining <= 0:
                    raise ValueError('SIGNED_WINDOW_TIMEOUT')
                self.capture.wait_pending(timeout=min(1, remaining))
                self.check()
            else:
                # A job may finish between response() and stats(). Poll its
                # durable response once more; zero pending alone proves nothing.
                empty_rounds = 0 if len(self.completed) > completed_before else empty_rounds + 1
                if empty_rounds > 2:
                    raise ValueError('SIGNED_WINDOW_PENDING_STATE')


def _observed_header(job, value):
    number = int(job[2][0], 16)
    if (not isinstance(value, dict) or value.get('number') != hex(number)
            or not _hash(value.get('hash')) or not _hash(value.get('parentHash'))
            or not isinstance(value.get('transactions'), list)
            or any(not _hash(tx) for tx in value['transactions'])
            or len({tx.lower() for tx in value['transactions']}) != len(value['transactions'])):
        raise ValueError('SIGNED_WINDOW_HEADER_SHAPE')
    return number, value['hash'].lower(), value['parentHash'].lower()


def prefill_signed_identities(capture, genesis_hash, budget_seconds):
    """Record the exact identity reads required by both ordinary body providers."""
    if not _hash(genesis_hash) or not 0 < budget_seconds <= 1800:
        raise ValueError('SIGNED_WINDOW_IDENTITY_INPUT')
    planner = _Planner(capture, time.monotonic() + budget_seconds, time.time() + budget_seconds)

    def check_identity(job, result):
        if job[1] == 'eth_chainId':
            valid = result == '0x1'
        else:
            valid = isinstance(result, dict) and result.get('number') == '0x0' and result.get('hash') == genesis_hash
        if not valid:
            raise ValueError('SIGNED_WINDOW_IDENTITY_CONFLICT')

    jobs = ((index, method, params) for index in (0, 1) for method, params in
            [('eth_chainId', []), ('eth_getBlockByNumber', ['0x0', False])])
    planner.run(jobs, check_identity)
    return len(planner.completed)


def prefill_signed_range(capture, template_block, first, last, expected_last_hash,
                         expected_parent=None, budget_seconds=1200):
    """Collect actual bytes for at most 128 linked heights; grant no readiness."""
    if (type(first) is not int or type(last) is not int or not 0 <= first <= last <= MAX_HEIGHT
            or last - first + 1 > 128 or type(budget_seconds) not in (int, float)
            or not math.isfinite(budget_seconds) or not 0 < budget_seconds <= 1800):
        raise ValueError('SIGNED_WINDOW_LIMITS')
    if (not isinstance(template_block, dict) or template_block.get('chain_key') != 'evm:1'
            or type(template_block.get('number_or_slot')) is not int
            or not 0 <= template_block['number_or_slot'] <= MAX_HEIGHT
            or not _hash(template_block.get('hash')) or not _hash(expected_last_hash)
            or expected_parent is not None and not _hash(expected_parent)):
        raise ValueError('SIGNED_WINDOW_BLOCK_BINDING')
    planner = _Planner(capture, time.monotonic() + budget_seconds, time.time() + budget_seconds)
    inventory = capture.request_inventory()
    planner.check()
    if not isinstance(inventory, list) or len(inventory) > 10000:
        raise ValueError('SIGNED_WINDOW_INVENTORY')
    for row in inventory:
        if (not isinstance(row, dict) or type(row.get('source_index')) is not int
                or not 0 <= row['source_index'] < 3
                or row.get('provider_id') != planner.providers[row['source_index']]
                or not isinstance(row.get('method'), str)
                or not isinstance(row.get('params'), (list, dict))):
            raise ValueError('SIGNED_WINDOW_INVENTORY')
        planner.cached.add(planner.key((row['source_index'], row['method'], row['params'])))
    old_number, old_hash = template_block['number_or_slot'], template_block['hash'].lower()
    templates = []
    for row in inventory:
        if _template(row, old_number, old_hash, old_number, old_hash) is not None:
            templates.append(copy.deepcopy(row))
    skipped = len(inventory) - len(templates)
    headers = {}
    def retain_header(job, value):
        number, block_hash, parent = _observed_header(job, value)
        headers[number] = (block_hash, parent)
    planner.run(((0, 'eth_getBlockByNumber', [hex(number), False])
                 for number in range(first, last + 1)), retain_header)
    previous = expected_parent.lower() if expected_parent is not None else None
    for number in range(first, last + 1):
        block_hash, parent = headers[number]
        if previous is not None and parent != previous:
            raise ValueError('SIGNED_WINDOW_PARENT_CONFLICT')
        previous = block_hash
    if previous != expected_last_hash.lower():
        raise ValueError('SIGNED_WINDOW_LAST_HASH')
    def requests():
        for number in range(first, last + 1):
            block_hash = headers[number][0]
            for source in (0, 1):
                yield source, 'eth_getBlockByNumber', [hex(number), True]
                yield source, 'eth_getBlockReceipts', [block_hash]
            yield 0, 'eth_getBlockByNumber', [hex(number), False]
            for row in templates:
                yield _template(row, old_number, old_hash, number, block_hash)
    planner.run(requests())
    planner.check()
    return dict(planned=len(planner.planned), completed=len(planner.completed), skipped=skipped)
