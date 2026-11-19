// Usage: configure the mined bank and prices as for ordered_split_http.py, then
// pass the built harness and a fresh output directory. Requests use loopback only.
import {createServer} from 'node:http';
import {createRequire} from 'node:module';
import {mkdtemp, readFile, writeFile, mkdir} from 'node:fs/promises';
import {spawn} from 'node:child_process';
import {fileURLToPath} from 'node:url';
import {resolve, join} from 'node:path';
import {setTimeout as delay} from 'node:timers/promises';
import assert from 'node:assert/strict';
const {chromium} = createRequire(import.meta.url)(process.env.PLAYWRIGHT_MODULE || 'playwright');
const root = fileURLToPath(new URL('../../', import.meta.url));
const [harness, output] = process.argv.slice(2).map(v => resolve(v));
await mkdir(output, {recursive: true});
const run = await mkdtemp(join(output, 'run-'));
const files = new Set(['/src/app.js','/src/model.js','/src/wallet.js','/src/styles.css']);
const server = createServer(async (req,res) => {
  const route = new URL(req.url, 'http://localhost').pathname;
  if (route !== '/' && !files.has(route)) { res.writeHead(404).end(); return; }
  const name = route === '/' ? 'index.html' : route.slice(1);
  try {
    const type = name.endsWith('js') ? 'application/javascript' : name.endsWith('css') ? 'text/css' : 'text/html';
    res.writeHead(200, {'Content-Type': `${type}; charset=utf-8`}).end(await readFile(join(root,'frontend',name)));
  } catch { res.end(); }
});
await new Promise(resolve => server.listen(0,'127.0.0.1',resolve));
const origin = `http://127.0.0.1:${server.address().port}`;
const infoFile = join(run,'info.json');
const child = spawn(harness,[infoFile,origin],{cwd:root,stdio:'inherit'});
let browser;
try {
  let info;
  for (let i=0; i<300 && !info; i++) {
    assert.equal(child.exitCode,null,'harness exited before becoming ready');
    try { info = JSON.parse(await readFile(infoFile,'utf8')); } catch(e) { if(e.code!=='ENOENT') throw e; await delay(100); }
  }
  assert(info,'harness startup timeout');
  browser = await chromium.launch({headless:true,channel:process.env.XONE_CHROME_CHANNEL || 'chrome'});
  const page = await browser.newPage();
  await page.goto(`${origin}/?api=${encodeURIComponent(info.api)}`);
  await page.uncheck('#autoQuote'); await page.uncheck('#autoRefresh');
  await page.fill('#apiToken',info.token); await page.locator('#apiToken').dispatchEvent('change');
  const order=info.request;
  await page.selectOption('#chainKey',order.chainKey);
  for(const field of ['sellToken','buyToken']) await page.selectOption(`#${field}`,order[field]);
  for(const field of ['caller','recipient']) await page.fill(`#${field}`,order[field]);
  const checks=[];
  for(const request of [order,info.exactOutRequest]) {
    const out=request.kind==='EXACT_OUT';
    await page.selectOption('#quoteKind',request.kind);
    await page.fill('#amount',out?request.buyAmount:request.sellAmount);
    if(out) await page.fill('#maxSell',request.maxSellAmount);
    const response=page.waitForResponse(r=>new URL(r.url()).pathname===(out?'/v1/quote-exact-out':'/v1/quote') && r.request().method()==='POST');
    await page.click('#calculateButton');
    const res=await response; assert.equal(res.status(),200);
    const quote=await res.json();
    const funding=quote.costs.nativeFunding;
    await page.waitForFunction(id=>document.querySelector('#result').textContent.includes(id),quote.quoteId);
    const text=await page.locator('#result').innerText();
    for(const amount of [quote.costs.payerNetNativeDebit,funding.wrappedPrincipalNative,funding.minimumOpeningBalanceNative,funding.totalNetDebitNative]) assert(text.includes(`${amount} wei`));
    assert(text.includes(quote.quality.costRanking.costAdjustmentApplied?'计入已折算的网络费':'按毛额比较'));
    checks.push({kind:request.kind,planHash:quote.planHash});
  }
  await writeFile(join(run,'report.json'), JSON.stringify({passed:true,checks})+'\n');
  console.log(run);
} finally { await browser?.close(); child.kill('SIGTERM'); server.close(); }
