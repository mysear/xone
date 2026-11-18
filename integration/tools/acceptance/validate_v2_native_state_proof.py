#!/usr/bin/env python3
"""Normal Go native V2 Pair/Factory commitments; no consensus/token-bank claim."""
import hashlib
import json
import os
from pathlib import Path
import re
import signal
import subprocess
import sys
import time
import uuid
import validate_cache_reclamation as shared
ROOT=Path(__file__).resolve().parents[3]
BASE=ROOT/'integration/artifacts/local/v2-native-state-proof-check'

def main():
    out=BASE/'runs'/uuid.uuid4().hex;out.mkdir(parents=True)
    env=dict(os.environ,XONE_V2_NATIVE_PROOF_OUTPUT=str(out/'actual-normal'))
    checks=[
      ('go-format',[sys.executable,'-c','import subprocess,sys;r=subprocess.check_output(["gofmt","-l","internal/chain/evm"],text=True);print(r,end="");sys.exit(bool(r))']),
      ('contracts',[sys.executable,'integration/tools/ci/contracts.py']),
      ('progress',[sys.executable,'-m','unittest','discover','-s','integration/tools/ci/tests','-p','test_progress_document.py']),
      ('go-evm',['go','test','-race','-timeout=20m','-count=1','-v','./internal/chain/evm']),
      ('go-normal-native',['go','test','-race','-count=1','-v','./internal/service','-run','Native|V2Complete|RuntimeFactoryNormalArchived']),
      ('go-contracts-reducer',['go','test','-race','-count=1','-v','./internal/domain','./internal/config','./internal/reducer']),
      ('go-vet',['go','vet','./internal/chain/evm','./internal/service']),
      ('go-build',['go','build','-trimpath','-o',str(out/'chain-parser-go'),'./cmd/chain-parser']),
    ]
    report=dict(state='RUNNING',passed=False,source_tree_sha256=shared.fingerprint(),test_inputs=shared.test_inputs(),run_directory=str(out.relative_to(ROOT)),started_ms=int(time.time()*1000),checks=[],verification_scope='NORMAL_NATIVE_PAIR_FACTORY_STATE_COMMITMENTS_ORIGINAL_BANK_SYNTHETIC_HEADER_REAL_MPT',token_balances_proven=False,consensus_verified=False,production_ready=False,all_requested_features_complete=False)
    def save():
        text=json.dumps(report,indent=2)+'\n';(out/'report.json').write_text(text);temp=BASE/'report.tmp';temp.write_text(text);temp.replace(BASE/'report.json')
    def interrupted(signum,frame):raise KeyboardInterrupt('INTERRUPTED')
    signal.signal(signal.SIGTERM,interrupted);signal.signal(signal.SIGINT,interrupted);save()
    try:
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
            if name=='go-evm':
                for required in ['TestV2CompleteNativeStateProofNormalReadBindsOriginalPairAndFactory','TestV2CompleteNativeStateProofRejectsGetterAndProofContradictions','TestV2CompleteNativeStateProofFailureExhaustsSharedSessionAndHonorsCancellation','TestV2CompleteAllNativeBanksIncludingReverts']:
                    if not re.search(r'^--- PASS: '+required+r' ',text,re.M):code=1
                for fault in ['committed-forward-map','committed-reverse-map','committed-identity','committed-reserves','committed-cumulative','committed-lock','committed-pair-code','getter-reserves','getter-cumulative','missing-storage','bad-node','wrong-account','budget','canceled','bank-timestamp','bank-hash']:
                    if not re.search(r'--- PASS: TestV2CompleteNativeStateProof[^\n]*/'+fault+r' ',text):code=1
            if name=='go-normal-native':
                for required in ['TestRuntimeFactoryNormalArchivedNativeProofRetainsSettlingAuthority','TestNormalHistoryRuntimeInstallsNativeEvidenceResponsibilitySink','TestNativeFactoryActualPublicProofNormalAndReplay']:
                    if not re.search(r'^--- PASS: '+required+r' ',text,re.M):code=1
            report['checks'].append(dict(name=name,exit_code=code,log_sha256=hashlib.sha256(raw).hexdigest(),go_test_executions=len(re.findall(r'^--- PASS:',text,re.M))))
            save()
            if code:raise RuntimeError(name+' failed: '+str(path))
        artifact=out/'actual-normal/normal-pair-factory-state-proof.json';actual=json.loads(artifact.read_text())
        if actual['proof_requests']<2 or actual['consensus_verified'] or actual['pool']['v2']['swap']['bank_block_hash']!=actual['reference']['hash']:raise RuntimeError('Missing normal committed Pair/Factory evidence')
        if shared.fingerprint()!=report['source_tree_sha256'] or shared.test_inputs()!=report['test_inputs']:raise RuntimeError('Source or fixed input changed during validation')
        report.update(state='LOCAL_SCOPE_VERIFIED',passed=True,go_test_executions=sum(c['go_test_executions'] for c in report['checks']),artifacts={str(artifact.relative_to(out)):shared.sha_file(artifact)},go_binary_sha256=shared.sha_file(out/'chain-parser-go'))
    except KeyboardInterrupt:report.update(state='CANCELLED',error='INTERRUPTED');raise
    except Exception as error:report.update(state='FAILED',error=str(error));raise
    finally:report['finished_ms']=int(time.time()*1000);save()
    print(json.dumps({key:report[key] for key in ['state','go_test_executions','run_directory']}),flush=True)
if __name__=='__main__':main()
