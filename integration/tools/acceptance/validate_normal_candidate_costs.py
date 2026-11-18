#!/usr/bin/env python3
"""Frozen optional normal-quote local execution ranking plus every previous gate."""
import json
import os
from pathlib import Path
import subprocess
import sys
import uuid
from karst_api_vectors import ROOT,digest,require
from validate_candidate_costs import main as validate


def main():
    output=ROOT/'integration/artifacts/local/normal-candidate-cost-check'
    original=output/'candidate-original-runs'/uuid.uuid4().hex
    original.mkdir(parents=True,exist_ok=False)
    corpus=original/'original.json'
    result=subprocess.run(['node','integration/tools/ci/candidate-cost-execution-reference.cjs',str(corpus)],cwd=ROOT,capture_output=True,text=True,timeout=180)
    (original/'generation.log').write_text(result.stdout+result.stderr)
    require(result.returncode==0,'Independent competing-path bank generation failed')
    doc=json.loads(corpus.read_bytes());require(len(doc['cases'])==4,'Competing original path coverage changed')
    refs=dict(doc['inputs']);refs[str(corpus.relative_to(ROOT))]=digest(corpus)
    for name,h in refs.items():require(digest(ROOT/name)==h,'Independent original input changed: '+name)
    prior=ROOT/'integration/artifacts/local/karst-api-check/runs/ecb91103ff6b4430ab10292f747e93b2/report.json'
    previous=json.loads(prior.read_bytes());require(previous['passed'] is True,'Fixed original signed API qualification required')
    refs[str(prior.relative_to(ROOT))]=digest(prior)
    for n,h in previous['reference_inputs'].items():require(digest(ROOT/n)==h,'Retained original input changed');refs[n]=h
    sources=ROOT/next(n for n in refs if n.endswith('/karst-api-sources.json'))
    signer=ROOT/next(n for n in refs if '/karst-api-check/' in n and n.endswith('/original-client/signer/runner'))
    modern=ROOT/next(n for n in refs if '/karst-api-check/' in n and n.endswith('/execution/runner/runner'))
    op=original/'op-unknown'
    http=original/'http'
    target=Path(os.environ.get('CARGO_TARGET_DIR',str(ROOT/'integration/.runtime/design-review-target')))
    binary=target/'debug/examples/evm_dynamic_fee_harness'
    cargo=['cargo','+1.90.0'];manifest=['--locked','--manifest-path','dex-router/Cargo.toml']
    validate(output_base=output,extra_references=refs,extra_artifacts=[str((p/'report.json').relative_to(ROOT)) for p in [http,op]],extra_checks=[
        ('normal-candidate-binding-control',cargo+['test']+manifest+['-p','xone-quote-api','--lib','candidate_costs::']),
        ('normal-candidate-router-config',cargo+['test']+manifest+['-p','xone-config','--test','native_valuation','candidate_execution_cost_config']),
        ('normal-candidate-parser-config',cargo+['test','--locked','--manifest-path','chain-parser-rust/Cargo.toml','-p','xone-config','--test','native_valuation','candidate_execution_cost_config']),
        ('normal-candidate-unsigned-op-fallback',[sys.executable,'integration/tools/acceptance/native_valuation_op_http.py',str(binary),str(sources),str(signer),str(modern),str(op),'16']),
        ('normal-candidate-actual-http',[sys.executable,'integration/tools/acceptance/candidate_cost_http.py',str(binary),str(corpus),str(http)]),
    ],extra_required={
        'normal-candidate-binding-control':[
            'candidate_costs::tests::candidate_execution_costs_reject_cross_plan_amount_recipient_and_fee_audits',
            'candidate_costs::tests::candidate_costs_preserve_negative_and_wide_objectives_and_unknown_execution',
            'candidate_costs::tests::candidate_cost_bridge_cancels_owned_execution_and_preserves_errors',
        ],
        'normal-candidate-router-config':['candidate_execution_cost_config_is_opt_in_bounded_and_single_route_only'],
        'normal-candidate-parser-config':['candidate_execution_cost_config_is_opt_in_bounded_and_single_route_only'],
    },extra_report=dict(normal_quote_cost_model_connected=True,
        normal_quote_local_v2_cost_ranking_opt_in=True,
        complete_quote_cost_ranking_supported=False,
        normal_candidate_quote_scenarios=16,
        normal_candidate_max_simulation_plans=16),
        verification_scope='OPTIONAL_NORMAL_LOCAL_V2_CANDIDATE_EXECUTION_FEE_RANKING_FIXED_PLAN_DEADLINE_PARENT_AUDIT_CONFIGURED_PRICE_UNIFORM_FALLBACK_AND_ALL_PRIOR_GATES_NOT_COMPLETE_QUOTE_NET_LIVE_MARKET_TRUSTED_BANK_OR_PRODUCTION')

if __name__=='__main__':main()
