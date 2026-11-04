#!/usr/bin/env python3
"""Independent C++ protoc OnlineSnapshot golden for the frozen Managed V2 bank.

Default: authenticate, encode, validate, then replace all four wire copies.
--check: compare every copy without writing (including Python bytecode).
--self-test: run the embedded behavioral tests; outputs stay in /tmp.
The snapshot envelope is synthetic; it does not attest chain or execution trust.
"""
import sys
sys.dont_write_bytecode = True

from pathlib import Path
import argparse
import copy
import hashlib
import json
import os
import subprocess
import tempfile
import unittest
from unittest import mock

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / 'integration/tools/acceptance'))
import managed_v2_projection as projection

BANK_SHA256 = '1ee7ec5dd981994af6dd42eb75717cf83a5b209857883fa2923885ce7181fb66'
BANK_BYTES = 7905624
AUTHORITY = 'native-state + synthetic snapshot envelope'
SOURCE = 'SYNTHETIC_MANAGED_V2_AUM_WIRE_CONTRACT'
PROJECTS = ('integration', 'chain-parser-go', 'chain-parser-rust', 'dex-router')
FILENAME = 'managed-v2-aum-wire.json'
MANAGED_FIELDS = tuple('''timestamp block_number block_hash factory factory_code_sha256
pool_runtime_template_sha256 fee_provider fee_provider_code_sha256 protocol_collector
protocol_collector_code_sha256 dependencies total_supply virtual_supply actual_supply
pending_aum vault_bpt_cash vault_bpt_managed bpt_index manager manager_bpt_balance
protocol_collector_bpt_balance management_aum_fee last_collection_timestamp provider_fee_id
live_provider_aum_percentage cached_protocol_aum_percentage swaps_enabled paused recovery_mode
allowlist_enabled pause_window_end buffer_period_end gradual_weight_end gradual_fee_end token_info vault_bpt_actual_balance vault_paused vault_pause_window_end vault_buffer_period_end'''.split())
DEPENDENCY_FIELDS = ('role', 'address', 'code_sha256', 'template_sha256')
TOKEN_FIELDS = ('cash', 'managed', 'asset_manager', 'last_change_block', 'circuit_breaker_enabled', 'vault_actual_balance')


AMOUNTS = frozenset("""reserve0 reserve1 balance scaling weights amplification total_supply
virtual_supply actual_supply pending_aum vault_bpt_cash vault_bpt_managed manager_bpt_balance
protocol_collector_bpt_balance management_aum_fee provider_fee_id live_provider_aum_percentage
cached_protocol_aum_percentage cash managed vault_actual_balance vault_bpt_actual_balance""".split())
BOOLS = frozenset(('swaps_enabled', 'paused', 'recovery_mode', 'allowlist_enabled', 'vault_paused'))
CLOCKS = frozenset(('timestamp', 'block_number', 'bpt_index', 'last_collection_timestamp',
                    'pause_window_end', 'buffer_period_end', 'gradual_weight_end', 'gradual_fee_end', 'vault_pause_window_end', 'vault_buffer_period_end'))
POOL_FIELDS = tuple("""chain_key pool_id protocol token0 token1 reserve0 reserve1 fee_numerator
fee_denominator adapter_version token_behavior multi_asset""".split())
MULTI_FIELDS = tuple('code_sha256 vault vault_code_sha256 pool_id assets weights amplification admin_fee managed_v2'.split())
BLOCK_FIELDS = ('chain_key', 'number_or_slot', 'hash', 'parent_hash', 'timestamp_ms')
REFERENCE_FIELDS = tuple("""block canonical_epoch journal_seq schema_version adapter_set_hash
online_generation scope_hash writer_epoch owner_service owner_assignment_version""".split())


def require(condition, message):
    # Explicit checks also run under python -O / -OO.
    if not condition:
        raise ValueError(message)


def exact_keys(value, keys, label):
    require(type(value) is dict, label + ': object required')
    missing, extra = set(keys) - value.keys(), value.keys() - set(keys)
    require(not missing and not extra,
            label + ': missing=' + str(sorted(missing)) + ', extra=' + str(sorted(extra)))


def uint(value, bits, label):
    require(type(value) is int and 0 <= value < 1 << bits, label + ': uint' + str(bits) + ' required')


