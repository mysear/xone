import test from 'node:test';
import assert from 'node:assert/strict';
import {nativeEconomicsText} from '../src/model.js';
function sample(receipt=false) {
 const identity={chainKey:'solana:unit',preparationHash:'prepared',transactionSha256:'executed'};
 const costs={modelVersion:'xone-solana-native-economics/1',sourceAuthority:receipt?'RPC_TRANSACTION_RECEIPT':'RPC_CAPTURE_SBF_EXECUTION',scope:'EVALUATED_PREPARED_TRANSACTION_ONLY',amountBasis:'OBSERVED_ACTUAL_SETTLEMENT',nativeUnit:'LAMPORTS',chainKey:identity.chainKey,preparationHash:identity.preparationHash,transactionSha256:'executed',historicalBankVerified:false,independentProductionVerification:false,quoteRankingApplied:false,deductedFromQuoteOutput:false,networkFeeIncludedInSwapAmounts:false,depositTreatment:'CAPITAL_DEPOSIT_SEPARATE_FROM_NETWORK_FEE',networkFeeNative:'5000',retainedRentDepositNative:'2039280',actualPayerNativeDelta:'-2033309',tokenValuationStatus:'NATIVE_UNIT_IDENTITY',netSellAmountEstimate:null,netBuyAmountEstimate:'5971',nativeCashReconciliation:'MATCHED',valuationRatio:{numerator:'1',denominator:'1',method:'NATIVE_UNIT_IDENTITY'}};
 return [{success:true,verified:true,transactionSha256:'executed',feeLamports:'5000',payerLamportDelta:costs.actualPayerNativeDelta,costs},identity,receipt];
}
test('native economics displays actual excess output and capital separately for SBF and receipt',()=>{
 for(const receipt of [false,true]) {
  const text=nativeEconomicsText(...sample(receipt));assert(text.includes('费用后收款 5971 lamports'));assert(text.includes('留存租金押金 2039280 lamports（资本另列）'));assert(text.includes(receipt?'RPC回执网络费':'本地执行网络费'));
 }
});
test('native economics preserves exact zero, negative-net unavailable, and old receipt compatibility',()=>{
 let [o,i]=sample();o.costs.netBuyAmountEstimate='0';assert(nativeEconomicsText(o,i).includes('费用后收款 0 lamports'));
 o.costs.netBuyAmountEstimate=null;o.costs.netBuyUnavailableReason='NETWORK_FEE_EXCEEDS_GROSS_OUTPUT';const text=nativeEconomicsText(o,i);assert(text.includes('网络费超过实际收款'));assert(!text.includes('费用后收款 0'));
 assert.equal(nativeEconomicsText({verified:true},{},true),'');
 o.costs.tokenValuationStatus='UNAVAILABLE';delete o.costs.netBuyUnavailableReason;assert(nativeEconomicsText(o,i).includes('缺少适用的同单位估值'));
});
test('native economics preserves input precision above Number safe integer without adding rent to fee',()=>{
 const [o,i]=sample();o.costs.netBuyAmountEstimate=null;o.costs.netSellAmountEstimate='9007199254745993';const text=nativeEconomicsText(o,i);assert(text.includes('费用后输入 9007199254745993 lamports'));assert(text.includes('网络费 5000 lamports'));assert(text.includes('留存租金押金 2039280 lamports'));
});
test('native economics rejects cross-transaction, wrong source and unsupported authority displays',()=>{
 for(const [field,value]of [['chainKey','other'],['preparationHash','other'],['transactionSha256','other'],['sourceAuthority','TRUSTED_CHAIN_BANK'],['historicalBankVerified',true],['quoteRankingApplied',true],['networkFeeNative','5001']]) {
  const args=sample();args[0].costs[field]=value;const text=nativeEconomicsText(...args);assert(text.includes('证据与当前交易不一致'));assert(!text.includes('5971'));
 }
 let args=sample();args[0].success=false;assert(nativeEconomicsText(...args).includes('证据与当前交易不一致'));
 args=sample(true);args[0].verified=false;assert(nativeEconomicsText(...args).includes('证据与当前交易不一致'));
 args=sample(true);args[1].transactionSha256='another';assert(nativeEconomicsText(...args).includes('证据与当前交易不一致'));
});
test('native economics rejects noncanonical amounts, two-sided net and fabricated conversion',()=>{
 for(const v of ['01','-1','1e3',(1n<<64n).toString(),1,'<script>']) {
  const args=sample();args[0].costs.netBuyAmountEstimate=v;assert(nativeEconomicsText(...args).includes('无效'));
 }
 let args=sample();args[0].costs.netSellAmountEstimate='1';assert(nativeEconomicsText(...args).includes('无效'));
 args=sample();args[0].costs.tokenValuationStatus='UNAVAILABLE';assert(nativeEconomicsText(...args).includes('无效'));
 args=sample();args[0].costs.valuationRatio.numerator='2';assert(nativeEconomicsText(...args).includes('估值证据无效'));
});
