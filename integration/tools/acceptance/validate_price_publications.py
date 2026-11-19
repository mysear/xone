#!/usr/bin/env python3
"""Freeze process-local signed book publication plus every previous source gate."""
import json
import os
from pathlib import Path
import subprocess
import sys
import uuid
from karst_api_vectors import ROOT, digest, require
from native_valuation_publications import FIXTURES, build
from validate_market_feeds import main


def validate(*, output_base=None, extra_checks=(), extra_required=None, extra_references=None,
             extra_artifacts=(), extra_report=None, verification_scope=None):
    output = output_base or ROOT/'integration/artifacts/local/price-publication-check'
    directory = output/'publication-runs'/uuid.uuid4().hex
    directory.mkdir(parents=True, exist_ok=False)
    expected = json.dumps(build(), indent=2)+'\n'
    for fixture in FIXTURES:
        if not fixture.exists(): fixture.write_text(expected)
        require(fixture.read_text() == expected, 'Independent publication fixtures changed')
    corpus = directory/'original.json'
    result = subprocess.run(['node', 'integration/tools/ci/candidate-cost-execution-reference.cjs', str(corpus)],
                            cwd=ROOT, capture_output=True, text=True, timeout=180)
    (directory/'generation.log').write_text(result.stdout+result.stderr)
    require(result.returncode == 0, 'Independent publication competing-path source failed')
    refs = dict(extra_references or {})
    refs.update(json.loads(corpus.read_bytes())['inputs'])
    refs[str(corpus.relative_to(ROOT))] = digest(corpus)
    for fixture in FIXTURES: refs[str(fixture.relative_to(ROOT))] = digest(fixture)
    target = Path(os.environ.get('CARGO_TARGET_DIR', str(ROOT/'integration/.runtime/design-review-target')))
    binary = target/'debug/examples/evm_dynamic_fee_harness'
    cli = target/'debug/xone-market-price'
    http, cli_output = directory/'http', directory/'cli'
    main(output_base=output, extra_references=refs,
        extra_artifacts=list(extra_artifacts)+[str((p/'report.json').relative_to(ROOT)) for p in [http,cli_output]],
        extra_checks=[
            ('price-publication-reference', [sys.executable, 'integration/tools/acceptance/native_valuation_publications.py', '--check']),
            ('price-publication-state', ['cargo', '+1.90.0', 'test', '--locked', '--manifest-path', 'dex-router/Cargo.toml', '-p', 'xone-quote-api', '--lib', 'transaction_economics::publication_tests']),
            ('price-publication-cli', [sys.executable, 'integration/tools/acceptance/native_valuation_publications.py', '--cli', str(cli), str(cli_output)]),
            ('price-publication-http', [sys.executable, 'integration/tools/acceptance/price_publication_http.py', str(binary), str(corpus), str(http)]),
        ]+list(extra_checks), extra_required={**(extra_required or {}), 'price-publication-state':[
            'transaction_economics::publication_tests::signed_publications_are_atomic_monotonic_and_preserve_old_snapshot_arithmetic',
            'transaction_economics::publication_tests::publication_cancellation_deadline_and_concurrent_readers_keep_one_complete_book',
        ]}, extra_report=dict(selected_candidate_quote_cost_disclosure_supported=True,
            selected_costs_do_not_include_complete_quote_operations=True,
            process_local_signed_price_book_publication_supported=True,
            quote_price_book_snapshot_and_cache_identity_bound=True,
            price_publication_revision_is_signed=True,
            price_publication_restart_rollback_protected=False, **(extra_report or {})),
        verification_scope=verification_scope or 'SIGNED_PROCESS_LOCAL_ATOMIC_BOOK_PUBLICATION_SINGLE_QUOTE_PINNED_CANDIDATES_CACHE_AND_OLD_QUOTE_EXECUTION_BINDING_PLUS_ALL_PRIOR_SOURCE_VM_GATES_NOT_DURABLE_RESTART_ROLLBACK_CONTINUOUS_LIVE_BANK_COMPLETE_NET_OR_PRODUCTION')


if __name__ == '__main__': validate()
