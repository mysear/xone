"""Original retained SBF reference for the controlled classic-SPL MM-only profile."""
from pathlib import Path
import json,hashlib,sys,time
from native_preflight import execute
from idl_layout import ROOT,BASE
sys.path.insert(0,str(ROOT/'integration/tools/acceptance'))
from r01_public_catalog_solana_bank import DEPS,verify_runtime_lock
from r01_public_catalog_solana_rebuild import BASE as CATALOG_BASE

def main():
    source=json.loads((BASE/'source-metadata.json').read_bytes())
    for item in source['files']:
        raw=(BASE/item['path']).read_bytes()
        if hashlib.sha256(raw).hexdigest()!=item['sha256']or len(raw)!=item['bytes']:raise ValueError('PINNED_SOURCE_CHANGED:'+item['path'])
    lock=verify_runtime_lock(json.loads((CATALOG_BASE/'solana-rebuild/runtime-lock.json').read_bytes()))
    helpers={str(p.relative_to(ROOT)):hashlib.sha256(p.read_bytes()).hexdigest()for p in [Path(__file__),BASE/'harness/native_preflight.py',BASE/'harness/idl_layout.py']}
    cases=[];started=time.monotonic()
    modes=[('static',0,0,0),('same-clock',0,2000,7500),('filter',299,1000,7500),('reduction',300,2000,7500),('decay',1200,2500,7500)]
    for mode,elapsed,share,control in modes:
        for zero in [True,False]:
            for amount in [1,2,100,1000000,100000000,100000000000,1000000000000000]:
                record=execute(zero=zero,amount=amount,minimum=1,mm_only=True,protocol_share=share,elapsed=elapsed,dynamic_control=control,followups=(10000,20000)if amount==1000000 else())
                record.update(index=len(cases),mode=mode,elapsed=elapsed,protocol_share=share,variable_fee_control=control)
                if record['passed']:
                    if int(record['payer_input_delta'])!=amount or int(record['recipient_output_delta'])<=0:raise ValueError('ORIGINAL_FUNDS')
                elif record['before']!=record['after']:raise ValueError('ORIGINAL_FAILED_BANK_MUTATED')
                cases.append(record)
            print(json.dumps(dict(mode=mode,zero_for_one=zero,cases=len(cases),elapsed_seconds=round(time.monotonic()-started,1))),flush=True)
    result=dict(schema_version=1,passed=True,generator_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),source_metadata_sha256=hashlib.sha256((BASE/'source-metadata.json').read_bytes()).hexdigest(),helper_sha256=helpers,runtime_lock=lock,upstream_commit=source['commit'],cases=cases,authority='retained upstream test SBF in new local LiteSVM with explicitly patched MM-only function policy, zero limit-order bookkeeping, LP-backed vault balances and classic SPL/WSOL funding; not deployed programdata or historical sourcebank qualification',service_implemented=False,all_requested_features_complete=False,production_ready=False)
    target=ROOT/'integration/artifacts/local/core-meteora-dlmm-mm-original-20261110.json';target.write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(dict(passed=True,cases=len(cases),full_swaps=sum(r['passed']for r in cases),sequence_steps=sum(len(r['sequence'])for r in cases if len(r['sequence'])>1),rejected=sum(not r['passed']for r in cases),path=str(target.relative_to(ROOT)))))

if __name__=='__main__':main()
