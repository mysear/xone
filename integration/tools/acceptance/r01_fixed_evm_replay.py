#!/usr/bin/env python3
"""Replay the eleven retained fixed-block EVM deployments with normal services."""

import argparse
from dataclasses import dataclass
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import subprocess
import tarfile

from r01_public_catalog import ROOT, load_json, read_blob, require
from r01_public_catalog_capture import reference, write_new
from r01_public_catalog_evm_normal import execute as execute_normal
from r01_public_catalog_slipstream_archive import rebuild as rebuild_slipstream
from r01_public_catalog_slipstream_normal import execute as execute_slipstream

BASE = ROOT / 'integration/test/source/deployment-catalog'
TOOLS = Path(__file__).resolve().parent
HISTORY = BASE / 'evm-continuation-v2'
NORMAL = BASE / 'evm-normal-v4'
ARCHIVES = {
    'fixed-evm-2026-11-28.tar.gz': '4f19190de2bd8da36e0c1b28cfc34544ded0f86f5170a485190b0638fbe61cf4',
    'base-v3-aerodrome-v2-63180533.tar.gz': 'a4978ab541680182016564b6ed37dd6758acc49740169611ea7bad2512280493',
    'op-arbitrum-fixed-2026-11-28.tar.gz': 'd461ed5e2c3e698d247107f3814ff1a3cce562b7957cb3e428d093047ced31f3',
    'avalanche-uniswap-v3-101597996.tar.gz': '9de2500bc750f8428d89e93cda26ee3bb09741d543cba63c65774328d950e85f',
    'avalanche-lfj-v21-101598757.tar.gz': 'f654014552fc0a2bad187beff4fd9adc111d40423b274b3201cd427a3bf87c35',
    'bsc-pancake-v3-136166651.tar.gz': '167e36ec2217ef40498ac0c9e6cc78aeaebcd4fa897a9aa3dfae9943075cc5fe',
}


@dataclass(frozen=True)
class Deployment:
    name: str
    chain: str
    block: str
    height: int
    amount: str
    normal: str = ''
    getter: str = ''
    probe: str = ''


BASE_BLOCK = 'anchor-iteration-ee66n2qk/base'
OP_BLOCK = 'anchor-iteration-ee66n2qk/optimism'
DEPLOYMENTS = (
    Deployment('base-uniswap-v3', 'base', BASE_BLOCK, 63180533, '166255',
               'base-uniswap-v3-fixed-h.ustwe78e', probe='probe-01.json'),
    Deployment('base-aerodrome-v2', 'base', BASE_BLOCK, 63180533, '166299',
               'base-aerodrome-v2-fixed-h.80092axw', probe='probe-02.json'),
    Deployment('base-aerodrome-slipstream', 'base', BASE_BLOCK, 63180533, '166349'),
    Deployment('optimism-uniswap-v3', 'optimism', OP_BLOCK, 160113647, '166230',
               'optimism-uniswap-v3-160113647-indexed', probe='op-normal-indexed-probe.json'),
    Deployment('optimism-velodrome-v2', 'optimism', OP_BLOCK, 160113647, '165448',
               'optimism-velodrome-v2-160113647', probe='velo-normal-probe-02.json'),
    Deployment('arbitrum-uniswap-v3', 'arbitrum', 'anchor-iteration-ee66n2qk/arbitrum',
               528150486, '166253', 'arbitrum-uniswap-v3-fixed-h.el56r46q', probe='probe-01.json'),
    Deployment('polygon-uniswap-v3', 'polygon', 'polygon-getter-anchor.6x8jv5hh',
               98087772, '7', 'polygon-getter-proof.3q923t3l', getter='input.json'),
    Deployment('bsc-pancake-v2', 'bsc', 'bsc-v2-getter-seed.umiwliu4', 136159243,
               '55361279474212605', 'bsc-pancake-v2-finalized.pefduaif', getter='input-00.json'),
    Deployment('bsc-pancake-v3', 'bsc', 'bsc-lm-proof.CnFonFFC', 136166651,
               '55536458068512518', 'bsc-lm-proof.CnFonFFC', getter='input.json'),
    Deployment('avalanche-uniswap-v3', 'avalanche', 'avalanche-getter-anchor.43nmhr50',
               101597996, '659', 'avalanche-getter-proof.7jbwf7xl', getter='input.json'),
    Deployment('avalanche-lfj-v21', 'avalanche', 'avalanche-lfj-anchor.EbsiIkbo',
               101598757, '134', 'avalanche-lfj-proof.no6DPSRo', getter='input.json'),
)


