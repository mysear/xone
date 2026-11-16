#!/usr/bin/env python3
"""Validate incremental graph publication against full builds and replica barriers."""
import hashlib
import json
import os
from pathlib import Path
import re
import signal
import subprocess
import sys
import time
import uuid

ROOT = Path(__file__).resolve().parents[3]
BASE = ROOT / 'integration/artifacts/local/incremental-graph-check'


def fingerprint():
    paths = subprocess.check_output(
        ['git', 'ls-files', '-co', '--exclude-standard', '-z'], cwd=ROOT).split(b'\0')
    digest = hashlib.sha256()
    for raw in sorted(set(paths)):
        if raw:
            path = ROOT / os.fsdecode(raw)
            if path.is_file() and path.suffix != '.md':
                digest.update(raw + b'\0' + path.read_bytes())
    return digest.hexdigest()


def test_inputs():
    inputs = {}
    for directory in ['dex-router/test/contracts', 'dex-router/test/fixtures', 'integration/test/contracts']:
        for path in sorted((ROOT / directory).rglob('*.json')):
            digest = hashlib.sha256()
            with path.open('rb') as stream:
                for block in iter(lambda: stream.read(1024 * 1024), b''):
                    digest.update(block)
            inputs[str(path.relative_to(ROOT))] = digest.hexdigest()
    return inputs


def main():
    out = BASE / 'runs' / uuid.uuid4().hex
    out.mkdir(parents=True, exist_ok=False)
    env = dict(os.environ, PATH=str(Path.home() / '.cargo/bin') + os.pathsep + os.environ['PATH'])
    env.setdefault('CARGO_BUILD_JOBS', '2')
    env.setdefault('RUST_TEST_THREADS', '2')
    cargo = ['cargo', '+1.90.0']
    manifest = ['--locked', '--manifest-path', 'dex-router/Cargo.toml']
    checks = [
        ('format', cargo + ['fmt', '--all', '--check', '--manifest-path', 'dex-router/Cargo.toml']),
        ('lint', cargo + ['clippy'] + manifest + ['-p', 'xone-routing', '-p', 'xone-state-replica', '-p', 'xone-optimizer', '-p', 'xone-quote-api', '--all-targets', '--', '-D', 'warnings']),
        ('routing-and-replica', cargo + ['test'] + manifest + ['-p', 'xone-routing', '-p', 'xone-state-replica']),
        ('optimizer', cargo + ['test'] + manifest + ['-p', 'xone-optimizer']),
        ('api', cargo + ['test'] + manifest + ['-p', 'xone-quote-api', '--test', 'api', '--test', 'exact_out', '--test', 'managed_v2_scope', '--test', 'composable_exact_out', '--test', 'singleton_exact_out']),
        ('contracts', [sys.executable, 'integration/tools/ci/contracts.py']),
        ('progress', [sys.executable, '-m', 'unittest', 'discover', '-s', 'integration/tools/ci/tests', '-p', 'test_progress_document.py']),
        ('progress-optimized', [sys.executable, '-OO', '-m', 'unittest', 'discover', '-s', 'integration/tools/ci/tests', '-p', 'test_progress_document.py']),
    ]
    report = dict(passed=False, state='RUNNING', source_tree_sha256=fingerprint(),
                  run_directory=str(out.relative_to(ROOT)), started_ms=int(time.time() * 1000),
                  checks=[], test_inputs=test_inputs(), publicTransactionsSubmitted=False, production_ready=False,
                  all_requested_features_complete=False,
                  verification_scope='INCREMENTAL_GRAPH_LOCAL_REPLICA_AND_QUOTE_REGRESSION')

    def save():
        text = json.dumps(report, indent=2) + '\n'
        (out / 'report.json').write_text(text)
        temp = BASE / 'report.tmp'
        temp.write_text(text)
        temp.replace(BASE / 'report.json')

    def interrupted(signum, frame):
        raise KeyboardInterrupt(f'Interrupted by signal {signum}')

    signal.signal(signal.SIGTERM, interrupted)
    signal.signal(signal.SIGINT, interrupted)
    save()
    try:
        for name, command in checks:
            print('Running ' + name, flush=True)
            with (out / f'{name}.log').open('wb') as log:
                child = subprocess.Popen(command, cwd=ROOT, env=env, stdout=log,
                                         stderr=subprocess.STDOUT, start_new_session=True)
                try:
                    code = child.wait()
                finally:
                    if child.poll() is None:
                        os.killpg(child.pid, signal.SIGTERM)
                        try:
                            child.wait(timeout=2)
                        except subprocess.TimeoutExpired:
                            os.killpg(child.pid, signal.SIGKILL)
                            child.wait()
            raw = (out / f'{name}.log').read_bytes()
            if b'\0' in raw:
                code = 1
            count = sum(int(n) for n in re.findall(r'test result: ok\. (\d+) passed;', raw.decode(errors='replace')))
            report['checks'].append(dict(name=name, exit_code=code, rust_test_executions=count,
                                        log_sha256=hashlib.sha256(raw).hexdigest()))
            save()
            if code:
                raise RuntimeError(f'{name} failed: {out / (name + ".log")}')
        if fingerprint() != report['source_tree_sha256'] or test_inputs() != report['test_inputs']:
            raise RuntimeError('Source or test inputs changed during validation')
        report.update(passed=True, state='LOCAL_SCOPE_VERIFIED',
                      rust_test_executions=sum(c['rust_test_executions'] for c in report['checks']))
    except KeyboardInterrupt:
        report.update(state='CANCELLED', error='INTERRUPTED')
        raise
    except Exception as exc:
        report.update(state='FAILED', error=str(exc))
        raise
    finally:
        report['finished_ms'] = int(time.time() * 1000)
        save()
    print(json.dumps({k: report[k] for k in ['state', 'rust_test_executions', 'run_directory']}))


if __name__ == '__main__':
    main()
