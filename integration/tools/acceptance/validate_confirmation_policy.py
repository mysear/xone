#!/usr/bin/env python3
"""Current-head policy across normal Go startup, CLI and two Rust compilers."""
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
BASE=ROOT/'integration/artifacts/local/confirmation-policy-check'

def main():
    out=BASE/'runs'/uuid.uuid4().hex;out.mkdir(parents=True)
    env=dict(os.environ,PATH=str(Path.home()/'.cargo/bin')+os.pathsep+os.environ['PATH'],CARGO_BUILD_JOBS='2',CARGO_TARGET_DIR=str(ROOT/'integration/.runtime/design-review-target'),XONE_CONFIRMATION_CONFIG_OUTPUT=str(out/'config-vectors'),XONE_CONFIRMATION_HTTP_OUTPUT=str(out/'actual-http'))
    if sys.platform=='darwin':env['CARGO_TARGET_'+('AARCH64' if platform.machine()=='arm64' else 'X86_64')+'_APPLE_DARWIN_RUNNER']=str(ROOT/'integration/tools/ci/native-test-runner.py')
    cargo=['cargo','+1.90.0'];router=['--locked','--manifest-path','dex-router/Cargo.toml'];parser=['--locked','--manifest-path','chain-parser-rust/Cargo.toml'];py=[sys.executable]
    checks=[
      ('go-format',py+['-c','import subprocess,sys;r=subprocess.check_output(["gofmt","-l","internal/config","internal/service"],text=True);print(r,end="");sys.exit(bool(r))']),
      ('router-format',cargo+['fmt','--all','--check','--manifest-path','dex-router/Cargo.toml']),
      ('parser-format',cargo+['fmt','--all','--check','--manifest-path','chain-parser-rust/Cargo.toml']),
      ('contracts',py+['integration/tools/ci/contracts.py']),
      ('progress',py+['-m','unittest','discover','-s','integration/tools/ci/tests','-p','test_progress_document.py']),
      ('go-config',['go','test','-race','-count=1','-v','./internal/config']),
      ('go-normal',['go','test','-race','-count=1','-v','./internal/service','-run','NormalSourceEvidenceDiscloses|NormalParserRejectsHistorical|NormalParserUnsupportedSafe|ParserAuthReadiness|NormalEthereumServiceIntegrity|RuntimeSourcesLifecycleDiscoveryAndHandler']),
      ('router-config',cargo+['test']+router+['-p','xone-config']),
      ('router-cross',cargo+['test']+router+['-p','xone-config','--test','confirmation','current_go_evm_confirmation_policy_and_hashes_match','--','--include-ignored']),
      ('parser-config',cargo+['test']+parser+['-p','xone-config']),
      ('parser-cross',cargo+['test']+parser+['-p','xone-config','--test','confirmation','current_go_evm_confirmation_policy_and_hashes_match','--','--include-ignored']),
      ('router-lint',cargo+['clippy']+router+['-p','xone-config','--all-targets','--','-D','warnings']),
      ('parser-lint',cargo+['clippy']+parser+['-p','xone-config','--all-targets','--','-D','warnings']),
      ('module-cache',py+['integration/tools/ci/verify_go_module_cache.py']),
      ('go-vet',['go','vet','./internal/config','./internal/service']),
      ('go-build',['go','build','-trimpath','-o',str(out/'chain-parser-go'),'./cmd/chain-parser']),
      ('router-build',cargo+['build']+router+['--bin','dex-router']),
      ('parser-build',cargo+['build']+parser+['--bin','chain-parser-rust']),
    ]
    report=dict(state='RUNNING',passed=False,source_tree_sha256=shared.fingerprint(),test_inputs=shared.test_inputs(),run_directory=str(out.relative_to(ROOT)),started_ms=int(time.time()*1000),checks=[],verification_scope='EVM_HEAD_FIRST_CURRENT_PROVIDER_TAG_POLICY_AND_NORMAL_STARTUP_NOT_CONSENSUS',consensus_verified=False,production_ready=False,all_requested_features_complete=False)
    def save():
        text=json.dumps(report,indent=2)+'\n';(out/'report.json').write_text(text);temp=BASE/'report.tmp';temp.write_text(text);temp.replace(BASE/'report.json')
    def interrupted(signum,frame):raise KeyboardInterrupt('INTERRUPTED')
    signal.signal(signal.SIGTERM,interrupted);signal.signal(signal.SIGINT,interrupted);save()
    try:
        vectors={}
        for name,args in checks:
            print('Running '+name,flush=True);path=out/(name+'.log');run_env=dict(env)
            if 'chain-parser-rust/Cargo.toml' in args:run_env['CARGO_TARGET_DIR']=str(ROOT/'integration/.runtime/design-review-parser-target')
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
            if name in ['router-cross','parser-cross'] and not re.search(r'^test current_go_evm_confirmation_policy_and_hashes_match \.\.\. ok$',text,re.M):code=1
            if name in ['router-config','parser-config'] and 'test evm_confirmation_policy_rejects_historical_pending_and_unknown_tags ... ok' not in text:code=1
            if name=='go-config':
                for required in ['TestEVMConfirmationPolicyRequiresCurrentHeadTagBeforeOpeningResources','TestEVMConfirmationPolicyExportsCurrentCrossCompilerInputsAndHashes']:
                    if not re.search(r'^--- PASS: '+required+r' ',text,re.M):code=1
            if name=='go-normal':
                for required in ['TestNormalParserRejectsHistoricalConfirmationBeforeCreatingState','TestNormalParserUnsupportedSafeTagDoesNotFallBackOrPublishReady','TestNormalSourceEvidenceDisclosesConfiguredHeadTagWithoutConsensusClaim','TestNormalEthereumServiceIntegrityFailureRevokesScopeAndRetainsRawDLQ']:
                    if not re.search(r'^--- PASS: '+required+r' ',text,re.M):code=1
            report['checks'].append(dict(name=name,exit_code=code,log_sha256=hashlib.sha256(raw).hexdigest(),cargo_target_directory=run_env['CARGO_TARGET_DIR'] if args[0]=='cargo' else None,go_tests=len(re.findall(r'^--- PASS:',text,re.M)),rust_tests=sum(int(n) for n in re.findall(r'test result: ok\. (\d+) passed;',text))));save()
            if code:raise RuntimeError(name+' failed: '+str(path))
            if name=='go-config':
                vectors={str(p.relative_to(out)):shared.sha_file(p) for p in sorted((out/'config-vectors').glob('*.json'))}
                if len(vectors)!=16:raise RuntimeError('Missing current configuration exports')
        print('Running actual-go-cli',flush=True)
        cases=json.loads((out/'config-vectors/confirmation-vectors.json').read_text())['cases'];cli=[]
        if len(cases)!=15 or sum(bool(row['valid']) for row in cases)!=3:raise RuntimeError('Incomplete confirmation cases')
        for row in cases:
            result=subprocess.run([str(out/'chain-parser-go'),'--config',str(out/'config-vectors'/row['file']),'--check-config'],cwd=ROOT,env=env,capture_output=True,text=True,timeout=10)
            if row['valid']:
                if result.returncode!=0:raise RuntimeError('Valid current tag failed CLI')
                response=json.loads(result.stdout)
                if response['public_hash']!=row['public_hash'] or response['economic_hash']!=row['economic_hash']:raise RuntimeError('CLI configuration hash disagrees')
            else:
                if result.returncode==0 or json.loads(result.stderr)['fields']['error_code']!='CONFIG_CHAIN_CONFIRMATION' or 'private-input-must-not-leak' in result.stderr:raise RuntimeError('Invalid head policy passed CLI or leaked input')
            cli.append(dict(file=row['file'],expected_valid=row['valid'],exit_code=result.returncode,stdout_sha256=hashlib.sha256(result.stdout.encode()).hexdigest(),stderr_sha256=hashlib.sha256(result.stderr.encode()).hexdigest()))
        if (out/'config-vectors/must-not-initialize-state').exists():raise RuntimeError('check-config initialized state')
        cli_path=out/'actual-cli.json';cli_path.write_text(json.dumps(dict(cases=cli,state_directory_created=False),indent=2)+'\n')
        report['checks'].append(dict(name='actual-go-cli',exit_code=0,go_tests=0,rust_tests=0,artifact_sha256=shared.sha_file(cli_path)));save()
        http_path=out/'actual-http/source-confirmation-policy.json';http=json.loads(http_path.read_text())['responses']
        if {row['confirmation_tag'] for row in http}!={'latest','safe','finalized'} or len(http)!=3 or any(row['consensus_verified'] or row['confirmation_policy']!='CONFIGURED_PROVIDER_TAG' for row in http):raise RuntimeError('Missing normal source policy disclosure')
        for relative,digest in vectors.items():
            if shared.sha_file(out/relative)!=digest:raise RuntimeError('Cross-compiler vector changed')
        if shared.fingerprint()!=report['source_tree_sha256'] or shared.test_inputs()!=report['test_inputs']:raise RuntimeError('Source or fixed input changed during validation')
        report.update(state='LOCAL_SCOPE_VERIFIED',passed=True,go_test_executions=sum(c['go_tests'] for c in report['checks']),rust_test_executions=sum(c['rust_tests'] for c in report['checks']),independent_vectors=vectors,artifacts={str(p.relative_to(out)):shared.sha_file(p) for p in [cli_path,http_path]},go_binary_sha256=shared.sha_file(out/'chain-parser-go'))
    except KeyboardInterrupt:report.update(state='CANCELLED',error='INTERRUPTED');raise
    except Exception as error:report.update(state='FAILED',error=str(error));raise
    finally:report['finished_ms']=int(time.time()*1000);save()
    print(json.dumps({key:report[key] for key in ['state','go_test_executions','rust_test_executions','run_directory']}),flush=True)
if __name__=='__main__':main()
