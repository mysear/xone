#!/usr/bin/env python3
"""Independent Fraction arithmetic and public TEST keys for approved feed tests."""
from fractions import Fraction
import hashlib
import json
from pathlib import Path
import sys
from native_valuation_vectors import policy, digest, Keypair

GENESIS = '0xd4e56740f876aef8c010b86a40d5f56745a118d0906a34e69aec8c0db1cb8fa3'


def word(n):
    return format(n, '064x')


def abi_string(s):
    b = s.encode()
    return '0x' + word(32) + word(len(b)) + b.hex() + '00' * ((-len(b)) % 32)


def public_test_policy():
    """Explicit test approval of the captured deployments, NOT production config.

    Never discover-and-trust an upgrade at collection time. New code/aggregators
    require a separately reviewed policy and new evidence qualification.
    """
    p = policy('evm:1', GENESIS, '0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48')
    p.update(source_id='PUBLIC_ETHEREUM_FEED_CAPTURE_TEST_AUTHORITY',
             max_age_ms=3600000, max_validity_ms=3600000)
    feeds = {}
    for name, proxy, proxy_hash, aggregator, aggregator_hash, description, age in [
        ('native_usd', '0x5f4ec3df9cbd43714fe2740f5e3616155c5b8419',
         'ed698309290de3517c7201fcad9a9dbd4b8cde4a72c9add23129201f299c6f2b',
         '0x7d4e742018fb52e48b08be73d041c18b21de6fb5',
         'ff1f27f82be30edc0bd127ba880e5ef5d50692c8251bde1b423aadf64e4db2a1', 'ETH / USD', 3600000),
        ('token_usd', '0x8fffffd4afb6115b954bd326cbe7b4ba576818f6',
         '53ca1c4d67c7b092f36c66e575874226de7e8eaada554f445f43fe2779743ec1',
         '0x54bcc589d9743e521c64233706fc8cb36d275b07',
         '19b83fc515a79d8ae4ffa84151b54a891914ed7dc2972fa162f70bf9c0b27a72', 'USDC / USD', 86400000),
    ]:
        feeds[name] = dict(proxy=proxy, proxy_code_sha256=proxy_hash, aggregator=aggregator,
                           aggregator_code_sha256=aggregator_hash, description=description,
                           decimals=8, min_answer='1', max_answer=str(10**17), max_age_ms=age)
    p['market_feed'] = dict(schema_version=1, native_decimals=18, token_decimals=6, **feeds)
    return p


def sign_observation(o):
    message = ('xone-native-token-price/1:' + digest(o)).encode()
    return dict(observation=o, signatures=[dict(key_id=str(i), signature=bytes(
        Keypair.from_seed(bytes([i])*32).sign_message(message)).hex()) for i in [1, 2]])


def make_signed(c):
    p, a, r = c['policy'], c['anchor'], c['market_feed']
    f = p['market_feed']
    n = int(r['native_usd']['round_return'][66:130], 16)
    t = int(r['token_usd']['round_return'][66:130], 16)
    rate = Fraction(n * 10**(f['token_usd']['decimals'] + f['token_decimals']),
                    t * 10**(f['native_usd']['decimals'] + f['native_decimals']))
    expected = dict(token_atoms=str(rate.numerator), native_atoms=str(rate.denominator),
                    overflow=max(rate.numerator.bit_length(), rate.denominator.bit_length()) > 256)
    if expected['overflow']:
        return None, expected
    o = dict(schema_version=1, policy_hash=digest(p), anchor=a, token=p['token'],
             token_atoms=str(rate.numerator), native_atoms=str(rate.denominator),
             observed_at_ms=a['timestamp_ms'], valid_until_ms=c['valid_until_ms'], market_feed=r)
    return sign_observation(o), expected


