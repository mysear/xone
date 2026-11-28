import {createServer} from 'node:http';
import {readFile,writeFile,mkdir,rm} from 'node:fs/promises';
import {createWriteStream} from 'node:fs';
import {spawn} from 'node:child_process';
import {createRequire} from 'node:module';
import {fileURLToPath} from 'node:url';
import path from 'node:path';
import assert from 'node:assert/strict';
const require=createRequire(import.meta.url),{chromium}=require(process.env.PLAYWRIGHT_MODULE||'playwright');
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');
const output=process.env.XONE_EVM_RPC_BROWSER_OUTPUT||path.join(root,'integration/artifacts/local/evm-rpc-browser');
await mkdir(output,{recursive:true});const infoFile=path.join(output,'info.json');await rm(infoFile,{force:true});
const allowed={'/':'index.html','/src/app.js':'src/app.js','/src/model.js':'src/model.js','/src/wallet.js':'src/wallet.js','/src/styles.css':'src/styles.css'};
const server=createServer(async(req,res)=>{try{const file=allowed[new URL(req.url,'http://local').pathname];if(!file){res.writeHead(404);return res.end();}res.setHeader('Content-Type',file.endsWith('.js')?'application/javascript':file.endsWith('.css')?'text/css':'text/html');res.end(await readFile(path.join(root,'frontend',file)));}catch{res.writeHead(500);res.end();}});
await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));const origin=`http://127.0.0.1:${server.address().port}`;
const target=process.env.CARGO_TARGET_DIR||path.join(root,'dex-router/target');
const child=spawn(path.join(target,'debug/examples/evm_preparation_frontend_harness'),[infoFile,origin],{cwd:root,stdio:['ignore','pipe','pipe']});
const log=createWriteStream(path.join(output,'harness.log'));child.stdout.pipe(log);child.stderr.pipe(log);
const report={passed:false,checks:[],scope:'RETAINED_ORIGINAL_STATE_LOOPBACK_RPC_BROWSER',publicTransactionsSubmitted:false,production_ready:false};let browser,page;
try{
 let info;for(let i=0;i<600;i++){try{info=JSON.parse(await readFile(infoFile,'utf8'));break;}catch{}if(child.exitCode!==null)throw Error(`harness exited ${child.exitCode}`);await new Promise(resolve=>setTimeout(resolve,100));}assert(info,'harness startup timeout');
 browser=await chromium.launch({headless:true,channel:process.env.XONE_CHROME_CHANNEL||'chrome'});page=await browser.newPage({viewport:{width:1360,height:1100},locale:'zh-CN'});
 await page.goto(`${origin}/?api=${encodeURIComponent(info.api)}`);await page.uncheck('#autoQuote');await page.uncheck('#autoRefresh');await page.fill('#apiToken',info.token);await page.locator('#apiToken').dispatchEvent('change');
 await page.waitForFunction(()=>Array.from(document.querySelector('#chainKey').options).some(o=>o.value==='evm:31337'));
 assert.equal(await page.locator('#amount').inputValue(),'');
 await page.selectOption('#chainKey','evm:31337');await page.selectOption('#sellToken',info.request.sellToken);await page.selectOption('#buyToken',info.request.buyToken);
 const maxHops=page.locator('#maxHops');
 assert(Number(await maxHops.getAttribute('max'))>=2);
 await maxHops.fill('2');
 assert(await page.locator('#evmFeeOptions').isVisible());assert(await page.locator('#sourceAccount').isHidden());assert(await page.locator('.wallet-panel').isHidden());report.checks.push('evm_fees_and_account_controls_are_chain_specific');
 let exactIn;
 for(const kind of ['EXACT_IN','EXACT_OUT']){
  await page.selectOption('#quoteKind',kind);
  if(kind==='EXACT_IN')await page.fill('#amount',info.request.sellAmount);
  else {await page.fill('#amount',exactIn.grossBuyAmount);await page.fill('#maxSell',info.request.sellAmount);}
  await page.fill('#caller','');await page.fill('#recipient','');
  const endpoint=kind==='EXACT_IN'?'/v1/quote':'/v1/quote-exact-out';
  const isQuote=r=>r.url()===info.api+endpoint&&r.request().method()==='POST';
  let estimate;
  for (const trigger of ['manual','automatic']) {
   const response=page.waitForResponse(isQuote);
   if(trigger==='manual')await page.click('#calculateButton');
   else await page.check('#autoQuote');
   const raw=await response;
   estimate=await raw.json();
   assert.equal(raw.status(),200,JSON.stringify(estimate));
   assert.equal(estimate.estimateOnly,true);
   assert.equal(estimate.quoteId,undefined);
   const body=raw.request().postDataJSON();
   assert.equal(body.caller,undefined);
   assert.equal(body.recipient,undefined);
   assert.equal(body.constraints.maxHops,2);
   await page.waitForFunction(amount=>document.querySelector('[data-testid="buy-amount"]')?.textContent===amount,estimate.grossBuyAmount);
   assert.equal(await page.locator('#error').textContent(),'');
   await page.uncheck('#autoQuote');
   report.checks.push(`${kind}_${trigger}_estimate_omits_accounts_and_uses_selected_hops`);
  }
  if(kind==='EXACT_IN')exactIn=estimate;
  await page.getByRole('heading',{name:'兑换量估算'}).waitFor();
  await page.click('#prepareButton');
  await page.locator('#preparation').filter({hasText:'请填写调用方和收款方地址'}).waitFor();
  assert.equal(await page.locator('#error').textContent(),'');
  await page.fill('#caller',info.request.caller);await page.fill('#recipient',info.request.recipient);
  assert((await page.locator('#result').innerText()).includes(estimate.grossBuyAmount));
  report.checks.push(`${kind}_estimate_survives_address_changes_and_has_no_quote_id`);
  await page.waitForFunction(()=>!document.querySelector('#prepareButton').disabled);
  const fundedResponse=page.waitForResponse(isQuote);
  const preparedResponse=page.waitForResponse(r=>r.url().endsWith('/prepare-evm')&&r.request().method()==='POST');
  await page.click('#prepareButton');
  const funded=await fundedResponse, q=await funded.json();
  assert.equal(funded.status(),200,JSON.stringify(q));
  assert.equal(funded.request().postDataJSON().caller,info.request.caller);
  assert.equal(funded.request().postDataJSON().recipient,info.request.recipient);
  const prepRaw=await preparedResponse;const prep=await prepRaw.json();assert.equal(prepRaw.status(),200,JSON.stringify(prep));assert.equal(prep.ready,true);assert.equal(prep.quoteKind,kind);assert.equal(prep.quoteId,q.quoteId);assert.equal(prep.quoteViewId,q.quoteViewId);assert.equal(prep.signed,false);assert.equal(prep.submitted,false);assert.equal(prep.chainFeesComplete,false);assert.equal(prep.transaction.type,'0x2');report.checks.push(`${kind}_rpc_preparation_matches_current_quote`);
  await page.waitForFunction(()=>document.querySelector('#preparation').textContent.includes('EVM账户检查通过'));
  assert(await page.locator('#independentButton').isDisabled());assert(await page.locator('#walletSign').isDisabled());assert(await page.locator('#walletSubmit').isDisabled());report.checks.push(`${kind}_independent_and_wallet_boundaries_stay_closed`);
  const downloaded=page.waitForEvent('download');await page.click('#downloadButton');const download=await downloaded;const destination=path.join(output,`${kind}-download.json`);await download.saveAs(destination);assert.deepEqual(JSON.parse(await readFile(destination,'utf8')),prep);report.checks.push(`${kind}_download_preserves_rpc_and_unsigned_commitments`);
  await writeFile(path.join(output,`${kind}-preparation.json`),JSON.stringify(prep,null,2)+'\n');
  await page.screenshot({path:path.join(output,`${kind}-complete.png`),fullPage:true});
  await page.fill('#recipient','');assert(await page.locator('#downloadButton').isDisabled());
  await page.getByRole('heading',{name:'兑换量估算'}).waitFor();
  await page.fill('#recipient',info.request.recipient);
  await page.fill('#evmPriorityFee','4000000000');assert(await page.locator('#downloadButton').isDisabled());await page.click('#prepareButton');await page.waitForFunction(()=>document.querySelector('#preparation').textContent.includes('优先费不能大于'));
  assert(await page.locator('#downloadButton').isDisabled());await page.fill('#evmPriorityFee','1000000000');report.checks.push(`${kind}_changed_fee_parameters_invalidate_old_wire_and_reject_bad_budget`);
 }
 report.passed=true;
}catch(error){report.error=String(error.stack||error);if(page)await page.screenshot({path:path.join(output,'failure.png'),fullPage:true}).catch(()=>{});throw error;}
finally{await writeFile(path.join(output,'report.json'),JSON.stringify(report,null,2)+'\n');await browser?.close();child.kill('SIGTERM');server.close();log.end();}
console.log(JSON.stringify({passed:true,checks:report.checks.length,publicTransactionsSubmitted:false}));
