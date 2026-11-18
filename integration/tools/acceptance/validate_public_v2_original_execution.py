#!/usr/bin/env python3
"""Pinned real V2 bank original EVM calls and independent Rust full-state math."""
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import signal
import subprocess
import sys
import time
import uuid
import validate_cache_reclamation as shared
ROOT=Path(__file__).resolve().parents[3]
BASE=ROOT/'integration/artifacts/local/public-v2-original-execution-check'

def main():
    out=BASE/'runs'/uuid.uuid4().hex;out.mkdir(parents=True)
    env=dict(os.environ,PATH=str(Path.home()/'.cargo/bin')+os.pathsep+os.environ['PATH'],CARGO_BUILD_JOBS='2',CARGO_TARGET_DIR=str(ROOT/'integration/.runtime/design-review-target'),XONE_PUBLIC_V2_EXECUTION_OUTPUT=str(out/'actual-original-execution'))
    if sys.platform=='darwin':env['CARGO_TARGET_'+('AARCH64' if platform.machine()=='arm64' else 'X86_64')+'_APPLE_DARWIN_RUNNER']=str(ROOT/'integration/tools/ci/native-test-runner.py')
    cargo=['cargo','+1.90.0'];router=['--locked','--manifest-path','dex-router/Cargo.toml'];py=[sys.executable]
    checks=[
        ('go-format',py+['-c','import subprocess,sys;r=subprocess.check_output(["gofmt","-l","internal/chain/evm"],text=True);print(r,end="");sys.exit(bool(r))']),
        ('router-format',cargo+['fmt','--all','--check','--manifest-path','dex-router/Cargo.toml']),
        ('contracts',py+['integration/tools/ci/contracts.py']),
        ('progress',py+['-m','unittest','discover','-s','integration/tools/ci/tests','-p','test_progress_document.py']),
        ('evidence-guards',py+['-m','unittest','discover','-s','integration/tools/acceptance/tests','-p','test_current_v2_evidence.py']),
        ('go-original',['go','test','-race','-timeout=10m','-count=1','-v','./internal/chain/evm','-run','V2RealPublic|V2CompleteRealPublic|CommittedView|NativeRejectedSession|NativeSourceRejection']),
        ('rust-original-cross',cargo+['test']+router+['-p','xone-protocol-math','--test','public_v2_execution','--','--include-ignored']),
        ('rust-v2',cargo+['test']+router+['-p','xone-protocol-math','--test','v2','--test','v2_complete','--test','balancer_composable','--test','balancer_composable_differential']),
        ('go-normal',['go','test','-race','-count=1','-v','./internal/service','-run','Native|NormalEthereumServiceIntegrity']),
        ('go-vet',['go','vet','./internal/chain/evm','./internal/service']),
        ('module-cache',py+['integration/tools/ci/verify_go_module_cache.py']),
        ('rust-math-lint',cargo+['clippy']+router+['-p','xone-protocol-math','--all-targets','--','-D','warnings']),
        ('go-build',['go','build','-trimpath','-o',str(out/'chain-parser-go'),'./cmd/chain-parser']),
        ('router-build',cargo+['build']+router+['--bin','dex-router']),
    ]
    report={'passed':False,'state':'RUNNING','source_tree_sha256':shared.fingerprint(),'test_inputs':shared.test_inputs(),'checks':[],'started_ms':int(time.time()*1000),'run_directory':str(out.relative_to(ROOT)),'verification_scope':'PINNED_PUBLIC_MPT_STATE_ORIGINAL_WETH_TRANSFER_PAIR_SWAP_COUNTERFACTUAL_NATIVE_FUNDING_RUST_FULL_AFTER_STATE','consensus_verified':False,'complete_signed_transaction_verified':False,'chain_fees_complete':False,'production_ready':False,'all_requested_features_complete':False,'public_transactions_submitted':False}
    def save():
        text=json.dumps(report,indent=2)+'\n';(out/'report.json').write_text(text);temp=BASE/'report.tmp';temp.write_text(text);temp.replace(BASE/'report.json')
    def interrupted(signum,frame):raise KeyboardInterrupt('INTERRUPTED')
    signal.signal(signal.SIGTERM,interrupted);signal.signal(signal.SIGINT,interrupted);save()
    try:
        exported=None
        for name,args in checks:
            print('Running '+name,flush=True);path=out/(name+'.log')
            with path.open('wb') as log:
                child=subprocess.Popen(args,cwd=ROOT/'chain-parser-go' if name.startswith('go-') else ROOT,env=env,stdout=log,stderr=subprocess.STDOUT,start_new_session=True)
                try:code=child.wait()
                finally:
                    if child.poll() is None:
                        os.killpg(child.pid,signal.SIGTERM)
                        try:child.wait(timeout=3)
                        except subprocess.TimeoutExpired:os.killpg(child.pid,signal.SIGKILL);child.wait()
            raw=path.read_bytes();text=raw.decode(errors='replace')
            if b'\0' in raw:code=1
            if name=='go-original':
                for required in ['TestV2RealPublicOriginalExecutionMatchesQuoteAndMaximalIntegerOutput','TestV2RealPublicOriginalExecutionRejectsMissingProofAndBoundsWork','TestV2CompleteRealPublicFixtureNormalReadAuthenticatesBodyStateAndTokenBalances','TestV2CompleteRealPublicFixtureRejectsOriginalProofAndHeaderMutation']:
                    if not re.search(r'^--- PASS: '+required+r' ',text,re.M):code=1
                for control in ['missing-slot','unproved-account','instructions','canceled']:
                    if not re.search(r'--- PASS: TestV2RealPublicOriginalExecutionRejectsMissingProofAndBoundsWork/'+control+r' ',text):code=1
            if name=='rust-original-cross' and not re.search(r'^test current_go_public_v2_original_execution_and_full_after_state_match \.\.\. ok$',text,re.M):code=1
            report['checks'].append({'name':name,'exit_code':code,'log_sha256':shared.sha_file(path),'go_tests':len(re.findall(r'^--- PASS:',text,re.M)),'rust_tests':sum(int(n) for n in re.findall(r'test result: ok\. (\d+) passed;',text))});save()
            if code:raise RuntimeError(name+' failed: '+str(path))
            if name=='go-original':exported=shared.sha_file(out/'actual-original-execution/original-public-v2-execution.json')
        artifact=out/'actual-original-execution/original-public-v2-execution.json';evidence=json.loads(artifact.read_text());result=evidence['result']
        if exported!=shared.sha_file(artifact) or not result['one_more_output_reverts_k'] or result['calls']!=3 or int(result['recipient_after'])-int(result['recipient_before'])!=int(result['output']) or result['instructions']>=1000000:raise RuntimeError('Missing unchanged original execution balance and maximality proof')
        if shared.fingerprint()!=report['source_tree_sha256'] or shared.test_inputs()!=report['test_inputs']:raise RuntimeError('Source or fixed inputs changed')
        report.update(passed=True,state='LOCAL_SCOPE_VERIFIED',go_test_executions=sum(row['go_tests'] for row in report['checks']),rust_test_executions=sum(row['rust_tests'] for row in report['checks']),artifacts={str(artifact.relative_to(out)):exported},observed_output=result['output'],source_block=result['pool']['v2']['swap']['bank_block_number'],go_binary_sha256=shared.sha_file(out/'chain-parser-go'),router_binary_sha256=shared.sha_file(ROOT/'integration/.runtime/design-review-target/debug/dex-router'))
    except KeyboardInterrupt:report.update(state='CANCELLED',error='INTERRUPTED');raise
    except Exception as error:report.update(state='FAILED',error=str(error));raise
    finally:report['finished_ms']=int(time.time()*1000);save()
    print(json.dumps({key:report[key] for key in ['state','go_test_executions','rust_test_executions','run_directory','observed_output']}),flush=True)
if __name__=='__main__':main()
