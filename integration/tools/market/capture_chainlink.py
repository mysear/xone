#!/usr/bin/env python3
"""Bounded read-only collector for an operator-approved Ethereum USD feed pair.

No private keys are used. The product validator recomputes the unsigned price;
operators must independently review/sign it before a Router accepts the book.
"""
import argparse
import base64
import hashlib
import json
from pathlib import Path
import re
import subprocess
import sys
import time
import urllib.error
import urllib.parse
import urllib.request

GENESIS = '0xd4e56740f876aef8c010b86a40d5f56745a118d0906a34e69aec8c0db1cb8fa3'
ZERO = '0x' + '00' * 20
ALLOWED = {'eth_chainId', 'eth_getBlockByNumber', 'eth_getBlockByHash', 'eth_getCode', 'eth_call'}


def require(value, code):
    if not value:
        raise ValueError(code)


def pairs(items):
    out = {}
    for key, value in items:
        require(key not in out, 'MARKET_DUPLICATE_JSON_KEY')
        out[key] = value
    return out


def load(raw):
    return json.loads(raw, object_pairs_hook=pairs,
                      parse_constant=lambda _: (_ for _ in ()).throw(ValueError('MARKET_NONFINITE_JSON')))


def digest(raw):
    return hashlib.sha256(raw).hexdigest()


def file_digest(path):
    h = hashlib.sha256()
    with Path(path).open('rb') as stream:
        for raw in iter(lambda: stream.read(1 << 20), b''):
            h.update(raw)
    return h.hexdigest()


def write(path, value):
    Path(path).write_text(json.dumps(value, indent=2) + '\n')


def quantity(value):
    require(isinstance(value, str) and re.fullmatch(r'0x(?:0|[1-9a-f][0-9a-f]*)', value),
            'MARKET_RPC_QUANTITY')
    return int(value, 16)


def header(value):
    require(isinstance(value, dict), 'MARKET_RPC_HEADER')
    for name in ['hash', 'parentHash', 'stateRoot']:
        require(isinstance(value.get(name), str) and re.fullmatch(r'0x[0-9a-f]{64}', value[name]),
                'MARKET_RPC_HEADER_HASH')
    number, timestamp = quantity(value['number']), quantity(value['timestamp'])
    require(0 < number < 2**64 and 0 < timestamp < 2**64 // 1000, 'MARKET_RPC_HEADER_RANGE')
    return {k: value[k] for k in ['number', 'hash', 'parentHash', 'timestamp', 'stateRoot']}


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, *args, **kwargs):
        raise ValueError('MARKET_RPC_REDIRECT')


class RPC:
    def __init__(self, url, directory, allow_loopback=False):
        u = urllib.parse.urlsplit(url)
        require(u.hostname and not u.username and not u.password and not u.query and not u.fragment,
                'MARKET_RPC_PUBLIC_URL_REQUIRED')
        require(u.scheme == 'https' or (allow_loopback and u.scheme == 'http' and u.hostname == '127.0.0.1'),
                'MARKET_RPC_HTTPS_REQUIRED')
        self.url = url
        self.provider_id = 'rpc-' + digest(url.encode())
        self.directory = Path(directory)
        self.directory.mkdir()
        self.records = []
        self.bytes = 0
        self.deadline = time.monotonic() + 60

    def call(self, method, params):
        require(method in ALLOWED and len(self.records) < 32, 'MARKET_RPC_CALL_BUDGET')
        remaining = self.deadline - time.monotonic()
        require(remaining > 0, 'MARKET_RPC_TOTAL_DEADLINE')
        request = dict(jsonrpc='2.0', id=len(self.records) + 1, method=method, params=params)
        try:
            child = subprocess.run([sys.executable, str(Path(__file__).resolve()), '--rpc-read'],
                                   input=json.dumps(dict(url=self.url, request=request)).encode(),
                                   capture_output=True, timeout=remaining)
        except subprocess.TimeoutExpired:
            # subprocess.run kills AND reaps its child before raising.
            raise ValueError('MARKET_RPC_TOTAL_DEADLINE') from None
        require(child.returncode == 0 and len(child.stdout) <= 512 << 10, 'MARKET_RPC_TRANSPORT')
        transport = load(child.stdout)
        require(transport.get('error') is None, 'MARKET_RPC_' + transport.get('error', 'TRANSPORT'))
        raw = base64.b64decode(transport['body'], validate=True)
        require(len(raw) <= 256 << 10, 'MARKET_RPC_RESPONSE_BUDGET')
        self.bytes += len(raw)
        require(self.bytes <= 2 << 20, 'MARKET_RPC_TOTAL_BYTES')
        name = f'{len(self.records):02d}.json'
        (self.directory / name).write_bytes(raw)
        self.records.append(dict(request=request, response_file=name, response_sha256=digest(raw),
                                 http_status=transport['status']))
        require(transport['status'] == 200, 'MARKET_RPC_HTTP_' + str(transport['status']))
        value = load(raw)
        require(isinstance(value, dict) and value.get('jsonrpc') == '2.0'
                and type(value.get('id')) is int and value['id'] == request['id']
                and 'result' in value and 'error' not in value, 'MARKET_RPC_RESULT_REQUIRED')
        require(time.monotonic() <= self.deadline, 'MARKET_RPC_TOTAL_DEADLINE')
        return value['result']