def amount(value, label):
    require(type(value) is str and value.isascii() and value.isdecimal()
            and str(int(value)) == value and int(value) < 1 << 256, label + ': canonical uint256 required')


def digest_bytes(value, label):
    require(type(value) is str, label + ': digest string required')
    text = value.removeprefix('0x')
    require(len(text) == 64 and all(c in '0123456789abcdef' for c in text), label + ': canonical 32-byte hex required')
    return bytes.fromhex(text)


def address(value, label):
    require(type(value) is str and value.startswith('0x') and len(value) == 42
            and all(c in '0123456789abcdef' for c in value[2:]), label + ': canonical address required')


def array_hash(values):
    # Domain scope_hash sorts pool IDs; service adapter_set_hash sorts versions.
    return hashlib.sha256(projection.canonical(sorted(values))).hexdigest()


def load_source():
    require(projection.BANK_SHA256 == BANK_SHA256 and projection.BANK_BYTES == BANK_BYTES,
            'projection original bank identity changed')
    bank = projection.load(ROOT / 'integration/test/contracts/managed-v2-aum.json')
    before = bank['cases'][0]['execution']['before']
    projected = projection.project_snapshot(bank, before)
    header = projection.resolve(bank, projected['source_refs']['state_header'], 'header')
    chain_id = bank['reference']['chain_options']['chain']['chainId']
    uint(chain_id, 64, 'original chainId')
    return {'projection': projected, 'header': header, 'chain_key': 'evm:' + str(chain_id)}


def build_snapshot(context):
    pool = copy.deepcopy(context['projection']['pool'])
    header, chain = context['header'], context['chain_key']
    m = pool['multi_asset']['managed_v2']
    require(pool['chain_key'] == chain, 'original chain mismatch')
    require(m['block_number'] == int(header['number'], 16) and m['block_hash'] == header['hash']
            and m['timestamp'] == int(header['timestamp'], 16), 'original header/Pool mismatch')
    # Epochs, assignment and source are explicitly synthetic; only block/Pool are
    # bound to the authenticated local native observation. No baseline is read.
    snapshot = {'reference': {
        'block': {'chain_key': chain, 'number_or_slot': int(header['number'], 16),
                  'hash': header['hash'], 'parent_hash': header['parentHash'],
                  'timestamp_ms': int(header['timestamp'], 16) * 1000},
        'canonical_epoch': 1, 'journal_seq': 1, 'schema_version': 1,
        'adapter_set_hash': array_hash([pool['adapter_version']]), 'online_generation': 1,
        'scope_hash': array_hash([pool['pool_id']]), 'writer_epoch': 1,
        'owner_service': 'chain-parser-go', 'owner_assignment_version': 1},
        'pools': [pool], 'source': SOURCE, 'history_gaps': []}
    validate_snapshot(snapshot)
    return snapshot


