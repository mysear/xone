#!/usr/bin/env python3
"""Freeze offline durable publication journal plus all prior normal API gates."""
import os
from pathlib import Path
import sys
import uuid
from karst_api_vectors import ROOT
from validate_price_publications import validate


def main():
    output = ROOT/'integration/artifacts/local/price-publication-journal-check'
    replay = output/'journal-runs'/uuid.uuid4().hex
    target = Path(os.environ.get('CARGO_TARGET_DIR', str(ROOT/'integration/.runtime/design-review-target')))
    cargo = ['cargo', '+1.90.0']
    manifest = ['--locked', '--manifest-path', 'dex-router/Cargo.toml']
    tests = ['journal_publications_require_fresh_signed_positive_same_anchor_books',
             'durable_high_water_is_exclusive_monotonic_and_bound_after_restart',
             'torn_tail_recovers_but_complete_corruption_and_missing_bootstrap_fail',
             'uncertain_writes_close_the_writer_and_recovery_observes_actual_file_state',
             'journal_replacement_nonregular_handles_and_capacity_are_rejected',
             'committed_high_water_survives_process_exit_without_drop']
    validate(output_base=output, extra_checks=[
        ('price-journal-state', cargo+['test']+manifest+['-p', 'xone-quote-api', '--lib', 'price_publication::tests']),
        ('price-journal-cli-build', cargo+['build']+manifest+['-p', 'xone-quote-api', '--bin', 'xone-price-publication']),
        ('price-journal-independent-replay', [sys.executable, 'integration/tools/acceptance/price_publication_journal_replay.py',
             str(target/'debug/xone-price-publication'), str(replay)]),
    ], extra_required={'price-journal-state':['price_publication::tests::'+name for name in tests]},
        extra_artifacts=[str((replay/'report.json').relative_to(ROOT))],
        extra_report=dict(offline_authorized_price_publication_journal_supported=True,
                          offline_journal_hash_chain_and_exclusive_fsync_recovery_supported=True,
                          normal_router_durable_price_publication_connected=False),
        verification_scope='OFFLINE_AUTHORIZED_DURABLE_PRICE_PUBLICATION_HIGH_WATER_HASH_CHAIN_EXCLUSIVE_WRITER_FSYNC_RECOVERY_AND_ALL_PRIOR_NORMAL_API_SOURCE_VM_GATES_NOT_NORMAL_ROUTER_DURABLE_CONNECTION_LIVE_MARKET_TRUSTED_BANK_COMPLETE_NET_OR_PRODUCTION')


if __name__ == '__main__': main()
