#!/usr/bin/env python3
"""Explicit public checkpoint logical budgets for both owners and bounded namespace scans."""
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
from live_rust_checkpoint_recovery import run_live
ROOT=Path(__file__).resolve().parents[3]
BASE=ROOT/'integration/artifacts/local/checkpoint-budget-check'

def main():
    out=BASE/'runs'/uuid.uuid4().hex;out.mkdir(parents=True,exist_ok=False)
    env=dict(os.environ,PATH=str(Path.home()/'.cargo/bin')+os.pathsep+os.environ['PATH'])
    env.setdefault('CARGO_BUILD_JOBS','2');env['CARGO_TARGET_DIR']=str(ROOT/'integration/.runtime/design-review-target')
    if sys.platform=='darwin':
        import platform
        target=('aarch64' if platform.machine()=='arm64' else 'x86_64')+'_APPLE_DARWIN'
        env['CARGO_TARGET_'+target.upper()+'_RUNNER']=str(ROOT/'integration/tools/ci/native-test-runner.py')
    env['XONE_CHECKPOINT_VECTORS_OUTPUT']=str(out/'go-vectors')
    env['XONE_RUST_OWNER_CHECKPOINT_OUTPUT']=str(out/'actual-parser')
    env['XONE_RUST_REMOTE_CHECKPOINT_OUTPUT']=str(out/'actual-parser')
    env['XONE_RUST_PERIODIC_CHECKPOINT_OUTPUT']=str(out/'actual-parser')
    env['XONE_CHECKPOINT_CONFIG_VECTORS_OUTPUT']=str(out/'config-vectors')
    env['XONE_RUST_RECOVERY_CATALOG_OUTPUT']=str(out/'actual-parser')
    env['XONE_REMOTE_CHECKPOINT_OUTPUT']=str(out/'actual-router')
    env['XONE_CHECKPOINT_BUDGET_VECTORS_OUTPUT']=str(out/'budget-vectors')
    py=[sys.executable];cargo=['cargo','+1.90.0'];router=['--locked','--manifest-path','dex-router/Cargo.toml'];parser=['--locked','--manifest-path','chain-parser-rust/Cargo.toml']
    checks=[
      ('go-format',py+['-c','import subprocess,sys;r=subprocess.check_output(["gofmt","-l","internal/checkpoint","internal/config"],text=True);print(r,end="");sys.exit(bool(r))']),
      ('router-format',cargo+['fmt','--all','--check','--manifest-path','dex-router/Cargo.toml']),
      ('parser-format',cargo+['fmt','--all','--check','--manifest-path','chain-parser-rust/Cargo.toml']),
      ('contracts',py+['integration/tools/ci/contracts.py']),
      ('progress',py+['-m','unittest','discover','-s','integration/tools/ci/tests','-p','test_progress_document.py']),
      ('go-checkpoint',['go','test','-race','-count=1','-v','./internal/checkpoint']),
      ('go-config',['go','test','-race','-count=1','-v','./internal/config']),
      ('go-workers',['go','test','-race','-count=1','-v','./internal/service','-run','CheckpointWorker|CheckpointClose|NormalCheckpointEndpoint']),
      ('router-transport',cargo+['test']+router+['-p','xone-kafka-transport','-p','xone-config','--','--include-ignored']),
      ('go-decode-router',['go','test','-race','-count=1','-v','./internal/checkpoint','-run','TestCurrentRustCheckpointVectors']),
      ('parser-transport',cargo+['test']+parser+['-p','xone-kafka-transport','-p','xone-config','--','--include-ignored']),
      ('go-decode-parser',['go','test','-race','-count=1','-v','./internal/checkpoint','-run','TestCurrentRustCheckpointVectors']),
      ('parser-service',cargo+['test']+parser+['-p','xone-solana-ingest','--lib','service::']),
      ('go-decode-remote',['go','test','-race','-count=1','-v','./internal/checkpoint','-run','TestCurrentRustRemotePublication']),
      ('go-decode-recovery',['go','test','-race','-count=1','-v','./internal/checkpoint','-run','TestCurrentRustRecoveryCatalog']),
      ('router-transport-lint',cargo+['clippy']+router+['-p','xone-kafka-transport','-p','xone-config','--all-targets','--','-D','warnings']),
      ('parser-lint',cargo+['clippy']+parser+['-p','xone-kafka-transport','-p','xone-solana-ingest','-p','xone-config','--all-targets','--','-D','warnings']),
      ('go-vet',['go','vet','./internal/checkpoint','./internal/config','./internal/service']),
      ('parser-build',cargo+['build']+parser+['--bin','chain-parser-rust']),
      ('router-api',cargo+['test']+router+['-p','xone-quote-api','--lib']),
      ('router-api-lint',cargo+['clippy']+router+['-p','xone-quote-api','--all-targets','--','-D','warnings']),
      ('router-build',cargo+['build']+router+['--bin','dex-router']),
    ]
    report=dict(passed=False,state='RUNNING',source_tree_sha256=shared.fingerprint(),test_inputs=shared.test_inputs(),
      run_directory=str(out.relative_to(ROOT)),started_ms=int(time.time()*1000),checks=[],
      verification_scope='GO_RUST_CHECKPOINT_PUBLIC_NAMESPACE_BUDGET_ADMISSION_AND_RECOVERY_REGRESSION',production_ready=False,
      all_requested_features_complete=False,publicTransactionsSubmitted=False)
    def save():
        text=json.dumps(report,indent=2)+'\n';(out/'report.json').write_text(text);(BASE/'report.tmp').write_text(text);(BASE/'report.tmp').replace(BASE/'report.json')
    def interrupted(signum,frame):raise KeyboardInterrupt('INTERRUPTED')
    signal.signal(signal.SIGTERM,interrupted);signal.signal(signal.SIGINT,interrupted)
    save()
    try:
        vector_hashes={}
        for name,args in checks:
            print('Running '+name,flush=True)
            run_env=dict(env)
            # The actual Rust HTTP export exists only after parser-service.
            if name=='go-checkpoint':
                run_env.pop('XONE_RUST_REMOTE_CHECKPOINT_OUTPUT',None)
                run_env.pop('XONE_RUST_RECOVERY_CATALOG_OUTPUT',None)
            if 'chain-parser-rust/Cargo.toml' in args:run_env['CARGO_TARGET_DIR']=str(ROOT/'integration/.runtime/design-review-parser-target')
            if name in ['router-transport','go-decode-router']:run_env['XONE_RUST_CHECKPOINT_VECTORS_OUTPUT']=str(out/'router-vectors')
            if name in ['parser-transport','go-decode-parser']:run_env['XONE_RUST_CHECKPOINT_VECTORS_OUTPUT']=str(out/'parser-vectors')
            path=out/(name+'.log')
            with path.open('wb') as log:
                child=subprocess.Popen(args,cwd=ROOT/'chain-parser-go' if name.startswith('go-') else ROOT,env=run_env,stdout=log,stderr=subprocess.STDOUT,start_new_session=True)
                try:code=child.wait()
                finally:
                    if child.poll() is None:
                        os.killpg(child.pid,signal.SIGTERM)
                        try:child.wait(timeout=3)
                        except subprocess.TimeoutExpired:os.killpg(child.pid,signal.SIGKILL);child.wait()
            raw=path.read_bytes();text=raw.decode(errors='replace')
            if b'\0' in raw:code=1
            if name in ['router-transport','parser-transport']:
                for test in ['current_go_checkpoint_vectors_match_reference_cursor_and_full_width_amounts','rust_local_checkpoint_exports_current_vectors_for_independent_go_decoder',
                    'current_go_checkpoint_config_defaults_and_secret_free_hashes_match',
                    'current_go_checkpoint_local_budget_configs_and_hashes_match',
                    'current_go_checkpoint_budget_projection_and_actual_objects_match']:
                    if not re.search(r'^test '+test+r' \.\.\. ok$',text,re.M):code=1
            if name in ['go-decode-router','go-decode-parser'] and not re.search(r'^--- PASS: TestCurrentRustCheckpointVectorsDecodeIndependentlyInGo ',text,re.M):code=1
            if name=='go-workers' and not re.search(r'^--- PASS: TestCheckpointWorkerLocalBudgetRejectsBeforePublicDiskAndRemoteIO ',text,re.M):code=1
            if name=='go-checkpoint':
                for required in ['TestCheckpointBudgetProjectsExactPublicObjectsAndAllowsIdempotentRetry','TestCheckpointBudgetBoundsScanningAndRejectsSymlinkAndDepth']:
                    if not re.search(r'^--- PASS: '+required+r' ',text,re.M):code=1
            if name=='go-decode-recovery' and not re.search(r'^--- PASS: TestCurrentRustRecoveryCatalogBindsOldObjectAndNewHeadInGo ',text,re.M):code=1
            if name=='go-decode-remote' and not re.search(r'^--- PASS: TestCurrentRustRemotePublicationDecodesWithIndependentGoCodec ',text,re.M):code=1
            if name=='parser-service':
                for test in ['service::checkpoint::tests::normal_rust_checkpoint_endpoint_requires_auth_and_committed_cursor','service::checkpoint::tests::rust_checkpoint_gate_wait_is_bounded_and_cancelled_before_file_io',
                    'service::checkpoint::remote_tests::normal_rust_remote_checkpoint_half_upload_retry_and_current_catalog_are_bound',
                    'service::checkpoint::remote_tests::rust_remote_checkpoint_false_ack_corruption_redirect_and_encoding_never_certify',
                    'service::checkpoint::remote_tests::rust_remote_checkpoint_source_change_between_payload_and_manifest_revokes_publication',
                    'service::checkpoint::remote_tests::rust_remote_checkpoint_close_interrupts_pending_http_before_manifest_and_drains_gate',
                    'service::checkpoint::periodic_tests::rust_periodic_checkpoint_is_explicit_and_manual_mode_stays_idle',
                    'service::checkpoint::periodic_tests::rust_periodic_checkpoint_rotates_past_failed_chain_and_publishes_new_generations',
                    'service::checkpoint::periodic_tests::rust_periodic_checkpoint_normal_shutdown_signal_cancels_remote_work_and_joins_task',
                    'service::checkpoint::recovery_tests::normal_rust_recovery_catalog_attests_uploaded_old_object_and_committed_head',
                    'service::checkpoint::recovery_tests::rust_recovery_catalog_rejects_corrupt_hint_and_owner_head_expiration',
                    'service::checkpoint::periodic_tests::rust_periodic_checkpoint_local_budget_rejects_before_file_and_remote_io']:
                    if 'test '+test+' ... ok' not in text:code=1
            report['checks'].append(dict(name=name,exit_code=code,log_sha256=hashlib.sha256(raw).hexdigest(),cargo_target_directory=run_env.get('CARGO_TARGET_DIR') if args[0]=='cargo' else None,go_tests=len(re.findall(r'^--- PASS:',text,re.M)),rust_tests=sum(int(n) for n in re.findall(r'test result: ok\. (\d+) passed;',text))))
            save()
            if code:raise RuntimeError(name+' failed: '+str(path))
            if name in ['go-checkpoint','go-config','router-transport','parser-transport']:
                directory=out/({'go-checkpoint':'go-vectors','go-config':'config-vectors','router-transport':'router-vectors','parser-transport':'parser-vectors'}[name])
                if len(json.loads((directory/'vectors.json').read_text())['vectors'])!=(5 if name=='go-config' else 3):raise RuntimeError('Incomplete independent exports')
                vector_hashes.update({str(p.relative_to(out)):shared.sha_file(p) for p in sorted(directory.rglob('*')) if p.is_file()})
                if name=='go-checkpoint':vector_hashes.update({str(p.relative_to(out)):shared.sha_file(p) for p in sorted((out/'budget-vectors').rglob('*')) if p.is_file()})
        infra=Infrastructure()
        try:
            infra.start()
            for topic in ['test.remote-checkpoint','test.remote-checkpoint-replay','test.rust-owner-recovery']:
                command([KAFKA/'bin/kafka-topics.sh','--bootstrap-server',infra.broker,'--create','--topic',topic,'--partitions','1','--replication-factor','1'],env=infra.env)
            live=run_live(out,infra.broker,env)
            for name,raw,count in live:
                report['checks'].append(dict(name=name,exit_code=0,log_sha256=hashlib.sha256(raw).hexdigest(),go_tests=0,rust_tests=count));save()
            for name,selector,required in [
                ('router-api-kafka','remote_checkpoint_normal_router_cold','test remote_checkpoint::tests::remote_checkpoint_normal_router_cold_restore_and_quote_with_actual_kafka ... ok'),
                ('router-api-bounded-replay','older_remote_checkpoint_replays_actual','test remote_checkpoint::replay_tests::older_remote_checkpoint_replays_actual_kafka_privately_then_publishes_only_attested_head ... ok')]:
                print('Running '+name,flush=True)
                path=out/(name+'.log')
                with path.open('wb') as log:
                    child=subprocess.Popen(cargo+['test']+router+['-p','xone-quote-api','--lib',selector,'--','--include-ignored'],cwd=ROOT,env=dict(env,XONE_TEST_KAFKA_BROKER=infra.broker),stdout=log,stderr=subprocess.STDOUT,start_new_session=True)
                    try:code=child.wait()
                    finally:
                        if child.poll() is None:
                            os.killpg(child.pid,signal.SIGTERM)
                            try:child.wait(timeout=3)
                            except subprocess.TimeoutExpired:os.killpg(child.pid,signal.SIGKILL);child.wait()
                raw=path.read_bytes();text=raw.decode(errors='replace')
                if required not in text or b'\0' in raw:code=1
                report['checks'].append(dict(name=name,exit_code=code,log_sha256=hashlib.sha256(raw).hexdigest(),go_tests=0,rust_tests=sum(int(n) for n in re.findall(r'test result: ok\. (\d+) passed;',text))));save()
                if code:raise RuntimeError(name+' failed')
            report['infrastructure_scope']='OWNED_SINGLE_LOOPBACK_KAFKA_PLUS_TWO_INDEPENDENT_TEST_PROCESSES_AND_NORMAL_HTTP_HANDLERS'
        finally:infra.close()
        for relative,digest in vector_hashes.items():
            if shared.sha_file(out/relative)!=digest:raise RuntimeError('Independent vector bytes changed')
        report['independent_vectors']=vector_hashes
        report['artifacts']={str(p.relative_to(out)):shared.sha_file(p) for directory in ['actual-parser','actual-router','actual-live-recovery'] for p in sorted((out/directory).glob('*')) if p.is_file()}
        if 'actual-parser/normal-rust-remote-checkpoint.json' not in report['artifacts'] or 'actual-parser/rust-periodic-two-chain.json' not in report['artifacts'] or 'actual-live-recovery/independent-router.json' not in report['artifacts']:raise RuntimeError('No actual Rust remote HTTP publication evidence')
        report['parser_binary_sha256']=shared.sha_file(ROOT/'integration/.runtime/design-review-parser-target/debug/chain-parser-rust')
        if shared.fingerprint()!=report['source_tree_sha256'] or shared.test_inputs()!=report['test_inputs']:raise RuntimeError('Source or fixed inputs changed during validation')
        report.update(passed=True,state='LOCAL_SCOPE_VERIFIED',go_test_executions=sum(c['go_tests'] for c in report['checks']),rust_test_executions=sum(c['rust_tests'] for c in report['checks']))
    except KeyboardInterrupt:report.update(state='CANCELLED',error='INTERRUPTED');raise
    except Exception as error:report.update(state='FAILED',error=str(error));raise
    finally:report['finished_ms']=int(time.time()*1000);save()
    print(json.dumps({key:report[key] for key in ['state','go_test_executions','rust_test_executions','run_directory']}),flush=True)
if __name__=='__main__':main()
