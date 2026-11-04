import sys
import unittest
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import curve_crypto_ramp_smoke as m

class CryptoRampRPC(unittest.TestCase):
    def test_ramp_origin_and_future_endpoints_share_the_same_canonical_bank(self):
        chain = m.Chain()
        for phase in range(14):
            chain.advance(phase)
            bank = chain.banks[chain.head]
            anchor = {'blockHash': bank['hash'], 'requireCanonical': True}
            for item in chain.items:
                p = item['pool']
                c = p['multi_asset']['curve_crypto']
                r = c['ramp']
                read = lambda slot: int(chain.rpc('eth_getStorageAt', [p['pool_id'], hex(slot), anchor]), 16)
                self.assertEqual(read(5), (int(r['initial_amp']) << 128) | int(r['initial_gamma']))
                self.assertEqual(read(6), (int(p['multi_asset']['amplification']) << 128) | int(c['gamma']))
                self.assertEqual(read(7), r['initial_time'])
                self.assertEqual(read(8), c['future_a_gamma_time'])
                self.assertEqual(c['block_hash'], anchor['blockHash'])

    def test_origin_faults_change_the_actual_read_dependency(self):
        chain = m.Chain()
        for fault, slot in [('ramp-origin-future', 7), ('ramp-origin-domain', 5), ('ramp-origin-width', 7), ('ramp-end-width', 8), ('ramp-end-before-origin', 8)]:
            chain.advance(0)
            p = chain.items[0]['pool']
            normal = chain.rpc('eth_getStorageAt', [p['pool_id'], hex(slot), {'blockHash': p['multi_asset']['curve_crypto']['block_hash'], 'requireCanonical': True}])
            chain.advance(0, fault)
            p = chain.items[0]['pool']
            altered = chain.rpc('eth_getStorageAt', [p['pool_id'], hex(slot), {'blockHash': p['multi_asset']['curve_crypto']['block_hash'], 'requireCanonical': True}])
            self.assertNotEqual(normal, altered, fault)
        chain.advance(0, 'ramp-origin-missing')
        p = chain.items[0]['pool']
        with self.assertRaises(ValueError):
            chain.rpc('eth_getStorageAt', [p['pool_id'], '0x5', {'blockHash': p['multi_asset']['curve_crypto']['block_hash'], 'requireCanonical': True}])

    def test_future_head_fault_changes_header_clock_and_recovers_to_original_bank(self):
        chain = m.Chain()
        chain.advance(13, 'future-head')
        self.assertGreater(int(chain.block(chain.head)['timestamp'], 16), int(m.base.time.time()) + 30)
        original_time = chain.timestamp
        chain.advance(13)
        self.assertEqual(int(chain.block(chain.head)['timestamp'], 16), original_time)

    def test_service_fault_recovery_cannot_rewind_the_original_bank_clock(self):
        chain = m.Chain()
        chain.advance(13)
        for fault in m.FAULTS:
            chain.advance(13, fault)
        with self.assertRaises(ValueError):
            chain.advance(0)

    def test_new_origin_reads_reject_unknown_or_unpinned_banks(self):
        chain = m.Chain()
        p = chain.items[0]['pool']
        for anchor in ['latest', {'blockHash': p['multi_asset']['curve_crypto']['block_hash']}, {'blockHash': '0x' + '0' * 64, 'requireCanonical': True}]:
            with self.assertRaises(ValueError):
                chain.rpc('eth_getStorageAt', [p['pool_id'], '0x5', anchor])

if __name__ == '__main__':
    unittest.main()
