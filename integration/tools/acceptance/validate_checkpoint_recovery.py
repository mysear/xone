#!/usr/bin/env python3
"""Current owner catalogue, Go wire downloads and real Kafka router cold recovery."""
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
from infrastructure import Infrastructure,command,KAFKA
ROOT=Path(__file__).resolve().parents[3]
BASE=ROOT/'integration/artifacts/local/checkpoint-recovery-check'

def main():
    out=BASE/'runs'/uuid.uuid4().hex;out.mkdir(parents=True,exist_ok=False)
    env=dict(os.environ,PATH=str(Path.home()/'.cargo/bin')+os.pathsep+os.environ['PATH'])
    env.setdefault('CARGO_BUILD_JOBS','2');env.setdefault('CARGO_TARGET_DIR',str(ROOT/'integration/.runtime/design-review-target'))
    if sys.platform=='darwin':
        import platform
        target=('aarch64' if platform.machine()=='arm64' else 'x86_64')+'_APPLE_DARWIN'
        env['CARGO_TARGET_'+target.upper()+'_RUNNER']=str(ROOT/'integration/tools/ci/native-test-runner.py')
    binary=out/'chain-parser-go'
    py=[sys.executable];cargo=['cargo','+1.90.0'];manifest=['--locked','--manifest-path','dex-router/Cargo.toml']
    vectors=out/'go-vectors'
    checks=[
      ('go-format',py+['-c','import subprocess,sys; r=subprocess.check_output(["gofmt","-l","internal/checkpoint","internal/service","internal/config"],text=True); print(r,end=""); sys.exit(bool(r))']),
      ('rust-format',cargo+['fmt','--all','--check','--manifest-path','dex-router/Cargo.toml']),
      ('parser-rust-format',cargo+['fmt','--all','--check','--manifest-path','chain-parser-rust/Cargo.toml']),
      ('contracts',py+['integration/tools/ci/contracts.py']),
      ('progress',py+['-m','unittest','discover','-s','integration/tools/ci/tests','-p','test_progress_document.py']),
      ('go-race', ['go','test','-race','-count=1','-v','./internal/checkpoint','./internal/journal','./internal/reducer','./internal/config']),
      ('go-service-checkpoint',['go','test','-race','-count=1','-v','./internal/service','-run','TestNormalCheckpointEndpoint']),
      ('go-vet',['go','vet','./internal/checkpoint','./internal/service','./internal/config']),
      ('router-transport',cargo+['test']+manifest+['-p','xone-kafka-transport','-p','xone-config','--','--include-ignored']),
      ('parser-transport',cargo+['test','--locked','--manifest-path','chain-parser-rust/Cargo.toml','-p','xone-kafka-transport','-p','xone-config','--','--include-ignored']),
      ('router-transport-lint',cargo+['clippy']+manifest+['-p','xone-kafka-transport','-p','xone-config','--all-targets','--','-D','warnings']),
      ('parser-transport-lint',cargo+['clippy','--locked','--manifest-path','chain-parser-rust/Cargo.toml','-p','xone-kafka-transport','-p','xone-config','--all-targets','--','-D','warnings']),
      ('go-service-build',['go','build','-trimpath','-o',str(binary),'./cmd/chain-parser']),
      ('router-api',cargo+['test']+manifest+['-p','xone-quote-api','--lib']),
      ('router-api-go-wire',cargo+['test']+manifest+['-p','xone-quote-api','--lib','remote_checkpoint_downloads_current_go_wire','--','--include-ignored']),
      ('router-api-lint',cargo+['clippy']+manifest+['-p','xone-quote-api','--all-targets','--','-D','warnings']),
      ('router-build',cargo+['build']+manifest+['--bin','dex-router']),

    ]
    env['XONE_CHECKPOINT_VECTORS_OUTPUT']=str(vectors)
    env['XONE_REMOTE_CHECKPOINT_OUTPUT']=str(out/'actual-api')
    report=dict(passed=False,state='RUNNING',source_tree_sha256=shared.fingerprint(),test_inputs=shared.test_inputs(),
      run_directory=str(out.relative_to(ROOT)),started_ms=int(time.time()*1000),checks=[],
      verification_scope='OWNER_CURRENT_CATALOGUE_REMOTE_OBJECT_DOWNLOAD_AND_ROUTER_KAFKA_RECOVERY',production_ready=False,
      all_requested_features_complete=False,publicTransactionsSubmitted=False)
    def save():
        text=json.dumps(report,indent=2)+'\n';(out/'report.json').write_text(text);(BASE/'report.tmp').write_text(text);(BASE/'report.tmp').replace(BASE/'report.json')
    def interrupted(signum,frame):raise KeyboardInterrupt('INTERRUPTED')
    signal.signal(signal.SIGTERM,interrupted);signal.signal(signal.SIGINT,interrupted)
    def run(name,args,extra=None):
        print('Running '+name,flush=True)
        with (out/(name+'.log')).open('wb') as log:
            run_env=dict(env,**(extra or {}))
            if 'chain-parser-rust/Cargo.toml' in args:
                run_env['CARGO_TARGET_DIR']=str(ROOT/'integration/.runtime/design-review-parser-target')
            child=subprocess.Popen(args,cwd=ROOT/'chain-parser-go' if name.startswith('go-') else ROOT,
                env=run_env,stdout=log,stderr=subprocess.STDOUT,start_new_session=True)
            try:code=child.wait()
            finally:
                if child.poll() is None:
                    os.killpg(child.pid,signal.SIGTERM)
                    try:child.wait(timeout=3)
                    except subprocess.TimeoutExpired:os.killpg(child.pid,signal.SIGKILL);child.wait()
        raw=(out/(name+'.log')).read_bytes();text=raw.decode(errors='replace')
        if b'\0' in raw:code=1
        if name=='go-race':
            for required in ['TestPublicBootstrapExportsIndependentLanguageVectors',
                'TestPublicBootstrapChecksManifestPinBeforePayloadIO',
                'TestPublicBootstrapPostPublicationRevocationReturnsNoQualification',
                'TestRemoteCheckpointPutAcknowledgementRequiresActualExactObject',
                'TestRemoteCheckpointManifestAuthenticationPrecedesPayloadDownload',
                'TestRemoteCheckpointCancellationDeadlineAndLocalCorruptionPreventQualification',
                'TestCheckpointStoreURLScopeAndSecretFingerprint',
                'TestRemoteCheckpointCatalogRequiresReadbackMarkerExactLiveStateAndStoreIdentity']:
                if not re.search(r'^--- PASS: '+required+r' ',text,re.M):code=1
        if name in ['router-transport','parser-transport']:
            if not re.search(r'^test current_go_checkpoint_vectors_match_reference_cursor_and_full_width_amounts \.\.\. ok$',text,re.M):code=1
            if 'test checkpoint_store::tests::checkpoint_store_url_scope_is_explicit_and_shared ... ok' not in text:code=1
        if name=='router-api-go-wire' and 'test remote_checkpoint::tests::remote_checkpoint_downloads_current_go_wire_without_reserializing_payload ... ok' not in text:code=1
        if name=='router-api-kafka' and 'test remote_checkpoint::tests::remote_checkpoint_normal_router_cold_restore_and_quote_with_actual_kafka ... ok' not in text:code=1
        report['checks'].append(dict(name=name,exit_code=code,log_sha256=hashlib.sha256(raw).hexdigest(),
            cargo_target_directory=run_env.get('CARGO_TARGET_DIR') if args and args[0]=='cargo' else None,
            go_tests=len(re.findall(r'^--- PASS:',text,re.M)),rust_tests=sum(int(n) for n in re.findall(r'test result: ok\. (\d+) passed;',text))))
        save()
        if code:raise RuntimeError(name+' failed: '+str(out/(name+'.log')))
    save()
    try:
        vector_hashes=None
        for name,args in checks:
            run(name,args)
            if name=='go-race':
                vector_hashes={str(p.relative_to(vectors)):shared.sha_file(p) for p in sorted(vectors.rglob('*')) if p.is_file()}
                if len(json.loads((vectors/'vectors.json').read_text())['vectors'])!=3:raise RuntimeError('Incomplete Go vectors')
                report['independent_go_vectors']=vector_hashes
        infra=Infrastructure()
        try:
            infra.start()
            command([KAFKA/'bin/kafka-topics.sh','--bootstrap-server',infra.broker,'--create','--topic','test.remote-checkpoint','--partitions','1','--replication-factor','1'],env=infra.env)
            run('router-api-kafka',cargo+['test']+manifest+['-p','xone-quote-api','--lib','remote_checkpoint_normal_router_cold','--','--include-ignored'],{'XONE_TEST_KAFKA_BROKER':infra.broker})
            report['infrastructure_scope']='OWNED_LOOPBACK_SINGLE_KAFKA_BROKER_CONFIGURED_HTTP_OBJECT_API'
        finally:infra.close()
        report['artifacts']={str(p.relative_to(out)):shared.sha_file(p) for p in sorted((out/'actual-api').glob('*')) if p.is_file()}
        if vector_hashes!={str(p.relative_to(vectors)):shared.sha_file(p) for p in sorted(vectors.rglob('*')) if p.is_file()}:
            raise RuntimeError('Go vector bytes changed during cross-language validation')
        report['service_binary_sha256']=shared.sha_file(binary)
        if shared.fingerprint()!=report['source_tree_sha256'] or shared.test_inputs()!=report['test_inputs']:
            raise RuntimeError('Source or fixed inputs changed during validation')
        report.update(passed=True,state='LOCAL_SCOPE_VERIFIED',go_test_executions=sum(c['go_tests'] for c in report['checks']),
            rust_test_executions=sum(c['rust_tests'] for c in report['checks']))
    except KeyboardInterrupt:report.update(state='CANCELLED',error='INTERRUPTED');raise
    except Exception as error:report.update(state='FAILED',error=str(error));raise
    finally:report['finished_ms']=int(time.time()*1000);save()
    print(json.dumps({key:report[key] for key in ['state','go_test_executions','rust_test_executions','run_directory']}),flush=True)
if __name__=='__main__':main()