def unpack(archive, digest):
    prefix = PurePosixPath('integration/test/source/deployment-catalog')
    require(hashlib.sha256(archive.read_bytes()).hexdigest() == digest, 'FIXED_ARCHIVE_DIGEST')
    with tarfile.open(archive, 'r:gz') as bundle:
        for member in bundle:
            entry = PurePosixPath(member.name)
            require(not entry.is_absolute() and '..' not in entry.parts, 'FIXED_ARCHIVE_PATH')
            if not entry.is_relative_to(prefix):
                # Ancillary source-rebuild inputs in the Pancake V3 archive
                # are consumed separately and are not needed for fixed-state replay.
                continue
            if member.isdir():
                continue
            require(member.isfile(), 'FIXED_ARCHIVE_MEMBER_TYPE')
            target = ROOT / entry
            require(target.resolve().is_relative_to(BASE), 'FIXED_ARCHIVE_SYMLINK')
            with bundle.extractfile(member) as reader:
                data = reader.read()
            if target.exists():
                require(target.is_file() and target.read_bytes() == data,
                        'FIXED_ARCHIVE_EXISTING_BYTES:' + member.name)
            else:
                write_new(target, data)


def invoke(args, log, timeout, cwd=ROOT, env=None):
    with log.open('xb') as output:
        subprocess.run([str(arg) for arg in args], stdout=output, stderr=subprocess.STDOUT,
                       check=True, timeout=timeout, cwd=cwd, env=env)


def replay_blocks(out):
    proofs = {}
    original_cases = {}
    receipt_inputs = []
    for entry in DEPLOYMENTS:
        if entry.block in proofs:
            continue
        block = HISTORY / entry.block
        # The Arbitrum, Base and Pancake V3 archives retain completed state
        # proofs alongside the original partial capture.
        state = 'prestate-evidence-v2.json' if (block / 'prestate-evidence-v2.json').is_file() else 'prestate-evidence.json'
        proof = out / f'{entry.chain}-{entry.height}-commitments.json'
        invoke(['node', TOOLS / 'r01_public_catalog_evm_commitments.cjs', block, '', state],
               proof, 120)
        require(load_json(proof)['passed'] is True, 'FIXED_STATE_PROOF')
        proofs[entry.block] = reference(proof)
        replay = out / f'{entry.chain}-{entry.height}-original.json'
        invoke(['python3', TOOLS / 'r01_public_catalog_evm_history.py', 'replay',
                '--directory', block], replay, 510)
        result = load_json(replay)
        require(result['passed'] is True and result['network_requests'] == 0,
                'FIXED_ORIGINAL_REPLAY')
        original_cases[entry.block] = {case['id']: case for case in result['cases']}
        receipt_inputs.append(block / 'receipt-input.json')
    receipt_log = out / 'verified-receipts.log'
    env = dict(os.environ, XONE_R01_RECEIPT_INPUTS=os.pathsep.join(map(str, receipt_inputs)))
    invoke(['go', 'test', '-count=1', '-v', './internal/chain/evm',
            '-run', '^TestPublicCatalogFixedReceiptRoot$'], receipt_log, 120,
           cwd=ROOT / 'chain-parser-go', env=env)
    return proofs, original_cases, reference(receipt_log)


