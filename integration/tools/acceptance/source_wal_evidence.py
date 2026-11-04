"""Recheck source-bound raw RPC evidence without granting production acceptance."""
from pathlib import Path
import hashlib, json, sys, time
ROOT = Path(__file__).resolve().parents[3]
SCOPE_REPORTS = ['three-service-smoke', 'journal-integration', 'kafka-three-service-smoke', 'reorg-smoke', 'reorg-http-smoke', 'v3-differential', 'v3-three-service-smoke', 'multidex-differential', 'multidex-three-service-smoke', 'stable-differential', 'stable-three-service-smoke', 'singleton-differential', 'singleton-three-service-smoke', 'raydium-differential', 'raydium-three-service-smoke', 'orca-differential', 'orca-three-service-smoke', 'rayclmm-differential', 'rayclmm-three-service-smoke', 'rayclmm-token-fee-differential', 'rayclmm-token-fee-three-service-smoke', 'rayclmm-bitmap-differential', 'rayclmm-bitmap-three-service-smoke', 'rayclmm-seed-differential', 'rayclmm-seed-three-service-smoke', 'rayclmm-dynamic-differential', 'rayclmm-dynamic-three-service-smoke', 'receipt-witness-three-service-smoke', 'source-wal-three-service-smoke']
PROCESS_CHECKS = ['normal-rust-rpc-raw-source-evidence-before-typed-baseline-and-both-native-quote-directions', 'authenticated-non-readiness-evidence-status-redacts-source-credentials-and-deduplicates-unchanged-rpc', 'source-wal-capacity-retains-raw-prefix-rejects-new-baseline-and-revokes-solana-only', 'parser-restart-verifies-prior-record-chain-starts-new-session-and-restores-only-from-fresh-recorded-rpc', 'new-bank-is-recorded-within-bounds-and-typed-quote-state-keeps-native-math', 'real-marker-commit-failure-poisons-writer-and-cannot-bypass-via-dedup-or-in-place-file-repair', 'reopen-verifies-db-ahead-unacknowledged-suffix-and-new-session-before-fresh-qualification', 'corrupt-source-head-startup-is-rejected-and-old-kv-cannot-resurrect-revoked-quotes', 'restored-exact-durable-head-and-new-recorded-rpc-start-new-qualified-session-after-corruption', 'memory-only-router-restores-owner-checkpoint-without-treating-source-wal-as-online-state']

def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

def validate(root, verification, report, source_digest):
    root = Path(root)
    out = root / 'integration/artifacts/local'
    if not (verification['passed'] and verification['source_unchanged_during_verification']):
        raise AssertionError('source WAL evidence validation failed')
    if not (report['passed'] and report['source_unchanged_during_verification']):
        raise AssertionError('source WAL evidence validation failed')
    if not source_digest == verification['source_tree_sha256'] == report['source_tree_sha256']:
        raise AssertionError('source changed after verification')
    if not report['fresh_scope_reports'] == SCOPE_REPORTS:
        raise AssertionError('regression scope set changed')
    if not set(report['scope_report_hashes']) == {name + '.json' for name in SCOPE_REPORTS}:
        raise AssertionError('source WAL evidence validation failed')
    for name in SCOPE_REPORTS:
        path = out / (name + '.json')
        value = json.loads(path.read_text())
        if not value['passed']:
            raise AssertionError(name)
        if not path.stat().st_mtime * 1000 >= verification['measured_at_unix_ms']:
            raise AssertionError('stale scope ' + name)
        measured = value.get('measured_at_unix_ms')
        if not (isinstance(measured, int) and (not isinstance(measured, bool)) and (measured >= verification['measured_at_unix_ms'])):
            raise AssertionError('old or missing generation time ' + name)
        if not sha(path) == report['scope_report_hashes'][path.name]:
            raise AssertionError('scope proof changed ' + name)
    review_file = out / 'source-wal-review.json'
    review = json.loads(review_file.read_text())
    if not sha(review_file) == report['review_sha256']:
        raise AssertionError('review changed')
    for stage in ['spec_review', 'quality_review', 'evidence_gate_review']:
        if not (review[stage]['status'] == 'CLOSED' and all((f['status'] == 'CLOSED' for f in review[stage].get('findings', [])))):
            raise AssertionError('source WAL evidence validation failed')
    process = json.loads((out / 'source-wal-three-service-smoke.json').read_text())
    if not (process['checks'] == PROCESS_CHECKS and report['process_checks'] == 10):
        raise AssertionError('process coverage incomplete')
    if not process['tool_sha256'] == sha(root / 'integration/tools/acceptance/source_wal_smoke.py'):
        raise AssertionError('process tool changed')
    status = process['source_evidence_status']
    if not status == report['final_evidence_status']:
        raise AssertionError('source WAL evidence validation failed')
    if not (status['session_id'] == 4 and status['records'] > 0 and (0 < status['retained_bytes'] <= status['max_retained_bytes'])):
        raise AssertionError('source WAL evidence validation failed')
    if not (status['evidence_only'] is True and status['confers_readiness'] is False and (status['write_poisoned'] is False)):
        raise AssertionError('source WAL evidence validation failed')
    if not (report['production_ready'] is False and report['full_work_packages_accepted'] == 0 and report['limits']):
        raise AssertionError('source WAL evidence validation failed')
    if not (report['spec_review_closed'] and report['quality_review_closed']):
        raise AssertionError('source WAL evidence validation failed')
    return True

def run():
    from rayclmm_dynamic_evidence import source_hash
    sys.path.insert(0, str(ROOT / 'integration/tools'))
    from progress_document import load_progress
    out = ROOT / 'integration/artifacts/local'
    load = lambda name: json.loads((out / name).read_text())
    verification = load('verification.json')
    report = load('source-wal-acceptance.json')
    digest = source_hash()
    validate(ROOT, verification, report, digest)
    progress = load_progress(ROOT)
    row = next((r for r in progress['requirement_traceability'] if r['design_clause'] == '8.9.solana-rpc-source-wal'))
    if not (row['status'] == 'PARTIAL_LOCAL' and set(row['work_packages']) == {'WP03', 'WP05', 'WP20', 'WP24'}):
        raise AssertionError('source WAL evidence validation failed')
    if not not any((w['full_acceptance_passed'] for w in progress['work_packages'])):
        raise AssertionError('source WAL evidence validation failed')
    if not progress['current_batch']['source_tree_sha256'] == digest:
        raise AssertionError('source WAL evidence validation failed')
    result = {'passed': True, 'source_tree_sha256': digest, 'fresh_scope_reports': len(SCOPE_REPORTS), 'process_checks': 10, 'full_work_packages_accepted': 0, 'production_ready': False, 'measured_at_unix_ms': int(time.time() * 1000)}
    (out / 'source-wal-evidence-check.json').write_text(json.dumps(result, indent=2) + '\n')
    return result
if __name__ == '__main__':
    print(json.dumps(run(), indent=2))
