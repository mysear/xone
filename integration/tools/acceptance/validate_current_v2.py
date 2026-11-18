#!/usr/bin/env python3
"""Current public V2 source -> normal owner/router, durable replay and rejection.

The live gate is explicit; offline replay authenticates captured bytes but does
not upgrade their provider authority or substitute for live-source availability.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import shutil
import signal
import subprocess
import sys
import time
import uuid
import current_v2_evidence as evidence
import validate_cache_reclamation as shared
ROOT=Path(__file__).resolve().parents[3]
BASE=ROOT/'integration/artifacts/local/current-v2-check'

def replay_round(out,source,owner_binary,router_binary):
    records,inventory=evidence.capture_index(source/'owner/state/raw-evidence/evm-1')
    target=out/'captured-source';target.mkdir();before={}
    for row in inventory:
        origin=source/'owner/state/raw-evidence/evm-1'/row['file'];dest=target/row['file'];shutil.copy2(origin,dest);evidence.require(shared.sha_file(dest)==row['sha256'],'CAPTURE_COPY_CHANGED');before[row['file']]=row['sha256']
    (target/'inventory.json').write_text(json.dumps({'source_report_sha256':shared.sha_file(source/'report.json'),'records':inventory},indent=2)+'\n')
    def scenario(name,fault=None,state=None,replica=False):
        servers=[evidence.RecordedRPC(records[evidence.provider_id(endpoint)],fault) for endpoint in evidence.ENDPOINTS]
        try:
            urls=[server.start() for server in servers]
            result=evidence.run_processes(out/name,owner_binary,router_binary,urls,owner_state=state,reject=fault is not None,replica=replica)
            if fault:
                evidence.require(any(row.get('mutated') for server in servers for row in server.trace),'FAULT_NOT_EXERCISED')
                expected={'pair-getter':'V2_COMPLETE_STATE_PROOF_VALUE','token-getter':'V2_COMPLETE_TOKEN_PROOF_BALANCE','pair-proof':'STATE_ACCOUNT_PROOF_INVALID','proof-unavailable':'RPC_ERROR_-32601','header':'BLOCK_INTEGRITY_HEADER_HASH'}[fault]
                evidence.require(expected in result['startup_failure_code'] and 'NATIVE_RUNTIME_PROOF_BUDGET' not in result['startup_failure_code'],'FALSE_OR_MISLABELED_SOURCE_REJECTION')
            evidence.require(not any(row.get('error') for server in servers for row in server.trace),'UNRECORDED_REPLAY_REQUEST')
            (out/name/'rpc-trace.json').write_text(json.dumps({'source_mode':'RETAINED_PUBLIC_BYTES_LOOPBACK_ONLY','providers':[{'original_endpoint':url,'json_byte_variants':server.variants,'requests':server.trace} for url,server in zip(evidence.ENDPOINTS,servers)]},indent=2)+'\n')
            return result
        finally:
            for server in servers:server.close()
    normal=scenario('offline-normal',replica=True)
    original=json.loads((source/'report.json').read_text())
    evidence.require(normal['baseline']['body']['reference']['block']==original['baseline']['body']['reference']['block'] and normal['baseline']['body']['pools']==original['baseline']['body']['pools'],'REPLAY_CHANGED_PUBLIC_STATE')
    resumed=scenario('offline-owner-restart',state=out/'offline-normal/owner/state')
    first=normal['baseline']['body']['reference'];next_=resumed['baseline']['body']['reference']
    evidence.require(first['block']==next_['block'] and all(next_[key]>first[key] for key in ['writer_epoch','online_generation','journal_seq']) and next_['canonical_epoch']==first['canonical_epoch'],'OWNER_RESTART_REUSED_STALE_GENERATION')
    faults={name:scenario('fault-'+name,name) for name in ['pair-getter','token-getter','pair-proof','proof-unavailable','header']}
    for name,digest in before.items():evidence.require(shared.sha_file(target/name)==digest,'RETAINED_SOURCE_CHANGED_DURING_REPLAY')
    return {'capture_inventory_sha256':shared.sha_file(target/'inventory.json'),'capture_files':len(inventory),'capture_bytes':sum(r['bytes'] for r in inventory),'offline_normal':normal,'owner_restart':resumed,'faults':{key:{'startup_failure_code':row['startup_failure_code'],'http_status':row['rejected_quote']['status'],'passed':row['passed']} for key,row in faults.items()},'original_capture_unchanged':True}

def main():
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--live',action='store_true');parser.add_argument('--capture',type=Path);parser.add_argument('--fixture',type=Path);args=parser.parse_args()
    if sum([args.live,bool(args.capture),bool(args.fixture)])!=1:parser.error('select exactly one of --live, --capture or --fixture')
    out=BASE/'runs'/uuid.uuid4().hex;out.mkdir(parents=True)
    env=dict(os.environ,PATH=str(Path.home()/'.cargo/bin')+os.pathsep+os.environ['PATH'],CARGO_BUILD_JOBS='2',CARGO_TARGET_DIR=str(ROOT/'integration/.runtime/design-review-target'))
    if sys.platform=='darwin':env['CARGO_TARGET_'+('AARCH64' if platform.machine()=='arm64' else 'X86_64')+'_APPLE_DARWIN_RUNNER']=str(ROOT/'integration/tools/ci/native-test-runner.py')
    cargo=['cargo','+1.90.0'];router=['--locked','--manifest-path','dex-router/Cargo.toml'];py=[sys.executable]
    checks=[
        ('go-format',py+['-c','import subprocess,sys;r=subprocess.check_output(["gofmt","-l","internal/chain/evm"],text=True);print(r,end="");sys.exit(bool(r))']),
        ('evidence-guards',py+['-m','unittest','discover','-s','integration/tools/acceptance/tests','-p','test_current_v2_evidence.py']),
        ('contracts',py+['integration/tools/ci/contracts.py']),
        ('progress',py+['-m','unittest','discover','-s','integration/tools/ci/tests','-p','test_progress_document.py']),
        ('go-proof-regression',['go','test','-race','-timeout=20m','-count=1','-v','./internal/chain/evm','-run','Native|ContractStorage|V2Complete|CommittedView']),
        ('go-normal',['go','test','-race','-count=1','-v','./internal/service','-run','Native|NormalEthereumServiceIntegrity|NormalSourceEvidenceDiscloses|NormalParserUnsupportedSafe']),
        ('go-vet',['go','vet','./internal/chain/evm','./internal/service']),
        ('module-cache',py+['integration/tools/ci/verify_go_module_cache.py']),
        ('go-build',['go','build','-trimpath','-o',str(out/'owner-binary'),'./cmd/chain-parser']),
        ('router-api',cargo+['test']+router+['-p','xone-quote-api','--lib']),
        ('router-lint',cargo+['clippy']+router+['-p','xone-quote-api','--all-targets','--','-D','warnings']),
        ('router-build',cargo+['build']+router+['--bin','dex-router']),
    ]
    report={'passed':False,'state':'RUNNING','source_tree_sha256':shared.fingerprint(),'test_inputs':shared.test_inputs(),'checks':[],'started_ms':int(time.time()*1000),'run_directory':str(out.relative_to(ROOT)),'verification_scope':'ONE_REAL_V2_PUBLIC_STATE_NORMAL_OWNER_ROUTER_HTTP_DEVELOPMENT_JOURNAL_REPLAY_RESTART_AND_SOURCE_REJECTION','live_public_source_observed':False,'consensus_verified':False,'independent_execution_balance_delta_verified':False,'production_ready':False,'all_requested_features_complete':False,'public_transactions_submitted':False}
    def save():
        text=json.dumps(report,indent=2)+'\n';(out/'report.json').write_text(text);temp=BASE/'report.tmp';temp.write_text(text);temp.replace(BASE/'report.json')
    def interrupted(signum,frame):raise KeyboardInterrupt('INTERRUPTED')
    signal.signal(signal.SIGTERM,interrupted);signal.signal(signal.SIGINT,interrupted);save()
    try:
        for name,command in checks:
            print('Running '+name,flush=True);run_env=dict(env)
            if name=='router-api':run_env['RUST_TEST_THREADS']='1'
            path=out/(name+'.log')
            with path.open('wb') as stream:
                child=subprocess.Popen(command,cwd=ROOT/'chain-parser-go' if name.startswith('go-') else ROOT,env=run_env,stdout=stream,stderr=subprocess.STDOUT,start_new_session=True)
                try:code=child.wait()
                finally:
                    if child.poll() is None:evidence.stop(child)
            raw=path.read_bytes();text=raw.decode(errors='replace')
            if b'\0' in raw:code=1
            if name=='go-proof-regression':
                for required in ['TestNativeSourceRejectionPreservesCauseWithoutInventingBudgetOverrun','TestNativeRejectedSessionCannotBeRevivedByRefillingItsAllowance','TestV2CompleteNativeBalanceProofBindsOriginalTokenCodeAndReachedSlots','TestContractStorageSessionCancellationAndRecorderFailureStayTerminal','TestContractStorageNormalSessionAliasBudgetAndContext']:
                    if not re.search(r'^--- PASS: '+required+r' ',text,re.M):code=1
            report['checks'].append({'name':name,'exit_code':code,'log_sha256':shared.sha_file(path),'go_tests':len(re.findall(r'^--- PASS:',text,re.M)),'rust_tests':sum(int(n) for n in re.findall(r'test result: ok\. (\d+) passed;',text)),'rust_test_threads':run_env.get('RUST_TEST_THREADS') if command[0]=='cargo' else None});save()
            if code:raise RuntimeError(name+' failed: '+str(path))
        router_binary=out/'router-binary';shutil.copy2(ROOT/'integration/.runtime/design-review-target/debug/dex-router',router_binary);router_binary.chmod(0o700)
        if args.live:
            print('Running actual-public-current-state',flush=True)
            live=evidence.run_processes(out/'live',out/'owner-binary',router_binary,evidence.ENDPOINTS,live=True)
            report['live_public_source_observed']=True;report['live_report_sha256']=shared.sha_file(out/'live/report.json');source=out/'live'
        elif args.fixture:
            source=out/'fixture-input'
            manifest=evidence.unpack_fixture(args.fixture.resolve(),source/'owner/state/raw-evidence/evm-1')
            stored={'baseline':{'body':{'reference':{'block':manifest['block']},'pools':[manifest['expected_pool']]}},'source_mode':'FIXED_PUBLIC_CAPTURE_NOT_CURRENT_SOURCE','acquisition':manifest['acquisition']}
            (source/'report.json').write_text(json.dumps(stored,indent=2)+'\n')
            report['fixed_fixture_manifest_sha256']=shared.sha_file(args.fixture/'manifest.json')
        else:
            source=args.capture.resolve();evidence.require(source.is_relative_to((ROOT/'integration/artifacts/local').resolve()),'LOCAL_CAPTURE_SCOPE')
            report['historical_capture']=str(source.relative_to(ROOT));report['historical_capture_report_sha256']=shared.sha_file(source/'report.json')
        print('Running offline-public-replay-and-faults',flush=True);replayed=replay_round(out,source,out/'owner-binary',router_binary)
        (out/'replay-results.json').write_text(json.dumps(replayed,indent=2)+'\n')
        report['checks'].append({'name':'normal-process-source-replay','exit_code':0,'go_tests':0,'rust_tests':0,'artifact_sha256':shared.sha_file(out/'replay-results.json')});save()
        evidence.require(shared.fingerprint()==report['source_tree_sha256'] and shared.test_inputs()==report['test_inputs'],'SOURCE_OR_FIXED_INPUT_CHANGED')
        report.update(passed=True,state='LOCAL_SCOPE_VERIFIED',go_test_executions=sum(row['go_tests'] for row in report['checks']),rust_test_executions=sum(row['rust_tests'] for row in report['checks']),capture_inventory_sha256=replayed['capture_inventory_sha256'],capture_files=replayed['capture_files'],capture_bytes=replayed['capture_bytes'],binary_sha256={'owner':shared.sha_file(out/'owner-binary'),'router':shared.sha_file(router_binary)},artifacts={str(p.relative_to(out)):shared.sha_file(p) for p in sorted(out.rglob('report.json')) if p.parent!=out})
    except KeyboardInterrupt:report.update(state='CANCELLED',error='INTERRUPTED');raise
    except Exception as error:report.update(state='FAILED',error=str(error));raise
    finally:report['finished_ms']=int(time.time()*1000);save()
    print(json.dumps({key:report[key] for key in ['state','go_test_executions','rust_test_executions','run_directory','live_public_source_observed','capture_files']}),flush=True)
if __name__=='__main__':main()
