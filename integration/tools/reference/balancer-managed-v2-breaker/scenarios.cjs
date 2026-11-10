'use strict';
// Required scenario inventory is independent of the retained generated rows.
// Deleting a case/transaction/bank together cannot redefine the accepted scope.
const assert=require('node:assert/strict');
function matrix(prefix){const rows=[];for(const name of ['A','B']){for(let i=0;i<3;i++)rows.push(`${prefix}:${name}:asset-${i}:join`,`${prefix}:${name}:asset-${i}:exit`);for(const [i,j]of [[0,1],[1,0]])rows.push(`${prefix}:${name}:regular-${i}-${j}`);}return rows;}
function cases(namespace){
 if(namespace==='cached')return ['zero-underlying-input','zero-BPT-input','minimum-underlying-0','minimum-underlying-1','minimum-underlying-2','minimum-BPT-input',...matrix('cached-original'),'cached-large-join','cached-large-exit'];
 if(namespace==='dynamic')return ['start-1','start','start+1','restart-1','restart','restart+1','end-1','end','end+1'].flatMap(matrix);
 if(namespace==='shared')return ['shared-A-join','shared-A-exit','shared-B-join','shared-B-exit','shared-capacity-A-exit-0','shared-capacity-B-join-0','shared-capacity-A-exit-1','shared-capacity-B-join-1'];
 if(namespace==='boundaries'){
  const out=[];for(let i=0;i<3;i++)out.push(`narrow-small-asset-${i}:join`,`narrow-small-asset-${i}:exit`,`lower-tripped-asset-${i}`,`disabled-large-join-asset-${i}`,`upper-small-exit-asset-${i}`,`upper-tripped-exit-asset-${i}`,`zero-price-disabled-asset-${i}:join`,`zero-price-disabled-asset-${i}:exit`,`quantized-disabled-asset-${i}:join`,`quantized-disabled-asset-${i}:exit`);
  out.push('subunit-upper-small-join-trips','subunit-upper-large-join-recovers');
  return {prefix:out,preparation:'prepare-real-tiny-reserve-',min_preparation:1,max_preparation:35,suffix:['dust-BPT-exit-internal-bound-trips','dust-BPT-exit-disabled-zero-credit','dust-regular-internal-bound-trips','dust-regular-disabled-zero-credit']};
 }
 throw Error('unknown scenario inventory');
}
function controls(namespace){
 if(namespace==='cached')return ['cached-bounds-A','cached-bounds-B'];
 if(namespace==='dynamic')return ['dynamic-reference-A','dynamic-reference-B','first-schedule-A','first-schedule-B','reschedule-computed-start-A','reschedule-computed-start-B','active-fee-A','active-fee-B'];
 if(namespace==='shared')return ['shared-reference-A','shared-reference-B'];
 if(namespace==='boundaries'){
  const out=[];for(let i=0;i<3;i++)out.push(`narrow-lower-asset-${i}`,`both-zero-disabled-asset-${i}`,`upper-only-asset-${i}`,`zero-reference-price-asset-${i}`,`upper-quantizes-to-zero-asset-${i}`);
  return [...out,'legal-subunit-upper-currently-tripped','dust-BPT-upper-reference','dust-BPT-disable-bound','dust-regular-upper-reference','dust-regular-disable-bound','below-lower-minimum','lower-above-one','upper-above-ten','upper-below-lower','BPT-is-not-underlying','stored-price-overflows-u96','non-owner-cannot-set-breaker'];
 }
 throw Error('unknown control inventory');
}
function checkpoints(namespace){return namespace==='dynamic'?['pending-AUM-before-active-weight-breakers','start-1','start','start+1','restart-1','restart','restart+1','end-1','end','end+1']:[{cached:'pending-AUM-before-cached-bounds',boundaries:'boundary-pending-AUM',shared:'shared-pending-AUM'}[namespace]];}
function validate(raw,namespace,eq){
 const observed=raw.cases.filter(c=>c.namespace===namespace).map(c=>c.label),expected=cases(namespace);
 if(Array.isArray(expected))eq(observed,expected,'exact original scenario cases');
 else{eq(observed.slice(0,expected.prefix.length),expected.prefix);eq(observed.slice(-expected.suffix.length),expected.suffix);const prep=observed.slice(expected.prefix.length,-expected.suffix.length);assert(prep.length>=expected.min_preparation&&prep.length<=expected.max_preparation);eq(prep,prep.map((_,i)=>expected.preparation+i),'complete monotonic original preparation sequence');}
 const b=raw.branches[namespace],controlRows=raw.controls.filter(c=>c.namespace===namespace);
 eq(controlRows.map(c=>b.transactions.find(t=>t.id===c.transaction)?.purpose),controls(namespace),'exact original control scenario inventory');
 const invalidControls=new Set(['below-lower-minimum','lower-above-one','upper-above-ten','upper-below-lower','BPT-is-not-underlying','stored-price-overflows-u96','non-owner-cannot-set-breaker']);
 for(const c of controlRows){const t=b.transactions.find(t=>t.id===c.transaction),status=invalidControls.has(t.purpose)?0:1;assert.equal(c.expected_native?.status??1,status,'independent expected control outcome '+t.purpose);assert.equal(Number(BigInt(t.receipt.status)),status,'actual control outcome '+t.purpose);}
 eq(raw.checkpoints.filter(c=>c.namespace===namespace).map(c=>c.label),checkpoints(namespace),'exact clock checkpoint scenario inventory');
 for(const c of raw.cases.filter(c=>c.namespace===namespace)){let status=1,reason=null;
  if(['zero-underlying-input','zero-BPT-input'].includes(c.label))status=0;
  if(c.label.includes('tripped')||c.label.endsWith('internal-bound-trips')||c.label.startsWith('cached-large-')||c.label==='subunit-upper-small-join-trips'){status=0;reason='BAL#356';}
  assert.equal(c.expected_native.status,status,'explicit expected original outcome '+c.label);if(reason)assert.equal(c.expected_native.rejection_reason,reason);
 }
}
module.exports={cases,controls,checkpoints,validate};
