#!/usr/bin/env python3
"""Freeze offline durable publication journal plus all prior normal API gates."""
import os
import json
from pathlib import Path
import subprocess
import sys
import uuid
from karst_api_vectors import ROOT, digest, require
from validate_price_publications import validate


def go_configs(directory):
    directory = Path(directory).resolve(); directory.mkdir(parents=True, exist_ok=False)
    r = subprocess.run(['go','test','-count=1','-v','./internal/config','-run','^TestDurablePriceJournalConfiguration$'],
        cwd=ROOT/'chain-parser-go', env=dict(os.environ, XONE_PRICE_JOURNAL_CONFIGS=str(directory)), capture_output=True, text=True, timeout=180)
    log = r.stdout+r.stderr; (directory/'go.log').write_text(log); print(log,end='')
    require(r.returncode==0 and '--- PASS: TestDurablePriceJournalConfiguration ' in log, 'Go journal configuration test did not pass')
    rows=json.loads((directory/'hashes.json').read_bytes())
    require([r['name'] for r in rows]==['default','durable'], 'Go journal export incomplete')
    (directory/'report.json').write_text(json.dumps(dict(passed=True,artifacts={p.name:digest(p) for p in directory.iterdir() if p.is_file()}),indent=2)+'\n')


def main(*, router=False):
    output = ROOT/'integration/artifacts/local'/('durable-price-router-check' if router else 'price-publication-journal-check')
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
    extra, required, artifacts = [], {}, []
    http = None
    if router:
        directory = output/'router-runs'/uuid.uuid4().hex
        configs, http = directory/'configs', directory/'http'
        split_receipt = directory/'ordered-split.json'
        extra.append(('durable-price-go-config', [sys.executable,__file__,'--go-configs',str(configs)]))
        extra.append(('ordered-v2-atomic-split', ['node', 'integration/tools/ci/ordered-v2-split-replay.cjs', str(split_receipt),'--bank']))
        bank_env=['env','XONE_ORDERED_V2_REPORT='+str(split_receipt),'XONE_SPLIT_WORKER='+str(target/'debug/examples/evm_dynamic_fee_harness')]
        extra.append(('ordered-v2-split-binding',bank_env+cargo+['test']+manifest+['-p','xone-execution-plan','--test','ordered_v2','--','--include-ignored']))
        extra.append(('ordered-v2-split-poststate',bank_env+cargo+['test']+manifest+['-p','xone-simulation','--test','v2_path_execution','eight_ordered_split_receipts_match_all_logs_and_declared_poststate','--','--include-ignored','--exact']))
        approval_test = 'approval_and_swap_use_consecutive_state_nonce_and_actual_fees'
        extra.append(('ordered-v2-approval-sequence', bank_env+cargo+['test']+manifest+['-p','xone-simulation','--test','v2_path_execution',approval_test,'--','--include-ignored','--exact']))
        required['ordered-v2-approval-sequence'] = [approval_test]
        reset_test = 'reset_then_approve_and_swap_match_three_mined_state_transitions'
        extra.append(('ordered-v2-reset-approval', bank_env + cargo + ['test'] + manifest + [
            '-p', 'xone-simulation', '--test', 'v2_path_execution', reset_test, '--', '--include-ignored', '--exact']))
        required['ordered-v2-reset-approval'] = [reset_test]
        split_http = directory/'ordered-split-http'
        extra.append(('ordered-v2-split-http', [sys.executable, 'integration/tools/acceptance/ordered_split_http.py',
            str(target/'debug/examples/evm_dynamic_fee_harness'), str(split_receipt), str(split_http)]))
        required['ordered-v2-split-poststate']=['eight_ordered_split_receipts_match_all_logs_and_declared_poststate']
        required['ordered-v2-split-binding']=['split_plans_match_mined_calldata_balances_and_pool_order',
            'split_rejects_recommitted_stale_leg_order_and_wrong_amounts','split_rebuild_rejects_substitution_and_missing_source',
            'split_worker_replays_four_mined_plans_and_rejects_inconsistent_sources',
            'exact_approval_calldata_matches_both_mined_sequences','reset_approval_policy_matches_both_mined_sequences',
            'worker_audits_sequential_approval_and_swap_fees']
        for workspace, label in [('dex-router','router'),('chain-parser-rust','parser')]:
            gate = 'durable-price-'+label+'-config'
            test = 'durable_journal_config_matches_go_hashes_and_rejects_invalid_paths'
            extra.append((gate,['env','XONE_PRICE_JOURNAL_CONFIGS='+str(configs)]+cargo+['test','--locked',
                '--manifest-path',workspace+'/Cargo.toml','-p','xone-config','--test','durable_price_journal',test,'--','--include-ignored','--exact']))
            required[gate]=[test]
        test='transaction_economics::publication_tests::durable_publication_revokes_old_readers_while_committing_and_after_lost_ack'
        extra.append(('durable-price-lost-ack',cargo+['test']+manifest+['-p','xone-quote-api','--lib',test,'--','--exact']))
        required['durable-price-lost-ack']=[test]
        artifacts = [str(p.relative_to(ROOT)) for p in [configs/'report.json', http/'report.json', split_receipt, split_http/'report.json']]
    validate(output_base=output, durable_router_output=http, extra_checks=[
        ('price-journal-state', cargo+['test']+manifest+['-p', 'xone-quote-api', '--lib', 'price_publication::tests']),
        ('price-journal-cli-build', cargo+['build']+manifest+['-p', 'xone-quote-api', '--bin', 'xone-price-publication']),
        ('market-book-offline-assembly', ['env', 'XONE_PUBLICATION_CLI='+str(target/'debug/xone-price-publication'),
            'XONE_MARKET_VALIDATOR='+str(target/'debug/xone-market-price'), sys.executable,
            'integration/tools/acceptance/tests/test_market_book_assembly.py', '-v']),
        ('market-book-bounded-schedule', ['env', 'XONE_PUBLICATION_CLI='+str(target/'debug/xone-price-publication'),
            'XONE_MARKET_VALIDATOR='+str(target/'debug/xone-market-price'), sys.executable,
            'integration/tools/acceptance/tests/test_market_book_schedule.py', '-v']),
        ('price-journal-independent-replay', [sys.executable, 'integration/tools/acceptance/price_publication_journal_replay.py',
             str(target/'debug/xone-price-publication'), str(replay)]),
    ]+extra, extra_required={**required,'price-journal-state':['price_publication::tests::'+name for name in tests]},
        extra_artifacts=[str((replay/'report.json').relative_to(ROOT))]+artifacts,
        extra_report=dict(offline_authorized_price_publication_journal_supported=True,
                          offline_journal_hash_chain_and_exclusive_fsync_recovery_supported=True,
                          normal_router_durable_price_publication_connected=router,
                          price_publication_restart_rollback_protected=router,
                          price_publication_restart_protection_requires_explicit_local_journal=True),
        verification_scope=('OPT_IN_NORMAL_ROUTER_LOCAL_DURABLE_PRICE_PUBLICATION_STARTUP_RESTART_AND_LOST_ACK_PLUS_ALL_PRIOR_GATES_NOT_LIVE_MARKET_TRUSTED_BANK_COMPLETE_NET_OR_PRODUCTION' if router else
        'OFFLINE_AUTHORIZED_DURABLE_PRICE_PUBLICATION_HIGH_WATER_HASH_CHAIN_EXCLUSIVE_WRITER_FSYNC_RECOVERY_AND_ALL_PRIOR_NORMAL_API_SOURCE_VM_GATES_NOT_NORMAL_ROUTER_DURABLE_CONNECTION_LIVE_MARKET_TRUSTED_BANK_COMPLETE_NET_OR_PRODUCTION'))


if __name__ == '__main__':
    if sys.argv[1:2]==['--go-configs'] and len(sys.argv)==3: go_configs(sys.argv[2])
    elif sys.argv[1:]==['--router']: main(router=True)
    elif not sys.argv[1:]: main()
    else: raise SystemExit('expected --router or --go-configs DIRECTORY')
