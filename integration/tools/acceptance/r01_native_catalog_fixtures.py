#!/usr/bin/env python3
"""Project pinned public deployment evidence into the Go service tests."""

import argparse
import gzip
import hashlib
import json
import tarfile
from contextlib import contextmanager

from r01_fixed_evm_replay import ARCHIVES, BASE, ROOT, require

FIXTURES = ROOT / 'chain-parser-go/test/fixtures/native-catalog-replay'
HISTORY = 'integration/test/source/deployment-catalog/evm-continuation-v2'
COMMON_RECORDS = ('002-genesis.json', '005-finalized-header.json', '006-witness-header.json')
CASES = {
    'arbitrum-528150486': (
        'op-arbitrum-fixed-2026-11-28.tar.gz',
        'anchor-iteration-ee66n2qk/arbitrum',
        COMMON_RECORDS + ('017-block-receipts.json',),
    ),
    'avalanche-101597996': (
        'avalanche-uniswap-v3-101597996.tar.gz',
        'avalanche-getter-anchor.43nmhr50',
        COMMON_RECORDS + (
            '007-sae-successor.json', '008-sae-successor.json', '009-sae-successor.json',
            '010-sae-successor.json', '011-sae-successor.json', '021-sae-receipt-window-block.json',
            '022-block-receipts.json', '023-block-receipts.json',
        ),
    ),
}

ETH_SOURCE = 'ethereum-uniswap-source-26511767.tar.gz'
ETH_SOURCE_DIGEST = '91e030d5ab89ac7838b6db0c96c1170c8fe34e72e6cb4236c48ad707429ecbc7'
V3_GETTERS = {
    'factory': '0xc45a0155',
    'token0': '0x0dfe1681',
    'token1': '0xd21220a7',
    'fee': '0xddca3f43',
    'tickSpacing': '0xd0c93a7c',
}


@contextmanager
def open_archive(archive_name, digest):
    archive_path = BASE / 'evidence' / archive_name
    require(hashlib.sha256(archive_path.read_bytes()).hexdigest() == digest,
            f'CATALOG_ARCHIVE_DIGEST {archive_name}')
    with tarfile.open(archive_path, 'r:gz') as archive:
        yield archive


def read_member(archive, name):
    member = archive.getmember(name)
    require(member.isfile() and member.size <= 16 << 20, f'CATALOG_FIXTURE_MEMBER {name}')
    with archive.extractfile(member) as record_file:
        return json.load(record_file)


def project(archive_name, directory, names):
    results = {}
    with open_archive(archive_name, ARCHIVES[archive_name]) as archive:
        for name in names:
            record = read_member(archive, f'{HISTORY}/{directory}/{name}')
            method = (
                'eth_getBlockReceipts' if name.endswith('-block-receipts.json')
                else 'eth_getBlockByNumber'
            )
            result = record['response'].get('result')
            require(record['method'] == method and record['http_status'] == 200 and result is not None,
                    f'NATIVE_CATALOG_RPC_RESULT {directory}/{name}')
            results[name] = result
    return results


def v3_pool(chain, code, probe):
    address, block = code['params']
    runtime = code['response']['result']
    anchor = probe['anchor']
    require(code['method'] == 'eth_getCode' and code['http_status'] == 200 and
            int(block, 16) == anchor['height'] and probe['passed'],
            f'V3_FIXTURE_OBSERVATION {chain}')
    fields = {}
    for field, selector in V3_GETTERS.items():
        calls = [call for call in probe['calls'] if call['to'] == address and call['data'] == selector]
        require(len(calls) == 1, f'V3_FIXTURE_GETTER {chain} {field}')
        call = calls[0]
        require(call['block_tag'] == block and call['error'] is None and
                call['all_code_and_storage_MPT_bound'] and len(call['value']) == 66,
                f'V3_FIXTURE_GETTER_BINDING {chain} {field}')
        value = int(call['value'], 16)
        fields[field] = f'0x{value:040x}' if field in ('factory', 'token0', 'token1') else str(value)
    # Tick.tickSpacingToMaxLiquidityPerTick in the pinned V3 release.
    tick_count = 2 * (887272 // int(fields['tickSpacing'])) + 1
    fields['maxLiquidityPerTick'] = str(((1 << 128) - 1) // tick_count)
    return {
        'chain_key': chain,
        'anchor': anchor,
        'address': address,
        'observed_runtime': runtime,
        'actual_runtime_sha256': hashlib.sha256(bytes.fromhex(runtime[2:])).hexdigest(),
        'immutable_fields': fields,
    }


def project_v3():
    catalog = 'integration/test/source/deployment-catalog'
    with open_archive(ETH_SOURCE, ETH_SOURCE_DIGEST) as archive:
        factory = read_member(archive, f'{catalog}/ethereum-source-26511767.QtMdQkte/ethereum-uniswap-v3-source.json')
    eth = 'ethereum-uniswap-26511767.tar.gz'
    with open_archive(eth, ARCHIVES[eth]) as archive:
        eth_code = read_member(archive, f'{HISTORY}/ethereum.2nsy6ES2/deployment-capture-l3pkfvwu/1/samples/ethereum-uniswap-v3/03-eth_getCode.json')
    normal = 'ethereum-uniswap-normal-26511767.tar.gz'
    with open_archive(normal, ARCHIVES[normal]) as archive:
        eth_probe = read_member(archive, f'{catalog}/evm-normal-v4/ethereum-26511767.CorGKXrn/ethereum-uniswap-v3/probe-01.json')
    op = 'op-arbitrum-fixed-2026-11-28.tar.gz'
    op_root = f'{HISTORY}/anchor-iteration-ee66n2qk/optimism'
    with open_archive(op, ARCHIVES[op]) as archive:
        op_code = read_member(archive, f'{op_root}/deployment-capture-q3u7nszq/1/samples/optimism-uniswap-v3/03-eth_getCode.json')
        op_probe = read_member(archive, f'{op_root}/op-normal-indexed-probe.json')
    pools = [v3_pool('evm:1', eth_code, eth_probe), v3_pool('evm:10', op_code, op_probe)]
    require(all(pool['immutable_fields']['factory'] == factory['address'].lower() for pool in pools),
            'V3_FIXTURE_FACTORY_ADDRESS')
    return {'factory_runtime': factory['runtimeBytecode']['onchainBytecode'], 'pools': pools}


def verify_fixture(name, data, write):
    expected = json.dumps(data, sort_keys=True, separators=(',', ':')).encode()
    target = FIXTURES / f'{name}.json.gz'
    if write:
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(gzip.compress(expected, mtime=0))
    require(gzip.decompress(target.read_bytes()) == expected, f'CATALOG_FIXTURE_MISMATCH {name}')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--write', action='store_true', help='regenerate the local test vectors')
    args = parser.parse_args()
    for name, case in CASES.items():
        verify_fixture(name, project(*case), args.write)
        print(f'{name}: verified {len(case[2])} archived responses')
    verify_fixture('official-v3-runtime', project_v3(), args.write)
    print('official-v3-runtime: verified two observed pool instances and the Ethereum factory')


if __name__ == '__main__':
    main()