def validate_snapshot(snapshot):
    """Reject incomplete public state before proto3 can erase unknown values."""
    exact_keys(snapshot, ('reference', 'pools', 'source', 'history_gaps'), 'snapshot')
    require(snapshot['source'] == SOURCE and snapshot['history_gaps'] == [], 'synthetic envelope required')
    ref = snapshot['reference']
    exact_keys(ref, REFERENCE_FIELDS, 'reference')
    block = ref['block']
    exact_keys(block, BLOCK_FIELDS, 'reference.block')
    require(type(snapshot['pools']) is list and len(snapshot['pools']) == 1, 'exactly one original Pool required')
    pool = snapshot['pools'][0]
    exact_keys(pool, POOL_FIELDS, 'Pool')
    multi = pool['multi_asset']
    exact_keys(multi, MULTI_FIELDS, 'multi_asset')
    managed = multi['managed_v2']
    exact_keys(managed, MANAGED_FIELDS, 'managed_v2 (39 fields)')
    require(pool['chain_key'] == block['chain_key'], 'snapshot chain mismatch')
    require(pool['protocol'] == projection.PROTOCOL and pool['adapter_version'] == projection.ADAPTER,
            'Managed V2 profile/adapter required')
    for name in ('hash', 'parent_hash'):
        digest_bytes(block[name], 'block.' + name)
    for name in ('number_or_slot', 'timestamp_ms'):
        uint(block[name], 64, 'block.' + name)
    for name in REFERENCE_FIELDS:
        if name not in ('block', 'adapter_set_hash', 'scope_hash', 'owner_service'):
            uint(ref[name], 32 if name == 'schema_version' else 64, 'reference.' + name)
    require(ref['schema_version'] == 1, 'schema version required')
    for name in ('canonical_epoch', 'journal_seq', 'online_generation', 'writer_epoch', 'owner_assignment_version'):
        require(ref[name] > 0, 'synthetic ' + name + ' must be positive')
    require(ref['owner_service'] == 'chain-parser-go', 'synthetic owner required')
    require(ref['adapter_set_hash'] == array_hash([pool['adapter_version']])
            and ref['scope_hash'] == array_hash([pool['pool_id']]), 'canonical array hash mismatch')
    for name in ('pool_id', 'token0', 'token1'):
        address(pool[name], 'Pool.' + name)
    for name in ('reserve0', 'reserve1'):
        amount(pool[name], 'Pool.' + name)
    for name in ('fee_numerator', 'fee_denominator'):
        uint(pool[name], 64, 'Pool.' + name)
    require(pool['token_behavior'] == 'standard', 'standard token behavior required')
    digest_bytes(multi['code_sha256'], 'multi_asset.code_sha256')
    digest_bytes(multi['vault_code_sha256'], 'multi_asset.vault_code_sha256')
    digest_bytes(multi['pool_id'], 'multi_asset.pool_id')
    address(multi['vault'], 'multi_asset.vault')
    amount(multi['amplification'], 'multi_asset.amplification')
    uint(multi['admin_fee'], 64, 'multi_asset.admin_fee')
    assets = multi['assets']
    require(type(assets) is list and 2 <= len(assets) <= 50, 'underlying assets required')
    require(type(multi['weights']) is list and len(multi['weights']) == len(assets), 'complete weights required')
    for weight in multi['weights']:
        amount(weight, 'weight')
    for asset in assets:
        exact_keys(asset, ('token', 'balance', 'scaling', 'code_sha256'), 'asset')
        address(asset['token'], 'asset.token')
        amount(asset['balance'], 'asset.balance')
        amount(asset['scaling'], 'asset.scaling')
        digest_bytes(asset['code_sha256'], 'asset.code_sha256')
    for name, value in managed.items():
        if name in AMOUNTS:
            amount(value, 'managed_v2.' + name)
        elif name in BOOLS:
            require(type(value) is bool, 'managed_v2.' + name + ': bool required')
        elif name in CLOCKS:
            uint(value, 32 if name in ('bpt_index', 'last_collection_timestamp') else 64, 'managed_v2.' + name)
        elif name.endswith('_sha256') or name == 'block_hash':
            digest_bytes(value, 'managed_v2.' + name)
        elif name not in ('dependencies', 'token_info'):
            address(value, 'managed_v2.' + name)
    require(managed['block_number'] == block['number_or_slot'] and managed['block_hash'] == block['hash']
            and managed['timestamp'] * 1000 == block['timestamp_ms'], 'snapshot block binding mismatch')
    deps = managed['dependencies']
    require(type(deps) is list and len(deps) == 5, 'all five dependencies required')
    for dep in deps:
        exact_keys(dep, DEPENDENCY_FIELDS, 'dependency')
        require(type(dep['role']) is str, 'dependency role required')
        address(dep['address'], 'dependency.address')
        digest_bytes(dep['code_sha256'], 'dependency.code_sha256')
        digest_bytes(dep['template_sha256'], 'dependency.template_sha256')
    require(sorted(d['role'] for d in deps) == sorted(projection.DEPENDENCIES.values()), 'exact dependency roles required')
    infos = managed['token_info']
    require(type(infos) is list and len(infos) == len(assets), 'complete token_info required')
    for info in infos:
        exact_keys(info, TOKEN_FIELDS, 'token_info')
        amount(info['cash'], 'token_info.cash')
        amount(info['managed'], 'token_info.managed')
        amount(info['vault_actual_balance'], 'token_info.vault_actual_balance')
        address(info['asset_manager'], 'token_info.asset_manager')
        uint(info['last_change_block'], 64, 'token_info.last_change_block')
        require(type(info['circuit_breaker_enabled']) is bool, 'token_info circuit breaker bool required')


