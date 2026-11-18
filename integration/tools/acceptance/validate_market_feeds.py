#!/usr/bin/env python3
"""Freeze approved fixed-block feed authorization plus every prior cost gate."""
import json
import os
from pathlib import Path
import subprocess
import sys
import uuid
from karst_api_vectors import ROOT, digest, require
from market_feed_vectors import build, public_test_policy
from validate_normal_candidate_costs import main as validate


def go_configs(directory):
    directory = Path(directory).resolve()
    directory.mkdir(parents=True, exist_ok=False)
    env = dict(os.environ, XONE_MARKET_FEED_CONFIG_VECTORS=str(directory))
    r = subprocess.run(['go', 'test', './internal/config', '-count=1', '-v', '-run',
                        '^TestApprovedMarketFeedPolicyIdentityAndInvalidConfigurations$'],
                       cwd=ROOT / 'chain-parser-go', env=env, capture_output=True, text=True, timeout=120)
    raw = r.stdout + r.stderr
    (directory / 'go.log').write_text(raw)
    print(raw, end='')
    require(r.returncode == 0 and '--- PASS: TestApprovedMarketFeedPolicyIdentityAndInvalidConfigurations ' in raw,
            'Go approved market policy gate failed')
    vectors = json.loads((directory / 'market-vectors.json').read_bytes())
    require(len(vectors['valid']) == 8 and len(vectors['invalid']) == 14, 'Market configuration matrix changed')
    (directory / 'report.json').write_text(json.dumps(dict(passed=True, valid_configs=8, invalid_configs=14,
        artifacts={str(p.relative_to(directory)): digest(p) for p in directory.iterdir() if p.is_file()}), indent=2)+'\n')


