#!/usr/bin/env python3
"""Exercise actual HTTP -> owned worker -> original Router dynamic fees locally."""
import hashlib
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.request

ROOT = Path(__file__).resolve().parents[3]


def require(value, message):
    if not value:
        raise RuntimeError(message)


def sha(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def run(binary, output, *, exact_out=False, path_case=None):
    output = output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    binary = binary.resolve(strict=True)
    with tempfile.TemporaryDirectory(prefix='xone-evm-fee-http-') as directory:
        staged = Path(directory) / binary.name
        digest = sha(binary)
        shutil.copy2(binary, staged)
        require(sha(staged) == digest == sha(binary), 'Harness changed while staging')
        info_path = output / 'info.json'
        with (output / 'process.log').open('wb') as log:
            env = dict(os.environ)
            env.pop('XONE_EVM_PATH_CASE', None)
            if path_case is not None: env['XONE_EVM_PATH_CASE'] = path_case
            process = subprocess.Popen([str(staged), str(info_path)], cwd=ROOT, env=env,
                                       stdout=log, stderr=subprocess.STDOUT, start_new_session=True)
            try:
                deadline = time.monotonic() + 30
                while not info_path.exists():
                    require(process.poll() is None, 'HTTP harness exited before readiness')
                    require(time.monotonic() < deadline, 'HTTP harness readiness timeout')
                    time.sleep(0.05)
                info = json.loads(info_path.read_bytes())
                opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))

                def post(path, body, authenticated=True):
                    headers = {'Content-Type': 'application/json'}
                    if authenticated:
                        headers['Authorization'] = 'Bearer ' + info['token']
                    data = b'' if body is None else json.dumps(body).encode()
                    request = urllib.request.Request(info['api'] + path, data=data, headers=headers)
                    try:
                        with opener.open(request, timeout=20) as response:
                            return response.status, json.load(response)
                    except urllib.error.HTTPError as error:
                        raw = error.read()
                        try: value = json.loads(raw)
                        except ValueError: value = {'error': raw.decode(errors='replace')[:512]}
                        return error.code, value

                status, denied = post('/v1/quotes/missing/simulate', None, False)
                require(status == 401 and denied['error'] == 'UNAUTHORIZED', 'Authentication boundary changed')
                request = info['exactOutRequest'] if exact_out else info['request']
                quote_path = '/v1/quote-exact-out' if exact_out else '/v1/quote'
                status, quote = post(quote_path, request)
                require(status == 200, 'Quote failed: ' + json.dumps(quote))
                quote_id = quote['quoteId']
                status, invalid = post(f'/v1/quotes/{quote_id}/simulate', {'gasPrice': '0'})
                require(status >= 400, 'Simulation accepted a caller fee override')
                status, result = post(f'/v1/quotes/{quote_id}/simulate', None)
                require(status == 200, 'Actual worker simulation failed: ' + json.dumps(result))
                (output / 'quote.json').write_text(json.dumps(quote, indent=2) + '\n')
                (output / 'simulation.json').write_text(json.dumps(result, indent=2) + '\n')
                fees = result['fees']
                native = result['outcome']['native_fees']
                gas = int(fees['evmGasUsed'])
                base = int(info['executionBaseFee'])
                maximum = int(info['maxFeePerGas'])
                effective = base + min(1, maximum - base)
                require(effective < maximum, 'Counterexample does not distinguish maximum from actual fee')
                require(fees['gasPriceWei'] == str(effective), 'API reports cap instead of effective price')
                require(fees['evmFeeWei'] == native['execution_fee'] == str(gas * effective), 'Execution fee mismatch')
                require(native['base_fee_burned'] == str(gas * base), 'Base fee burn mismatch')
                require(native['priority_fee_paid'] == str(gas * (effective - base)), 'Priority fee mismatch')
                require(int(native['caller_before']) - int(native['caller_after']) == gas * effective,
                        'Measured caller debit differs')
                require(int(native['coinbase_after']) - int(native['coinbase_before']) == gas * (effective-base),
                        'Measured coinbase credit differs')
                require(result['transactionContext']['gas_price'] == str(maximum), 'Original maximum commitment lost')
                require(result['transactionContext']['max_priority_fee_per_gas'] == '1', 'Original priority commitment lost')
                require(result['verification']['success'] is True and result['verified'] is False,
                        'Replay success or authority changed')
                require(result['authority'] == 'PROVIDED_LOCAL_STATE'
                        and fees['chainFeeCompleteness'] == 'LOCAL_EVM_ONLY', 'Authority upgraded')
                require(result['outcome']['actual_buy_amount'] == quote['grossBuyAmount'], 'Recipient delta differs from quote')
                if exact_out:
                    require(result['quoteKind'] == 'EXACT_OUT', 'Worker used wrong target mode')
                    require(result['outcome']['actual_buy_amount'] == quote['buyAmount'], 'Actual target amount differs')
                    require(result['outcome']['actual_input_consumed'] == quote['requiredSellAmount'], 'Actual input differs')
                    require(int(result['outcome']['unused_input_budget']) + int(result['outcome']['actual_input_consumed'])
                            == int(quote['maxSellAmount']), 'Encoded maximum conservation mismatch')
                    require(result['outcome']['refund'] == '0', 'Unused budget was falsely classified as refund')
                    require(result['transaction']['data'].startswith('0x8803dbee'), 'Worker did not execute original ExactOut selector')
                    require(result['planHash'] == quote['planHash'] and result['snapshotRef'] == quote['snapshotRef'],
                            'Quote/plan/snapshot binding changed')
                    evidence_request = urllib.request.Request(info['api']+f'/internal/quotes/{quote_id}/evidence',
                        headers={'Authorization': 'Bearer '+info['token']})
                    with opener.open(evidence_request, timeout=5) as response:
                        evidence = json.load(response)
                    require(evidence['quote'] == quote and evidence['simulation_evidence'] == result,
                            'ExactOut evidence overwrote quote or omitted simulation')
                    (output/'evidence.json').write_text(json.dumps(evidence,indent=2)+'\n')
                report = dict(passed=True, harness_sha256=digest, actual_worker_executions=1,
                              quote_kind='EXACT_OUT' if exact_out else 'EXACT_IN', path_case=path_case,
                              gas_used=gas, effective_gas_price=str(effective), max_fee_per_gas=str(maximum),
                              actual_fee=str(gas*effective), overstatement_prevented=str(gas*(maximum-effective)),
                              publicTransactionsSubmitted=False, production_ready=False,
                              all_requested_features_complete=False,
                              artifacts={name: sha(output/name) for name in ['quote.json','simulation.json']})
                (output/'report.json').write_text(json.dumps(report,indent=2)+'\n')
                print(json.dumps(report), flush=True)
                return report
            finally:
                if process.poll() is None:
                    os.killpg(process.pid, signal.SIGTERM)
                    try:
                        process.wait(timeout=5)
                    except subprocess.TimeoutExpired:
                        os.killpg(process.pid, signal.SIGKILL)
                        process.wait()


if __name__ == '__main__':
    require(len(sys.argv) == 3, 'Expected BINARY OUTPUT_DIRECTORY')
    run(Path(sys.argv[1]), Path(sys.argv[2]))
