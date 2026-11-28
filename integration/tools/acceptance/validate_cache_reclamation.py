#!/usr/bin/env python3
"""Validate baseline graph sharing, idle cache reclamation and worker cleanup."""
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
BASE = ROOT / 'integration/artifacts/local/cache-reclamation-check'


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


def sha_file(path):
    digest = hashlib.sha256()
    with Path(path).open('rb') as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b''):
            digest.update(block)
    return digest.hexdigest()


def runtime_inputs():
    path = ROOT / 'dex-router/test/contracts/sbf-runtime/token2022.json'
    manifest = json.loads(path.read_bytes())
    inputs = {str(path.relative_to(ROOT)): sha_file(path)}
    declared = [(Path(manifest['python_executable']['path']), manifest['python_executable']['sha256'])]
    declared.extend((Path(manifest['runtime_directory']) / name, digest)
                    for name, digest in manifest['runtime_files'].items())
    declared.extend((Path(bank['path']), bank['sha256']) for bank in manifest['banks'])
    for source, expected in declared:
        if not source.is_absolute():
            source = ROOT / source
        actual = sha_file(source)
        if actual != expected:
            raise ValueError('Pinned runtime input changed: ' + str(source))
        inputs[str(source)] = actual
    return inputs


def test_inputs():
    # Include every operand of contracts.py, not just the Router's JSON banks.
    inputs = {}
    directories = [
        'chain-parser-go/test/contracts', 'chain-parser-go/test/fixtures',
        'chain-parser-rust/test/contracts', 'chain-parser-rust/test/fixtures',
        'dex-router/test/contracts', 'dex-router/test/fixtures',
        'integration/test/contracts', 'integration/test/evm', 'integration/test/solana',
    ]
    for directory in directories:
        for path in sorted((ROOT / directory).rglob('*')):
            if path.is_file():
                inputs[str(path.relative_to(ROOT))] = sha_file(path)
    source = ROOT / 'integration/test/source'
    manifest = json.loads((source / 'chain-receipt-vectors/manifest.json').read_bytes())
    selected = [
        Path('chain-receipt-vectors/manifest.json'),
        Path('ethereum-mainnet/uniswap-v2-26355865-evidence.json'),
        Path('current-v2-public/manifest.json'),
        Path('current-v2-public/responses.json.gz'),
        Path('current-v2-execution-public/manifest.json'),
        Path('current-v2-execution-public/responses.json.gz'),
        Path('current-v2-execution-public/extra-proofs.json'),
    ]
    for entry in manifest['files']:
        relative = Path(entry['path'])
        if relative.is_absolute() or '..' in relative.parts or not relative.parts:
            raise ValueError('Nonlocal retained source fixture')
        selected.append(Path('chain-receipt-vectors') / relative)
    for relative in selected:
        path = source / relative
        inputs[str(path.relative_to(ROOT))] = sha_file(path)
    for name in ['Pool.sol', 'Reference.sol']:
        path = ROOT / 'integration/tools/reference/velodrome-v2' / name
        inputs[str(path.relative_to(ROOT))] = sha_file(path)
    # Documentation checks also consume retained compiler/source artifacts.
    documents = [ROOT / 'README.md', ROOT / 'DEVELOPMENT_PROGRESS.md', ROOT / 'integration/README.md']
    documents.extend((ROOT / 'docs').rglob('*.md'))
    for document in documents:
        text = re.sub(r'```.*?```', '', document.read_text(), flags=re.S)
        for link in re.findall(r'\]\(([^\s)]+)\)', text):
            if re.match(r'^[a-zA-Z]+:', link) or link.startswith('#'):
                continue
            path = Path(os.path.normpath(document.parent / link.split('#', 1)[0]))
            try:
                relative = path.relative_to(ROOT)
            except ValueError:
                continue
            if str(relative).startswith(('integration/tools/reference/', 'integration/test/source/')) and path.is_file():
                inputs[str(relative)] = sha_file(path)
    return inputs


