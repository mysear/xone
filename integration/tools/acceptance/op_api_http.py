#!/usr/bin/env python3
"""Normal historical OP HTTP build -> fixed local wallet -> owned worker.

Both signing and the independent full Router transition use the pinned original
client. All network requests target the local harness; no transaction is sent.
"""
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import tempfile
import time
import urllib.error
import urllib.request
from op_bound_vectors import ROOT, RUNNER, SOURCE, digest, original_inputs, require

SIGNER = ROOT / 'integration/tools/acceptance/reference/op_api_sign.go'


def prepare(directory):
    directory = Path(directory).resolve()
    directory.mkdir(parents=True, exist_ok=False)
    refs = original_inputs()
    refs[str(SIGNER.relative_to(ROOT))] = digest(SIGNER)
    refs[str(RUNNER.relative_to(ROOT))] = digest(RUNNER)
    binaries = {}
    for name in ['signer', 'transition']:
        module = directory / name
        module.mkdir()
        source = SIGNER.read_text() if name == 'signer' else RUNNER.read_text()
        if name == 'transition':
            require(source.count('if len(inputs) != 448 {') == 1, 'Original wrapper input guard changed')
            source = source.replace('if len(inputs) != 448 {', 'if len(inputs) != 1 {')
        (module / 'main.go').write_text(source)
        (module / 'go.mod').write_text('module xone-op-api-' + name + '\n\ngo 1.24.0\n\nrequire github.com/ethereum/go-ethereum v0.0.0\n\nreplace github.com/ethereum/go-ethereum => ' + str(SOURCE) + '\n')
        binary = module / 'runner'
        result = subprocess.run(['go', 'build', '-mod=mod', '-o', str(binary), '.'], cwd=module, capture_output=True, text=True, timeout=180)
        (module / 'build.log').write_text(result.stdout + result.stderr)
        require(result.returncode == 0, 'Original API runner build failed: ' + result.stderr[-1000:])
        for path in [module / 'main.go', module / 'go.mod', module / 'go.sum', module / 'build.log', binary]:
            refs[str(path.relative_to(ROOT))] = digest(path)
        binaries[name] = binary
    return binaries, refs


def original(binary, value):
    result = subprocess.run([str(binary)], input=json.dumps(value), capture_output=True, text=True, timeout=30)
    require(result.returncode == 0, 'Original runner rejected normal HTTP transaction: ' + result.stderr[-1000:])
    return json.loads(result.stdout)


def write(path, value):
    path.write_text(json.dumps(value, indent=2) + '\n')