def rpc_read_worker():
    """One owned subprocess lets the parent bound even slow-drip responses."""
    try:
        data = sys.stdin.buffer.read(65537)
        require(len(data) <= 65536, 'INPUT_LIMIT')
        value = load(data)
        require(value['request']['method'] in ALLOWED, 'METHOD_NOT_ALLOWED')
        req = urllib.request.Request(value['url'], data=json.dumps(value['request']).encode(),
                                     headers={'Content-Type': 'application/json'})
        opener = urllib.request.build_opener(NoRedirect())
        try:
            response = opener.open(req, timeout=8)
        except urllib.error.HTTPError as error:
            response = error
        with response:
            raw = response.read((256 << 10) + 1)
            result = dict(status=response.code, body=base64.b64encode(raw).decode())
    except BaseException as error:
        # Never serialize provider exception text or an endpoint URL.
        result = dict(error=type(error).__name__)
    print(json.dumps(result))


def collect(policy_path, rpc_url, validator, output, block_hash=None, allow_loopback=False):
    output = Path(output).resolve()
    output.mkdir(parents=True, exist_ok=False)
    result = dict(passed=False, authorized=False, production_ready=False,
                  publicTransactionsSubmitted=False, storage_proof_verified=False,
                  consensus_verified=False, source_independence_attested=False)
    rpc = None
    try:
        raw = Path(policy_path).read_bytes()
        require(len(raw) <= 65536, 'MARKET_POLICY_BYTES')
        p = load(raw)
        require(p['chain_key'] == 'evm:1' and p['genesis_hash'] == GENESIS,
                'MARKET_ONLY_APPROVED_ETHEREUM')
        feed = p['market_feed']
        (output / 'policy.json').write_bytes(raw)
        rpc = RPC(rpc_url, output / 'rpc', allow_loopback)
        result['provider_id'] = rpc.provider_id
        require(rpc.call('eth_chainId', []) == '0x1', 'MARKET_CHAIN_ID')
        genesis = rpc.call('eth_getBlockByNumber', ['0x0', False])
        require(genesis['hash'] == GENESIS and genesis['number'] == '0x0', 'MARKET_GENESIS')
        if block_hash is None:
            h = header(rpc.call('eth_getBlockByNumber', ['finalized', False]))
        else:
            require(re.fullmatch(r'0x[0-9a-f]{64}', block_hash), 'MARKET_BLOCK_HASH')
            h = header(rpc.call('eth_getBlockByHash', [block_hash, False]))
            require(h['hash'] == block_hash, 'MARKET_BLOCK_CHANGED')
        block = dict(blockHash=h['hash'], requireCanonical=True)

        def call(address, selector):
            require(re.fullmatch(r'0x[0-9a-f]{40}', address), 'MARKET_FEED_ADDRESS')
            return rpc.call('eth_call', [dict(to=address, data=selector, **{'from': ZERO},
                                              value='0x0', gas='0x1e8480'), block])

        def read(f):
            aggregator = call(f['proxy'], '0x245a7bfc')
            require(aggregator == '0x' + '0' * 24 + f['aggregator'][2:],
                    'MARKET_APPROVED_AGGREGATOR_CHANGED')
            return dict(proxy_code=rpc.call('eth_getCode', [f['proxy'], block]),
                        aggregator_code=rpc.call('eth_getCode', [f['aggregator'], block]),
                        aggregator_return=aggregator,
                        decimals_return=call(f['proxy'], '0x313ce567'),
                        description_return=call(f['proxy'], '0x7284e416'),
                        round_return=call(f['proxy'], '0xfeaf968c'))

        native, token = read(feed['native_usd']), read(feed['token_usd'])
        token_decimals = call(p['token'], '0x313ce567')
        require(header(rpc.call('eth_getBlockByNumber', [h['number'], False])) == h,
                'MARKET_CANONICAL_HEADER_CHANGED')
        at = quantity(h['timestamp']) * 1000
        read_at = int(time.time() * 1000)
        r = dict(schema_version=1, provider_id=rpc.provider_id, source_read_at_ms=read_at,
                 token_decimals_return=token_decimals, native_usd=native, token_usd=token)
        expiry = at + p['max_validity_ms']
        for name, data in [('native_usd', native), ('token_usd', token)]:
            w = data['round_return']
            require(isinstance(w, str) and re.fullmatch(r'0x[0-9a-f]{320}', w), 'MARKET_ROUND_ABI')
            expiry = min(expiry, int(w[194:258], 16) * 1000 + feed[name]['max_age_ms'])
        candidate = dict(policy=p, anchor=dict(chain_key='evm:1', genesis_hash=GENESIS,
                         block_number=quantity(h['number']), block_hash=h['hash'],
                         parent_hash=h['parentHash'], timestamp_ms=at), market_feed=r,
                         valid_until_ms=expiry)
        write(output / 'candidate.json', candidate)
        run = subprocess.run([str(Path(validator).resolve())], input=json.dumps(candidate).encode(),
                             capture_output=True, timeout=10)
        (output / 'validation.stdout').write_bytes(run.stdout)
        (output / 'validation.stderr').write_bytes(run.stderr)
        require(run.returncode == 0 and len(run.stdout) <= 1 << 20, 'MARKET_PRODUCT_VALIDATION_FAILED')
        unsigned = load(run.stdout)
        require(unsigned['authorized'] is False and unsigned['productionVerified'] is False,
                'MARKET_FALSE_AUTHORIZATION')
        write(output / 'unsigned.json', unsigned)
        result.update(passed=True, anchor=candidate['anchor'], validator_sha256=file_digest(validator),
                      policy_sha256=digest(raw), valid_until_ms=expiry,
                      rpc_finality_authority=('CONFIGURED_PROVIDER_FINALIZED_TAG_NOT_CONSENSUS'
                                              if block_hash is None else 'EXPLICIT_BLOCK_HASH_NO_FINALITY_CLAIM'))
    except BaseException as error:
        result['error'] = str(error) if isinstance(error, ValueError) else type(error).__name__
        raise
    finally:
        if rpc is not None:
            write(output / 'rpc' / 'index.json', dict(provider_id=rpc.provider_id,
                  records=rpc.records, total_response_bytes=rpc.bytes))
        result['artifacts'] = {str(p.relative_to(output)): file_digest(p)
                               for p in sorted(output.rglob('*')) if p.is_file()}
        write(output / 'report.json', result)
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ['policy', 'rpc-url', 'validator', 'output']:
        parser.add_argument('--' + name, required=True)
    parser.add_argument('--block-hash')
    args = parser.parse_args()
    result = collect(args.policy, args.rpc_url, args.validator, args.output, args.block_hash)
    print(json.dumps({k: result[k] for k in ['passed', 'authorized', 'provider_id', 'anchor']}))


if __name__ == '__main__':
    if sys.argv[1:] == ['--rpc-read']:
        rpc_read_worker()
    else:
        try:
            main()
        except Exception as error:
            print(json.dumps(dict(error=str(error) if isinstance(error, ValueError)
                                  else type(error).__name__)), file=sys.stderr)
            sys.exit(1)
