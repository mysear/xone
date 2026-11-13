/** Wallet Standard discovery and exact-message signing. No private keys or broadcast. */
export function walletChain(chainKey) {
  const chains={'solana:mainnet-beta':'solana:mainnet','solana:devnet':'solana:devnet','solana:testnet':'solana:testnet','solana:localnet':'solana:localnet'};
  if(!chains[chainKey])throw new Error('不支持的钱包链');return chains[chainKey];
}
export function decodeBase64(encoded) {
  if(typeof encoded!=='string'||encoded.length>1644||!/^[A-Za-z0-9+/]*={0,2}$/.test(encoded))throw new Error('交易编码无效');
  const bytes=Uint8Array.from(atob(encoded),x=>x.charCodeAt(0));if(bytes.length>1232||btoa(String.fromCharCode(...bytes))!==encoded)throw new Error('交易编码无效');return bytes;
}
export function encodeBase64(bytes){return btoa(String.fromCharCode(...bytes));}
export function validateSignedMessage(unsigned,signed){
  if(!(signed instanceof Uint8Array)||signed.length!==unsigned.length||unsigned[0]!==1||unsigned.length<100||unsigned.slice(1,65).some(x=>x!==0)||signed[0]!==1||signed.slice(1,65).every(x=>x===0))throw new Error('钱包签名交易格式无效');
  for(let i=65;i<unsigned.length;i++)if(unsigned[i]!==signed[i])throw new Error('钱包修改了已模拟的交易消息，请重新报价');
}
export function createWalletRegistry(target,onChange=()=>{}){
  const wallets=new Set();
  const register=(...items)=>{for(const item of items){if(item?.version==='1.0.0'&&item.features?.['standard:connect']&&item.features?.['solana:signTransaction'])wallets.add(item);}onChange([...wallets]);return()=>{for(const item of items)wallets.delete(item);onChange([...wallets]);};};
  const api=Object.freeze({register});const listener=event=>{if(typeof event.detail==='function')event.detail(api);};
  target.addEventListener('wallet-standard:register-wallet',listener);target.dispatchEvent(new CustomEvent('wallet-standard:app-ready',{detail:api}));
  return{get:()=>[...wallets],destroy:()=>target.removeEventListener('wallet-standard:register-wallet',listener)};
}
export async function connectWallet(wallet,chainKey){
  const chain=walletChain(chainKey);if(!wallet.chains?.includes(chain))throw new Error('钱包不支持所选链');
  const result=await wallet.features['standard:connect'].connect();
  const accounts=(result?.accounts||wallet.accounts||[]).filter(account=>account.chains?.includes(chain)&&account.features?.includes('solana:signTransaction'));
  if(!accounts.length)throw new Error('钱包没有可签名的当前链账户');return{wallet,account:accounts[0],chain};
}
export async function signPreparedTransaction(connection,context,preparation,evidence){
  const {wallet,account,chain}=connection;
  if(chain!==walletChain(context.chainKey)||account.address!==context.caller)throw new Error('钱包账户或链与报价调用方不一致');
  if(!wallet.accounts?.some(a=>a.address===account.address))throw new Error('钱包账户已断开');
  if(evidence?.success!==true||evidence.preparationHash!==preparation.preparationHash||evidence.planHash!==context.planHash||evidence.quoteViewId!==context.quoteViewId)throw new Error('缺少当前交易的独立模拟证据');
  if(Date.now()>=context.expiresAtMs)throw new Error('报价已过期');
  const feature=wallet.features['solana:signTransaction'];if(feature?.version!=='1.0.0'||!feature.supportedTransactionVersions?.includes('legacy'))throw new Error('钱包不支持本交易版本');
  const unsigned=decodeBase64(preparation.unsignedTransaction);
  const outputs=await feature.signTransaction({account,transaction:unsigned.slice(),chain,options:{preflightCommitment:'confirmed',minContextSlot:preparation.observedSlot}});
  if(!Array.isArray(outputs)||outputs.length!==1)throw new Error('钱包返回签名数量无效');
  const signed=outputs[0].signedTransaction;validateSignedMessage(unsigned,signed);
  if(Date.now()>=context.expiresAtMs)throw new Error('签名返回时报价已过期，请重新报价');
  // Server verifies Ed25519 and exact bytes again before any network submission.
  return{signedTransaction:encodeBase64(signed),preparationHash:preparation.preparationHash,evidenceHash:evidence.evidenceHash,planHash:context.planHash,quoteViewId:context.quoteViewId};
}
