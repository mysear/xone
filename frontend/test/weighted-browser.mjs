import { createServer } from 'node:http';
import { readFile, writeFile, mkdir, rm } from 'node:fs/promises';
import { createWriteStream } from 'node:fs';
import { spawn } from 'node:child_process';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
import path from 'node:path';
import assert from 'node:assert/strict';
const require=createRequire(import.meta.url),{chromium}=require(process.env.PLAYWRIGHT_MODULE||'playwright');
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');
const output=process.env.XONE_WEIGHTED_BROWSER_OUTPUT||path.join(root,'integration/artifacts/local/weighted-browser');
await mkdir(output,{recursive:true});const infoFile=path.join(output,'info.json');await rm(infoFile,{force:true});
const allowed={'/':'index.html','/src/app.js':'src/app.js','/src/model.js':'src/model.js','/src/wallet.js':'src/wallet.js','/src/styles.css':'src/styles.css'};
const server=createServer(async(req,res)=>{try{const file=allowed[new URL(req.url,'http://local').pathname];if(!file){res.writeHead(404);return res.end();}res.setHeader('Content-Type',file.endsWith('.js')?'application/javascript':file.endsWith('.css')?'text/css':'text/html');res.end(await readFile(path.join(root,'frontend',file)));}catch{res.writeHead(500);res.end();}});
await new Promise(r=>server.listen(0,'127.0.0.1',r));const origin=`http://127.0.0.1:${server.address().port}`;
const child=spawn(path.join(root,'dex-router/target/debug/examples/weighted_frontend_harness'),[infoFile,origin],{cwd:root,stdio:['ignore','pipe','pipe']});const log=createWriteStream(path.join(output,'harness.log'));child.stdout.pipe(log);child.stderr.pipe(log);
const report={passed:false,checks:[],authority:'Original callback corpus, real Router API, Chromium UI',publicTransactionsSubmitted:false};let browser,page;
try{
 let info;for(let i=0;i<600;i++){try{info=JSON.parse(await readFile(infoFile,'utf8'));break;}catch{}if(child.exitCode!==null)throw Error(`harness exited ${child.exitCode}`);await new Promise(r=>setTimeout(r,100));}assert(info,'harness startup timeout');
 browser=await chromium.launch({headless:true,channel:process.env.XONE_CHROME_CHANNEL||'chrome'});page=await browser.newPage({viewport:{width:1360,height:1000},locale:'zh-CN'});
 for(const service of info.services){
  await page.goto(`${origin}/?api=${encodeURIComponent(service.api)}`);await page.uncheck('#autoQuote');await page.fill('#apiToken',info.token);await page.locator('#apiToken').dispatchEvent('change');await page.waitForFunction(()=>Array.from(document.querySelector('#chainKey').options).some(o=>o.value==='evm:31337'));
  if(await page.locator('#autoRefresh').isChecked())await page.uncheck('#autoRefresh');await page.selectOption('#chainKey','evm:31337');await page.fill('#maxHops','1');
  for(const c of service.cases){const p=c.pool,t=p.multi_asset?p.multi_asset.assets.map(a=>a.token):[p.token0,p.token1];
   await page.selectOption('#sellToken',t[c.i]);await page.selectOption('#buyToken',t[c.j]);await page.selectOption('#quoteKind','EXACT_OUT');await page.fill('#caller','0x000000000000000000000000000000000000dead');await page.fill('#recipient','0x000000000000000000000000000000000000beef');await page.fill('#maxSell','1000000000000000000000000000000000');await page.fill('#amount',c.target);
   const response=page.waitForResponse(r=>r.url().endsWith('/v1/quote-exact-out')&&r.request().method()==='POST');await page.click('#calculateButton');const raw=await response;const q=await raw.json();assert.equal(raw.status(),200,JSON.stringify(q));assert.equal(q.requiredSellAmount,c.expected.amount_in);assert.equal(q.grossBuyAmount,c.target);await page.waitForFunction(({buy,sell})=>document.querySelector('[data-testid="buy-amount"]')?.textContent===buy&&document.querySelector('[data-testid="sell-amount"]')?.textContent===sell,{buy:c.target,sell:c.expected.amount_in});report.checks.push(`${p.protocol}_${c.i}_${c.j}_original_amounts`);
   for(const id of ['prepareButton','downloadButton','walletSign','walletSubmit'])assert(await page.locator('#'+id).isDisabled(),id);report.checks.push(`${p.protocol}_${c.i}_${c.j}_execution_gates`);
  }
  await page.screenshot({path:path.join(output,`${service.cases[0].pool.protocol}.png`),fullPage:true});
 }
 report.passed=true;
}catch(e){report.error=String(e.stack||e);if(page){report.uiError=await page.locator('#error').innerText().catch(()=>null);await page.screenshot({path:path.join(output,'failure.png'),fullPage:true}).catch(()=>{});}throw e;}finally{await writeFile(path.join(output,'report.json'),JSON.stringify(report,null,2));await browser?.close();child.kill('SIGTERM');server.close();log.end();}
console.log(JSON.stringify(report));
