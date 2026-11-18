#!/usr/bin/env python3
"""Independent Python integers and pinned Solders Ed25519 oracle fixtures.

Fixed public test keys are used only here. These signatures prove an explicitly
configured fixture authority, not a live market price or independent sources.
"""
import hashlib
import json
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / 'integration/artifacts/local/r01-solana-rebuild/python'))
from solders.keypair import Keypair


def digest(value):
    raw = json.dumps(value, sort_keys=True, separators=(',', ':'), ensure_ascii=False)
    raw = raw.replace('<', '\\u003c').replace('>', '\\u003e').replace('&', '\\u0026').replace('\u2028', '\\u2028').replace('\u2029', '\\u2029')
    return hashlib.sha256(raw.encode()).hexdigest()


def policy(chain, genesis, token):
    return dict(source_id='PINNED_TEST_ORACLE_NOT_LIVE_MARKET', chain_key=chain, genesis_hash=genesis, token=token,
                signers={str(i): bytes(Keypair.from_seed(bytes([i])*32).pubkey()).hex() for i in [1, 2, 3]},
                threshold=2, max_age_ms=60000, max_validity_ms=60000)


def sign(policy, anchor, numerator, denominator, observed=None, until=None, *, revision=0):
    timestamp=anchor['timestamp_ms']
    observation=dict(schema_version=1, policy_hash=digest(policy), anchor=anchor, token=policy['token'],
                     token_atoms=str(numerator), native_atoms=str(denominator),
                     observed_at_ms=timestamp if observed is None else observed,
                     valid_until_ms=timestamp+60000 if until is None else until)
    if revision:observation['publication_revision']=revision
    message=('xone-native-token-price/1:'+digest(observation)).encode()
    return dict(observation=observation, signatures=[dict(key_id=str(i), signature=bytes(Keypair.from_seed(bytes([i])*32).sign_message(message)).hex()) for i in [1,2]])


def vectors():
    anchor=dict(chain_key='evm:31337',genesis_hash='0x'+'11'*32,block_number=10,block_hash='0x'+'22'*32,parent_hash='0x'+'33'*32,timestamp_ms=1700000000000)
    p=policy(anchor['chain_key'],anchor['genesis_hash'],'0x'+'00'*19+'10')
    rows=[]
    for cost in [0,1,2,2**64-1,2**128,2**256-1]:
        for n,d in [(1,1),(1,3),(3,2),(2**256-1,2**256-1),(2**256-1,1),(1,2**256-1),(2**128+1,2**128-1)]:
            expected=(cost*n+d-1)//d
            rows.append(dict(cost=str(cost),signed=sign(p,anchor,n,d),expected=None if expected>=2**256 else str(expected),overflow=expected>=2**256))
    return dict(schema_version=1,scope='PINNED_TEST_ORACLE_SIGNATURES_AND_INDEPENDENT_INTEGER_COSTS_NOT_MARKET_PRICE',policy=p,anchor=anchor,cases=rows)


if __name__=='__main__':
    paths=[ROOT/'integration/test/contracts/native-valuation-vectors.json',ROOT/'dex-router/test/contracts/native-valuation-vectors.json']
    value=vectors();raw=json.dumps(value,indent=2)+'\n'
    if sys.argv[1:]==['--check']:
        for path in paths:
            if path.read_text()!=raw:raise ValueError('Independent native valuation vectors changed: '+str(path))
        print(json.dumps(dict(passed=True,cases=len(value['cases']),independent_signer='PINNED_SOLDERS_ED25519',oracle_scope=value['scope'])))
    else:
        for path in paths:path.write_text(raw)
