#!/usr/bin/env python3
"""Finite local Raydium funding, capture, original SBF and wallet acceptance."""
import hashlib,json,os,re,signal,subprocess,sys,time,uuid
from pathlib import Path
ROOT=Path(__file__).resolve().parents[3]
BASE=ROOT/'integration/artifacts/local/raydium-funding-check'
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
    for name in ['integration/test/contracts/raydium-execution-bank.json','integration/.runtime/raydium-reference/target/debug/raydium-cpmm-reference','integration/test/contracts/raydium-funding-bank.json','dex-router/test/contracts/raydium-funding-bank.json']:
        result[name]=sha(ROOT/name)
    return result

def main():
    out=BASE/'runs'/uuid.uuid4().hex;out.mkdir(parents=True,exist_ok=False)
    def interrupted(signum,frame):raise KeyboardInterrupt(f'Interrupted by signal {signum}')
    signal.signal(signal.SIGTERM,interrupted)
    env=dict(os.environ,PATH=str(Path.home()/'.cargo/bin')+os.pathsep+os.environ['PATH'])
    env.setdefault('CARGO_BUILD_JOBS','2');env.setdefault('RUST_TEST_THREADS','2')
    env.setdefault('XONE_SOLANA_PYTHON_PATH',str(ROOT/'integration/artifacts/local/r01-solana-rebuild/python'))
    bundled=Path.home()/'.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/playwright'
    if bundled.exists():env.setdefault('PLAYWRIGHT_MODULE',str(bundled))
    env.update(XONE_RAYDIUM_FUNDING_DIR=str(out/'api'),XONE_RAYDIUM_FUNDING_RPC_DIR=str(out/'rpc'),XONE_BROWSER_OUTPUT=str(out/'browser'),XONE_TEST_RPC_CAPTURE='1',XONE_TEST_SUBMISSION_DIR=str(out/'browser/receipts'),XONE_TEST_SBF_MANIFEST_DIR=str(ROOT/'dex-router/test/contracts/sbf-runtime'))
    cargo=['cargo','+1.90.0'];manifest=['--locked','--manifest-path','dex-router/Cargo.toml'];py=[sys.executable]
    checks=[
        ('bank-reference',py+['integration/tools/ci/raydium-funding-bank.py']),
        ('format',cargo+['fmt','--all','--check','--manifest-path','dex-router/Cargo.toml']),
        ('contracts',py+['integration/tools/ci/contracts.py']),
        ('protocol',py+['common/protoc/check.py']),
        ('capabilities',cargo+['test']+manifest+['-p','xone-protocol-math','--test','capabilities']),
        ('settlement-unit',cargo+['test']+manifest+['-p','xone-quote-api','--lib','funding_tests']),
        ('raydium-api',cargo+['test']+manifest+['-p','xone-quote-api','--test','raydium_funding','--test','raydium_preparation']),
        ('native-original',py+['integration/tools/acceptance/raydium_funding_reference.py','--api-dir',str(out/'api'),'--rpc-dir',str(out/'rpc'),'--output',str(out/'original.json')]),
        ('evidence-controls',py+['-m','unittest','discover','-s','integration/tools/acceptance/tests','-p','test_raydium_funding_reference.py']),
        ('evidence-controls-optimized',py+['-OO','-m','unittest','discover','-s','integration/tools/acceptance/tests','-p','test_raydium_funding_reference.py']),
        ('existing-funding',cargo+['test']+manifest+['-p','xone-quote-api','--test','pump_amm_funding']),
        ('account-capture-regression',cargo+['test']+manifest+['-p','xone-quote-api','--test','execution_preparation','--test','rpc_capture']),
        ('receipt-regression',cargo+['test']+manifest+['-p','xone-quote-api','--test','transaction_lifecycle']),
        ('api-lint',cargo+['clippy']+manifest+['-p','xone-quote-api','--lib','--test','raydium_funding','--test','raydium_preparation','--example','frontend_harness','--','-D','warnings']),
        ('core-lint',cargo+['clippy']+manifest+['-p','xone-execution-preparation','-p','xone-protocol-math','--lib','--test','capabilities','--','-D','warnings']),
        ('build',cargo+['build']+manifest+['-p','xone-quote-api','--example','frontend_harness']),
        ('frontend-check',['npm','--prefix','frontend','run','check']),
        ('frontend-unit',['npm','--prefix','frontend','test']),
        ('browser',['node','frontend/test/browser.mjs']),
        ('progress-docs',py+['-m','unittest','discover','-s','integration/tools/ci/tests','-p','test_progress_document.py']),
        ('progress-docs-optimized',py+['-OO','-m','unittest','discover','-s','integration/tools/ci/tests','-p','test_progress_document.py']),
    ]
    source=fingerprint();inputs=runtime_inputs();report=dict(passed=False,state='RUNNING',source_tree_sha256=source,run_directory=str(out.relative_to(ROOT)),runtime_and_bank_inputs=inputs,checks=[],started_ms=int(time.time()*1000),publicTransactionsSubmitted=False,production_ready=False,all_requested_features_complete=False)
    target=BASE/'report.json'
    def save():
        text=json.dumps(report,indent=2)+'\n';(out/'report.json').write_text(text);temp=target.with_suffix('.tmp');temp.write_text(text);temp.replace(target)
    save()
    for name,command in checks:
        print('Running '+name,flush=True)
        with(out/f'{name}.log').open('wb')as log:
            child=subprocess.Popen(command,cwd=ROOT,env=env,stdout=log,stderr=subprocess.STDOUT,start_new_session=True)
            try:returncode=child.wait()
            finally:
                if child.poll()is None:
                    os.killpg(child.pid,signal.SIGTERM)
                    try:child.wait(timeout=2)
                    except subprocess.TimeoutExpired:os.killpg(child.pid,signal.SIGKILL);child.wait()
        raw=(out/f'{name}.log').read_bytes()
        if bytes([0])in raw:returncode=1
        count=sum(int(n)for n in re.findall(r'test result: ok\. (\d+) passed;',raw.decode(errors='replace')))
        report['checks'].append(dict(name=name,exit_code=returncode,rust_test_executions=count));save()
        if returncode:report.update(state='FAILED',error=name);save();raise SystemExit(name+' failed: '+str(out/f'{name}.log'))
    if fingerprint()!=source or runtime_inputs()!=inputs:report.update(state='SOURCE_OR_INPUT_CHANGED');save();raise SystemExit(report['state'])
    browser=json.loads((out/'browser/result.json').read_bytes());original=json.loads((out/'original.json').read_bytes())
    if not browser['passed']or not original['passed']or sum(x.startswith('raydium_funding_')for x in browser['checks'])!=40:raise ValueError('Missing real browser/native qualification')
    prep=list((out/'api').glob('raydium-*-preparation.json'));receipts=list((out/'api').glob('raydium-*-receipt.json'))
    if len(prep)!=10 or len(receipts)!=10 or len(original['rows'])!=10:raise ValueError('Missing actual service captures/receipts')
    for p in receipts:
        value=json.loads(p.read_bytes());r=value['receipt'];f=value['preparation']
        if r['state']!='FINALIZED'or r['settlement']['payerLamportDelta']!=f['expectedPayerLamportDelta']or r['settlement']['actualPersistentRentLamports']!=str(int(f['rentLamports'])-int(f['rentRefundLamports'])):raise ValueError('Actual receipt rent/funding mismatch')
    artifacts=[p for p in out.rglob('*')if p.is_file()and p.name!='report.json']
    artifacts += [ROOT/'integration/test/contracts/raydium-funding-bank.json',ROOT/'dex-router/test/contracts/raydium-funding-bank.json',ROOT/'dex-router/target/debug/examples/frontend_harness']
    report.update(passed=True,state='LOCAL_SCOPE_VERIFIED',verification_scope='CLASSIC_RAYDIUM_FUNDING_CAPTURE_SBF_WALLET_LOCAL',finished_ms=int(time.time()*1000),rust_test_executions=sum(c['rust_test_executions']for c in report['checks']),browser_checks=len(browser['checks']),raydium_funding_browser_checks=40,actual_api_preparations=10,actual_api_receipts=10,original_transactions=20,artifacts=[dict(path=str(p.relative_to(ROOT)),sha256=sha(p))for p in sorted(artifacts)])
    save();print(json.dumps({k:report[k]for k in ['passed','state','rust_test_executions','browser_checks','original_transactions']}))
if __name__=='__main__':main()
