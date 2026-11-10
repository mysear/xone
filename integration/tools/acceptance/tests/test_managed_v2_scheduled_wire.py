"""Discovery entry for the independent scheduled wire contract (no native proof)."""
import sys
sys.dont_write_bytecode = True
import copy
import hashlib
import importlib.util
import json
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[4]
SCRIPT = ROOT / 'integration/tools/ci/managed-v2-scheduled-wire-golden.py'


def codec():
    spec = importlib.util.spec_from_file_location('scheduled_wire', SCRIPT)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class ScheduledWireTests(unittest.TestCase):
    def test_four_discovered_contract_copies(self):
        paths = [ROOT / p / 'test/contracts/managed-v2-scheduled-wire.json'
                 for p in ('integration', 'chain-parser-go', 'chain-parser-rust', 'dex-router')]
        data = [p.read_bytes() for p in paths]
        self.assertTrue(all(b == data[0] for b in data))
        codec().validate_fixture(json.loads(data[0]))

    def test_raw_words_are_nested_big_endian_amounts(self):
        c = codec()
        s = c.build_snapshot()
        raw = c.encode_protobuf(s)
        fields = c.legacy._test_wire_fields
        managed = fields(fields(raw)[2][0])[28][0]  # Pool.multi_asset
        managed = fields(managed)[13][0]  # MultiAssetState.managed_v2
        schedule = fields(managed)[40][0]
        f = fields(schedule)
        expected = s['pools'][0]['multi_asset']['managed_v2']['schedule']
        self.assertEqual(fields(f[1][0])[1][0], int(expected['pool_state_word']).to_bytes(32, 'big'))
        for token, row in zip(f[2], expected['tokens']):
            self.assertEqual(fields(token)[1][0].decode(), row['token'])
            self.assertEqual(fields(fields(token)[2][0])[1][0], int(row['state_word']).to_bytes(32, 'big'))

    def test_required_nullable_amounts_and_presence(self):
        c = codec()
        for path in [('pool_state_word',), ('tokens',), ('tokens', 0, 'token'), ('tokens', 0, 'state_word')]:
            for null in (False, True):
                s = c.build_snapshot()
                obj = s['pools'][0]['multi_asset']['managed_v2']['schedule']
                for key in path[:-1]:
                    obj = obj[key]
                if null:
                    obj[path[-1]] = None
                else:
                    del obj[path[-1]]
                with self.assertRaises(ValueError):
                    c.encode_protobuf(s)

    def test_bad_words_order_target_and_getter(self):
        c = codec()
        for kind in range(8):
            s = c.build_snapshot()
            p = s['pools'][0]
            m = p['multi_asset']['managed_v2']
            schedule = m['schedule']
            if kind == 0: schedule['pool_state_word'] = '0'
            elif kind == 1: schedule['tokens'][0]['state_word'] = '0'
            elif kind == 2: schedule['tokens'][0]['state_word'] = str(int(schedule['tokens'][0]['state_word']) | (1 << 133))
            elif kind == 3: schedule['tokens'].reverse()
            elif kind == 4: schedule['tokens'][1]['token'] = schedule['tokens'][0]['token']
            elif kind == 5: schedule['tokens'][0]['state_word'] = str(int(schedule['tokens'][0]['state_word']) + (1 << 64))
            elif kind == 6: p['multi_asset']['weights'][0] = str(int(p['multi_asset']['weights'][0]) - 2)
            else: m['execution_context'] = {'timestamp': m['timestamp'], 'block_number': m['block_number']}
            with self.subTest(kind=kind), self.assertRaises(ValueError): c.encode_protobuf(s)

    def test_profile_header_and_missing_field40(self):
        c = codec()
        for kind in range(4):
            s = c.build_snapshot()
            p = s['pools'][0]
            if kind == 0: p['protocol'] = 'balancer-v2-managed-aum'
            elif kind == 1: del p['multi_asset']['managed_v2']['schedule']
            elif kind == 2: s['reference']['block']['timestamp_ms'] += 1000
            else: p['adapter_version'] = 'balancer-v2/20230411-managed-aum-3'
            with self.assertRaises(ValueError): c.encode_protobuf(s)

    def test_runtime_encode_accepts_service_source_and_keeps_known_zero(self):
        c = codec()
        s = c.build_snapshot()
        s['source'] = 'CURRENT_CHAIN_STATE'
        s['reference']['owner_service'] = 'test'
        text = c.encode(s)
        self.assertIn('paused: false', text)
        self.assertIn('gradual_fee_end: 0', text)
        self.assertIn('source: "CURRENT_CHAIN_STATE"', text)
        self.assertEqual(c.legacy._protoc('decode', c.legacy._protoc('encode', text.encode())).count(b'schedule {'), 1)

    def test_digest_drift_and_check_never_write(self):
        c = codec()
        fixture = c.build_fixture()
        bad = copy.deepcopy(fixture)
        bad['protobuf_hex'] = '00' + bad['protobuf_hex'][2:]
        bad['protobuf_sha256'] = hashlib.sha256(bytes.fromhex(bad['protobuf_hex'])).hexdigest()
        with self.assertRaises(ValueError): c.validate_fixture(bad)
        with tempfile.TemporaryDirectory() as temp:
            paths = [Path(temp) / str(i) / c.FILENAME for i in range(4)]
            for p in paths: p.parent.mkdir()
            c.legacy.publish(b'correct', paths)
            before = [(p.read_bytes(), p.stat().st_mtime_ns) for p in paths]
            c.legacy.publish(b'correct', paths, check=True)
            with self.assertRaises(ValueError): c.legacy.publish(b'drift', paths, check=True)
            self.assertEqual(before, [(p.read_bytes(), p.stat().st_mtime_ns) for p in paths])

    def test_check_cli_readonly_normal_and_optimized(self):
        paths = [ROOT / p / 'test/contracts/managed-v2-scheduled-wire.json'
                 for p in ('integration', 'chain-parser-go', 'chain-parser-rust', 'dex-router')]
        before = [(p.read_bytes(), p.stat().st_mtime_ns) for p in paths]
        for flags in ([], ['-OO']):
            subprocess.run([sys.executable, *flags, str(SCRIPT), '--check'], check=True, capture_output=True)
        self.assertEqual(before, [(p.read_bytes(), p.stat().st_mtime_ns) for p in paths])


if __name__ == '__main__': unittest.main()
