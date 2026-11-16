#!/usr/bin/env python3
"""Finite local Raydium CLMM classic execution, capture, original SBF and wallet acceptance."""
import hashlib,json,os,re,shutil,signal,subprocess,sys,time,uuid
from pathlib import Path
ROOT=Path(__file__).resolve().parents[3]
BASE=ROOT/'integration/artifacts/local/rayclmm-execution-check'
def sha(path):return hashlib.sha256(Path(path).read_bytes()).hexdigest()
def fingerprint():
    paths=subprocess.check_output(['git','ls-files','-co','--exclude-standard','-z'],cwd=ROOT).split(b'\0');h=hashlib.sha256()
    for raw in sorted(set(paths)):
        if raw:
            p=ROOT/os.fsdecode(raw)
            if p.is_file()and p.suffix!='.md':h.update(raw+b'\0'+p.read_bytes())
    return h.hexdigest()
def runtime_inputs():
    manifest=ROOT/'dex-router/test/contracts/sbf-runtime/rpc-capture.json';m=json.loads(manifest.read_bytes());result={str(manifest.relative_to(ROOT)):sha(manifest)}
    for name,digest in m['runtime_files'].items():
        p=Path(m['runtime_directory'])/name
        if sha(p)!=digest:raise ValueError('Pinned runtime changed: '+name)
        result[str(p)]=digest
    executable=Path(m['python_executable']['path'])
    if sha(executable)!=m['python_executable']['sha256']:raise ValueError('Python executable identity changed')
    result[str(executable)]=sha(executable)
    for pattern in ['*execution-bank.json','*funding-bank.json','meteora-wsol-bank.json','meteora-token2022-bank.json']:
        for p in sorted((ROOT/'dex-router/test/contracts').glob(pattern)):result[str(p.relative_to(ROOT))]=sha(p)
    for name in ['integration/test/contracts/rayclmm-execution-bank.json','integration/tools/reference/orca-whirlpools/target/release/solana-advanced-orca-reference','integration/test/contracts/orca-exact-out.json']:
        result[name]=sha(ROOT/name)
    adaptive=ROOT/'integration/test/contracts/rayclmm-execution-bank.json'
    for name,digest in json.loads(adaptive.read_bytes())['inputs'].items():
        path=Path(name) if Path(name).is_absolute() else ROOT/name
        if sha(path)!=digest:raise ValueError('Original reference input changed: '+name)
        result[str(path) if Path(name).is_absolute() else name]=digest
    return result