def main(*, output_base=None, extra_report=None, verification_scope=None):
    output = output_base or ROOT / 'integration/artifacts/local/approved-market-feed-check'
    directory = output / 'market-runs' / uuid.uuid4().hex
    directory.mkdir(parents=True, exist_ok=False)
    retained = ROOT / 'integration/artifacts/local/approved-market-feed-precheck-20261119'
    captures = [retained / 'explicit-publicnode']
    references = {}
    for source in captures:
        report = json.loads((source / 'report.json').read_bytes())
        require(report['passed'] is True and report['authorized'] is False, 'Retained public collector qualification required')
        require(json.loads((source / 'policy.json').read_bytes()) == public_test_policy(),
                'Reviewed public TEST deployment policy changed')
        for name, expected in report['artifacts'].items():
            require(digest(source / name) == expected, 'Public raw response changed')
        for p in source.rglob('*'):
            if p.is_file(): references[str(p.relative_to(ROOT))] = digest(p)
    official = retained / 'official'
    manifest = json.loads((official / 'manifest.json').read_bytes())
    expected = {
        'AggregatorV3Interface.sol': '87fd1685e015c2001673b442c212f07847a64afa2f3b22bbbcb7f211ed293e88',
        'AggregatorProxy.sol': '2e387f54834b18fe652a6dc96aa4a0cee938440b93207fcc62450c29cf528ae2',
        'EACAggregatorProxy.sol': '1895b698ad5eb0369bb5ba9881dcacb8ad8be53bcf2ab7033b1c0bfff1286af2',
    }
    require(manifest['commit'] == 'cfc3cc2381a2c83ba0e3242ed740609a800f5fff', 'Official ABI source commit changed')
    for name, h in expected.items():
        require(digest(official / name) == h, 'Official ABI/proxy source changed')
        references[str((official / name).relative_to(ROOT))] = h
    references[str((official / 'manifest.json').relative_to(ROOT))] = digest(official / 'manifest.json')
    vectors = directory / 'vectors.json'
    v = build(vectors, captures)
    require(len(v['synthetic']) == 48 and len(v['public']) == 1, 'Independent market price coverage changed')
    references[str(vectors.relative_to(ROOT))] = digest(vectors)
    configs, replay = directory / 'configs', directory / 'replay'
    target = Path(os.environ.get('CARGO_TARGET_DIR', str(ROOT / 'integration/.runtime/design-review-target')))
    binary = target / 'debug/xone-market-price'
    cargo = ['cargo', '+1.90.0']
    manifest = ['--locked', '--manifest-path', 'dex-router/Cargo.toml']
    parser_manifest = ['--locked', '--manifest-path', 'chain-parser-rust/Cargo.toml']
    guards = ['-m', 'unittest', 'discover', '-s', 'integration/tools/acceptance/tests', '-p', 'test_market_feed_collector.py']
    env = ['env', 'XONE_MARKET_FEED_CONFIG_VECTORS=' + str(configs)]
    validate(output_base=output, extra_references=references,
        extra_artifacts=[str((p/'report.json').relative_to(ROOT)) for p in [configs, replay]], extra_checks=[
            ('market-go-format', [sys.executable, '-c', 'import subprocess,sys;r=subprocess.check_output(["gofmt","-l","chain-parser-go/internal/config/market_feed_test.go","chain-parser-go/internal/config/native_valuation.go"],text=True);print(r,end="");sys.exit(bool(r))']),
            ('market-collector-controls', [sys.executable] + guards),
            ('market-collector-controls-optimized', [sys.executable, '-OO'] + guards),
            ('market-go-policy', [sys.executable, 'integration/tools/acceptance/validate_market_feeds.py', '--go-configs', str(configs)]),
            ('market-router-policy', env + cargo + ['test'] + manifest + ['-p', 'xone-config', '--test', 'market_feed', '--', '--include-ignored']),
            ('market-parser-policy', env + cargo + ['test'] + parser_manifest + ['-p', 'xone-config', '--test', 'market_feed', '--', '--include-ignored']),
            ('market-source-and-integers', ['env', 'XONE_MARKET_FEED_VECTORS=' + str(vectors)] + cargo + ['test'] + manifest + ['-p', 'xone-execution-preparation', '--test', 'market_feed', '--', '--include-ignored']),
            ('market-validator-build', cargo + ['build'] + manifest + ['-p', 'xone-execution-preparation', '--bin', 'xone-market-price']),
            ('market-validator-raw-replay', [sys.executable, 'integration/tools/acceptance/market_feed_replay.py', str(binary), str(vectors), str(replay)] + [str(p) for p in captures]),
        ], extra_required={
            'market-router-policy': ['approved_market_policy_is_optional_and_ethereum_specific', 'approved_market_policy_hashes_and_rejections_match_current_go'],
            'market-parser-policy': ['approved_market_policy_is_optional_and_ethereum_specific', 'approved_market_policy_hashes_and_rejections_match_current_go'],
            'market-source-and-integers': [
                'approved_market_prices_match_public_raw_returns_and_independent_full_width_ratios',
                'market_authorization_rejects_resigned_source_abi_upgrades_time_and_ratio_substitutions',
                'market_feed_age_is_checked_at_execution_without_reinterpreting_capture_time',
                'market_policy_is_ethereum_only_and_feed_evidence_cannot_downgrade_to_plain_rates',
            ],
        }, extra_report=dict(approved_ethereum_chainlink_usd_pair_adapter_supported=True,
            market_feed_policy_bound_to_configuration_and_signatures=True,
            market_public_capture_scenarios=1, market_synthetic_integer_cases=48,
            market_capture_test_keys_are_production_authorities=False,
            market_storage_proofs_supported=False, market_source_consensus_claimed=False,
            continuous_live_market_book_refresh_supported=False, **(extra_report or {})),
        verification_scope=verification_scope or 'APPROVED_ETHEREUM_CHAINLINK_USD_PAIR_FIXED_BLOCK_RAW_ABI_UPGRADE_TIME_PRECISION_AND_SIGNED_RATE_AUTHORIZATION_UNSIGNED_BOUNDED_CAPTURE_PLUS_ALL_NORMAL_CANDIDATE_AND_PRIOR_GATES_NOT_LIVE_REFRESH_TRUSTED_BANK_COMPLETE_QUOTE_COST_OR_PRODUCTION')


if __name__ == '__main__':
    if sys.argv[1:2] == ['--go-configs']:
        go_configs(sys.argv[2])
    else:
        main()
