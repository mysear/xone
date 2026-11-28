#!/usr/bin/env python3
"""Replay a fixed Base Slipstream context through the normal Parser and Router."""

import argparse
from concurrent.futures import ThreadPoolExecutor
import json
from pathlib import Path
import subprocess
import time

from r01_public_catalog import ROOT, load_json, read_blob, require
from r01_public_catalog_capture import reference, write_new
from r01_public_catalog_evm_normal import BASE
from r01_public_catalog_evm_process import FixedRPC, call_key, run


class ContextRPC(FixedRPC):
    def __init__(self, source, calls, refs, origin, getter_responses):
        super().__init__(source, calls, refs)
        self.origin = origin
        self.getter_responses = getter_responses

    def dispatch(self, method, params):
        if method != 'eth_call':
            return super().dispatch(method, params)
        self.check_anchor(params[1])
        query = params[0]
        require(set(query) <= {'from', 'to', 'data'} and query.get('from', self.origin) == self.origin,
                'SLIPSTREAM_GETTER_CALLER')
        key = call_key(query['to'], query['data'])
        require(key in self.getter_responses, 'SLIPSTREAM_UNRECORDED_GETTER:' + key)
        row = self.getter_responses[key]
        if 'from' not in query:
            require(not any(op['op'] == 'ORIGIN' for op in row['environment']),
                    'SLIPSTREAM_IMPLICIT_ORIGIN')
        return row['value']


def execute(capture_path, work, output, parser_bin, router_bin, jobs):
    capture = load_json(capture_path)
    require(capture['public_batches'] == [] and capture['call_count'] == 0, 'SLIPSTREAM_PRESEEDED_GETTERS')
    row = capture['catalog_row']
    source = (ROOT / row['directory']).resolve()
    require(source.is_relative_to(BASE.resolve()), 'OWNED_SLIPSTREAM_SOURCE')
    execution_file = source / 'fork-execution.json'
    execution = json.loads(read_blob(ROOT, row['inputs']['execution']))
    require(row['inputs']['execution'] == reference(execution_file), 'SLIPSTREAM_EXECUTION_REFERENCE')
    origin = execution['sender']
    case = next(case for case in execution['cases'] if case['catalog_deployment_id'] == row['id'])
    require(case['passed'] and case['pool'] == row['pool'] and execution['anchor']['hash'] == row['anchor']['hash'],
            'SLIPSTREAM_ORIGINAL_EXECUTION_BINDING')
    work.mkdir()
    bound_capture = work / 'capture.json'
    write_new(bound_capture, capture_path.read_bytes())
    worker = Path(__file__).with_name('r01_public_catalog_slipstream_getters.cjs')

    def reexecute(index):
        target = work / f'getters-{index:02}.jsonl'
        with (work / f'getters-{index:02}.log').open('xb') as log:
            result = subprocess.run(['node', str(worker), str(bound_capture), str(target),
                                     str(index), str(jobs)], stdout=log, stderr=subprocess.STDOUT, timeout=600)
        require(result.returncode == 0, f'SLIPSTREAM_GETTER_REEXECUTION:{index:02}')
        return target

    getter_paths = []
    with ThreadPoolExecutor(max_workers=jobs) as executor:
        getter_paths.extend(executor.map(reexecute, range(jobs)))
    values = {}
    height = hex(row['anchor']['height'])
    for path in getter_paths:
        with path.open() as reader:
            for line in reader:
                getter = json.loads(line)
                require(getter['block_tag'] == height and getter['request']['from'] == origin and
                        getter['all_code_and_storage_MPT_bound'] and getter['steps'] > 0 and
                        not getter['forbidden'] and getter['error'] is None,
                        'SLIPSTREAM_GETTER_TRACE_BINDING')
                key = call_key(getter['to'], getter['data'])
                require(key not in values, 'SLIPSTREAM_DUPLICATE_GETTER')
                values[key] = getter
    timestamp_ms = execution['anchor']['timestamp'] * 1000
    replay_age_ms = int(time.time() * 1000) - timestamp_ms
    require(replay_age_ms >= 0, 'SLIPSTREAM_FUTURE_REPLAY_HEIGHT')
    result = run(work, output, 45, parser_bin, router_bin,
                 rpc_factory=lambda source, calls, refs: ContextRPC(source, calls, refs, origin, values),
                 freshness_ms=max(60000, replay_age_ms + 120000))
    amount = result.get('quote', {}).get('body', {}).get('grossBuyAmount')
    passed = (result['normal_parser_router_quote_observed'] and amount == case['actual_net_recipient'] and
              result['pool_gross_equals_original_VM_recipient_delta'] and not result.get('error') and
              not result['unrecorded_rpc_requests'])
    report = {
        'passed': passed,
        'anchor': row['anchor'],
        'getter_count': len(values),
        'getter_reexecution': [reference(path) for path in getter_paths],
        'normal_process': reference(output / 'report.json'),
        'reference_execution': row['inputs']['execution'],
        'router_pool_gross': amount,
        'original_VM_recipient_token_delta': case['actual_net_recipient'],
    }
    write_new(output / 'proof-bound-report.json', (json.dumps(report, indent=2) + '\n').encode())
    require(passed, 'SLIPSTREAM_NORMAL_DIFFERENTIAL_FAILED')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--capture', type=Path, required=True)
    parser.add_argument('--work', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--parser-bin', type=Path)
    parser.add_argument('--router-bin', type=Path)
    parser.add_argument('--jobs', type=int, choices=range(1, 9), default=6)
    args = parser.parse_args()
    work = args.work.resolve()
    output = args.out.resolve()
    require(work.is_relative_to(BASE.resolve()), 'OWNED_SLIPSTREAM_WORK')
    artifacts = ROOT / 'integration/artifacts/local'
    require(output.is_relative_to(artifacts.resolve()) and output.name.startswith('r01-public-'),
            'OWNED_SLIPSTREAM_OUTPUT')
    execute(args.capture.resolve(), work, output, args.parser_bin, args.router_bin, args.jobs)


if __name__ == '__main__':
    main()
