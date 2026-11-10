#!/usr/bin/env python3
"""Run every required breaker stage against one committed source/input freeze."""
from pathlib import Path
import hashlib
import json
import os
import subprocess
import time
import managed_v2_breaker_acceptance as gate
from weighted_evidence import runtime_fingerprint, snapshot_inputs, write_result

ROOT = Path(__file__).resolve().parents[3]
OUT = ROOT / 'integration/artifacts/local'
TARGET = OUT / 'managed-v2-breaker-final-serial-run.json'


def run_serial():
    acceptance = OUT / 'managed-v2-breaker-local-acceptance.json'
    report = dict(schema_version=2, passed=False, state='RUNNING', production_ready=False,
                  all_requested_features_complete=False, started_at_unix_ms=int(time.time() * 1000), commands=[])
    # Invalidate a previous success before even reading the next freeze. Abrupt
    # process death then leaves an explicitly incomplete result, never success.
    write_result(acceptance, dict(report))
    write_result(TARGET, report)
    try:
        freeze_path = OUT / 'managed-v2-breaker-freeze.json'
        frozen_bytes = freeze_path.read_bytes()
        frozen = json.loads(frozen_bytes)
        progress_sha256 = gate.progress_structure_sha256(ROOT)
        gate.validate_progress_binding(frozen, progress_sha256)
        report.update(progress_structure_sha256=progress_sha256, runtime=frozen['runtime'], input_hashes=frozen['input_hashes'],
                      freeze_sha256=hashlib.sha256(frozen_bytes).hexdigest())
        write_result(TARGET, report)

        def unchanged():
            gate.require(gate.progress_structure_sha256(ROOT) == report['progress_structure_sha256'],
                         'progress structure changed during serial run')
            gate.require(hashlib.sha256(freeze_path.read_bytes()).hexdigest() == report['freeze_sha256']
                         and runtime_fingerprint() == frozen['runtime']
                         and snapshot_inputs() == frozen['input_hashes'],
                         'source, binary, declared inputs or freeze changed during serial run')

        unchanged()
        for name, command in gate.COMMANDS:
            stage = dict(stage=name, command=list(command), exit_code=None,
                         started_at_unix_ms=int(time.time() * 1000),
                         log=f'managed-v2-breaker-final-serial-{name}.log')
            report['commands'].append(stage)
            write_result(TARGET, report)
            with (OUT / stage['log']).open('w') as output:
                result = subprocess.run(list(command), cwd=ROOT / 'integration', env=os.environ,
                                        stdout=output, stderr=subprocess.STDOUT)
            stage.update(exit_code=result.returncode, completed_at_unix_ms=int(time.time() * 1000),
                         log_sha256=gate.sha(OUT / stage['log']))
            write_result(TARGET, report)
            gate.require(type(result.returncode) is int and result.returncode == 0,
                         name + ' command failed: ' + str(result.returncode))
            stage['evidence_sha256'] = {path: gate.sha(OUT / path) for path in gate.STAGE_REPORTS.get(name, ())}
            write_result(TARGET, report)
            unchanged()
        report.update(passed=True, state='PASSED', completed_at_unix_ms=int(time.time() * 1000), inputs_unchanged=True)
        gate.validate_serial(report, frozen['runtime'], frozen['input_hashes'], report['freeze_sha256'],
                             lambda name: (OUT / name).read_bytes(), progress_sha256)
        unchanged()
        write_result(TARGET, report)
        return report
    except BaseException as error:
        report.update(passed=False, state='FAILED', error=str(error), error_type=type(error).__name__,
                      completed_at_unix_ms=int(time.time() * 1000))
        write_result(TARGET, report)
        write_result(acceptance, dict(passed=False, state='FAILED', production_ready=False,
                                     error=str(error), error_type=type(error).__name__,
                                     completed_at_unix_ms=report['completed_at_unix_ms']))
        raise


def main():
    run_serial()
    gate.main()


if __name__ == '__main__':
    main()
