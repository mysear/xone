#!/usr/bin/env python3
"""Fair multi-chain checkpoint publication and unchanged lifecycle regression."""
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
BASE=ROOT/'integration/artifacts/local/checkpoint-fairness-check'

def main():
    out=BASE/'runs'/uuid.uuid4().hex;out.mkdir(parents=True,exist_ok=False)
    binary=out/'chain-parser-go'
    py=[sys.executable]
    checks=[
      ('go-format',py+['-c','import subprocess,sys;r=subprocess.check_output(["gofmt","-l","internal/service"],text=True);print(r,end="");sys.exit(bool(r))']),
      ('contracts',py+['integration/tools/ci/contracts.py']),
      ('progress',py+['-m','unittest','discover','-s','integration/tools/ci/tests','-p','test_progress_document.py']),
      ('go-core',['go','test','-race','-count=1','-v','./internal/checkpoint','./internal/journal','./internal/reducer','./internal/config','./internal/app']),
      ('go-workers',['go','test','-race','-count=1','-v','./internal/service','-run','CheckpointWorker|CheckpointClose|NormalCheckpointEndpoint']),
      ('go-vet',['go','vet','./internal/service','./internal/checkpoint','./internal/config','./internal/app']),
      ('go-build',['go','build','-trimpath','-o',str(binary),'./cmd/chain-parser']),
    ]
    report=dict(passed=False,state='RUNNING',source_tree_sha256=shared.fingerprint(),test_inputs=shared.test_inputs(),
      run_directory=str(out.relative_to(ROOT)),started_ms=int(time.time()*1000),checks=[],
      verification_scope='GO_NORMAL_SERVICE_TWO_CHAIN_CHECKPOINT_FAILURE_ISOLATION',production_ready=False,
      all_requested_features_complete=False,publicTransactionsSubmitted=False)
    def save():
        text=json.dumps(report,indent=2)+'\n';(out/'report.json').write_text(text);(BASE/'report.tmp').write_text(text);(BASE/'report.tmp').replace(BASE/'report.json')
    def interrupted(signum,frame):raise KeyboardInterrupt('INTERRUPTED')
    signal.signal(signal.SIGTERM,interrupted);signal.signal(signal.SIGINT,interrupted)
    save()
    try:
        for name,args in checks:
            print('Running '+name,flush=True)
            path=out/(name+'.log')
            with path.open('wb') as log:
                child=subprocess.Popen(args,cwd=ROOT/'chain-parser-go' if name.startswith('go-') else ROOT,
                    env=os.environ.copy(),stdout=log,stderr=subprocess.STDOUT,start_new_session=True)
                try:code=child.wait()
                finally:
                    if child.poll() is None:
                        os.killpg(child.pid,signal.SIGTERM)
                        try:child.wait(timeout=3)
                        except subprocess.TimeoutExpired:os.killpg(child.pid,signal.SIGKILL);child.wait()
            raw=path.read_bytes();text=raw.decode(errors='replace')
            if b'\0' in raw:code=1
            if name=='go-workers':
                for test in ['TestCheckpointWorkerFairRotationIsolatesPersistentChainFailureAndRecovers',
                    'TestCheckpointWorkerPublishesWithoutManualPOSTAndRecoversFailedUpload',
                    'TestCheckpointWorkerGateCancellationAndCloseInterruptInflightUpload',
                    'TestCheckpointCloseAlsoCancelsManualUploadAndDisabledWorkerStaysIdle',
                    'TestCheckpointWorkerParentCancellationLeavesManualPublicationAvailable']:
                    if not re.search(r'^--- PASS: '+test+r' ',text,re.M):code=1
            report['checks'].append(dict(name=name,exit_code=code,log_sha256=hashlib.sha256(raw).hexdigest(),go_tests=len(re.findall(r'^--- PASS:',text,re.M))))
            save()
            if code:raise RuntimeError(name+' failed: '+str(path))
        report['service_binary_sha256']=shared.sha_file(binary)
        if shared.fingerprint()!=report['source_tree_sha256'] or shared.test_inputs()!=report['test_inputs']:
            raise RuntimeError('Source or fixed inputs changed during validation')
        report.update(passed=True,state='LOCAL_SCOPE_VERIFIED',go_test_executions=sum(c['go_tests'] for c in report['checks']))
    except KeyboardInterrupt:report.update(state='CANCELLED',error='INTERRUPTED');raise
    except Exception as error:report.update(state='FAILED',error=str(error));raise
    finally:report['finished_ms']=int(time.time()*1000);save()
    print(json.dumps({key:report[key] for key in ['state','go_test_executions','run_directory']}),flush=True)
if __name__=='__main__':main()
