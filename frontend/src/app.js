import {createWalletRegistry,connectWallet,signPreparedTransaction} from './wallet.js';
import {positiveInteger, boundedInteger, routeView, snapshotView, protocolBindingsMatch, singlePreparationPool, escapeHtml as esc} from './model.js';
const $ = id => document.getElementById(id);
const params = new URLSearchParams(location.search);
const state = {metadata:{chains:[]}, optionsSeq:0, quoteSeq:0, prepareSeq:0, quote:null, preparation:null, independent:null, independentAbort:null, timer:null, debounce:null, optionsAbort:null, quoteAbort:null, prepareAbort:null};
const walletState={wallets:[],connection:null,unsubscribe:null,signed:null,receipt:null,receiptBase:null,receiptSeq:0,timer:null,submitting:false,signing:false};
const apiBase = $('apiBase');
apiBase.value = sessionStorage.getItem(`xone-api-base:${location.pathname}${location.search}`) || params.get('api') || localStorage.getItem('xone-api-base') || 'http://127.0.0.1:8080';
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
  ++state.quoteSeq; ++state.prepareSeq;resetWalletSigning();
  state.quoteAbort?.abort(); state.prepareAbort?.abort(); clearTimeout(state.debounce);
  state.quote=null; state.preparation=null; state.independent=null;state.independentAbort?.abort();$('independentButton').disabled=true;
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
  const preparedPool=singlePreparationPool(state.quote,c);const executionPools=state.quote?(preparedPool?[preparedPool]:[]):(c?.pools||[]).filter(p=>p.available&&(p.directions||[]).some(d=>d.sellToken===$('sellToken').value&&d.buyToken===$('buyToken').value));
  const wrapAllowed=$('sellToken').value==='So11111111111111111111111111111111111111112' && executionPools.some(p=>p.execution?.wrapSourceSol);
  $('wrapSourceSol').disabled=!wrapAllowed;if(!wrapAllowed)$('wrapSourceSol').checked=false;
  const createAllowed=$('buyToken').value!=='native:solana:sol' && executionPools.some(p=>p.execution?.createDestinationAta);$('createDestinationAta').disabled=!createAllowed;if(!createAllowed)$('createDestinationAta').checked=false;
  const refundAllowed=wrapAllowed && $('wrapSourceSol').checked && !$('sourceAccount').value.trim();
  $('refundWrappedSource').disabled=!refundAllowed;if(!refundAllowed)$('refundWrappedSource').checked=false;
  const unwrapAllowed=$('buyToken').value==='So11111111111111111111111111111111111111112' && executionPools.some(p=>p.execution?.unwrapDestinationSol) && $('caller').value.trim()!=='' && $('caller').value.trim()===$('recipient').value.trim() && !$('destinationAccount').value.trim() && !$('createDestinationAta').checked;
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
    const next=chain(); const changed=oldId!==next?.viewId || selected!==next?.chainKey || old?.ready!==next?.ready || !protocolBindingsMatch(state.quote,next);
    const oldSell=$('sellToken').value, oldBuy=$('buyToken').value;
    tokens(); badge('healthBadge','已连接','ok');
    if (changed || oldSell!==$('sellToken').value || oldBuy!==$('buyToken').value) {invalidate('解析状态已更新，请重新计算'); schedule();}
    if (!next?.ready) invalidate('当前链未就绪，等待解析状态恢复');
    localStorage.setItem('xone-api-base',address); sessionStorage.setItem(`xone-api-base:${location.pathname}${location.search}`,address); sessionStorage.setItem(`xone-api-token:${address}`,credential);resumeReceipt();refreshWalletActions();
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
function preparationAvailable(q=state.quote) {return Boolean(q&&singlePreparationPool(q,chain())?.execution?.preparation);}
function renderQuote(q) {
  const routes=q.routes||[]; const prep=preparationAvailable(q);
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
    if(!protocolBindingsMatch(q,chain()))throw new Error('协议报价状态已更新，请重新计算');
    state.quote=q; modes();renderQuote(q); $('quoteTime').textContent=`${Math.round(performance.now()-started)} ms`; badge('stateBadge','报价成功','ok');
  } catch(e) {if (seq===state.quoteSeq && e.name!=='AbortError') {invalidate('没有可用报价'); error(e.message);badge('stateBadge','报价失败','bad');}}
});
$('prepareButton').addEventListener('click',async()=>{
  const q=state.quote; if (!q||!preparationAvailable(q)) return; const seq=state.quoteSeq, prepSeq=++state.prepareSeq; state.preparation=null;state.independent=null;state.independentAbort?.abort();resetWalletSigning();$('independentButton').disabled=true; $('downloadButton').disabled=true; $('prepareButton').disabled=true; $('preparation').textContent='正在核对账户…';
  try {
    const payload={createDestinationAta:$('createDestinationAta').checked,simulate:$('simulatePreparation').checked,wrapSourceSol:$('wrapSourceSol').checked,refundWrappedSource:$('refundWrappedSource').checked,unwrapDestinationSol:$('unwrapDestinationSol').checked}; if ($('sourceAccount').value.trim()) payload.sourceTokenAccount=$('sourceAccount').value.trim(); if ($('destinationAccount').value.trim()) payload.destinationTokenAccount=$('destinationAccount').value.trim();
    state.prepareAbort=new AbortController();
    const prepared=await request(`/v1/quotes/${encodeURIComponent(q.quoteId)}/prepare`,{method:'POST',body:JSON.stringify(payload),signal:state.prepareAbort.signal});
    if (seq!==state.quoteSeq || prepSeq!==state.prepareSeq || state.quote!==q) return;
    state.preparation=prepared; $('downloadButton').disabled=!prepared.ready;refreshWalletActions();
    $('independentButton').disabled=!prepared.ready || !(chain()?.pools||[]).some(p=>p.execution?.independentLocalSimulation&&(q.routes||[]).some(r=>(r.path?.edges||r.edges||[]).some(e=>e.pool_id===p.poolId)));
    const wrapText=prepared.wrapLamports && prepared.wrapLamports!=='0'?` · SOL包装 ${prepared.wrapLamports} lamports${prepared.sourceWsolRetained?' · 预计剩余 '+prepared.expectedResidualWsol+' WSOL最小单位，余额仍保留为WSOL':' · 未用资金通过本次临时账户关闭转回SOL'}`:'';
    const tokenFeeText=prepared.destinationTokenProgram==='TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb'?` · Token2022转账费 ${prepared.expectedDestinationTransferFee} 最小单位 · 收款账户 ${prepared.destinationAccountSize} 字节`:'';
    const prefundText=prepared.accountCreation&&prepared.destinationPrefundedLamports&&prepared.destinationPrefundedLamports!=='0'?` · 收款地址已有 ${prepared.destinationPrefundedLamports} lamports，本次只补足 ${prepared.destinationRentLamports??prepared.rentLamports} lamports 租金`:'';
    const sourcePrefundText=prepared.sourceAccountCreation&&prepared.sourcePrefundedLamports&&prepared.sourcePrefundedLamports!=='0'?` · 输入地址已有 ${prepared.sourcePrefundedLamports} lamports，本次只补足 ${prepared.sourceRentLamports} lamports 租金`:'';
    const nativeText=prepared.sourceNativeSol||prepared.destinationNativeSol?` · 原生SOL${prepared.sourceNativeSol?'付款':'收款'} · 交易金额与网络费分别结算 · 预计用户SOL净变动 ${prepared.expectedPayerLamportDelta} lamports`:'';
    const closeText=prepared.temporaryWsolAccount?` · 临时账户关闭转回 ${prepared.expectedNativeRefundLamports} lamports（含退租 ${prepared.rentRefundLamports}） · 预计payer SOL净变动 ${prepared.expectedPayerLamportDelta} lamports`:'';
    $('preparation').textContent=prepared.ready?`账户检查通过 · 观察 slot ${prepared.observedSlot} · 交易费 ${prepared.feeLamports} lamports · 创建租金 ${prepared.rentLamports||0} lamports${wrapText}${closeText}${tokenFeeText}${nativeText}${prefundText}${sourcePrefundText}${prepared.sourceAccountCreation?' · 已加入输入WSOL账户创建指令':''}${prepared.accountCreation?' · 已加入收款 ATA 创建指令':''}${prepared.simulation?.success?' · RPC模拟到账 '+prepared.simulation.actualOutput+(prepared.unwrapDestinationSol||prepared.destinationNativeSol?' lamports SOL':''):''} · 待签名，生产独立验证仍未完成`:`账户准备未满足：${(prepared.blockers||[]).join(', ')}。目标账户：${prepared.destinationTokenAccount}`;
  } catch(e) {if(seq===state.quoteSeq && prepSeq===state.prepareSeq && e.name!=='AbortError') {if (/QUOTE_|PROGRAM_CHANGED/.test(e.message)) invalidate('执行状态已变化，请重新报价'); $('preparation').textContent=e.message;}}
  finally {if (seq===state.quoteSeq && prepSeq===state.prepareSeq) $('prepareButton').disabled=!preparationAvailable();}
});
$('independentButton').addEventListener('click',async()=>{
  const q=state.quote,p=state.preparation,seq=state.quoteSeq,preparationSeq=state.prepareSeq;if(!q||!p?.ready)return;
  state.independentAbort?.abort();state.independentAbort=new AbortController();state.independent=null;resetWalletSigning();$('independentButton').disabled=true;
  try{
    const evidence=await request(`/v1/quotes/${encodeURIComponent(q.quoteId)}/preparations/${encodeURIComponent(p.preparationHash)}/simulate-independent`,{method:'POST',signal:state.independentAbort.signal});
    if(seq!==state.quoteSeq||preparationSeq!==state.prepareSeq||state.preparation!==p)return;
    if(evidence.preparationHash!==p.preparationHash||evidence.planHash!==q.planHash||evidence.quoteViewId!==q.quoteViewId||evidence.success!==true)throw new Error('独立模拟证据与当前交易不一致');
    state.independent=evidence;refreshWalletActions();$('preparation').textContent+=` · 独立本地SBF模拟通过，实际输入 ${evidence.actualInput} / 到账 ${evidence.actualOutput}，CU ${evidence.unitsConsumed}，证据 ${evidence.evidenceHash}（${evidence.accountSource==='PREPARATION_RPC_CAPTURE'?'RPC账户快照，非历史银行证明':'受控银行'}，生产资格未开放）`;
  }catch(e){if(seq===state.quoteSeq&&preparationSeq===state.prepareSeq&&e.name!=='AbortError'){$('preparation').textContent=`独立模拟失败：${e.message}`;state.independent=null;}}
  finally{if(seq===state.quoteSeq&&preparationSeq===state.prepareSeq)$('independentButton').disabled=false;}
});
$('downloadButton').addEventListener('click',()=>{
  if (!state.preparation?.ready || !state.quote || Date.now()>=state.quote.validity.expiresAtMs) {invalidate('报价已过期，请重新计算'); return;}
  const url=URL.createObjectURL(new Blob([JSON.stringify(state.preparation,null,2)],{type:'application/json'})); const a=document.createElement('a'); a.href=url;a.download=`xone-unsigned-${state.quote.quoteId}.json`;a.click();setTimeout(()=>URL.revokeObjectURL(url),1000);
});
$('chainKey').addEventListener('change',()=>{invalidate(); tokens();schedule();});
for (const id of ['sellToken','buyToken','quoteKind','amount','maxSell','caller','recipient','slippageBps','maxHops']) $(id).addEventListener('input',()=>{invalidate();modes();schedule();});
for (const id of ['sourceAccount','destinationAccount','createDestinationAta','simulatePreparation','wrapSourceSol','refundWrappedSource','unwrapDestinationSol']) $(id).addEventListener('input',()=>{modes();++state.prepareSeq;state.prepareAbort?.abort();state.preparation=null;state.independent=null;state.independentAbort?.abort();resetWalletSigning();$('independentButton').disabled=true;$('downloadButton').disabled=true;$('prepareButton').disabled=!preparationAvailable();$('preparation').textContent=preparationAvailable()?'执行准备参数已变化，请重新检查':'当前协议或 Router RPC 配置尚未提供执行准备能力。';});
$('flipButton').addEventListener('click',()=>{const sell=$('sellToken').value;$('sellToken').value=$('buyToken').value;$('buyToken').value=sell;invalidate();schedule();});
$('refreshButton').addEventListener('click',()=>loadOptions());
$('autoQuote').addEventListener('change',()=>{$('autoQuote').checked?schedule():clearTimeout(state.debounce);});
let connectionRevision=apiBase.value+'\n'+$('apiToken').value;
function connectionChanged() {const next=apiBase.value+'\n'+$('apiToken').value;if(next===connectionRevision)return;connectionRevision=next;clearReceiptContext();state.optionsAbort?.abort();invalidate();state.metadata={chains:[]};tokens();loadOptions();}
apiBase.addEventListener('change',()=>{try {$('apiToken').value=sessionStorage.getItem(tokenKey())||'';} catch {}connectionChanged();});
$('apiToken').addEventListener('change',connectionChanged);
// Poll metadata every ten seconds and check quote expiry every second.
let nextPoll=Date.now()+10000;
state.timer=setInterval(()=>{const now=Date.now(); if(now>=nextPoll){nextPoll=now+10000;if($('autoRefresh').checked&&!document.hidden)loadOptions(true);} if(state.quote && now>=state.quote.validity.expiresAtMs){invalidate('报价已过期，请重新计算');schedule();}},1000);
function resetWalletSigning(){
  walletState.signed=null;$('confirmSubmission').checked=false;$('confirmSubmission').disabled=true;$('walletSign').disabled=true;$('walletSubmit').disabled=true;
}
function submissionAvailable(){return (chain()?.pools||[]).some(p=>p.execution?.walletSubmission&&(state.quote?.routes||[]).some(r=>(r.path?.edges||r.edges||[]).some(e=>e.pool_id===p.poolId)));}
function refreshWalletActions(){
  const connected=walletState.connection,q=state.quote,p=state.preparation;
  const valid=Boolean(q&&p?.ready&&state.independent?.success&&Date.now()<q.validity.expiresAtMs&&connected?.account.address===$('caller').value.trim()&&submissionAvailable());
  $('walletSign').disabled=!valid||walletState.signing||walletState.submitting;const signed=valid&&walletState.signed?.preparationHash===p?.preparationHash;
  $('confirmSubmission').disabled=!signed;$('walletSubmit').disabled=!signed||!$('confirmSubmission').checked||walletState.submitting;
}
function renderWallets(wallets){
  walletState.wallets=wallets;const wanted=$('walletSelect').value;
  $('walletSelect').replaceChildren(...(wallets.length?wallets.map((wallet,i)=>new Option(String(wallet.name||'Wallet').slice(0,128),String(i))):[new Option('未发现钱包','')]));
  if(wallets.some((_,i)=>String(i)===wanted))$('walletSelect').value=wanted;$('walletConnect').disabled=!wallets.length;
}
createWalletRegistry(window,renderWallets);
$('walletSelect').addEventListener('change',()=>{walletState.unsubscribe?.();walletState.connection=null;resetWalletSigning();$('walletStatus').textContent='钱包已改变，请重新连接';});
$('walletConnect').addEventListener('click',async()=>{
  try{
    const wallet=walletState.wallets[Number($('walletSelect').value)];if(!wallet)throw new Error('未发现可用钱包');
    const connection=await connectWallet(wallet,$('chainKey').value);walletState.unsubscribe?.();walletState.connection=connection;resetWalletSigning();
    const changed=$('caller').value.trim()!==connection.account.address;$('caller').value=connection.account.address;if(changed)invalidate('钱包调用方已改变，请重新报价');
    walletState.unsubscribe=wallet.features['standard:events']?.on('change',()=>{walletState.connection=null;invalidate('钱包账户或链已改变，请重新连接并报价');$('walletStatus').textContent='钱包状态已改变';});
    $('walletStatus').textContent=`已连接 ${wallet.name} · ${connection.account.address} · ${connection.chain}`;refreshWalletActions();
  }catch(e){walletState.connection=null;resetWalletSigning();$('walletStatus').textContent=e.message;}
});
$('walletSign').addEventListener('click',async()=>{
  const q=state.quote,p=state.preparation,evidence=state.independent,connection=walletState.connection,seq=state.quoteSeq,preparationSeq=state.prepareSeq;
  if(!q||!p||!connection||walletState.signing||walletState.submitting)return;walletState.signing=true;$('walletSign').disabled=true;
  try{
    const signed=await signPreparedTransaction(connection,{chainKey:q.chainKey,caller:$('caller').value.trim(),planHash:q.planHash,quoteViewId:q.quoteViewId,expiresAtMs:q.validity.expiresAtMs},p,evidence);
    if(seq!==state.quoteSeq||preparationSeq!==state.prepareSeq||walletState.connection!==connection||state.preparation!==p)return;
    const verified=await request(`/v1/quotes/${encodeURIComponent(q.quoteId)}/preparations/${encodeURIComponent(p.preparationHash)}/verify-signature`,{method:'POST',body:JSON.stringify({signedTransaction:signed.signedTransaction,evidenceHash:signed.evidenceHash})});
    if(seq!==state.quoteSeq||preparationSeq!==state.prepareSeq||walletState.connection!==connection)return;
    if(Date.now()>=q.validity.expiresAtMs)throw new Error('签名校验返回时报价已过期，请重新计算');
    if(verified.preparationHash!==p.preparationHash||verified.evidenceHash!==signed.evidenceHash||verified.signed!==true||verified.submitted!==false)throw new Error('签名校验结果与当前交易不一致');
    walletState.signed={...signed,signature:verified.signature};$('walletStatus').textContent=`签名和消息字节校验通过，尚未广播 · ${verified.signature}`;refreshWalletActions();
  }catch(e){if(seq===state.quoteSeq&&preparationSeq===state.prepareSeq){resetWalletSigning();$('walletStatus').textContent=`签名未完成：${e.message}`;}}
  finally{walletState.signing=false;refreshWalletActions();}
});
$('confirmSubmission').addEventListener('change',refreshWalletActions);
function showReceipt(receipt){
  walletState.receipt=receipt;$('receiptRefresh').disabled=false;if(receipt.signed===true)$('walletStatus').textContent='该签名已有发送记录，执行状态见下方回执。';
  const settlement=receipt.settlement;const states={SUBMITTING:'正在发送或结果待核对',SUBMITTED:'RPC已接受，等待链上确认',UNKNOWN:'发送或链上状态不明确，按签名查询',PROCESSED:'已处理，等待确认',RECEIPT_PENDING:'链上状态已返回，等待回执校验',CONFIRMED:'已确认，等待最终确认',FINALIZED:'已最终确认',FAILED:'链上执行失败',EXPIRED:'有效期已过且当前节点未观察到签名',SETTLEMENT_MISMATCH:'链上回执或收付不一致'};
  $('transactionReceipt').textContent=`${states[receipt.state]||receipt.state} · ${receipt.chainKey}${receipt.sellToken&&receipt.buyToken?' · 交易 '+receipt.sellToken+' → '+receipt.buyToken:''}${receipt.quoteId?' · 回执报价 '+receipt.quoteId:''} · 收款 ${receipt.recipient} · 签名 ${receipt.signature}${settlement?.verified?' · 实际输入 '+settlement.actualInput+' / 到账 '+settlement.actualOutput+' · 费用 '+settlement.feeLamports+' lamports'+(settlement.actualPersistentRentLamports!==undefined?' · 实际账户租金 '+settlement.actualPersistentRentLamports+' lamports':'')+(settlement.payerLamportDelta!==undefined?' · 用户SOL净变动 '+settlement.payerLamportDelta+' lamports':''):''}${receipt.error?' · '+(receipt.error.code||JSON.stringify(receipt.error)):''}`;
}
function clearReceiptContext(){
  ++walletState.receiptSeq;clearTimeout(walletState.timer);walletState.receipt=null;walletState.receiptBase=null;$('receiptRefresh').disabled=true;
  $('transactionReceipt').textContent='当前 Router 暂无已提交交易回执。';
  const connection=walletState.connection;
  $('walletStatus').textContent=connection?`已连接 ${connection.wallet.name} · ${connection.account.address} · ${connection.chain}`:'先完成独立模拟，再由钱包用户确认签名。';
}
async function refreshReceipt(automatic=false){
  const receipt=walletState.receipt;if(!receipt?.signature)return;
  const address=base(),seq=++walletState.receiptSeq;clearTimeout(walletState.timer);
  const current=()=>seq===walletState.receiptSeq&&walletState.receiptBase===address&&walletState.receipt?.signature===receipt.signature;
  try{const next=await request(`/v1/transactions/${encodeURIComponent(receipt.signature)}`);if(!current()||base()!==address)return;showReceipt(next);
    if(!['FINALIZED','FAILED','EXPIRED','SETTLEMENT_MISMATCH'].includes(next.state)){walletState.timer=setTimeout(()=>{if(current())refreshReceipt(true);},2000);}
  }catch(e){if(!current())return;$('transactionReceipt').textContent=`回执查询未完成：${e.message} · 签名 ${receipt.signature}。可手动刷新；不会重复广播。`;if(!automatic)clearTimeout(walletState.timer);}
}
$('receiptRefresh').addEventListener('click',()=>refreshReceipt());
function resumeReceipt(){
  const address=base();if(walletState.receiptBase===address)return;clearReceiptContext();walletState.receiptBase=address;
  try{const signature=sessionStorage.getItem(`xone-last-transaction:${address}`);if(signature){walletState.receipt={signature};refreshReceipt();}}catch{}
}
$('walletSubmit').addEventListener('click',async()=>{
  const q=state.quote,p=state.preparation,signed=walletState.signed,address=base();if(!q||!p||!signed||!$('confirmSubmission').checked||walletState.submitting)return;
  if(Date.now()>=q.validity.expiresAtMs){invalidate('报价已过期，请重新计算');return;}
  walletState.submitting=true;refreshWalletActions();
  try{sessionStorage.setItem(`xone-last-transaction:${address}`,signed.signature);const receipt=await request(`/v1/quotes/${encodeURIComponent(q.quoteId)}/preparations/${encodeURIComponent(p.preparationHash)}/submit`,{method:'POST',body:JSON.stringify({signedTransaction:signed.signedTransaction,evidenceHash:signed.evidenceHash,confirm:true})});
    if(base()!==address)return;sessionStorage.setItem(`xone-last-transaction:${address}`,receipt.signature);showReceipt(receipt);resetWalletSigning();await refreshReceipt(true);
  }catch(e){if(base()===address){walletState.receipt={signature:signed.signature};sessionStorage.setItem(`xone-last-transaction:${address}`,signed.signature);$('transactionReceipt').textContent=`发送结果待查询：${e.message} · 签名 ${signed.signature}，不会自动重复广播`;$('receiptRefresh').disabled=false;resetWalletSigning();}}
  finally{walletState.submitting=false;refreshWalletActions();}
});

loadOptions();