def replay_deployment(entry, replay_paths, out, parser, router):
    result_dir = out / entry.name
    if not entry.normal:
        rebuilt = replay_paths / 'slipstream-archive'
        responses = NORMAL / 'base-slipstream-dependencies.hoknqrom'
        rebuild_slipstream(
            responses / 'context-cached._mgho3yq/capture.json', HISTORY / entry.block,
            sorted(responses.glob('context-proof-*.json')) + sorted(responses.glob('ticks-proof-*.json')),
            [responses / '0x0ad08370c76ff426f534bb2affd9b5555338ee68.code.json'], rebuilt,
        )
        execute_slipstream(rebuilt / 'capture.json', replay_paths / 'slipstream-getters',
                           result_dir, parser, router, 6)
        bound = load_json(result_dir / 'proof-bound-report.json')
        require(bound['passed'] is True and bound['router_pool_gross'] == entry.amount,
                'FIXED_SLIPSTREAM_COMPARISON')
    else:
        case = NORMAL / entry.normal
        if entry.getter:
            probe = replay_paths / (entry.name + '-probe.json')
            invoke(['node', TOOLS / 'r01_public_catalog_evm_readonly.cjs', case / entry.getter, probe],
                   out / (entry.name + '-getters.log'), 300)
            case = replay_paths / entry.name
            case.mkdir()
        else:
            probe = (case if (case / entry.probe).is_file() else HISTORY / entry.block) / entry.probe
        bound = execute_normal(case, probe, result_dir, parser, router, timeout_seconds=150)
        require(bound['id'] == entry.name and bound['anchor']['height'] == entry.height and
                bound['status'] == 'QUALIFIED_FINITE_NORMAL_REFERENCE_DIFFERENTIAL' and
                bound['quantities']['router_pool_gross'] == entry.amount,
                'FIXED_NORMAL_COMPARISON:' + entry.name)
    report = load_json(result_dir / 'report.json')
    require(report['id'] == entry.name and report['quote']['status'] == 200 and
            report['unrecorded_rpc_requests'] == [] and report['network_requests_during_process_replay'] == 0,
            'FIXED_NORMAL_RPC_INVENTORY:' + entry.name)
    return reference(result_dir / 'report.json')


def main():
    cli = argparse.ArgumentParser(description=__doc__)
    cli.add_argument('--out', type=Path, required=True)
    cli.add_argument('--parser-bin', type=Path, required=True)
    cli.add_argument('--router-bin', type=Path, required=True)
    args = cli.parse_args()
    parser, router = args.parser_bin.resolve(), args.router_bin.resolve()
    for binary in (parser, router):
        require(binary.is_relative_to(ROOT) and binary.is_file() and os.access(binary, os.X_OK),
                'FIXED_REPLAY_BINARY:' + str(binary))
    out = args.out.resolve()
    require(out.is_relative_to(ROOT / 'integration/artifacts/local') and
            out.name.startswith('r01-public-') and not out.exists(), 'FIXED_REPLAY_OUTPUT')
    compiler = load_json(ROOT / 'integration/config/r01-evm-helper-compiler.json')
    read_blob(ROOT, compiler['blob'])
    for filename, digest in ARCHIVES.items():
        unpack(BASE / 'evidence' / filename, digest)
    replay_paths = NORMAL / out.name
    require(not replay_paths.exists(), 'FIXED_REPLAY_WORKSPACE')
    out.mkdir(parents=True)
    replay_paths.mkdir()
    proofs, original_cases, receipts = replay_blocks(out)
    entries = []
    for entry in DEPLOYMENTS:
        original = original_cases[entry.block][entry.name]
        require(original['passed'] and original['actual_net_recipient'] == entry.amount,
                'FIXED_REFERENCE_DELTA:' + entry.name)
        report = replay_deployment(entry, replay_paths, out, parser, router)
        entries.append({'id': entry.name, 'chain': entry.chain, 'height': entry.height,
                        'amount_atomic': entry.amount, 'state_proof': proofs[entry.block],
                        'normal_report': report})
    report = {'passed': True, 'scope': 'fixed_block_gross_recipient_differential',
              'receipts': receipts, 'deployments': entries}
    write_new(out / 'fixed-replay-report.json', (json.dumps(report, indent=2) + '\n').encode())


if __name__ == '__main__':
    main()