def main():
    out = BASE / 'runs' / uuid.uuid4().hex
    out.mkdir(parents=True, exist_ok=False)
    env = dict(os.environ, PATH=str(Path.home() / '.cargo/bin') + os.pathsep + os.environ['PATH'])
    env.setdefault('CARGO_BUILD_JOBS', '2')
    env.setdefault('RUST_TEST_THREADS', '2')
    env.setdefault('XONE_SOLANA_PYTHON_PATH', str(ROOT / 'integration/artifacts/local/r01-solana-rebuild/python'))
    cargo = ['cargo', '+1.90.0']
    manifest = ['--locked', '--manifest-path', 'dex-router/Cargo.toml']
    checks = [
        ('format', cargo + ['fmt', '--all', '--check', '--manifest-path', 'dex-router/Cargo.toml']),
        ('contracts', [sys.executable, 'integration/tools/ci/contracts.py']),
        ('progress', [sys.executable, '-m', 'unittest', 'discover', '-s', 'integration/tools/ci/tests', '-p', 'test_progress_document.py']),
        ('progress-optimized', [sys.executable, '-OO', '-m', 'unittest', 'discover', '-s', 'integration/tools/ci/tests', '-p', 'test_progress_document.py']),
        ('replica', cargo + ['test'] + manifest + ['-p', 'xone-state-replica']),
        ('cache-lifetimes', cargo + ['test'] + manifest + ['-p', 'xone-quote-api', '--lib', 'cache_gc::']),
        ('api-regression', cargo + ['test'] + manifest + ['-p', 'xone-quote-api', '--test', 'api', '--test', 'exact_out', '--test', 'transaction_lifecycle']),
        ('lint', cargo + ['clippy'] + manifest + ['-p', 'xone-state-replica', '-p', 'xone-quote-api', '--all-targets', '--', '-D', 'warnings']),
    ]

    required_tests = {
        'replica': [
            'baseline_scope_expansion_reuses_untouched_edges_and_revokes_old_generation',
            'numeric_rebaseline_reuses_graph_without_reusing_previous_validity_flag',
        ],
        'cache-lifetimes': [
            'cache_gc::tests::expiry_boundary_prunes_all_three_caches_and_retains_live_entries',
            'cache_gc::tests::revocation_reclaims_cached_view_and_account_capture_without_more_requests',
            'cache_gc::tests::in_flight_handle_survives_cache_reclamation_until_request_drops_it',
            'cache_gc::tests::reclaimed_values_are_destroyed_after_cache_lock_is_released',
            'cache_gc::task_tests::maintenance_runs_without_quote_traffic_and_can_be_cancelled',
            'cache_gc::task_tests::dropping_server_task_guard_aborts_its_workers',
            'cache_gc::task_tests::normal_http_service_reclaims_idle_caches_and_stops_maintenance_on_shutdown',
        ],
    }

    report = dict(passed=False, state='RUNNING', source_tree_sha256=fingerprint(),
                  run_directory=str(out.relative_to(ROOT)), started_ms=int(time.time() * 1000),
                  checks=[], test_inputs={}, runtime_inputs={}, publicTransactionsSubmitted=False, production_ready=False,
                  all_requested_features_complete=False,
                  verification_scope='BASELINE_GRAPH_REUSE_IDLE_CACHE_RECLAMATION_LOCAL')

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
        report.update(test_inputs=test_inputs(), runtime_inputs=runtime_inputs())
        save()
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
            text = raw.decode(errors='replace')
            count = sum(int(n) for n in re.findall(r'test result: ok\. (\d+) passed;', text))
            passed_tests = set(re.findall(r'^test ([A-Za-z0-9_:]+) \.\.\. ok$', text, re.M))
            missing = [test for test in required_tests.get(name, []) if test not in passed_tests]
            if missing:
                code = 1
            report['checks'].append(dict(name=name, exit_code=code, rust_test_executions=count,
                                        missing_required_tests=missing,
                                        log_sha256=hashlib.sha256(raw).hexdigest()))
            save()
            if code:
                raise RuntimeError(f'{name} failed: {out / (name + ".log")}')
        if (fingerprint() != report['source_tree_sha256']
                or test_inputs() != report['test_inputs']
                or runtime_inputs() != report['runtime_inputs']):
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