def synthetic(token_decimals=6, native_feed_decimals=8, token_feed_decimals=8,
              native_answer=200000000000, token_answer=100000000):
    at = 1700000000000
    a = dict(chain_key='evm:1', genesis_hash=GENESIS, block_number=10,
             block_hash='0x'+'22'*32, parent_hash='0x'+'33'*32, timestamp_ms=at)
    p = policy('evm:1', GENESIS, '0x'+'10'*20)
    p.update(max_age_ms=3600000, max_validity_ms=3600000)
    feeds, reads = {}, {}
    for index, (name, description, decimals, answer) in enumerate([
        ('native_usd', 'ETH / USD', native_feed_decimals, native_answer),
        ('token_usd', 'FIXTURE / USD', token_feed_decimals, token_answer),
    ]):
        code = bytes([0x60, index+1])
        proxy, aggregator = '0x'+format(index+1, '040x'), '0x'+format(index+3, '040x')
        feeds[name] = dict(proxy=proxy, proxy_code_sha256=hashlib.sha256(code).hexdigest(),
                           aggregator=aggregator, aggregator_code_sha256=hashlib.sha256(code).hexdigest(),
                           description=description, decimals=decimals, min_answer='1',
                           max_answer=str(2**255-1), max_age_ms=3600000)
        reads[name] = dict(proxy_code='0x'+code.hex(), aggregator_code='0x'+code.hex(),
                           aggregator_return='0x'+'00'*12+aggregator[2:],
                           decimals_return='0x'+word(decimals), description_return=abi_string(description),
                           round_return='0x'+''.join(word(v) for v in [7, answer, at//1000-2, at//1000-1, 7]))
    p['market_feed'] = dict(schema_version=1, native_decimals=18, token_decimals=token_decimals, **feeds)
    r = dict(schema_version=1, provider_id='rpc-'+'44'*32, source_read_at_ms=at+1000,
             token_decimals_return='0x'+word(token_decimals), **reads)
    return dict(policy=p, anchor=a, market_feed=r, valid_until_ms=at+60000)


def build(destination, public_directories=()):
    cases = []
    for td in [0, 6, 18, 36]:
        for nd, fd in [(0, 18), (8, 8), (18, 0)]:
            for n, d in [(200000000000, 100000000), (1, 3), (2**200+1, 2**199+3), (2**255-1, 1)]:
                c = synthetic(td, nd, fd, n, d)
                signed, expected = make_signed(c)
                conversions = []
                if not expected['overflow']:
                    for cost in [0, 1, 2**64-1, 2**256-1]:
                        v = (cost*int(expected['token_atoms'])+int(expected['native_atoms'])-1)//int(expected['native_atoms'])
                        conversions.append(dict(cost=str(cost), expected=str(v) if v < 2**256 else None))
                cases.append(dict(candidate=c, signed=signed, expected=expected, conversions=conversions))
    public = []
    for directory in public_directories:
        directory = Path(directory)
        report = json.loads((directory/'report.json').read_bytes())
        if report['passed'] is not True:
            raise ValueError('Successful real public collector result required')
        for n, h in report['artifacts'].items():
            if hashlib.sha256((directory/n).read_bytes()).hexdigest() != h:
                raise ValueError('Public capture bytes changed')
        c = json.loads((directory/'candidate.json').read_bytes())
        signed, expected = make_signed(c)
        unsigned = json.loads((directory/'unsigned.json').read_bytes())
        if unsigned['observation'] != signed['observation']:
            raise ValueError('Independent public ratio or product unsigned observation mismatch')
        public.append(dict(candidate=c, signed=signed, expected=expected,
                           capture_report_sha256=hashlib.sha256((directory/'report.json').read_bytes()).hexdigest()))
    value = dict(schema_version=1, synthetic=cases, public=public,
                 authority='PUBLIC_TEST_ED25519_KEYS_NOT_PRODUCTION_APPROVAL',
                 synthetic_feed_returns_are_not_market_prices=True)
    Path(destination).write_text(json.dumps(value, indent=2)+'\n')
    return value


if __name__ == '__main__':
    if sys.argv[1:2] == ['--write-public-test-policy']:
        Path(sys.argv[2]).write_text(json.dumps(public_test_policy(), indent=2)+'\n')
    else:
        value = build(sys.argv[1], sys.argv[2:])
        print(json.dumps(dict(synthetic=len(value['synthetic']), public=len(value['public']))))
