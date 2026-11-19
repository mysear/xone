import base64
import json
from pathlib import Path
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from beacon_owner_archive import ACCOUNTS, PROVIDERS, audit_archive
from current_v2_evidence import sha


def encode(value):
    return json.dumps(value, separators=(',', ':')).encode()


class BeaconOwnerArchive(unittest.TestCase):
    def setUp(self):
        tmp = tempfile.TemporaryDirectory()
        self.addCleanup(tmp.cleanup)
        self.root = Path(tmp.name)
        self.block = dict(chain_key='evm:1', number_or_slot=7, hash='0x' + 'a' * 64)
        self.sources = list(PROVIDERS)

    def record(self, source, data, block=None):
        raw = encode(data)
        owner = dict(Chain='evm:1', Height=block['number_or_slot'] if block else 0,
                     Hash=block['hash'] if block else sha(raw), ProviderID=source)
        name = sha(encode(owner))
        ref = dict(ID=name, Digest=sha(raw), Owner=owner, Bytes=len(raw))
        payload = dict(Ref=ref, Data=base64.b64encode(raw).decode(), CapturedAt='2026-11-19T00:00:00Z')
        (self.root / (name + '.raw')).write_bytes(encode(dict(Digest=sha(encode(payload)), Payload=payload)))

    def rpc(self, source, method, params, **body):
        response = encode(dict(jsonrpc='2.0', id=1, **body))
        self.record(source, dict(source_kind='raw-json-rpc-response', method=method,
                                 params=params, response=base64.b64encode(response).decode()))

    def test_checks_both_block_bodies_and_proofs_at_requested_hash(self):
        for source in self.sources:
            self.rpc(source, 'eth_getBlockByNumber', ['0x7', True], result=dict(number='0x7', hash=self.block['hash']))
        for a in ACCOUNTS:
            self.rpc(self.sources[0], 'eth_getProof', [a, [], dict(blockHash=self.block['hash'], requireCanonical=True)],
                     result=dict(address=a, accountProof=['0xc0']))
        self.assertEqual(audit_archive(self.root, [self.block])['coverage_gaps'], [])
        other = dict(self.block, number_or_slot=8, hash='0x' + 'b' * 64)
        self.record('derived-state-from-recorded-rpc', dict(block=other), other)
        gaps = audit_archive(self.root, [self.block, other])['coverage_gaps']
        self.assertEqual(gaps, [dict(height=8, missing_accounts=sorted(ACCOUNTS),
                                     missing_body_providers=sorted(PROVIDERS), missing_staged_block=False)])

    def test_rpc_error_preserves_method_and_provider(self):
        self.rpc(self.sources[1], 'eth_getProof', ['address', [], {'blockHash': self.block['hash']}],
                 error=dict(code=-32000, message='unavailable'))
        result = audit_archive(self.root, [self.block]); error = result['rpc_errors'][0]
        self.assertEqual(error['rpc_error']['code'], -32000)
        self.assertEqual(error['provider'], PROVIDERS[self.sources[1]])
        self.assertEqual(error['method'], 'eth_getProof')
        self.assertEqual(result['coverage_gaps'][0]['missing_accounts'], sorted(ACCOUNTS))

    def test_derived_identity_must_match_archived_block(self):
        self.record('derived-state-from-recorded-rpc', dict(block=dict(self.block, number_or_slot=8)), self.block)
        with self.assertRaisesRegex(ValueError, 'STAGED_BLOCK_IDENTITY'):
            audit_archive(self.root, [self.block])

    def test_operator_config_binds_expected_sources_and_redacts_rpc_diagnostics(self):
        from verify_beacon_owner import GENESIS, execution_endpoints
        urls = ['https://one.example/private-key', 'https://two.example/other-key']
        config = self.root / 'config.json'
        def install(addresses, genesis):
            config.write_text(json.dumps({'chains': [dict(key='evm:1', genesis_hash=genesis, rpc_urls=addresses)]}))
        install(urls, GENESIS)
        self.assertEqual(execution_endpoints(config), tuple(urls))
        from current_v2_evidence import provider_id
        self.rpc(provider_id(urls[0]), 'eth_getProof', [], error=dict(code=15, message=urls[0]))
        report = audit_archive(self.root, [], urls)
        self.assertNotIn('private-key', json.dumps(report))
        self.assertEqual(report['rpc_errors'][0]['rpc_error'], {'code': 15})
        self.assertEqual(report['rpc_errors'][0]['provider'], provider_id(urls[0]))
        with self.assertRaisesRegex(ValueError, 'CAPTURE_PROVIDER'):
            audit_archive(self.root, [])
        for addresses, genesis, reason in ((urls, '', 'CHAIN'), ([urls[0]] * 2, GENESIS, 'WITNESSES'),
                                          (['http://one.example', urls[1]], GENESIS, 'URL')):
            install(addresses, genesis)
            with self.assertRaisesRegex(ValueError, reason):
                execution_endpoints(config)
