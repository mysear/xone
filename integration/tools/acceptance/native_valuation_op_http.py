#!/usr/bin/env python3
"""Four priced signed Karst Routers, full original VM and parent fee audit."""
import json
from pathlib import Path
import sys
from native_valuation_http import prepare,audit_result
from op_api_http import run,require,digest


def main(binary,sources,signer,modern,output,candidate_cost_max_plans=None):
    output=Path(output).resolve();output.mkdir(parents=True,exist_ok=False)
    rows=json.loads(Path(sources).read_bytes());require(len(rows)==4,'Expected four scoped Karst sources')
    configs={};books={}
    for row in rows:
        prefix,mode=row['id'].rsplit('-',1)
        if mode!='in':continue
        q=row['request'];bound=q['bound'];context=bound['plan']['context']
        source=dict(state_anchor=q['fixture']['state_anchor'],token_in=context['sell_token'],token_out=context['buy_token'])
        path,book,config=prepare(output/(prefix+'-oracle'),source=source)
        configs[prefix]=path;books[prefix]=(book,config)
    executed=run(Path(binary),Path(sources),dict(signer=Path(signer),karst_transition=Path(modern)),output/'execution',scoped_karst=True,valuation_configs=configs,candidate_cost_max_plans=candidate_cost_max_plans)
    require(executed['actual_worker_executions']==4 and executed['rejected_requests']==24,'Priced Karst coverage changed')
    for row in rows:
        prefix,mode=row['id'].rsplit('-',1);book,config=books[prefix]
        audit_result(output/'execution'/prefix/mode,book,config,'priced')
        if candidate_cost_max_plans is not None:
            q=json.loads((output/'execution'/prefix/mode/'quote.json').read_bytes())
            ranking=q['quality']['costRanking']
            require(ranking['costAdjustmentApplied'] is False and ranking['simulationAttempts']==0 and ranking['unavailableReasons']=={'OP_SIGNED_ENVELOPE_REQUIRED':1},'Unsigned OP candidate fabricated complete fee/ranking')

    report=dict(passed=True,actual_worker_executions=4,rejected_requests=24,sources_sha256=digest(Path(sources)),
        artifacts={str(p.relative_to(output)):digest(p) for p in sorted(output.rglob('*')) if p.is_file()},
        full_original_karst_same_signed_wire=True,oracle_scope='PINNED_TEST_SIGNER_QUORUM_NOT_LIVE_MARKET',quote_ranking_applied=False,production_ready=False,publicTransactionsSubmitted=False)
    (output/'report.json').write_text(json.dumps(report,indent=2)+'\n')


if __name__=='__main__':main(*sys.argv[1:])