def main():
    out=BASE/'runs'/uuid.uuid4().hex;out.mkdir(parents=True,exist_ok=False)
    env=dict(os.environ,PATH=str(Path.home()/'.cargo/bin')+os.pathsep+os.environ['PATH'])
    env.setdefault('CARGO_BUILD_JOBS','2');env.setdefault('RUST_TEST_THREADS','2')
    env.setdefault('XONE_SOLANA_PYTHON_PATH',str(ROOT/'integration/artifacts/local/r01-solana-rebuild/python'))
    bundled=Path.home()/'.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/playwright'
    if bundled.exists():env.setdefault('PLAYWRIGHT_MODULE',str(bundled))
    env.update(XONE_RAYCLMM_EXECUTION_DIR=str(out/'api'),XONE_RAYCLMM_EXECUTION_RPC_DIR=str(out/'rpc'),XONE_BROWSER_OUTPUT=str(out/'browser'),XONE_TEST_RPC_CAPTURE='1',XONE_TEST_SUBMISSION_DIR=str(out/'browser/receipts'),XONE_TEST_SBF_MANIFEST_DIR=str(ROOT/'dex-router/test/contracts/sbf-runtime'))
    cargo=['cargo','+1.90.0'];manifest=['--locked','--manifest-path','dex-router/Cargo.toml'];py=[sys.executable]
    checks=[
        ('bank-reference',py+['integration/tools/ci/rayclmm-execution-bank.py']),
        ('format',cargo+['fmt','--all','--check','--manifest-path','dex-router/Cargo.toml']),
        ('contracts',py+['integration/tools/ci/contracts.py']),
        ('protocol',py+['common/protoc/check.py']),
        ('capabilities',cargo+['test']+manifest+['-p','xone-protocol-math','--test','capabilities']),
        ('settlement-unit',cargo+['test']+manifest+['-p','xone-quote-api','--lib','funding_tests']),
        ('rayclmm-api',cargo+['test']+manifest+['-p','xone-quote-api','--test','rayclmm_preparation','--test','rayclmm_exact_out']),
        ('native-original',py+['integration/tools/acceptance/rayclmm_execution_reference.py','--api-dir',str(out/'api'),'--rpc-dir',str(out/'rpc'),'--output',str(out/'original.json')]),
        ('evidence-controls',py+['-m','unittest','discover','-s','integration/tools/acceptance/tests','-p','test_rayclmm_execution_reference.py']),
        ('evidence-controls-optimized',py+['-OO','-m','unittest','discover','-s','integration/tools/acceptance/tests','-p','test_rayclmm_execution_reference.py']),
        ('rayclmm-worker-controls',py+['-m','unittest','discover','-s','integration/tools/acceptance/tests','-p','test_rayclmm_worker.py']),
        ('rayclmm-worker-controls-optimized',py+['-OO','-m','unittest','discover','-s','integration/tools/acceptance/tests','-p','test_rayclmm_worker.py']),
        ('legacy-classic-execution',cargo+['test']+manifest+['-p','xone-quote-api','--test','pump_amm_funding','--test','raydium_funding','--test','raydium_preparation','--test','orca_preparation','--test','orca_adaptive_preparation']),
        ('account-capture-regression',cargo+['test']+manifest+['-p','xone-quote-api','--test','execution_preparation','--test','rpc_capture']),
        ('receipt-regression',cargo+['test']+manifest+['-p','xone-quote-api','--test','transaction_lifecycle']),
        ('rayclmm-math',cargo+['test']+manifest+['-p','xone-protocol-math','--test','rayclmm_exact_out']),
        ('rayclmm-routing',cargo+['test']+manifest+['-p','xone-routing','-p','xone-optimizer','--test','rayclmm_exact_out']),
        ('orca-math',cargo+['test']+manifest+['-p','xone-protocol-math','--test','orca_exact_out','--test','orca_differential','--test','orca_steps','--test','orca_adaptive_online','--test','orca_adaptive_rewards','--test','orca_adaptive_dynamic']),
        ('orca-routing',cargo+['test']+manifest+['-p','xone-routing','-p','xone-optimizer','--test','orca_exact_out']),
        ('parser',cargo+['test','--locked','--manifest-path','chain-parser-rust/Cargo.toml','-p','xone-solana-ingest','--test','orca_decode','--test','orca_adaptive','--test','orca_adaptive_rewards','--test','orca_adaptive_dynamic','--test','rayclmm_decode','--test','rayclmm_exact_out']),
        ('api-lint',cargo+['clippy']+manifest+['-p','xone-quote-api','--lib','--test','rayclmm_preparation','--test','rayclmm_exact_out','--example','rayclmm_frontend_harness','--example','orca_adaptive_frontend_harness','--example','orca_frontend_harness','--example','frontend_harness','--','-D','warnings']),
        ('core-lint',cargo+['clippy']+manifest+['-p','xone-execution-preparation','-p','xone-protocol-math','-p','xone-simulation','--lib','--test','capabilities','--','-D','warnings']),
        ('build',cargo+['build']+manifest+['-p','xone-quote-api','--example','frontend_harness','--example','orca_frontend_harness','--example','orca_adaptive_frontend_harness','--example','rayclmm_frontend_harness']),
        ('frontend-check',['npm','--prefix','frontend','run','check']),
        ('frontend-unit',['npm','--prefix','frontend','test']),
        ('legacy-browser',['node','frontend/test/browser.mjs']),
        ('orca-browser',['node','frontend/test/orca-execution-browser.mjs']),
        ('adaptive-browser',['node','frontend/test/orca-adaptive-execution-browser.mjs']),
        ('rayclmm-browser',['node','frontend/test/rayclmm-execution-browser.mjs']),
        ('progress-docs',py+['-m','unittest','discover','-s','integration/tools/ci/tests','-p','test_progress_document.py']),
        ('progress-docs-optimized',py+['-OO','-m','unittest','discover','-s','integration/tools/ci/tests','-p','test_progress_document.py']),
    ]
    source=fingerprint();inputs=runtime_inputs();report=dict(passed=False,state='RUNNING',source_tree_sha256=source,run_directory=str(out.relative_to(ROOT)),runtime_and_bank_inputs=inputs,checks=[],started_ms=int(time.time()*1000),publicTransactionsSubmitted=False,production_ready=False,all_requested_features_complete=False)
    target=BASE/'report.json'
    def save():
        text=json.dumps(report,indent=2)+'\n';(out/'report.json').write_text(text);temp=target.with_suffix('.tmp');temp.write_text(text);temp.replace(target)
    save()
    def interrupted(signum,frame):
        report.update(passed=False,state='CANCELLED',error='INTERRUPTED',interrupted_signal=signum,finished_ms=int(time.time()*1000))
        save()
        raise KeyboardInterrupt(f'Interrupted by signal {signum}')
    signal.signal(signal.SIGTERM,interrupted)
    signal.signal(signal.SIGINT,interrupted)
    for name,command in checks:
        print('Running '+name,flush=True)
        with(out/f'{name}.log').open('wb')as log:
            check_env=env.copy()
            if name=='orca-browser':check_env.update(XONE_BROWSER_OUTPUT=str(out/'orca-browser'),XONE_TEST_SUBMISSION_DIR=str(out/'orca-browser/receipts'))
            if name=='adaptive-browser':check_env.update(XONE_BROWSER_OUTPUT=str(out/'adaptive-browser'),XONE_TEST_SUBMISSION_DIR=str(out/'adaptive-browser/receipts'))
            if name=='rayclmm-browser':check_env.update(XONE_BROWSER_OUTPUT=str(out/'rayclmm-browser'),XONE_TEST_SUBMISSION_DIR=str(out/'rayclmm-browser/receipts'))
            child=subprocess.Popen(command,cwd=ROOT,env=check_env,stdout=log,stderr=subprocess.STDOUT,start_new_session=True)
            try:returncode=child.wait()
            finally:
                if child.poll()is None:
                    os.killpg(child.pid,signal.SIGTERM)
                    try:child.wait(timeout=2)
                    except subprocess.TimeoutExpired:os.killpg(child.pid,signal.SIGKILL);child.wait()
        if name=='build'and returncode==0:
            directory=out/'binaries';directory.mkdir(exist_ok=True);bindings=[]
            for executable in ['frontend_harness','orca_frontend_harness','orca_adaptive_frontend_harness','rayclmm_frontend_harness']:
                source_path=ROOT/'dex-router/target/debug/examples'/executable;destination=directory/executable;digest=sha(source_path);shutil.copy2(source_path,destination)
                if sha(destination)!=digest:raise ValueError('Retained executable differs: '+executable)
                bindings.append(dict(source=str(source_path.relative_to(ROOT)),retained=str(destination.relative_to(ROOT)),sha256=digest))
            (directory/'manifest.json').write_text(json.dumps(dict(source_tree_sha256=source,boundExecutables=bindings),indent=2)+'\n')
        raw=(out/f'{name}.log').read_bytes()
        if bytes([0])in raw:returncode=1
        count=sum(int(n)for n in re.findall(r'test result: ok\. (\d+) passed;',raw.decode(errors='replace')))
        report['checks'].append(dict(name=name,exit_code=returncode,rust_test_executions=count));save()
        if returncode:report.update(state='FAILED',error=name);save();raise SystemExit(name+' failed: '+str(out/f'{name}.log'))
    if fingerprint()!=source or runtime_inputs()!=inputs:report.update(state='SOURCE_OR_INPUT_CHANGED');save();raise SystemExit(report['state'])
    browser=json.loads((out/'browser/result.json').read_bytes());orca_browser=json.loads((out/'orca-browser/result.json').read_bytes());adaptive_browser=json.loads((out/'adaptive-browser/result.json').read_bytes());rayclmm_browser=json.loads((out/'rayclmm-browser/result.json').read_bytes());original=json.loads((out/'original.json').read_bytes())
    if not browser['passed']or not orca_browser['passed']or not adaptive_browser['passed']or not rayclmm_browser['passed']or not original['passed']or len(rayclmm_browser['checks'])!=129 or sum(x.startswith('rayclmm_execution_')for x in rayclmm_browser['checks'])!=129 or len(orca_browser['checks'])!=81 or len(adaptive_browser['checks'])!=121 or sum(x.startswith('orca_adaptive_execution_')for x in adaptive_browser['checks'])!=121:raise ValueError('Missing actual original/browser qualification')
    prep=list((out/'api').glob('rayclmm-*-preparation.json'));receipts=list((out/'api').glob('rayclmm-*-receipt.json'))
    if len(prep)!=56 or len(receipts)!=56 or len(original['rows'])!=56 or original['originalTransactions']!=112:raise ValueError('Missing actual service captures/receipts/transactions')
    for p in receipts:
        value=json.loads(p.read_bytes());r=value['receipt'];f=value['preparation']
        if r['state']!='FINALIZED'or r['settlement']['payerLamportDelta']!=f['expectedPayerLamportDelta']or r['settlement']['actualPersistentRentLamports']!=str(int(f['rentLamports'])-int(f['rentRefundLamports'])):raise ValueError('Actual receipt rent/funding mismatch')
    artifacts=[p for p in out.rglob('*')if p.is_file()and p.name!='report.json']
    artifacts += [ROOT/'integration/test/contracts/rayclmm-execution-bank.json',ROOT/'dex-router/test/contracts/rayclmm-execution-bank.json',ROOT/'dex-router/target/debug/examples/frontend_harness',ROOT/'dex-router/target/debug/examples/orca_frontend_harness',ROOT/'dex-router/target/debug/examples/orca_adaptive_frontend_harness',ROOT/'dex-router/target/debug/examples/rayclmm_frontend_harness']
    report.update(passed=True,state='LOCAL_SCOPE_VERIFIED',verification_scope='CLASSIC_RAYDIUM_CLMM_STATIC_REWARDS_TOKEN_FEE_CAPTURE_SBF_WALLET_LOCAL',finished_ms=int(time.time()*1000),rust_test_executions=sum(c['rust_test_executions']for c in report['checks']),browser_checks=len(browser['checks'])+len(orca_browser['checks'])+len(adaptive_browser['checks'])+len(rayclmm_browser['checks']),rayclmm_browser_checks=len(rayclmm_browser['checks']),adaptive_browser_checks=len(adaptive_browser['checks']),orca_browser_checks=len(orca_browser['checks']),legacy_browser_checks=len(browser['checks']),actual_api_preparations=56,actual_api_receipts=56,original_transactions=112,artifacts=[dict(path=str(p.relative_to(ROOT)),sha256=sha(p))for p in sorted(artifacts)])
    save();print(json.dumps({k:report[k]for k in ['passed','state','rust_test_executions','browser_checks','original_transactions']}))
if __name__=='__main__':main()
