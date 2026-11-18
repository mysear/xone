#!/usr/bin/env python3
"""Pinned original precompile corpus plus full signed regular-user transitions."""
import json
from pathlib import Path
import subprocess
from op_chain_cost_vectors import ROOT, SOURCE, original_inputs, digest, require
from op_execution_vectors import cases as execution_cases

RUNNER = ROOT / 'integration/tools/acceptance/reference/op_precompiles.go'
HELPERS = ROOT / 'integration/tools/acceptance/reference/op_execution.go'
FILES = {8: ['bn256Pairing'], 10: ['pointEvaluation'], 11: ['blsG1Add', 'fail-blsG1Add'],
         12: ['blsG1Mul', 'blsG1MultiExp', 'fail-blsG1Mul', 'fail-blsG1MultiExp'],
         13: ['blsG2Add', 'fail-blsG2Add'],
         14: ['blsG2Mul', 'blsG2MultiExp', 'fail-blsG2Mul', 'fail-blsG2MultiExp'],
         15: ['blsPairing', 'fail-blsPairing'], 16: ['blsMapG1', 'fail-blsMapG1'],
         17: ['blsMapG2', 'fail-blsMapG2'], 256: ['p256VerifyFjord']}


def cases(include_jovian=False):
    corpus = SOURCE / 'core/vm/testdata/precompiles'
    rows, refs, samples = [], {}, {}
    for address, names in FILES.items():
        for name in names:
            path = corpus / (name + '.json')
            refs[str(path.relative_to(ROOT))] = digest(path)
            for index, value in enumerate(json.loads(path.read_bytes())):
                data = value['Input']
                gas = value.get('Gas', 30_000_000)
                if name == names[0] and index == 0:
                    samples[address] = data
                for suffix, offered in [('normal', gas), ('low', max(0, gas-1))]:
                    rows.append(dict(id=f'{name}-{index}-{suffix}', fork='isthmus', address=f'0x{address:040x}', data=data, gas=offered, transition=False))
    sources = {rule: next(r for r in execution_cases() if r['rule'] == rule) for rule in ['regolith', 'canyon', 'ecotone', 'fjord', 'granite', 'holocene', 'isthmus']}
    # All seven fork dispatches: before activation a target is an ordinary empty
    # account. The full signed transition must exhibit that exact difference.
    for rule in sources:
        for address in FILES:
            rows.append(dict(id=f'dispatch-{rule}-{address}', fork=rule, address=f'0x{address:040x}', data=samples[address], gas=500_000, transition=True, attributes=sources[rule]['attributes'], timestamp=sources[rule]['timestamp']))
    for address in [10, 11, 12, 13, 14, 15, 16, 17]:
        for label, data, gas in [('empty', '', 500_000), ('low', samples[address], 0), ('corrupt', 'ff' + samples[address][2:], 500_000)]:
            rows.append(dict(id=f'invalid-{address}-{label}', fork='isthmus', address=f'0x{address:040x}', data=data, gas=gas, transition=True, attributes=sources['isthmus']['attributes'], timestamp=sources['isthmus']['timestamp']))
    # Exact OP caps, first malformed byte and first valid-width input beyond the
    # cap. Large all-infinity points keep the test bounded without hiding work.
    for address, cap, width in [(8, 112687, 192), (12, 513760, 160), (14, 488448, 288), (15, 235008, 384)]:
        for label, size in [('last-valid-width', cap//width*width), ('cap', cap), ('cap-plus-one', cap+1), ('first-over-width', (cap//width+1)*width)]:
            rows.append(dict(id=f'cap-{address}-{label}', fork='isthmus', address=f'0x{address:040x}', data='00'*size, gas=30_000_000, transition=True, attributes=sources['isthmus']['attributes'], timestamp=sources['isthmus']['timestamp']))
    require(len(rows) == 3558 and sum(r['transition'] for r in rows) == 110 and len({r['id'] for r in rows}) == len(rows), 'Precompile corpus incomplete')
    if include_jovian:
        from op_chain_cost_vectors import cases as fee_cases
        source = next(r for r in fee_cases() if r['rule'] == 'jovian')
        extra = []
        caps = {8: (81984, 192), 12: (288960, 160), 14: (278784, 288), 15: (156672, 384)}
        for old in rows:
            if old['fork'] != 'isthmus':
                continue
            row = dict(old, id='jovian-' + old['id'], fork='jovian')
            if row['transition']:
                row.update(attributes=source['attributes'], timestamp=source['timestamp'])
            if old['id'].startswith('cap-'):
                address = int(old['address'], 16)
                cap, width = caps[address]
                label = old['id'].split('-', 2)[2]
                size = {'last-valid-width': cap//width*width, 'cap': cap,
                        'cap-plus-one': cap+1, 'first-over-width': (cap//width+1)*width}[label]
                row['data'] = '00' * size
            extra.append(row)
        require(len(extra) == 3498 and sum(r['transition'] for r in extra) == 50, 'Jovian corpus incomplete')
        rows.extend(extra)
    return rows, refs


def audit(inputs, rows):
    jovian = any(r['fork'] == 'jovian' for r in inputs)
    expected, _ = cases(include_jovian=jovian)
    require(inputs == expected and len(rows) == len(inputs), 'Original precompile matrix changed')
    for expected, row in zip(inputs, rows):
        require(row['input'] == expected, 'Original precompile input changed')
        address = int(expected['address'], 16)
        present = address <= 9 or (address == 10 and expected['fork'] not in ['regolith', 'canyon']) or (address == 256 and expected['fork'] in ['fjord', 'granite', 'holocene', 'isthmus', 'jovian']) or (11 <= address <= 17 and expected['fork'] in ['isthmus', 'jovian'])
        require(row['present'] is present, 'Original fork dispatch changed')
        if row['present'] and row['success']:
            require(row['remaining_gas'] + row['required_gas'] == expected['gas'] and not row['out_of_gas'], 'Original precompile gas not conserved')
        if expected['transition']:
            t = row['transition']
            require(t['nonce_after'] == 1 and 21000 <= t['gas_used'] <= t['gas_limit'], 'Original full transition invalid')
            if not row['present']:
                require(t['success'] and t['output'] == '', 'Pre-fork target is not an ordinary account')
            require(sum(map(int, t['before'].values())) == sum(map(int, t['after'].values())), 'Original full fees not conserved')


def write(output, include_jovian=False):
    output = Path(output).resolve()
    output.parent.mkdir(parents=True, exist_ok=False)
    refs = original_inputs()
    inputs, corpus = cases(include_jovian=include_jovian)
    refs.update(corpus)
    for p in [RUNNER, HELPERS]:
        refs[str(p.relative_to(ROOT))] = digest(p)
    module = output.parent / 'runner'
    module.mkdir()
    (module / 'main.go').write_bytes(RUNNER.read_bytes())
    helper = HELPERS.read_text()
    require(helper.count('func main() {') == 1, 'Original helper entry changed')
    (module / 'helpers.go').write_text(helper.replace('func main() {', 'func retainedExecutionMain() {'))
    (module / 'go.mod').write_text('module xone-op-precompile-reference\n\ngo 1.24.0\n\nrequire github.com/ethereum/go-ethereum v0.0.0\n\nreplace github.com/ethereum/go-ethereum => ' + str(SOURCE) + '\n')
    (output.parent / 'inputs.json').write_text(json.dumps(inputs) + '\n')
    p = subprocess.run(['go', 'run', '-mod=mod', '.'], cwd=module, input=json.dumps(inputs), capture_output=True, text=True, timeout=300)
    (output.parent / 'original.stderr').write_text(p.stderr)
    require(p.returncode == 0, 'Original precompile execution failed: ' + p.stderr[-2500:])
    rows = json.loads(p.stdout)
    audit(inputs, rows)
    for path in [output.parent / 'inputs.json', output.parent / 'original.stderr', module / 'main.go', module / 'helpers.go', module / 'go.mod', module / 'go.sum']:
        refs[str(path.relative_to(ROOT))] = digest(path)
    report = dict(passed=True, cases=rows, case_count=len(rows), signed_transitions=sum(r['transition'] for r in inputs), references=refs, production_ready=False, scope='LOCAL_OP_PRECOMPILES_NOT_FULL_FORK_OR_TRUSTED_BANK')
    output.write_text(json.dumps(report) + '\n')
    return refs


def replay(vector, output):
    vector = Path(vector).resolve()
    v = json.loads(vector.read_bytes())
    for name, expected in v['references'].items():
        require(digest(ROOT / name) == expected, 'Pinned original precompile operand changed: ' + name)
    inputs, _ = cases(include_jovian=any(r['input']['fork'] == 'jovian' for r in v['cases']))
    p = subprocess.run(['go', 'run', '-mod=readonly', '.'], cwd=vector.parent / 'runner', input=json.dumps(inputs), capture_output=True, text=True, timeout=300)
    Path(output).with_suffix('.stderr').write_text(p.stderr)
    require(p.returncode == 0, 'Original precompile replay failed: ' + p.stderr[-1500:])
    rows = json.loads(p.stdout)
    require(rows == v['cases'], 'Original precompile replay changed')
    audit(inputs, rows)
    Path(output).write_text(json.dumps(dict(passed=True, cases=len(rows), signed_transitions=v['signed_transitions'], vectors_sha256=digest(vector))) + '\n')


if __name__ == '__main__':
    import sys
    if len(sys.argv) == 4 and sys.argv[1] == '--replay':
        replay(sys.argv[2], sys.argv[3])
    else:
        write(sys.argv[1])
