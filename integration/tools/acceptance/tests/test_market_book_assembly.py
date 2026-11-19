import copy
import json
import os
from pathlib import Path
import unittest
from unittest.mock import patch

import test_market_book_capture as capture_tests
import assemble_chainlink_book as assembly
from market_feed_vectors import sign_observation, digest


class AssemblyTests(unittest.TestCase):
    def test_signed_receipts_to_offline_candidate_then_explicit_advance(self):
        fixture = capture_tests.MarketBookTests()
        fixture.setUp()
        self.addCleanup(fixture.doCleanups)
        root = fixture.root
        bundle = fixture.run_book(observations_file='active.json', publication_journal='prices.journal')
        request = root/'result/signing-request.json'
        config, journal = root/'config.json', root/'prices.journal'
        cli = os.environ.get('XONE_PUBLICATION_CLI', Path(fixture.binary).with_name('xone-price-publication'))
        now = fixture.at + 2500
        prices = [sign_observation(r['observation']) for r in bundle['signing_requests']]
        old = [sign_observation(dict(p['observation'], publication_revision=6)) for p in prices]
        def operate(mode, entries):
            return assembly.run([cli, mode, journal], dict(policy=json.loads(config.read_bytes()),
                book=dict(schema_version=1, prices=entries), anchor=bundle['anchor'], execution_timestamp_ms=now))
        operate('initialize', old)
        original = journal.read_bytes()
        signatures = [dict(token=p['observation']['token'], **s) for p in prices for s in p['signatures']]
        response = dict(schema_version=1, request_sha256=assembly.digest(request.read_bytes()),
                        signatures=list(reversed(signatures)))

        def attempt(data, out='candidate-book.json', at=now):
            receipts = root/'receipts.json'
            receipts.write_text(json.dumps(data))
            with patch('time.time_ns', return_value=at * 1_000_000):
                return assembly.assemble(config, request, receipts, fixture.binary, cli, root/out)

        invalid = copy.deepcopy(signatures)
        invalid[0]['signature'] = signatures[-1]['signature']
        for data, out, at, code in [
            (dict(response, signatures=signatures[1:]), 'missing', now, 'NATIVE_VALUATION_AUTHORIZATION'),
            (dict(response, signatures=invalid), 'wrong', now, 'NATIVE_VALUATION_AUTHORIZATION'),
            (dict(response, signatures=signatures + signatures[:1]), 'duplicate', now, 'MARKET_ASSEMBLY_SIGNER'),
            (dict(response, request_sha256='00'*32), 'unbound', now, 'MARKET_ASSEMBLY_RECEIPT_REQUEST'),
            (response, 'expired', bundle['valid_until_ms'], 'MARKET_ASSEMBLY_TIME'),
            (response, 'active.json', now, 'MARKET_ASSEMBLY_ACTIVE_PATH'),
        ]:
            with self.subTest(out=out), self.assertRaisesRegex(ValueError, code):
                attempt(data, out, at)
            self.assertFalse((root/out).exists())
            self.assertEqual(journal.read_bytes(), original)
        preview = attempt(response)
        candidate = json.loads((root/'candidate-book.json').read_bytes())
        self.assertEqual(candidate, dict(schema_version=1, prices=prices))
        self.assertEqual(preview, dict(published=False, wouldAdvance=True, revision=7, bookHash=digest(candidate)))
        self.assertEqual(journal.read_bytes(), original)
        with self.assertRaises(FileExistsError):
            attempt(response)
        self.assertTrue(operate('advance', candidate['prices'])['receipt']['changed'])
        committed = journal.read_bytes()
        with self.assertRaisesRegex(ValueError, 'PRICE_PUBLICATION_ALREADY_CURRENT'):
            attempt(response, 'replay')
        self.assertEqual(journal.read_bytes(), committed)
        self.assertFalse((root/'replay').exists())


if __name__ == '__main__':
    unittest.main()