def _text_message(obj, level=''):
    def blob(raw):
        return '"' + ''.join('\\x%02x' % byte for byte in raw) + '"'
    out = []
    for name, value in obj.items():
        for item in value if isinstance(value, list) else [value]:
            if item is None:
                continue
            if isinstance(item, dict):
                out.append(name + ' { ' + _text_message(item, name) + ' }')
            elif name in AMOUNTS:
                out.append(name + ' { uint256_be: ' + blob(int(item).to_bytes(32, 'big')) + ' }')
            elif name in ('hash', 'parent_hash', 'block_hash', 'adapter_set_hash', 'scope_hash') or name.endswith('_sha256') or (name == 'pool_id' and level == 'multi_asset'):
                out.append(name + ': ' + blob(digest_bytes(item, name)))
            else:
                out.append(name + ': ' + json.dumps(item, ensure_ascii=True, allow_nan=False))
    return ' '.join(out)


def _protoc(mode, payload):
    proto_root = ROOT / 'common/protoc/proto'
    result = subprocess.run(['protoc', '-I', str(proto_root), '--' + mode + '=dex.v1.OnlineSnapshot',
                             str(proto_root / 'dex/v1/state.proto')], input=payload,
                            stdout=subprocess.PIPE, stderr=subprocess.PIPE, check=False)
    require(result.returncode == 0, 'C++ protoc ' + mode + ' failed: ' + result.stderr.decode(errors='replace'))
    return result.stdout


def encode(snapshot, level=''):
    """Return protobuf text for an OnlineSnapshot or nested message.

    This public adapter accepts runtime envelopes without imposing golden-only
    source/owner/generation constraints. Feed ``encode(snapshot).encode()`` to
    C++ protoc --encode=dex.v1.OnlineSnapshot. Amounts use 32-byte uint256_be;
    digest fields use raw 32 bytes; None is omitted but false/0 are retained.
    Full golden qualification remains in encode_protobuf/build_fixture.
    """
    return _text_message(snapshot, level)


def encode_protobuf(snapshot):
    validate_snapshot(snapshot)
    return _protoc('encode', encode(snapshot).encode())


def build_fixture(context):
    snapshot = build_snapshot(context)
    raw = encode_protobuf(snapshot)
    fixture = {'snapshot': snapshot, 'protobuf_hex': raw.hex(),
               'protobuf_sha256': hashlib.sha256(raw).hexdigest(),
               'original_bank_sha256': BANK_SHA256, 'original_bank_bytes': BANK_BYTES,
               'authority': AUTHORITY,
               'source_refs': copy.deepcopy(context['projection']['source_refs']),
               'original_fields': copy.deepcopy(context['projection']['pool'])}
    validate_fixture(fixture, context)
    return fixture


def validate_fixture(fixture, context):
    exact_keys(fixture, ('snapshot', 'protobuf_hex', 'protobuf_sha256', 'original_bank_sha256',
                        'original_bank_bytes', 'authority', 'source_refs', 'original_fields'), 'wire fixture')
    for name, expected in (('authority', AUTHORITY), ('original_bank_sha256', BANK_SHA256),
                           ('original_bank_bytes', BANK_BYTES),
                           ('source_refs', context['projection']['source_refs']),
                           ('original_fields', context['projection']['pool']),
                           ('snapshot', build_snapshot(context))):
        require(projection.canonical(fixture[name]) == projection.canonical(expected), name + ' mismatch')
    text = fixture['protobuf_hex']
    require(type(text) is str and len(text) % 2 == 0 and all(c in '0123456789abcdef' for c in text),
            'canonical protobuf_hex required')
    raw = bytes.fromhex(text)
    require(hashlib.sha256(raw).hexdigest() == fixture['protobuf_sha256'], 'protobuf_sha256 mismatch')
    require(raw == encode_protobuf(fixture['snapshot']), 'protobuf bytes differ from complete snapshot')
    # C++ parsing plus canonical re-encoding rejects malformed/trailing bytes.
    require(_protoc('encode', _protoc('decode', raw)) == raw, 'C++ protobuf roundtrip mismatch')


