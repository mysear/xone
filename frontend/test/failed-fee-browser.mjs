/** Real local Router HTTP + Chromium integration. No quote responses are mocked. */
import {createRequire} from 'node:module';
import {createServer} from 'node:http';
import {spawn} from 'node:child_process';
import {readFile,writeFile,mkdir} from 'node:fs/promises';
import {createWriteStream} from 'node:fs';
import {fileURLToPath} from 'node:url';
import path from 'node:path';
import assert from 'node:assert/strict';
const require=createRequire(import.meta.url);
const {chromium}=require(process.env.PLAYWRIGHT_MODULE || 'playwright');
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');
const output=process.env.XONE_BROWSER_OUTPUT || path.join(root,'integration/artifacts/local/failed-fee-browser');
await mkdir(output,{recursive:true});
const mime={'.html':'text/html; charset=utf-8','.js':'application/javascript; charset=utf-8','.css':'text/css; charset=utf-8'};
const allowed={'/':'index.html','/src/app.js':'src/app.js','/src/model.js':'src/model.js','/src/wallet.js':'src/wallet.js','/src/styles.css':'src/styles.css'};
const server=createServer(async(req,res)=>{try{const file=allowed[new URL(req.url,'http://local').pathname];if(!file){res.writeHead(204);return res.end();}res.writeHead(200,{'Content-Type':mime[path.extname(file)]});res.end(await readFile(path.join(root,'frontend',file)));}catch{res.writeHead(500);res.end();}});
await new Promise(r=>server.listen(0,'127.0.0.1',r));
const origin=`http://127.0.0.1:${server.address().port}`;
const infoFile=path.join(output,'harness-info.json');
await import('node:fs/promises').then(fs=>fs.rm(infoFile,{force:true}));
const log=createWriteStream(path.join(output,'harness.log'));
const child=spawn(process.env.XONE_HARNESS_BIN || path.join(root,'dex-router/target/debug/examples/frontend_harness'),[infoFile,origin],{cwd:root,env:{...process.env,XONE_SOLANA_PYTHON_PATH:process.env.XONE_SOLANA_PYTHON_PATH||path.join(root,'integration/artifacts/local/r01-solana-rebuild/python')},stdio:['ignore','pipe','pipe']});
child.stdout.pipe(log);child.stderr.pipe(log);
let browser,page;const checks=[];const report={passed:false,authority:'controlled original account banks, production Router handlers/math, actual local signed SBF execution and Chromium UI',checks,publicTransactionsSubmitted:false};
try {
  let info;
  const harnessStarted=Date.now(),startupMs=Number(process.env.XONE_HARNESS_STARTUP_MS||180000);
  assert(Number.isSafeInteger(startupMs)&&startupMs>=1000&&startupMs<=300000);
  for(let i=0;i<Math.ceil(startupMs/100);i++){try{info=JSON.parse(await readFile(infoFile,'utf8'));break;}catch{}if(child.exitCode!==null)throw new Error(`harness exit ${child.exitCode}`);await new Promise(r=>setTimeout(r,100));}
  assert(info,'harness startup timeout');report.harnessStartupMs=Date.now()-harnessStarted;
  report.metadata=await (await fetch(`${info.api}/v1/quote-options`,{headers:{Authorization:`Bearer ${info.token}`}})).json();
  browser=await chromium.launch({headless:true,channel:process.env.XONE_CHROME_CHANNEL || 'chrome'});
  const context=await browser.newContext({viewport:{width:1360,height:1080},locale:'zh-CN',acceptDownloads:true});
  await context.addInitScript(({api})=>{
    const address=new URLSearchParams(location.search).get('wallet');if(!address)return;
    const alphabet='123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz';let n=0n;for(const c of address)n=n*58n+BigInt(alphabet.indexOf(c));const publicKey=new Uint8Array(32);for(let i=31;i>=0;i--){publicKey[i]=Number(n&255n);n>>=8n;}
    const account={address,publicKey,chains:['solana:mainnet'],features:['solana:signTransaction']};
    const wallet={version:'1.0.0',name:'Xone controlled failure wallet',icon:'data:image/svg+xml,<svg xmlns="http://www.w3.org/2000/svg"/>',chains:['solana:mainnet'],accounts:[account],features:{'standard:connect':{version:'1.0.0',connect:async()=>({accounts:[account]})},'solana:signTransaction':{version:'1.0.0',supportedTransactionVersions:['legacy'],signTransaction:async input=>{
      const response=await fetch(`${api}/test/sign`,{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({transaction:btoa(String.fromCharCode(...input.transaction))})});const signed=await response.json();return[{signedTransaction:Uint8Array.from(atob(signed.signedTransaction),c=>c.charCodeAt(0))}];
    }}}};window.addEventListener('wallet-standard:app-ready',e=>e.detail.register(wallet));window.dispatchEvent(new CustomEvent('wallet-standard:register-wallet',{detail:api=>api.register(wallet)}));
  },{api:info.api});
  page=await context.newPage();const errors=[];page.on('pageerror',e=>errors.push(e.message));
  const cases=[
    {name:'native-success-control',api:info.curveApi,bank:info.curveBank,success:true},
    {name:'ordinary-token-failed',api:info.api,bank:info.bank},
    {name:'native-ata-failed',api:info.curveAtaApi,bank:info.curveAtaBank,create:true},
    {name:'wrapped-refund-failed',...info.raydiumFunding.find(v=>v.index===1)}
  ];
  const mode=async value=>assert.equal((await fetch(`${info.api}/test/mode`,{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({mode:value,walletBanks:true})})).status,200);
  for (const entry of cases) {
    await mode('ready');
    const b=entry.bank,options=b.preparationOptions||{},terminalText=entry.success?'已最终确认':'失败交易网络费 5000 lamports（已最终确认）';
    await page.goto(`${origin}/?api=${encodeURIComponent(entry.api)}&wallet=${b.caller}`);
    await page.uncheck('#autoQuote');await page.uncheck('#autoRefresh');await page.fill('#apiToken',info.token);await page.locator('#apiToken').dispatchEvent('change');
    await page.waitForFunction(()=>document.querySelector('#chainKey').options.length>0);
    await page.selectOption('#chainKey',b.pool.chain_key);await page.selectOption('#sellToken',b.pool[b.zeroForOne===false?'token1':'token0']);await page.selectOption('#buyToken',b.pool[b.zeroForOne===false?'token0':'token1']);await page.selectOption('#quoteKind','EXACT_OUT');await page.fill('#amount',b.target);await page.fill('#maxSell','1000000000000000');await page.fill('#caller',b.caller);await page.fill('#recipient',b.recipient);await page.fill('#maxHops','1');
    const response=page.waitForResponse(r=>new URL(r.url()).pathname==='/v1/quote-exact-out'&&r.request().method()==='POST');await page.click('#calculateButton');const quoted=await response;assert.equal(quoted.status(),200,await quoted.text());await page.waitForFunction(()=>document.querySelector('#stateBadge').textContent==='报价成功');
    for(const field of ['sourceAccount','destinationAccount'])await page.fill(`#${field}`,'');
    await page.fill('#sourceAccount',options.sourceTokenAccount||(!options.wrapSourceSol?b.sourceTokenAccount:''));await page.fill('#destinationAccount',options.destinationTokenAccount||(!options.unwrapDestinationSol?b.destinationTokenAccount:''));
    for(const field of ['createDestinationAta','wrapSourceSol','refundWrappedSource','unwrapDestinationSol']) {
      if(await page.locator(`#${field}`).isChecked())await page.uncheck(`#${field}`);
    }
    for(const field of ['wrapSourceSol','refundWrappedSource','unwrapDestinationSol','createDestinationAta'])if(options[field]||(field==='createDestinationAta'&&entry.create))await page.check(`#${field}`);
    await page.uncheck('#simulatePreparation');const preparedResponse=page.waitForResponse(r=>new URL(r.url()).pathname.endsWith('/prepare'));await page.click('#prepareButton');const prepared=await preparedResponse;assert.equal(prepared.status(),200,await prepared.text());const p=await prepared.json();assert.equal(p.ready,true,JSON.stringify(p));
    const independentResponse=page.waitForResponse(r=>new URL(r.url()).pathname.endsWith('/simulate-independent'));await page.click('#independentButton');const independent=await independentResponse;assert.equal(independent.status(),200,await independent.text());const evidence=await independent.json();await page.waitForFunction(()=>document.querySelector('#preparation').textContent.includes('独立本地SBF模拟通过'),null,{timeout:30000});
    assert(await page.locator('#preparation').isVisible());assert.equal(evidence.success,true);checks.push(`${entry.name}_actual_sbf`);
    await page.click('#walletConnect');await page.waitForFunction(()=>document.querySelector('#walletStatus').textContent.includes('已连接'));await page.click('#walletSign');await page.waitForFunction(()=>document.querySelector('#walletStatus').textContent.includes('尚未广播'),null,{timeout:30000});await page.check('#confirmSubmission');await mode(entry.success?'ready':'send_fail');const submission=page.waitForResponse(r=>new URL(r.url()).pathname.endsWith('/submit'));await page.click('#walletSubmit');const submitted=await submission;assert.equal(submitted.status(),200,await submitted.text());const sent=await submitted.json();await page.waitForFunction(expected=>document.querySelector('#transactionReceipt').textContent.includes(expected),terminalText,{timeout:30000});
    assert(await page.locator('#transactionReceipt').isVisible());const text=await page.locator('#transactionReceipt').innerText();
    if(entry.success){assert(text.includes('费用后收款 5971 lamports'),text);assert(!text.includes('失败交易网络费'));}
    else{assert(text.includes('链上执行失败'),text);assert(text.includes('付款人SOL净变动 -5000 lamports'));assert(text.includes('留存租金押金 0 lamports'));assert(text.includes('失败收付不计为成交净额'));assert(!text.includes('实际输入 '));assert(!text.includes('费用后收款 '));}checks.push(`${entry.name}_receipt_fee_visible`);
    const receipt=await(await fetch(`${entry.api}/v1/transactions/${sent.signature}`,{headers:{Authorization:`Bearer ${info.token}`}})).json();await writeFile(path.join(output,entry.name+'.json'),JSON.stringify({preparation:p,evidence,receipt},null,2));
    await page.screenshot({path:path.join(output,entry.name+'-execution.png'),fullPage:true});
    const stats=await(await fetch(`${info.api}/test/wallet-stats`,{method:'POST'})).json();await page.reload();await page.waitForFunction(expected=>document.querySelector('#transactionReceipt').textContent.includes(expected),terminalText,{timeout:30000});assert(await page.locator('#transactionReceipt').isVisible());assert(await page.locator('#receiptRefresh').isVisible());assert((await page.locator('#transactionReceipt').innerText()).includes(terminalText));assert.deepEqual(await(await fetch(`${info.api}/test/wallet-stats`,{method:'POST'})).json(),stats);checks.push(`${entry.name}_reload_without_resend`);
    await page.screenshot({path:path.join(output,entry.name+'.png'),fullPage:true});
  }
  await page.setViewportSize({width:390,height:844});assert(await page.locator('#transactionReceipt').isVisible());await page.screenshot({path:path.join(output,'mobile.png'),fullPage:true});assert((await page.locator('#transactionReceipt').innerText()).includes('失败交易网络费 5000'));assert(await page.evaluate(()=>document.documentElement.scrollWidth<=window.innerWidth));checks.push('failure_fee_mobile_visible_without_overflow');assert.deepEqual(errors,[]);report.passed=true;
}catch(error){report.error=error.stack;if(page){report.uiFailure=await page.evaluate(()=>({preparation:document.querySelector('#preparation')?.textContent,wallet:document.querySelector('#walletStatus')?.textContent,receipt:document.querySelector('#transactionReceipt')?.textContent,error:document.querySelector('#error')?.textContent})).catch(()=>null);await page.screenshot({path:path.join(output,'failure.png'),fullPage:true}).catch(()=>{});}throw error;
}finally{await writeFile(path.join(output,'result.json'),JSON.stringify(report,null,2));await browser?.close();child.kill('SIGTERM');server.close();log.end();}
console.log(JSON.stringify({passed:true,checks:checks.length}));
