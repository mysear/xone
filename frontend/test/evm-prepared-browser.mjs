import {createServer} from 'node:http';
import {readFile,writeFile,mkdir,rm,copyFile,chmod,mkdtemp} from 'node:fs/promises';
import {createWriteStream} from 'node:fs';
import {spawn} from 'node:child_process';
import {createRequire} from 'node:module';
import {fileURLToPath} from 'node:url';
import {tmpdir} from 'node:os';
import {createHash} from 'node:crypto';
import path from 'node:path';
import assert from 'node:assert/strict';
const require=createRequire(import.meta.url),{chromium}=require(process.env.PLAYWRIGHT_MODULE||'playwright');
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');
const output=process.env.XONE_EVM_PREPARED_BROWSER_OUTPUT||path.join(root,'integration/artifacts/local/evm-prepared-browser');
await mkdir(output,{recursive:true});const infoFile=path.join(output,'info.json');await rm(infoFile,{force:true});
const allowed={'/':'index.html','/src/app.js':'src/app.js','/src/model.js':'src/model.js','/src/wallet.js':'src/wallet.js','/src/styles.css':'src/styles.css'};
const server=createServer(async(req,res)=>{try{const file=allowed[new URL(req.url,'http://local').pathname];if(!file){res.writeHead(404);return res.end();}res.setHeader('Content-Type',file.endsWith('.js')?'application/javascript':file.endsWith('.css')?'text/css':'text/html');res.end(await readFile(path.join(root,'frontend',file)));}catch{res.writeHead(500);res.end();}});
await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));const origin=`http://127.0.0.1:${server.address().port}`;
const target=process.env.CARGO_TARGET_DIR||path.join(root,'integration/.runtime/design-review-target');
const temp=await mkdtemp(path.join(tmpdir(),'xone-prepared-browser-'));
const source=path.join(target,'debug/examples/evm_dynamic_fee_harness'),binary=path.join(temp,'harness');
await copyFile(source,binary);await chmod(binary,0o700);
const sha=bytes=>createHash('sha256').update(bytes).digest('hex');
assert.equal(sha(await readFile(source)),sha(await readFile(binary)));
const child=spawn(binary,[infoFile,origin],{cwd:root,env:{...process.env,XONE_EVM_PREPARED_HTTP:'bank'},stdio:['ignore','pipe','pipe']});
const log=createWriteStream(path.join(output,'harness.log'));child.stdout.pipe(log);child.stderr.pipe(log);
const report={passed:false,checks:[],scope:'RETAINED_ORIGINAL_BANK_RPC_PREPARED_ACTUAL_BROWSER_WORKER',publicTransactionsSubmitted:false,production_ready:false};let browser,page;
report.harnessSha256=sha(await readFile(binary));
try{
 let info;for(let i=0;i<300;i++){try{info=JSON.parse(await readFile(infoFile,'utf8'));break;}catch{}if(child.exitCode!==null)throw Error(`harness exited ${child.exitCode}`);await new Promise(resolve=>setTimeout(resolve,100));}assert(info,'harness startup timeout');
 browser=await chromium.launch({headless:true,channel:process.env.XONE_CHROME_CHANNEL||'chrome'});report.browserVersion=browser.version();page=await browser.newPage({viewport:{width:1360,height:1100},locale:'zh-CN'});
 await page.goto(`${origin}/?api=${encodeURIComponent(info.api)}`);await page.uncheck('#autoQuote');await page.uncheck('#autoRefresh');await page.fill('#apiToken',info.token);await page.locator('#apiToken').dispatchEvent('change');
 await page.waitForFunction(()=>Array.from(document.querySelector('#chainKey').options).some(o=>o.value==='evm:31337'));
 await page.selectOption('#chainKey','evm:31337');await page.selectOption('#sellToken',info.request.sellToken);await page.selectOption('#buyToken',info.request.buyToken);await page.fill('#caller',info.request.caller);await page.fill('#recipient',info.request.recipient);await page.fill('#maxHops','1');
 await page.fill('#evmGasLimit','500000');await page.fill('#evmMaxFee','2000000000');await page.fill('#evmPriorityFee','1');await page.fill('#evmFeeBudget','1000000000000000');
 let exactIn;let currentQuote;let currentPreparation;
 for(const kind of ['EXACT_IN','EXACT_OUT']){
  await page.selectOption('#quoteKind',kind);if(kind==='EXACT_IN')await page.fill('#amount',info.request.sellAmount);else{await page.fill('#amount',exactIn.grossBuyAmount);await page.fill('#maxSell',info.request.sellAmount);}
  const endpoint=kind==='EXACT_IN'?'/v1/quote':'/v1/quote-exact-out';const quoted=page.waitForResponse(r=>r.url()===info.api+endpoint&&r.request().method()==='POST');await page.click('#calculateButton');const qr=await quoted;const q=await qr.json();assert.equal(qr.status(),200,JSON.stringify(q));if(kind==='EXACT_IN')exactIn=q;currentQuote=q;
  await page.waitForFunction(()=>!document.querySelector('#prepareButton').disabled);
  const prepared=page.waitForResponse(r=>r.url().endsWith('/prepare-evm')&&r.request().method()==='POST');await page.click('#prepareButton');const pr=await prepared;const p=await pr.json();assert.equal(pr.status(),200,JSON.stringify(p));assert.equal(p.ready,true);currentPreparation=p;
  await page.waitForFunction(()=>!document.querySelector('#independentButton').disabled);report.checks.push(`${kind}_runtime_bank_enables_prepared_independent_action`);
  const simulated=page.waitForResponse(r=>r.url().endsWith('/simulate-evm')&&r.request().method()==='POST');await page.click('#independentButton');const sr=await simulated;const evidence=await sr.json();assert.equal(sr.status(),200,JSON.stringify(evidence));
  assert.equal(evidence.apiPreparationHash,p.preparationHash);assert.equal(evidence.planHash,q.planHash);assert.equal(evidence.quoteViewId,q.quoteViewId);
  assert.equal(evidence.preparedExecution.unsigned_transaction_hash,sha(Buffer.from(p.unsignedTransaction.slice(2),'hex')));
  assert.equal(evidence.outcome.actual_buy_amount,q.grossBuyAmount);assert.equal(evidence.outcome.actual_input_consumed,p.requiredInput);assert.equal(evidence.verified,false);
  await page.waitForFunction(()=>document.querySelector('#preparation').textContent.includes('本地EVM独立执行通过'));report.checks.push(`${kind}_actual_wire_hash_balances_fees_and_browser_digest_agree`);
  assert(await page.locator('#walletSign').isDisabled());assert(await page.locator('#walletSubmit').isDisabled());assert(await page.locator('.wallet-panel').isHidden());report.checks.push(`${kind}_wallet_and_production_boundary_stays_closed`);
  await writeFile(path.join(output,`${kind}-evidence.json`),JSON.stringify(evidence,null,2)+'\n');await page.screenshot({path:path.join(output,`${kind}-desktop.png`),fullPage:true});
 }
 // A successful response with an unrelated wire must never be accepted by the UI.
 const bad=async route=>{const response=await route.fetch();const value=await response.json();value.preparedExecution.unsigned_transaction_hash='00'.repeat(32);await route.fulfill({response,json:value});};
 await page.route('**/simulate-evm',bad);await page.click('#independentButton');await page.waitForFunction(()=>document.querySelector('#preparation').textContent.includes('独立模拟证据与当前EVM交易不一致'));
 report.checks.push('changed_unsigned_wire_digest_is_rejected_by_browser');await page.unroute('**/simulate-evm',bad);
 // Re-prepare after editing fees; the old UI result and transaction cannot survive the change.
 await page.fill('#evmPriorityFee','2');assert(await page.locator('#downloadButton').isDisabled());assert(await page.locator('#independentButton').isDisabled());report.checks.push('changed_fee_context_clears_old_wire_and_independent_evidence');
 const prepAgain=page.waitForResponse(r=>r.url().endsWith('/prepare-evm'));await page.click('#prepareButton');const next=await (await prepAgain).json();assert.notEqual(next.preparationHash,currentPreparation.preparationHash);
 await page.waitForFunction(()=>!document.querySelector('#independentButton').disabled);
 const last=page.waitForResponse(r=>r.url().endsWith('/simulate-evm'));await page.click('#independentButton');assert.equal((await last).status(),200);await page.waitForFunction(()=>document.querySelector('#preparation').textContent.includes('本地EVM独立执行通过'));
 await page.setViewportSize({width:390,height:844});await page.screenshot({path:path.join(output,'mobile-complete.png'),fullPage:true});
 assert(await page.locator('#evmFeeOptions').isVisible());report.checks.push('mobile_layout_keeps_fee_and_result_details_visible');
 report.artifacts={};
 for(const file of ['EXACT_IN-evidence.json','EXACT_OUT-evidence.json','EXACT_IN-desktop.png','EXACT_OUT-desktop.png','mobile-complete.png'])report.artifacts[file]=sha(await readFile(path.join(output,file)));
 report.passed=true;
}catch(error){report.error=String(error.stack||error);if(page)await page.screenshot({path:path.join(output,'failure.png'),fullPage:true}).catch(()=>{});throw error;}
finally{await writeFile(path.join(output,'report.json'),JSON.stringify(report,null,2)+'\n');await browser?.close();child.kill('SIGTERM');server.close();log.end();await rm(temp,{recursive:true,force:true});}
console.log(JSON.stringify({passed:true,checks:report.checks.length,publicTransactionsSubmitted:false}));
