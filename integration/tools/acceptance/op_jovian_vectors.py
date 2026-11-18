#!/usr/bin/env python3
"""Explicit OP Mainnet historical Jovian signed-user references, not block consensus."""
import json
from pathlib import Path
import subprocess
from op_chain_cost_vectors import ROOT, SOURCE, cases as fee_cases, digest, require
from op_execution_vectors import write as execution_write, audit as execution_audit
from op_precompile_vectors import write as precompile_write, replay as precompile_replay


def cases():
    rows = []
    for row in fee_cases():
        if row['rule'] == 'jovian':
            rows.append(dict(row, chain=10, access=row['kind'] != 'legacy' and len(rows) % 3 == 0))
    require(len(rows) == 64 and len({r['id'] for r in rows}) == 64, 'Jovian coverage changed')
    return rows


def audit(rows):
    execution_audit(rows, cases())
    for row in rows:
        scalar = int(row['input']['attributes'][328:336], 16)
        constant = int(row['input']['attributes'][336:352], 16)
        require(int(row['operator_maximum']) == row['input']['gas_limit'] * scalar * 100 + constant, 'Jovian maximum operator units changed')
        for name in ['actual', 'exact_funding']:
            require(int(row[name]['operator_fee']) == row[name]['gas_used'] * scalar * 100 + constant, 'Jovian actual operator units changed')


def write(directory):
    directory = Path(directory).resolve()
    execution = directory / 'execution/vectors.json'
    result = execution_write(execution, selected=cases())
    audit(json.loads(execution.read_bytes())['cases'])
    refs = result['references']
    refs[str(execution.relative_to(ROOT))] = digest(execution)
    crypto = directory / 'precompiles/vectors.json'
    refs.update(precompile_write(crypto, include_jovian=True))
    refs[str(crypto.relative_to(ROOT))] = digest(crypto)
    # Schedule and kill switch are immutable original-source operands too.
    for name in ['superchain/superchain-configs.zip', 'consensus/beacon/consensus.go', 'README.md']:
        p = SOURCE / name
        refs[str(p.relative_to(ROOT))] = digest(p)
    return refs


def replay(directory, output):
    directory = Path(directory).resolve()
    vector = directory / 'execution/vectors.json'
    v = json.loads(vector.read_bytes())
    for p, h in v['references'].items():
        require(digest(ROOT / p) == h, 'Original Jovian operand changed: ' + p)
    result = subprocess.run(['go', 'run', '-mod=readonly', '.'], cwd=vector.parent / 'runner', input=json.dumps(cases()), text=True, capture_output=True, timeout=300)
    Path(output).with_suffix('.stderr').write_text(result.stderr)
    require(result.returncode == 0, 'Original Jovian replay failed: ' + result.stderr[-2000:])
    rows = json.loads(result.stdout)
    require(rows == v['cases'], 'Original Jovian replay changed')
    audit(rows)
    precompile_replay(directory / 'precompiles/vectors.json', Path(output).with_suffix('.precompiles.json'))
    Path(output).write_text(json.dumps(dict(passed=True, signed_scenarios=64, signed_executions=128, insufficient_funding=sum(r['below_funding'] is not None for r in rows), original_execution_sha256=digest(vector))) + '\n')


if __name__ == '__main__':
    import sys
    if len(sys.argv) == 4 and sys.argv[1] == '--replay':
        replay(sys.argv[2], sys.argv[3])
    else:
        write(sys.argv[1])