def publish(data, outputs, *, check=False):
    """Stage every file first, then use per-file atomic os.replace publication.

    Four filesystem paths do not form one atomic transaction. All generation,
    validation and staging finish before replacement; a filesystem failure
    during replacement is reported and may leave a mix of old/new copies.
    """
    outputs = [Path(p) for p in outputs]
    require(len(outputs) == 4 and len({p.resolve() for p in outputs}) == 4, 'four distinct output copies required')
    for output in outputs:
        require(output.name != 'managed-v2-aum.json' and not output.resolve().is_relative_to(projection.REFERENCE),
                'refuse original-bank/reference output: ' + str(output))
    if check:
        drift = []
        for output in outputs:
            try:
                if output.read_bytes() != data:
                    drift.append(str(output))
            except OSError as error:
                drift.append(str(output) + ': ' + str(error))
        require(not drift, 'wire golden missing/drift: ' + '; '.join(drift))
        return
    staged = []
    try:
        for output in outputs:
            fd, temporary = tempfile.mkstemp(prefix=output.name + '.', suffix='.tmp', dir=output.parent)
            staged.append((output, Path(temporary)))
            with os.fdopen(fd, 'wb') as stream:
                stream.write(data)
                stream.flush()
                os.fsync(stream.fileno())
        for output, temporary in staged:
            os.replace(temporary, output)
    finally:
        for _, temporary in staged:
            temporary.unlink(missing_ok=True)


def _test_wire_fields(raw):
    """Small test-only protobuf reader, independent of the text encoder."""
    cursor, fields = 0, {}

    def varint():
        nonlocal cursor
        value, shift = 0, 0
        while True:
            byte = raw[cursor]
            cursor += 1
            value |= (byte & 127) << shift
            if byte < 128:
                return value
            shift += 7
            if shift > 63:
                raise ValueError('bad varint')
    while cursor < len(raw):
        key = varint()
        tag, kind = key >> 3, key & 7
        if kind == 0:
            value = varint()
        elif kind == 2:
            size = varint()
            value = raw[cursor:cursor + size]
            cursor += size
        else:
            raise ValueError('unexpected test wire type')
        fields.setdefault(tag, []).append(value)
    return fields


class WireGoldenTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        # Test oracle reads only the integration original, with the current loader.
        bank = projection.load(ROOT / 'integration/test/contracts/managed-v2-aum.json')
        cls.before = copy.deepcopy(bank['cases'][0]['execution']['before'])
        projected = projection.project_snapshot(bank, cls.before)
        cls.context = {'projection': projected,
                       'header': projection.resolve(bank, projected['source_refs']['state_header'], 'header'),
                       'chain_key': 'evm:' + str(bank['reference']['chain_options']['chain']['chainId'])}
        p, h = projected['pool'], cls.context['header']
        digest = lambda x: hashlib.sha256(json.dumps(x, separators=(',', ':')).encode()).hexdigest()
        cls.snapshot = {'reference': {
            'block': {'chain_key': cls.context['chain_key'], 'number_or_slot': int(h['number'], 16),
                      'hash': h['hash'], 'parent_hash': h['parentHash'], 'timestamp_ms': int(h['timestamp'], 16) * 1000},
            'canonical_epoch': 1, 'journal_seq': 1, 'schema_version': 1,
            'adapter_set_hash': digest([p['adapter_version']]), 'online_generation': 1,
            'scope_hash': digest([p['pool_id']]), 'writer_epoch': 1,
            'owner_service': 'chain-parser-go', 'owner_assignment_version': 1},
            'pools': [copy.deepcopy(p)], 'source': SOURCE, 'history_gaps': []}

    def test_public_encode_supports_runtime_envelopes_as_protobuf_text(self):
        snapshot = copy.deepcopy(self.snapshot)
        snapshot['source'] = 'RPC_BASELINE'
        snapshot['reference']['online_generation'] = 42
        snapshot['reference']['writer_epoch'] = 7
        snapshot['reference']['owner_service'] = 'runtime-owner'
        snapshot['history_gaps'] = [{'from': 1, 'to': 25, 'reason': 'HEAD_FIRST'}]
        snapshot['pools'][0]['v2'] = None
        text = encode(snapshot)
        self.assertIsInstance(text, str)
        self.assertIn('paused: false', text)
        self.assertIn('bpt_index: 0', text)
        self.assertIn('uint256_be:', text)
        self.assertNotIn('v2:', text)
        raw = _protoc('encode', text.encode())
        decoded = _protoc('decode', raw).decode()
        self.assertIn('RPC_BASELINE', decoded)
        self.assertIn('online_generation: 42', decoded)
        self.assertEqual(_protoc('encode', decoded.encode()), raw)

    def test_source_is_exact_first_before_and_only_integration_bank(self):
        with mock.patch.object(projection, 'load', wraps=projection.load) as loader:
            actual = load_source()
        self.assertEqual(loader.call_args_list, [mock.call(ROOT / 'integration/test/contracts/managed-v2-aum.json')])
        self.assertEqual(actual, self.context)
        self.assertEqual(actual['projection']['source_refs']['snapshot'], self.before)

    def test_header_and_canonical_array_hashes_bind_entire_envelope(self):
        self.assertEqual(build_snapshot(self.context), self.snapshot)

    def test_encoding_retains_all_39_fields_and_nested_dependencies(self):
        raw = encode_protobuf(self.snapshot)
        pool = _test_wire_fields(_test_wire_fields(raw)[2][0])
        multi = _test_wire_fields(pool[28][0])
        managed = _test_wire_fields(multi[13][0])
        self.assertEqual(set(managed), set(range(1, 40)))
        self.assertEqual(len(managed[11]), 5)
        self.assertEqual(len(managed[35]), len(self.snapshot['pools'][0]['multi_asset']['assets']))
        for value in managed[11]:
            self.assertEqual(set(_test_wire_fields(value)), {1, 2, 3, 4})
        for value in managed[35]:
            self.assertEqual(set(_test_wire_fields(value)), {1, 2, 3, 4, 5, 6})

    def test_all_managed_values_match_native_pool_including_nested_wire(self):
        raw = encode_protobuf(self.snapshot)
        p = _test_wire_fields(_test_wire_fields(raw)[2][0])
        m = _test_wire_fields(_test_wire_fields(p[28][0])[13][0])
        original = self.context['projection']['pool']['multi_asset']['managed_v2']

        def expected(value):
            if type(value) in (int, bool):
                return int(value)
            if value.isdecimal():
                return b'\x0a\x20' + int(value).to_bytes(32, 'big')
            return value.encode()

        for tag, key in enumerate(MANAGED_FIELDS, 1):
            value = original[key]
            if key in ('dependencies', 'token_info'):
                keys = DEPENDENCY_FIELDS if key == 'dependencies' else TOKEN_FIELDS
                for wire, item in zip(m[tag], value, strict=True):
                    fields = _test_wire_fields(wire)
                    for nested_tag, nested_key in enumerate(keys, 1):
                        v = item[nested_key]
                        e = bytes.fromhex(v) if nested_key.endswith('_sha256') else expected(v)
                        self.assertEqual(fields[nested_tag], [e], (key, nested_key))
            else:
                e = bytes.fromhex(value.removeprefix('0x')) if key == 'block_hash' or key.endswith('_sha256') else expected(value)
                self.assertEqual(m[tag], [e], key)

    def test_null_is_never_treated_as_absent_or_known_zero(self):
        for key in MANAGED_FIELDS:
            snapshot = copy.deepcopy(self.snapshot)
            snapshot['pools'][0]['multi_asset']['managed_v2'][key] = None
            with self.subTest(key=key), self.assertRaises(ValueError):
                encode_protobuf(snapshot)

    def test_explicit_false_zero_presence_survives_cpp_protoc(self):
        snapshot = copy.deepcopy(self.snapshot)
        m = snapshot['pools'][0]['multi_asset']['managed_v2']
        for key in ('timestamp', 'block_number', 'bpt_index', 'last_collection_timestamp',
                    'pause_window_end', 'buffer_period_end', 'gradual_weight_end', 'gradual_fee_end'):
            m[key] = 0
        for key in ('swaps_enabled', 'paused', 'recovery_mode', 'allowlist_enabled', 'vault_paused'):
            m[key] = False
        m['pending_aum'] = '0'
        for token in m['token_info']:
            token['last_change_block'] = 0
            token['circuit_breaker_enabled'] = False
        snapshot['reference']['block'].update(number_or_slot=0, timestamp_ms=0)
        raw = encode_protobuf(snapshot)
        p = _test_wire_fields(_test_wire_fields(raw)[2][0])
        m = _test_wire_fields(_test_wire_fields(p[28][0])[13][0])
        for tag in (1, 2, 18, 23, 27, 28, 29, 30, 31, 32, 33, 34, 37):
            self.assertEqual(m[tag], [0], tag)
        self.assertEqual(_test_wire_fields(m[15][0]), {1: [bytes(32)]})
        for value in m[35]:
            t = _test_wire_fields(value)
            self.assertEqual(t[4], [0])
            self.assertEqual(t[5], [0])

    def test_every_missing_managed_field_is_rejected_before_protoc(self):
        for key in MANAGED_FIELDS:
            with self.subTest(key=key):
                snapshot = copy.deepcopy(self.snapshot)
                del snapshot['pools'][0]['multi_asset']['managed_v2'][key]
                with self.assertRaises(ValueError):
                    encode_protobuf(snapshot)

    def test_nested_dependency_and_token_omissions_are_rejected(self):
        for part, keys in (('dependencies', DEPENDENCY_FIELDS), ('token_info', TOKEN_FIELDS)):
            for key in keys:
                with self.subTest(part=part, key=key):
                    snapshot = copy.deepcopy(self.snapshot)
                    del snapshot['pools'][0]['multi_asset']['managed_v2'][part][0][key]
                    with self.assertRaises(ValueError):
                        encode_protobuf(snapshot)
            snapshot = copy.deepcopy(self.snapshot)
            snapshot['pools'][0]['multi_asset']['managed_v2'][part].pop()
            with self.assertRaises(ValueError):
                encode_protobuf(snapshot)

    def test_fixture_metadata_matches_projection_verbatim(self):
        f = build_fixture(self.context)
        self.assertEqual(f['snapshot'], self.snapshot)
        self.assertEqual(f['original_fields'], self.context['projection']['pool'])
        self.assertEqual(f['source_refs'], self.context['projection']['source_refs'])
        self.assertEqual(f['authority'], 'native-state + synthetic snapshot envelope')
        self.assertEqual(f['original_bank_sha256'], BANK_SHA256)
        self.assertEqual(f['original_bank_bytes'], BANK_BYTES)
        self.assertNotIn('cases', f)
        self.assertEqual(f['protobuf_sha256'], hashlib.sha256(bytes.fromhex(f['protobuf_hex'])).hexdigest())
        validate_fixture(f, self.context)

    def test_byte_flip_and_resealed_wrong_wire_are_rejected(self):
        for reseal in (False, True):
            f = build_fixture(self.context)
            raw = bytearray.fromhex(f['protobuf_hex'])
            raw[-1] ^= 1
            f['protobuf_hex'] = raw.hex()
            if reseal:
                f['protobuf_sha256'] = hashlib.sha256(raw).hexdigest()
            with self.assertRaises(ValueError):
                validate_fixture(f, self.context)

    def test_metadata_and_snapshot_drift_are_rejected(self):
        edits = {'authority': 'trusted-chain', 'original_bank_sha256': '0' * 64,
                 'original_bank_bytes': BANK_BYTES - 1, 'source_refs': {}, 'original_fields': {}}
        for key, value in edits.items():
            f = build_fixture(self.context)
            f[key] = value
            with self.subTest(key=key), self.assertRaises(ValueError):
                validate_fixture(f, self.context)
        for key in self.snapshot['reference']['block']:
            f = build_fixture(self.context)
            f['snapshot']['reference']['block'][key] = 'wrong'
            with self.subTest(block=key), self.assertRaises(ValueError):
                validate_fixture(f, self.context)

    def test_check_reads_all_copies_without_modifying_bytes_or_mtime(self):
        data = (json.dumps(build_fixture(self.context), indent=2) + '\n').encode()
        with tempfile.TemporaryDirectory(prefix='managed-v2-wire-', dir='/tmp') as temporary:
            outputs = [Path(temporary) / (p + '.json') for p in PROJECTS]
            publish(data, outputs)
            before = [(p.read_bytes(), p.stat().st_mtime_ns) for p in outputs]
            publish(data, outputs, check=True)
            self.assertEqual(before, [(p.read_bytes(), p.stat().st_mtime_ns) for p in outputs])
            for output in outputs:
                output.write_bytes(b'drift')
                drifted = [(p.read_bytes(), p.stat().st_mtime_ns) for p in outputs]
                with self.subTest(copy=output.name), self.assertRaises(ValueError):
                    publish(data, outputs, check=True)
                self.assertEqual(drifted, [(p.read_bytes(), p.stat().st_mtime_ns) for p in outputs])
                output.write_bytes(data)
            outputs[0].write_bytes(b'drift')
            outputs[2].unlink()
            before = [(p.read_bytes(), p.stat().st_mtime_ns) if p.exists() else None for p in outputs]
            with self.assertRaises(ValueError) as caught:
                publish(data, outputs, check=True)
            self.assertIn(str(outputs[0]), str(caught.exception))
            self.assertIn(str(outputs[2]), str(caught.exception))
            self.assertEqual(before, [(p.read_bytes(), p.stat().st_mtime_ns) if p.exists() else None for p in outputs])


    def test_publication_stages_every_copy_before_first_replacement(self):
        data = b'already validated fixture'
        with tempfile.TemporaryDirectory(prefix='managed-v2-wire-', dir='/tmp') as temporary:
            outputs = [Path(temporary) / (p + '.json') for p in PROJECTS]
            for output in outputs:
                output.write_bytes(b'old')
            replace = os.replace
            replaced = []

            def inspect_stage(source, target):
                if not replaced:
                    self.assertEqual(len(list(Path(temporary).glob('*.tmp'))), 4)
                    self.assertTrue(all(p.read_bytes() == b'old' for p in outputs))
                self.assertEqual(Path(source).read_bytes(), data)
                replaced.append(target)
                return replace(source, target)

            with mock.patch.object(os, 'replace', side_effect=inspect_stage):
                publish(data, outputs)
            self.assertEqual(replaced, outputs)
            self.assertEqual(list(Path(temporary).glob('*.tmp')), [])

    def test_staging_failure_changes_no_destination_and_cleans_temporary_files(self):
        with tempfile.TemporaryDirectory(prefix='managed-v2-wire-', dir='/tmp') as temporary:
            outputs = [Path(temporary) / (p + '.json') for p in PROJECTS]
            for output in outputs:
                output.write_bytes(b'old')
            before = [(p.read_bytes(), p.stat().st_mtime_ns) for p in outputs]
            mkstemp = tempfile.mkstemp
            calls = 0

            def fail_third_stage(*args, **kwargs):
                nonlocal calls
                calls += 1
                if calls == 3:
                    raise OSError('injected staging failure')
                return mkstemp(*args, **kwargs)

            with mock.patch.object(tempfile, 'mkstemp', side_effect=fail_third_stage):
                with self.assertRaises(OSError):
                    publish(b'new', outputs)
            self.assertEqual(before, [(p.read_bytes(), p.stat().st_mtime_ns) for p in outputs])
            self.assertEqual(list(Path(temporary).glob('*.tmp')), [])


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument('--check', action='store_true')
    mode.add_argument('--self-test', action='store_true')
    args = parser.parse_args()
    if args.self_test:
        suite = unittest.defaultTestLoader.loadTestsFromTestCase(WireGoldenTests)
        return 0 if unittest.TextTestRunner(verbosity=2).run(suite).wasSuccessful() else 1
    context = load_source()
    fixture = build_fixture(context)
    data = (json.dumps(fixture, indent=2, ensure_ascii=False, allow_nan=False) + '\n').encode()
    outputs = [ROOT / project / 'test/contracts' / FILENAME for project in PROJECTS]
    publish(data, outputs, check=args.check)
    print(json.dumps({'checked': args.check, 'copies': len(outputs), 'bytes': len(data),
                      'protobuf_bytes': len(bytes.fromhex(fixture['protobuf_hex'])),
                      'protobuf_sha256': fixture['protobuf_sha256'],
                      'original_bank_sha256': BANK_SHA256, 'authority': AUTHORITY}, sort_keys=True))
    return 0


if __name__ == '__main__':
    try:
        sys.exit(main())
    except (ValueError, OSError, subprocess.CalledProcessError) as error:
        print('managed-v2-aum-wire: ' + str(error), file=sys.stderr)
        sys.exit(1)
