import {positiveInteger, boundedInteger, routeView, snapshotView, escapeHtml as esc} from './model.js';
const $ = id => document.getElementById(id);
const params = new URLSearchParams(location.search);
const state = {metadata:{chains:[]}, optionsSeq:0, quoteSeq:0, prepareSeq:0, quote:null, preparation:null, timer:null, debounce:null, optionsAbort:null, quoteAbort:null, prepareAbort:null};
const apiBase = $('apiBase');
apiBase.value = params.get('api') || localStorage.getItem('xone-api-base') || 'http://127.0.0.1:8080';
localStorage.removeItem('xone-api-token');
function base() {
  const url = new URL(apiBase.value);
  if (!['http:', 'https:'].includes(url.protocol) || url.username || url.password || url.search || url.hash) throw new Error('Router 地址必须是 HTTP(S) 地址');
  return url.href.replace(/\/$/, '');
}
function tokenKey() { return `xone-api-token:${base()}`; }
try {$('apiToken').value = sessionStorage.getItem(tokenKey()) || '';} catch {}
function badge(id, text, kind='muted') { $(id).textContent=text; $(id).className=`badge ${kind}`; }
function error(message='') { $('error').textContent=message; }
function chain() {return state.metadata.chains.find(c => c.chainKey === $('chainKey').value);}
function fill(id, values, wanted=$(id).value) {
  $(id).replaceChildren(...values.map(v => new Option(typeof v==='string'?v:v.label, typeof v==='string'?v:v.value)));
  if ([...$(id).options].some(o=>o.value===wanted)) $(id).value=wanted;
}
function invalidate(message='输入或状态已变化，请重新计算') {
  ++state.quoteSeq; ++state.prepareSeq;
  state.quoteAbort?.abort(); state.prepareAbort?.abort(); clearTimeout(state.debounce);
  state.quote=null; state.preparation=null;
  $('downloadButton').disabled=true; $('prepareButton').disabled=true;
  $('preparation').textContent='';
  $('result').className='empty'; $('result').textContent=message; $('quoteTime').textContent='—';
}
async function request(path, options={}) {
  const token=$('apiToken').value.trim();
  const response=await fetch(`${base()}${path}`, {...options, redirect:'error', headers:{...(token?{Authorization:`Bearer ${token}`} : {}), ...(options.body?{'Content-Type':'application/json'}:{}), ...options.headers}});
  const raw=await response.text(); let body;
  try {body=JSON.parse(raw);} catch {throw new Error(`Router 返回非 JSON 响应（HTTP ${response.status}）`);}
  if (!response.ok) throw new Error(body.error || body.message || `HTTP ${response.status}`);
  return body;
}
function renderPools() {
  const c=chain(); const pools=c?.pools || [];
  $('poolCount').textContent=`${pools.length} 个`;
  $('chainVersion').textContent=c?`图版本：${c.graphVersion ?? '—'} · View：${c.viewId ?? '—'} · 协议：${(c.protocols||[]).join(', ')||'—'}`:'';
  $('poolTable').innerHTML=pools.length?`<table><thead><tr><th>协议</th><th>Adapter</th><th>协议视图 / 能力</th><th>Token0</th><th>Token1</th><th>报价模式</th><th>状态</th><th>Pool</th></tr></thead><tbody>${pools.map(p=>`<tr><td>${esc(p.protocol)}</td><td>${esc(p.adapterVersion)}</td><td>${esc(p.protocolViewId||'—')}<br>${esc(p.capabilities?.modelVersion||'—')}<br>${esc(p.execution?.preparation?'可准备待签名交易':p.capabilities?.executionBlocker||'执行未接入')}</td><td>${esc(p.token0)}</td><td>${esc(p.token1)}</td><td>${esc((p.quoteModes||[]).join(' / ')||'—')}</td><td>${esc(p.available?'可用':p.unavailableReason||'不可用')}</td><td>${esc(p.poolId)}</td></tr>`).join('')}</tbody></table>`:'<p class="muted">当前链没有已发布池。</p>';
}
function modes() {
  const c=chain(); const exactOut=(c?.pools||[]).some(p=>p.available && (p.quoteModes||[]).includes('EXACT_OUT'));
  const exactOption=$('quoteKind').querySelector('option[value="EXACT_OUT"]'); exactOption.disabled=!exactOut;
  if (!exactOut && $('quoteKind').value==='EXACT_OUT') $('quoteKind').value='EXACT_IN';
  const wrapAllowed=$('sellToken').value==='So11111111111111111111111111111111111111112' && (c?.pools||[]).some(p=>p.available&&p.execution?.wrapSourceSol);
  $('wrapSourceSol').disabled=!wrapAllowed;if(!wrapAllowed)$('wrapSourceSol').checked=false;
  const refundAllowed=wrapAllowed && $('wrapSourceSol').checked && !$('sourceAccount').value.trim();
  $('refundWrappedSource').disabled=!refundAllowed;if(!refundAllowed)$('refundWrappedSource').checked=false;
  const unwrapAllowed=$('buyToken').value==='So11111111111111111111111111111111111111112' && (c?.pools||[]).some(p=>p.available&&p.execution?.unwrapDestinationSol) && $('caller').value.trim()!=='' && $('caller').value.trim()===$('recipient').value.trim() && !$('destinationAccount').value.trim() && !$('createDestinationAta').checked;
  $('unwrapDestinationSol').disabled=!unwrapAllowed;if(!unwrapAllowed)$('unwrapDestinationSol').checked=false;

  const exact=$('quoteKind').value==='EXACT_OUT';
  $('amountText').textContent=exact?'目标买入数量（最小单位整数）':'卖出数量（最小单位整数）';
  $('maxSellLabel').classList.toggle('hidden',!exact);
}
function tokens() {
  const c=chain(); fill('sellToken',c?.tokens||[]); fill('buyToken',c?.tokens||[]);
  if ($('sellToken').value===$('buyToken').value && $('buyToken').options.length>1) $('buyToken').selectedIndex=1;
  modes(); renderPools(); badge('stateBadge',c?.ready?'可报价':'未就绪',c?.ready?'ok':'bad');
  $('calculateButton').disabled=!c?.ready;
  if (state.metadata.limits?.maxHops) {$('maxHops').max=state.metadata.limits.maxHops;}
}
function schedule() {
  clearTimeout(state.debounce);
  if (!$('autoQuote').checked || !$('caller').value.trim() || !$('recipient').value.trim() || !chain()?.ready || !/^[0-9]+$/.test($('amount').value) || /^0+$/.test($('amount').value)) return;
  state.debounce=setTimeout(()=>{$('quoteForm').requestSubmit();},700);
}
async function loadOptions(silent=false) {
  if (silent && state.optionsLoading) return;
  state.optionsLoading=true;
  const seq=++state.optionsSeq; state.optionsAbort?.abort(); state.optionsAbort=new AbortController();
  const controller=state.optionsAbort;
  const timeout=setTimeout(()=>controller.abort(new DOMException("Router 连接超时","TimeoutError")),10000);
  if (!silent) badge('healthBadge','连接中…');
  try {
    const old=chain(); const oldId=old?.viewId; const selected=$('chainKey').value;
    const address=base(), credential=$('apiToken').value;
    const data=await request('/v1/quote-options',{signal:controller.signal});
    if (seq!==state.optionsSeq || address!==base() || credential!==$('apiToken').value) return;
    if (!Array.isArray(data.chains)) throw new Error('元数据响应缺少 chains');
    state.metadata=data;
    fill('chainKey',data.chains.map(c=>({value:c.chainKey,label:`${c.chainKey}${c.ready?'':'（未就绪）'}`})),$('chainKey').value);
    const next=chain(); const changed=oldId!==next?.viewId || selected!==next?.chainKey || old?.ready!==next?.ready;
    const oldSell=$('sellToken').value, oldBuy=$('buyToken').value;
    tokens(); badge('healthBadge','已连接','ok');
    if (changed || oldSell!==$('sellToken').value || oldBuy!==$('buyToken').value) {invalidate('解析状态已更新，请重新计算'); schedule();}
    if (!next?.ready) invalidate('当前链未就绪，等待解析状态恢复');
    localStorage.setItem('xone-api-base',address); sessionStorage.setItem(`xone-api-token:${address}`,credential);
  } catch (e) {
    if (seq!==state.optionsSeq || e.name==='AbortError') return;
    badge('healthBadge','连接失败','bad'); badge('stateBadge','状态不可确认','bad');
    state.metadata={chains:[]}; tokens(); invalidate('连接失败，旧报价已撤销'); error(e.message);
  } finally {clearTimeout(timeout);if(seq===state.optionsSeq)state.optionsLoading=false;}
}
function makeRequest() {
  if (!chain()?.ready) throw new Error('当前链没有可报价状态');
  if ($('sellToken').value===$('buyToken').value) throw new Error('买卖币种不能相同');
  const exact=$('quoteKind').value==='EXACT_OUT';
  const body={chainKey:$('chainKey').value,kind:exact?'EXACT_OUT':'EXACT_IN',sellToken:$('sellToken').value,buyToken:$('buyToken').value,caller:$('caller').value.trim(),recipient:$('recipient').value.trim(),slippageBps:boundedInteger($('slippageBps').value,0,10000,'滑点'),verification:'FAST',constraints:{maxHops:boundedInteger($('maxHops').value,1,state.metadata.limits?.maxHops||8,'最大跳数')}};
  if (!body.caller || !body.recipient) throw new Error('请填写调用方和收款方地址');
  if (exact) {body.buyAmount=positiveInteger($('amount').value,'目标买入数量');body.maxSellAmount=positiveInteger($('maxSell').value,'最大卖出数量');}
  else body.sellAmount=positiveInteger($('amount').value,'卖出数量');
  return body;
}
function renderQuote(q) {
  const routes=q.routes||[]; const prep=(chain()?.pools||[]).some(p=>p.execution?.preparation && routes.some(r=>(r.path?.edges||r.edges||[]).some(e=>e.pool_id===p.poolId)));
  const details=[['Quote ID',q.quoteId],['快照',snapshotView(q)],['图版本',q.graphVersion],['协议视图',Object.values(q.protocolViewIds||{}).join(' / ')||'—'],['计划哈希',q.planHash],['最小到账',q.minBuyAmount],['额外到账',q.outputSurplus||'0'],['未用输入预算',q.unusedInputBudget||'0'],['退款',q.refund],['验证',q.verification?.level]];
  $('result').className='';
  $('result').innerHTML=`<div class="metric-grid"><div class="metric"><span>买入数量（最小单位）</span><strong data-testid="buy-amount">${esc(q.grossBuyAmount||q.buyAmount||'—')}</strong></div><div class="metric"><span>实际消耗输入（最小单位）</span><strong data-testid="sell-amount">${esc(q.requiredSellAmount||q.consumedIn||'—')}</strong></div></div><h3>路线（${routes.length}）</h3>${routes.map((r,i)=>{const v=routeView(r);return `<div class="route"><strong>#${i+1} ${esc(v.protocols.join(' / '))}</strong><br><small>${esc(v.tokens.join(' → '))}<br>Pool: ${esc(v.pools.join(' → '))}<br>input ${esc(v.input)} · output ${esc(v.output)}</small></div>`;}).join('')}<div class="details">${details.map(([k,v])=>`<div><b>${esc(k)}：</b>${esc(v??'—')}</div>`).join('')}</div>`;
  $('prepareButton').disabled=!prep;
  $('preparation').textContent=prep?'可进行只读账户检查并生成待签名交易。':'当前协议或 Router RPC 配置尚未提供执行准备能力。';
}
$('quoteForm').addEventListener('submit',async event=>{
  event.preventDefault(); invalidate('计算中…'); error(''); const seq=state.quoteSeq;
  try {
    const body=makeRequest(); state.quoteAbort=new AbortController(); const abort=state.quoteAbort;
    const timeout=setTimeout(()=>abort.abort(),35000); const started=performance.now();
    let q; try {q=await request(body.kind==='EXACT_OUT'?'/v1/quote-exact-out':'/v1/quote',{method:'POST',body:JSON.stringify(body),signal:abort.signal});} finally {clearTimeout(timeout);}
    if (seq!==state.quoteSeq) return;
    if (q.quoteViewId!==chain()?.viewId || q.chainKey!==$('chainKey').value) throw new Error('报价状态已更新，请重新计算');
    for(const [key,id] of Object.entries(q.protocolViewIds||{})){const matched=(chain()?.protocolViews||[]).find(p=>`${p.protocol}@${p.adapterVersion}`===key);if(!matched?.ready||matched.viewId!==id)throw new Error('协议报价状态已更新，请重新计算');}
    state.quote=q; renderQuote(q); $('quoteTime').textContent=`${Math.round(performance.now()-started)} ms`; badge('stateBadge','报价成功','ok');
  } catch(e) {if (seq===state.quoteSeq && e.name!=='AbortError') {invalidate('没有可用报价'); error(e.message);badge('stateBadge','报价失败','bad');}}
});
$('prepareButton').addEventListener('click',async()=>{
  const q=state.quote; if (!q) return; const seq=state.quoteSeq, prepSeq=++state.prepareSeq; state.preparation=null; $('downloadButton').disabled=true; $('prepareButton').disabled=true; $('preparation').textContent='正在核对账户…';
  try {
    const payload={createDestinationAta:$('createDestinationAta').checked,simulate:$('simulatePreparation').checked,wrapSourceSol:$('wrapSourceSol').checked,refundWrappedSource:$('refundWrappedSource').checked,unwrapDestinationSol:$('unwrapDestinationSol').checked}; if ($('sourceAccount').value.trim()) payload.sourceTokenAccount=$('sourceAccount').value.trim(); if ($('destinationAccount').value.trim()) payload.destinationTokenAccount=$('destinationAccount').value.trim();
    state.prepareAbort=new AbortController();
    const prepared=await request(`/v1/quotes/${encodeURIComponent(q.quoteId)}/prepare`,{method:'POST',body:JSON.stringify(payload),signal:state.prepareAbort.signal});
    if (seq!==state.quoteSeq || prepSeq!==state.prepareSeq || state.quote!==q) return;
    state.preparation=prepared; $('downloadButton').disabled=!prepared.ready;
    const wrapText=prepared.wrapLamports && prepared.wrapLamports!=='0'?` · SOL包装 ${prepared.wrapLamports} lamports${prepared.sourceWsolRetained?' · 预计剩余 '+prepared.expectedResidualWsol+' WSOL最小单位，余额仍保留为WSOL':' · 未用资金通过本次临时账户关闭转回SOL'}`:'';
    const tokenFeeText=prepared.destinationTokenProgram==='TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb'?` · Token2022转账费 ${prepared.expectedDestinationTransferFee} 最小单位 · 收款账户 ${prepared.destinationAccountSize} 字节`:'';
    const closeText=prepared.temporaryWsolAccount?` · 临时账户关闭转回 ${prepared.expectedNativeRefundLamports} lamports（含退租 ${prepared.rentRefundLamports}） · 预计payer SOL净变动 ${prepared.expectedPayerLamportDelta} lamports`:'';
    $('preparation').textContent=prepared.ready?`账户检查通过 · 观察 slot ${prepared.observedSlot} · 交易费 ${prepared.feeLamports} lamports · 创建租金 ${prepared.rentLamports||0} lamports${wrapText}${closeText}${tokenFeeText}${prepared.sourceAccountCreation?' · 已加入输入WSOL账户创建指令':''}${prepared.accountCreation?' · 已加入收款 ATA 创建指令':''}${prepared.simulation?.success?' · RPC模拟到账 '+prepared.simulation.actualOutput+(prepared.unwrapDestinationSol?' lamports SOL':''):''} · 待签名，生产独立验证仍未完成`:`账户准备未满足：${(prepared.blockers||[]).join(', ')}。目标账户：${prepared.destinationTokenAccount}`;
  } catch(e) {if(seq===state.quoteSeq && prepSeq===state.prepareSeq && e.name!=='AbortError') {if (/QUOTE_|PROGRAM_CHANGED/.test(e.message)) invalidate('执行状态已变化，请重新报价'); $('preparation').textContent=e.message;}}
  finally {if (seq===state.quoteSeq && prepSeq===state.prepareSeq) $('prepareButton').disabled=false;}
});
$('downloadButton').addEventListener('click',()=>{
  if (!state.preparation?.ready || !state.quote || Date.now()>=state.quote.validity.expiresAtMs) {invalidate('报价已过期，请重新计算'); return;}
  const url=URL.createObjectURL(new Blob([JSON.stringify(state.preparation,null,2)],{type:'application/json'})); const a=document.createElement('a'); a.href=url;a.download=`xone-unsigned-${state.quote.quoteId}.json`;a.click();setTimeout(()=>URL.revokeObjectURL(url),1000);
});
$('chainKey').addEventListener('change',()=>{invalidate(); tokens();schedule();});
for (const id of ['sellToken','buyToken','quoteKind','amount','maxSell','caller','recipient','slippageBps','maxHops']) $(id).addEventListener('input',()=>{invalidate();modes();schedule();});
for (const id of ['sourceAccount','destinationAccount','createDestinationAta','simulatePreparation','wrapSourceSol','refundWrappedSource','unwrapDestinationSol']) $(id).addEventListener('input',()=>{modes();++state.prepareSeq;state.prepareAbort?.abort();state.preparation=null;$('downloadButton').disabled=true;$('prepareButton').disabled=!state.quote;$('preparation').textContent='执行准备参数已变化，请重新检查';});
$('flipButton').addEventListener('click',()=>{const sell=$('sellToken').value;$('sellToken').value=$('buyToken').value;$('buyToken').value=sell;invalidate();schedule();});
$('refreshButton').addEventListener('click',()=>loadOptions());
$('autoQuote').addEventListener('change',()=>{$('autoQuote').checked?schedule():clearTimeout(state.debounce);});
let connectionRevision=apiBase.value+'\n'+$('apiToken').value;
function connectionChanged() {const next=apiBase.value+'\n'+$('apiToken').value;if(next===connectionRevision)return;connectionRevision=next;state.optionsAbort?.abort();invalidate();state.metadata={chains:[]};tokens();loadOptions();}
apiBase.addEventListener('change',()=>{try {$('apiToken').value=sessionStorage.getItem(tokenKey())||'';} catch {}connectionChanged();});
$('apiToken').addEventListener('change',connectionChanged);
// Poll metadata every ten seconds and check quote expiry every second.
let nextPoll=Date.now()+10000;
state.timer=setInterval(()=>{const now=Date.now(); if(now>=nextPoll){nextPoll=now+10000;if($('autoRefresh').checked&&!document.hidden)loadOptions(true);} if(state.quote && now>=state.quote.validity.expiresAtMs){invalidate('报价已过期，请重新计算');schedule();}},1000);
loadOptions();
