#!/usr/bin/env python3
"""Frozen configured oracle authorization, scoped economics and all OP gates."""
import json
import os
from pathlib import Path
import subprocess
import sys
import uuid
from karst_api_vectors import ROOT,prepare,digest,require
from validate_karst_api import main as validate


def go_configs(directory):
    directory=Path(directory).resolve();directory.mkdir(parents=True,exist_ok=False)
    env=dict(os.environ,XONE_NATIVE_VALUATION_CONFIG_VECTORS=str(directory))
    r=subprocess.run(['go','test','./internal/config','-count=1','-v'],cwd=ROOT/'chain-parser-go',env=env,capture_output=True,text=True,timeout=300)
    raw=r.stdout+r.stderr;(directory/'go.log').write_text(raw);print(raw,end='')
    require(r.returncode==0 and all('--- PASS: '+name+' ' in raw for name in ['TestOraclePolicyRejectsWrongChainsKeysQuorumsAndBackend','TestOracleAuthoritiesArePartOfPublicAndEconomicIdentity']),'Go oracle configuration gate failed')
    doc=json.loads((directory/'vectors.json').read_bytes());require(len(doc['vectors'])==4,'Oracle config identity coverage changed')
    (directory/'report.json').write_text(json.dumps(dict(passed=True,configs=4,artifacts={str(p.relative_to(directory)):digest(p) for p in directory.iterdir() if p.is_file()}),indent=2)+'\n')


def main(*, output_base=None, extra_checks=(), extra_required=None, extra_report=None, verification_scope='CONFIGURED_ORACLE_QUORUM_SOURCE_BLOCK_TIME_BOUND_ATOMIC_CONVERSION_AND_EVALUATED_TRANSACTION_ECONOMICS_PLUS_FULL_KARST_HISTORICAL_REGRESSION_NOT_LIVE_MARKET_TRUSTED_BANK_ALL_CANDIDATE_NET_RANKING_OR_PRODUCTION'):
    output=output_base or ROOT/'integration/artifacts/local/native-valuation-check';directory=output/'original-runs'/uuid.uuid4().hex
    v=prepare(directory);refs={}
    prior=ROOT/'integration/artifacts/local/karst-api-check/runs/ecb91103ff6b4430ab10292f747e93b2/report.json';old=json.loads(prior.read_bytes());require(old['passed'] is True,'Completed normal Karst qualification required')
    refs[str(prior.relative_to(ROOT))]=digest(prior)
    for n,h in old['reference_inputs'].items():
        if '/original-client/signer/' in n:
            require(digest(ROOT/n)==h,'Original signer input changed');refs[n]=h
    signer=ROOT/next(n for n in refs if n.endswith('/original-client/signer/runner'))
    gate=output/'valuation-runs'/uuid.uuid4().hex
    configs=gate/'configs';http=gate/'http';op=gate/'op-http'
    target=Path(os.environ.get('CARGO_TARGET_DIR',str(ROOT/'integration/.runtime/design-review-target')))
    binary=target/'debug/examples/evm_dynamic_fee_harness'
    cargo=['cargo','+1.90.0'];manifest=['--locked','--manifest-path','dex-router/Cargo.toml'];parser_manifest=['--locked','--manifest-path','chain-parser-rust/Cargo.toml']
    env=['env','XONE_NATIVE_VALUATION_CONFIG_VECTORS='+str(configs)]
    tests=['current_go_oracle_authority_config_and_public_economic_hashes_match','oracle_authorities_are_optional_and_restricted_to_configured_local_evm_chains']
    validate(output_base=output,prepared=v,extra_references=refs,extra_checks=[
        ('valuation-go-format',[sys.executable,'-c','import subprocess,sys;r=subprocess.check_output(["gofmt","-l","chain-parser-go/internal/config/config.go","chain-parser-go/internal/config/native_valuation.go","chain-parser-go/internal/config/native_valuation_test.go"],text=True);print(r,end="");sys.exit(bool(r))']),
        ('valuation-parser-format',cargo+['fmt','--all','--check','--manifest-path','chain-parser-rust/Cargo.toml']),
        ('valuation-original-signatures-integers',[sys.executable,'integration/tools/acceptance/native_valuation_vectors.py','--check']),
        ('valuation-go-configs',[sys.executable,'integration/tools/acceptance/validate_native_valuation.py','--go-configs',str(configs)]),
        ('valuation-router-configs',env+cargo+['test']+manifest+['-p','xone-config','--test','native_valuation','--','--include-ignored']),
        ('valuation-parser-configs',env+cargo+['test']+parser_manifest+['-p','xone-config','--test','native_valuation','--','--include-ignored']),
        ('valuation-authority-and-integers',cargo+['test']+manifest+['-p','xone-execution-preparation','--test','native_valuation']),
        ('valuation-http-build',cargo+['build']+manifest+['-p','xone-quote-api','--example','evm_dynamic_fee_harness']),
        ('valuation-actual-http',[sys.executable,'integration/tools/acceptance/native_valuation_http.py',str(binary),str(http)]),
        ('valuation-signed-karst-http',[sys.executable,'integration/tools/acceptance/native_valuation_op_http.py',str(binary),str(v['sources']),str(signer),str(v['binary']),str(op)]),
        ('valuation-common-lint',cargo+['clippy']+manifest+['-p','xone-domain','-p','xone-config','-p','xone-execution-preparation','--all-targets','--','-D','warnings']),
        ('valuation-parser-common-lint',cargo+['clippy']+parser_manifest+['-p','xone-domain','-p','xone-config','--all-targets','--','-D','warnings']),
    ]+list(extra_checks),extra_required={**(extra_required or {}),
        'valuation-router-configs':tests,'valuation-parser-configs':tests,
        'valuation-authority-and-integers':[
            'independent_solders_signatures_and_42_python_integer_conversions_match',
            'authority_quorum_policy_rotation_unknown_signers_and_payload_substitutions_fail',
            'signed_prices_cannot_cross_source_blocks_chains_time_or_expiry',
            'transaction_net_amounts_bind_token_units_round_up_and_never_clamp_or_double_charge',
        ],
    },extra_artifacts=[str((p/'report.json').relative_to(ROOT)) for p in [configs,http,op]],extra_report=dict(extra_report or {},
        authenticated_atomic_price_cost_arithmetic=True,configured_oracle_quorum_supported=True,
        price_reference_cases=42,priced_evm_http_executions=13,priced_signed_karst_http_executions=4,
        forged_oracle_startup_rejections=6,live_market_price_adapter_supported=False,
        all_candidate_net_cost_ranking_supported=False,oracle_signatures_attest_bank_consensus=False,
    ),verification_scope=verification_scope)


if __name__=='__main__':
    if len(sys.argv)==3 and sys.argv[1]=='--go-configs':go_configs(sys.argv[2])
    else:main()
