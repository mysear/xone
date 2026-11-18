import test from 'node:test';
import assert from 'node:assert/strict';
import {failedFeeText} from '../src/model.js';
function sample(){
 const hash='a'.repeat(64),error={InstructionError:[0,{Custom:17}]};
 const r={state:'FAILED',confirmationStatus:'finalized',chainKey:'solana:unit',preparationHash:hash,transactionSha256:hash,signature:'signed',slot:10,settlement:null,error:{code:'ONCHAIN_TRANSACTION_FAILED',detail:error}};
 r.failureFee={modelVersion:'xone-solana-failed-fee/1',scope:'FAILED_PREPARED_RECENT_BLOCKHASH_INSTRUCTION',sourceAuthority:'RPC_TRANSACTION_RECEIPT',chainKey:r.chainKey,preparationHash:hash,transactionSha256:hash,signature:r.signature,slot:10,transactionResultHash:hash,metaProjectionHash:hash,nativeUnit:'LAMPORTS',onchainError:error,observedNativeRollbackMatched:true,observedTokenRollbackMatched:true,retainedRentDepositNative:'0',settlementApplied:false,netBuyAmountEstimate:null,netSellAmountEstimate:null,quoteRankingApplied:false,historicalBankVerified:false,independentProductionVerification:false,networkFeeNative:'5000',payerNativeBefore:'1000000000000',payerNativeAfter:'999999995000',payerLamportDelta:'-5000'};return r;
}
test('failed receipt displays bound spent fee and rollback separately from settlement',()=>{
 let r=sample(),text=failedFeeText(r);assert(text.includes('失败交易网络费 5000 lamports（已最终确认）'));assert(text.includes('付款人SOL净变动 -5000'));assert(text.includes('留存租金押金 0'));assert(text.includes('失败收付不计为成交净额'));
 r.state='RECEIPT_PENDING';r.confirmationStatus='confirmed';assert(failedFeeText(r).includes('待最终确认'));
});
test('failed receipt distinguishes pending, inconsistent, old and disappeared fee observations',()=>{
 let r=sample();delete r.failureFee;assert(failedFeeText(r).includes('尚无已核验费用记录'));
 r.state='RECEIPT_PENDING';assert(failedFeeText(r).includes('待核验'));
 r.state='SETTLEMENT_MISMATCH';assert(failedFeeText(r).includes('费用或余额核验未通过'));
 r.state='UNKNOWN';r.error={code:'SIGNATURE_NOT_OBSERVED'};assert.equal(failedFeeText(r),'');
 assert.equal(failedFeeText({state:'FINALIZED',settlement:{verified:true}}),'');
});
test('failed fee display refuses mixed settlement and changed source or transaction identity',()=>{
 for(const [key,value]of [['signature','other'],['chainKey','solana:other'],['preparationHash','b'.repeat(64)],['transactionSha256','b'.repeat(64)],['slot',11],['sourceAuthority','TRUSTED_BANK'],['historicalBankVerified',true],['networkFeeNative','5001'],['retainedRentDepositNative','2039280'],['netBuyAmountEstimate','0'],['observedTokenRollbackMatched',false]]){
  let r=sample();r.failureFee[key]=value;let text=failedFeeText(r);assert(text.includes('不一致'),key);assert(!text.includes('失败交易网络费 5000'));
 }
 for(const key of ['state','confirmationStatus','settlement','error']){let r=sample();r[key]={state:'FINALIZED',confirmationStatus:'processed',settlement:{verified:true},error:{code:'other'}}[key];assert(failedFeeText(r).includes('不一致'),key);}
});
test('failed fee display keeps u64 precision, exact zero and canonical native conservation',()=>{
 let r=sample();r.failureFee.payerNativeBefore='18446744073709551615';r.failureFee.payerNativeAfter='18446744073709546615';assert(failedFeeText(r).includes('网络费 5000'));
 r.failureFee.networkFeeNative='0';r.failureFee.payerNativeAfter=r.failureFee.payerNativeBefore;r.failureFee.payerLamportDelta='0';assert(failedFeeText(r).includes('网络费 0'));
 for(const v of ['01','+1','1e3',5000,'18446744073709551616']){r=sample();r.failureFee.networkFeeNative=v;assert(failedFeeText(r).includes('不一致'));}
 r=sample();r.failureFee.payerNativeAfter='999999995001';assert(failedFeeText(r).includes('不一致'));
 r=sample();r.failureFee.payerLamportDelta='-0';assert(failedFeeText(r).includes('不一致'));
});
