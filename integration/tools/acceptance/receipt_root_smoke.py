#!/usr/bin/env python3
"""Shared-omission/process proof bound to the exact inputs used in this attempt."""
import hashlib
import json
from pathlib import Path
import secrets
import time

ROOT = Path(__file__).resolve().parents[3]
OUT = ROOT/'integration/artifacts/local'
FAULTS = [
    ('common-tail-original-bloom', 'trailing-log', 'trailing-log', 'RECEIPT_BLOOM_MISMATCH'),
    ('common-tail-rebuilt-bloom', 'trailing-log-rebloom', 'trailing-log-rebloom', 'RECEIPT_ROOT_MISMATCH'),
    ('primary-status-tamper', 'status-flip', None, 'RECEIPT_ROOT_MISMATCH'),
    ('witness-status-tamper', None, 'status-flip', 'RECEIPT_WITNESS_INVALID'),
    ('primary-unsupported-chain-receipt', 'unknown-type', None, 'RECEIPT_TYPE_UNSUPPORTED'),
    ('witness-unsupported-chain-receipt', None, 'unknown-type', 'RECEIPT_WITNESS_INVALID'),
]

def publish(path, value):
    temporary=path.with_name(path.name+'.'+secrets.token_hex(6)+'.tmp')
    try:
        temporary.write_text(json.dumps(value,indent=2)+'\n');temporary.replace(path)
    finally:
        temporary.unlink(missing_ok=True)

def main():
    OUT.mkdir(parents=True, exist_ok=True)
    output=OUT/'receipt-root-three-service-smoke.json'
    pending={'passed':False,'status':'RUNNING','attempt_id':secrets.token_hex(12),'production_ready':False,'measured_at_unix_ms':int(time.time()*1000)}
    try:
        # Invalidate earlier success before importing any fixture/harness code.
        publish(output,pending)
        from receipt_root_evidence import snapshot_inputs
        inputs=snapshot_inputs(ROOT)
        from receipt_witness_smoke import run
        report=run(prefix='receipt-root',faults=FAULTS)
        if len(report['checks'])!=13 or not report['passed']:
            raise RuntimeError('incomplete receipt-root process coverage')
        if snapshot_inputs(ROOT)!=inputs:
            raise RuntimeError('process inputs changed during execution')
        report.update(profile='ethereum-receipt-root-real-pg-kafka',
                      attempt_id=pending['attempt_id'],input_hashes=inputs,
                      tool_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
                      harness_sha256=hashlib.sha256((Path(__file__).parent/'receipt_witness_smoke.py').read_bytes()).hexdigest(),
                      header_trust='RPC_ADVERTISED_NOT_INDEPENDENT_CONSENSUS',
                      measured_at_unix_ms=int(time.time()*1000))
        publish(output,report);print(json.dumps(report,indent=2))
    except BaseException as exc:
        failed={**pending,'status':'FAILED','error_type':type(exc).__name__,'error':str(exc),'measured_at_unix_ms':int(time.time()*1000)}
        try:publish(output,failed)
        except OSError:pass
        raise

if __name__=='__main__':main()
