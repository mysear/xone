import json
import os
from pathlib import Path
import unittest
from unittest.mock import patch

import test_market_book_capture as capture_tests
import schedule_chainlink_book as schedule
from assemble_chainlink_book import run
from market_feed_vectors import sign_observation, digest


class RoundTests(unittest.TestCase):
    def setUp(self):
        f = self.fixture = capture_tests.MarketBookTests()
        f.setUp()
        self.addCleanup(f.doCleanups)
        self.now = f.at + 2000
        self.base_config = f.root/'config.json'
        self.root = f.root/'schedule'
        self.cli = os.environ.get('XONE_PUBLICATION_CLI', Path(f.binary).with_name('xone-price-publication'))
        initial = f.run_book(revision=6, publication_journal='journal', observations_file='active.json')
        self.anchor = initial['anchor']
        self.active = f.root/'active.json'
        self.active.write_text(json.dumps(dict(schema_version=1, prices=[sign_observation(r['observation'])
                                    for r in initial['signing_requests']])))
        self.commands = []
        self.crash = False
        self.drop_apply = False
        self.update('initialize')

    def update(self, mode):
        r = run([self.cli, mode, self.fixture.root/'journal'], dict(policy=json.loads(self.base_config.read_bytes()),
                book=json.loads(self.active.read_bytes()), anchor=self.anchor, execution_timestamp_ms=self.now))['receipt']
        self.current = dict(priceBookHash=r['high_water']['book_hash'], publicationRevision=r['high_water']['revision'],
                            bankAnchor=r['high_water']['anchor'], bankConsensusAttested=False,
                            journalSequence=r['sequence'], journalEntryHash=r['entry_hash'], fsyncConfirmed=True)

    def call(self, operation, **fields):
        self.commands.append(operation)
        if operation == 'status':
            return self.current.copy()
        value = json.loads(self.active.read_bytes())
        if operation == 'preview':
            return dict(candidateBookHash=digest(value), bankAnchor=self.anchor, candidateRevision=value['prices'][0]['observation']['publication_revision'],
                        baseEntryHash=self.current['journalEntryHash'], published=False, wouldAdvance=True)
        self.assertEqual(operation, 'apply')
        self.assertEqual(fields, dict(baseEntryHash=self.current['journalEntryHash'], candidateBookHash=digest(value)))
        if self.drop_apply:
            raise TimeoutError()
        self.update('advance')
        if self.crash:
            raise SystemExit('simulated process interruption after actual journal advance')
        return dict(self.current, changed=True)

    def step(self, block_hash=None):
        f = self.fixture
        with patch.object(schedule, 'capture_book', side_effect=lambda *args, **kwargs: capture_tests.book.capture_book(*args, allow_loopback=True, **kwargs)), \
             patch('time.time', return_value=(f.at + 1000)/1000), patch('time.time_ns', return_value=self.now*1_000_000):
            return schedule.step(self.root, self.base_config, f.url, f.binary, self.cli, 'fixture-router', self.call, block_hash)

    def sign(self, revision):
        directory = self.root/f'round-{revision}'
        raw = (directory/'capture/signing-request.json').read_bytes()
        signatures = []
        for row in json.loads(raw)['signing_requests']:
            o = row['observation']
            signatures.extend(dict(s, token=o['token']) for s in sign_observation(o)['signatures'])
        receipts = directory/'receipts.json'
        receipts.write_text(json.dumps(dict(schema_version=1, request_sha256=schedule.digest(raw), signatures=signatures)))
        return receipts

    def test_interrupted_publication_recovers_by_status_only(self):
        self.assertEqual(self.step()['phase'], 'WAITING')
        self.assertEqual(self.step()['phase'], 'WAITING')
        self.sign(7)
        self.crash = True
        with self.assertRaises(SystemExit):
            self.step()
        self.assertEqual(json.loads((self.root/'state.json').read_bytes())['phase'], 'PUBLISHING')
        self.assertEqual(self.current['publicationRevision'], 7)
        self.commands.clear()
        s = self.step()
        self.assertEqual((s['phase'], s['confirmed'], self.commands), ('CONFIRMED', self.current, ['status']))

    def test_bank_is_selected_by_hash_and_checked_again_before_publication(self):
        self.fixture.calls.clear()
        with self.assertRaisesRegex(ValueError, 'MARKET_SCHEDULE_BLOCK_CONFLICT'):
            self.step('0x' + 'ff'*32)
        self.assertEqual(self.fixture.calls, [])
        self.assertEqual(self.step()['phase'], 'WAITING')
        self.assertEqual(self.fixture.calls.count(('eth_getBlockByHash', [self.anchor['block_hash'], False])), 2)
        self.assertNotIn(('eth_getBlockByNumber', ['finalized', False]), self.fixture.calls)
        self.sign(7)
        old = self.active.read_bytes()
        self.current['bankAnchor'] = dict(self.anchor, parent_hash='0x'+'aa'*32)
        self.assertEqual(self.step()['phase'], 'SUPERSEDED')
        self.assertEqual(self.active.read_bytes(), old)
        self.assertNotIn('apply', self.commands)

    def test_missing_authority_or_mismatched_header_never_reaches_signer(self):
        self.fixture.calls.clear()
        del self.current['bankAnchor']
        with self.assertRaisesRegex(ValueError, 'MARKET_SCHEDULE_BANK_ANCHOR'):
            self.step()
        self.assertFalse(self.fixture.calls)
        self.current['bankAnchor'] = dict(self.anchor, parent_hash='0x'+'aa'*32)
        with self.assertRaisesRegex(ValueError, 'MARKET_BOOK_BANK_ANCHOR'):
            self.step()
        self.assertFalse((self.root/'round-7/capture/signing-request.json').exists())

    def test_expiry_recaptures_but_old_receipts_cannot_authorize_new_revision(self):
        first = self.step()
        old = self.sign(first['revision'])
        self.now += 3_600_000
        self.assertEqual(self.step()['phase'], 'EXPIRED')
        self.now = self.fixture.at + 2500  # next synthetic capture is fresh at the fixture clock
        second = self.step()
        self.assertEqual((second['phase'], second['revision']), ('WAITING', 8))
        (self.root/'round-8/receipts.json').write_bytes(old.read_bytes())
        with self.assertRaisesRegex(ValueError, 'MARKET_ASSEMBLY_RECEIPT_REQUEST'):
            self.step()
        self.assertNotIn('apply', self.commands)
        self.sign(8)
        self.assertEqual(self.step()['phase'], 'CONFIRMED')
        self.assertEqual(self.commands.count('apply'), 1)

    def test_unknown_delivery_never_replays_or_allocates_a_new_round(self):
        self.step()
        self.sign(7)
        self.drop_apply = True
        with self.assertRaisesRegex(ValueError, 'MARKET_PUBLISH_OUTCOME_UNKNOWN'):
            self.step()
        self.commands.clear()
        for _ in range(2):
            with self.assertRaisesRegex(ValueError, 'MARKET_PUBLISH_OUTCOME_UNKNOWN'):
                self.step()
        self.assertEqual(self.commands, ['status', 'status'])
        self.assertFalse((self.root/'round-8').exists())
        self.assertEqual(json.loads((self.root/'state.json').read_bytes())['phase'], 'PUBLISHING')


if __name__ == '__main__':
    unittest.main()