def run(binary, sources, runners, output, maximum_groups=None):
    binary = Path(binary).resolve(strict=True)
    output = Path(output).resolve()
    output.mkdir(parents=True, exist_ok=False)
    sources = json.loads(Path(sources).read_bytes())
    expected = 32 if any(r['request']['fixture']['execution_environment']['fork'] == 'op-jovian' for r in sources) else 28
    require(len(sources) == expected and len({r['id'] for r in sources}) == expected, 'Normal API source matrix changed')
    groups = {}
    for source in sources:
        prefix, mode = source['id'].rsplit('-', 1)
        groups.setdefault(prefix, {})[mode] = source
    require(len(groups) == expected // 2 and all(set(v) == {'in', 'out'} for v in groups.values()), 'Normal API build mode coverage changed')
    rows, rejected = [], 0
    binary_sha = digest(binary)
    with tempfile.TemporaryDirectory(prefix='xone-op-api-http-') as temp:
        staged = Path(temp) / binary.name
        shutil.copy2(binary, staged)
        require(digest(staged) == binary_sha == digest(binary), 'Harness changed while staging')
        for group_index, (prefix, modes) in enumerate(groups.items()):
            if maximum_groups is not None and group_index >= maximum_groups:
                break
            group = output / prefix
            group.mkdir()
            source_path = group / 'source.json'
            write(source_path, modes['in']['request'])
            env = {k: v for k, v in os.environ.items() if not k.startswith('XONE_EVM_') and k != 'XONE_OP_API_SOURCE'}
            env['XONE_OP_API_SOURCE'] = str(source_path)
            info_path = group / 'info.json'
            with (group / 'process.log').open('wb') as log:
                process = subprocess.Popen([str(staged), str(info_path)], cwd=ROOT, env=env, stdout=log, stderr=subprocess.STDOUT, start_new_session=True)
                try:
                    deadline = time.monotonic() + 30
                    while not info_path.exists():
                        require(process.poll() is None, 'OP HTTP harness exited: ' + (group / 'process.log').read_text()[-1500:])
                        require(time.monotonic() < deadline, 'OP HTTP harness readiness timeout')
                        time.sleep(.05)
                    info = json.loads(info_path.read_bytes())
                    opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))

                    def post(path, body=None, auth=True):
                        headers = {'Content-Type': 'application/json'}
                        if auth:
                            headers['Authorization'] = 'Bearer ' + info['token']
                        data = b'' if body is None else json.dumps(body).encode()
                        try:
                            with opener.open(urllib.request.Request(info['api'] + path, data=data, headers=headers), timeout=20) as response:
                                return response.status, json.load(response)
                        except urllib.error.HTTPError as error:
                            return error.code, json.loads(error.read())

                    for mode, source in modes.items():
                        directory = group / mode
                        directory.mkdir()
                        status, quote = post('/v1/quote' if mode == 'in' else '/v1/quote-exact-out', info['request'] if mode == 'in' else info['exactOutRequest'])
                        write(directory / 'quote.json', quote)
                        require(status == 200, 'Normal OP quote failed: ' + json.dumps(quote))
                        qid = quote['quoteId']
                        status, denied = post(f'/v1/quotes/{qid}/build', auth=False)
                        require(status == 401 and denied['error'] == 'UNAUTHORIZED', 'Build authentication changed')
                        status, built = post(f'/v1/quotes/{qid}/build')
                        write(directory / 'build.json', built)
                        require(status == 200, 'Normal OP build failed: ' + json.dumps(built))
                        require(built['purpose'] == 'LOCAL_HISTORICAL_OP_SIGNED_SIMULATION' and built['ready'] is False and built['completeChainFeeNative'] is None, 'Unsigned template claimed full chain fee or production readiness')
                        require(built['quoteId'] == qid and built['planHash'] == quote['planHash'] and built['verified'] is False and built['submitted'] is False, 'Build quote binding changed')
                        signed = original(runners['signer'], built['rpcTransaction'])
                        write(directory / 'signed.json', signed)
                        negative_results = []
                        old = source['request']['op_user']
                        old_signature = dict(transactionHash=old['transaction_hash'], signedWire=old['signed_wire'])
                        for bad_body in [{'signedOpTransaction': old_signature}, None, {'signedOpTransaction': dict(signed, attributesCalldata='0x')}, {'gasPrice': '0'}, {'signedOpTransaction': dict(signed, transactionHash='0x' + '00' * 32)}, {'signedOpTransaction': dict(signed, signedWire=signed['signedWire'][:-2])}]:
                            status, failed = post(f'/v1/quotes/{qid}/simulate', bad_body)
                            require(status >= 400 and 'error' in failed and 'outcome' not in failed, 'Malformed or unbound OP request yielded evidence')
                            negative_results.append(dict(status=status, response=failed))
                            write(directory / 'negative-requests.json', negative_results)
                            rejected += 1
                        simulation_started = time.monotonic()
                        status, simulation = post(f'/v1/quotes/{qid}/simulate', {'signedOpTransaction': signed})
                        write(directory / 'simulation.json', simulation)
                        write(directory / 'simulation-timing.json', dict(status=status, elapsed_seconds=time.monotonic()-simulation_started))
                        require(status == 200, 'Normal signed OP simulation failed: ' + json.dumps(simulation))
                        require(simulation['bindingHash'] == built['bindingHash'] and simulation['transaction'] == built['transaction'], 'Signed simulation differs from built BOUND calldata')
                        require(simulation['quoteId'] == qid and simulation['planHash'] == quote['planHash'] and simulation['bundleHash'] == built['bundleHash'] == info['bundleHash'], 'Normal OP source/quote binding changed')
                        require(simulation['verification'] == dict(level='LOCAL_SIGNED_HISTORICAL_OP_EXECUTED', success=True, verified=False) and simulation['verified'] is False, 'Historical execution promoted to production')
                        require(simulation['workerHash'] == binary_sha, 'Actual worker binary identity changed')
                        request = source['request']
                        context = request['bound']['plan']['context'] if request.get('bound') is not None else request['exact_out_bound']['plan']['context']
                        reference_input = dict(id=source['id'], rule=request['fixture']['execution_environment']['fork'][3:], fixture=request['fixture'], signed_wire=signed['signedWire'][2:], transaction_hash=signed['transactionHash'], caller=context['caller'], token_in=context['sell_token'], token_out=context['buy_token'], recipient=context['recipient'])
                        reference = original(runners['transition'], [reference_input])[0]
                        outcome = simulation['outcome']
                        fees, costs = outcome['op_fees'], simulation['costs']
                        require(outcome['execution']['gas_used'] == reference['gas_used'] and outcome['actual_input_consumed'] == reference['spent'] and outcome['actual_buy_amount'] == reference['bought'], 'Normal OP Gas or actual Token transfers differ from original client')
                        require(fees['transaction_hash'] == signed['transactionHash'] and outcome.get('native_fees') is None, 'OP fee identity or burn scope changed')
                        before = {a['address'].lower(): a['balance'] for a in request['fixture']['accounts']}
                        for owner, balances in fees['accounts'].items():
                            require(balances['before'] == before[owner] and balances['after'] == reference['accounts'][owner]['balance'], 'Normal OP fee balances differ from source/original client')
                        fee = fees['chain_cost']
                        require(fee['l1_data_fee'] == reference['l1_fee'] and fee['operator_fee'] == reference['operator_fee'], 'Normal OP L1/operator fee differs from original client')
                        require(int(fee['execution_fee']) + int(fee['l1_data_fee']) + int(fee['operator_fee']) == int(fee['gross_charge']), 'Normal OP gross charge is not conserved')
                        require(costs['nativeKnownSubtotal'] == fee['gross_charge'] and costs['payerNetNativeDebit'] == fees['payer_cost']['net_native_debit'], 'Parent costs differ from audited source')
                        require(costs['parentFeeAuditReads'] > 0 and costs['totalUsage']['reads'] == outcome['usage']['reads'] + costs['parentFeeAuditReads'], 'Parent source reads omitted from aggregate budget')
                        require(costs['totalUsage']['read_bytes'] == outcome['usage']['read_bytes'] + costs['parentFeeAuditReadBytes'], 'Parent source bytes omitted from aggregate budget')
                        require(all(costs[key] is None for key in ['totalNativeEstimate', 'approvalNative', 'netBuyAmountEstimate', 'netSellAmountEstimate']), 'Unknown quote economic cost fabricated')
                        for name, value in [('quote', quote), ('build', built), ('signed', signed), ('simulation', simulation), ('original-input', reference_input), ('original', reference)]:
                            write(directory / (name + '.json'), value)
                        rows.append(dict(id=source['id'], quote_kind=simulation['quoteKind'], gas_used=reference['gas_used'],
                                         artifacts={str(p.relative_to(output)): digest(p) for p in directory.glob('*.json')}))
                finally:
                    if process.poll() is None:
                        os.killpg(process.pid, signal.SIGINT)
                        try:
                            process.wait(timeout=3)
                        except subprocess.TimeoutExpired:
                            os.killpg(process.pid, signal.SIGKILL)
                            process.wait()
            require(process.returncode == 0, 'OP HTTP harness did not cleanly exit')
        require(digest(binary) == digest(staged) == binary_sha, 'Actual normal API binary changed during run')
    require(len(rows) == (expected if maximum_groups is None else maximum_groups * 2), 'Actual normal API scenario count changed')
    report = dict(passed=True, cases=rows, actual_worker_executions=len(rows), rejected_requests=rejected,
                  harness_sha256=binary_sha, publicTransactionsSubmitted=False, production_ready=False,
                  all_requested_features_complete=False, scope='NORMAL_HTTP_SIGNED_LOCAL_HISTORICAL_OP_NOT_TRUSTED_BANK_OR_CURRENT_VM')
    write(output / 'report.json', report)
    return report


if __name__ == '__main__':
    import sys
    require(len(sys.argv) == 6, 'Expected HARNESS SOURCES SIGNER TRANSITION OUTPUT')
    run(Path(sys.argv[1]), Path(sys.argv[2]), dict(signer=Path(sys.argv[3]), transition=Path(sys.argv[4])), Path(sys.argv[5]))
