// Read-only independent static ABI/storage verification of original target banks.
const fs=require('node:fs'),path=require('node:path');
const root=path.resolve(__dirname,'../../..'),{ethers}=require(path.join(root,'integration/.runtime/stable-reference/node_modules/ethers'));
const demand=(v,w)=>{if(!v)throw Error(w);};
const n=JSON.parse(fs.readFileSync(process.argv[2],'utf8')),d=n.deployment;
const abi=new ethers.utils.Interface(['function getFactory() view returns(address)','function getTokenX() view returns(address)','function getTokenY() view returns(address)','function getBinStep() view returns(uint16)','function getActiveId() view returns(uint24)','function getStaticFeeParameters() view returns(uint16,uint16,uint16,uint16,uint24,uint16,uint24)','function getVariableFeeParameters() view returns(uint24,uint24,uint24,uint40)','function getReserves() view returns(uint128,uint128)','function getProtocolFees() view returns(uint128,uint128)','function getNextNonEmptyBin(bool,uint24) view returns(uint24)','function getBin(uint24) view returns(uint128,uint128)','function totalSupply(uint256) view returns(uint256)','function balanceOf(address) view returns(uint256)']);
const field=(s,o,b)=>String((BigInt(s.parameters)>>BigInt(o))&((1n<<BigInt(b))-1n));
const eq=(a,b)=>Array.isArray(a)&&Array.isArray(b)?a.length===b.length&&a.every((x,i)=>eq(x,b[i])):String(a).toLowerCase()===String(b).toLowerCase();
let banks=0,reads=0;
function check(s,records,storage,header){
 const seen=new Set();
 function get(to,name,expected,args=[]){const data=abi.encodeFunctionData(name,args).toLowerCase(),matches=records.filter(r=>r.to===to&&r.data===data);demand(matches.length===1,'LFJ_TARGET_GETTER_IDENTITY:'+name);const r=matches[0];demand(r.block_hash===header.hash&&r.block_number===header.number&&String(header.timestamp)===s.timestamp,'LFJ_TARGET_GETTER_ANCHOR');const values=abi.decodeFunctionResult(name,r.result);demand(abi.encodeFunctionResult(name,values).toLowerCase()===r.result,'LFJ_TARGET_GETTER_ENCODING');demand(eq(values.length===1?values[0]:values,expected),'LFJ_TARGET_GETTER_VALUE:'+name+':'+JSON.stringify(args)+':'+String(values)+':'+String(expected));demand(!seen.has(r),'LFJ_TARGET_GETTER_DUPLICATE');seen.add(r);}
 for(const [name,value]of [['getFactory',d.factory],['getTokenX',d.token0],['getTokenY',d.token1],['getBinStep',s.bin_step],['getActiveId',field(s,232,24)]])get(d.pool,name+'()',value);
 get(d.pool,'getStaticFeeParameters()',[[0,16],[16,12],[28,12],[40,14],[54,24],[78,14],[92,20]].map(([o,b])=>field(s,o,b)));
 get(d.pool,'getVariableFeeParameters()',[[112,20],[132,20],[152,24],[176,40]].map(([o,b])=>field(s,o,b)));
 get(d.pool,'getReserves()',[BigInt(s.raw_reserve0)-BigInt(s.protocol_fees0),BigInt(s.raw_reserve1)-BigInt(s.protocol_fees1)]);
 get(d.pool,'getProtocolFees()',[s.protocol_fees0,s.protocol_fees1]);
 get(d.token0,'balanceOf(address)',s.raw_reserve0,[d.pool]);get(d.token1,'balanceOf(address)',s.raw_reserve1,[d.pool]);
 const ids=s.bins.map(b=>b.id);get(d.pool,'getNextNonEmptyBin(bool,uint24)',ids[0]??0,[false,0]);
 for(const b of s.bins){get(d.pool,'getBin(uint24)',[b.reserve0,b.reserve1],[b.id]);get(d.pool,'totalSupply(uint256)',b.total_shares,[b.id]);get(d.pool,'getNextNonEmptyBin(bool,uint24)',ids.find(id=>id>b.id)??0,[false,b.id]);get(d.pool,'getNextNonEmptyBin(bool,uint24)',ids.filter(id=>id<b.id).at(-1)??16777215,[true,b.id]);}
 demand(seen.size===records.length,'LFJ_TARGET_UNVALIDATED_GETTERS');
 const expected=new Map([['0x3',1n],['0x4',BigInt(s.parameters)],['0x5',BigInt(s.raw_reserve0)+(BigInt(s.raw_reserve1)<<128n)],['0x6',BigInt(s.protocol_fees0)+(BigInt(s.protocol_fees1)<<128n)],...s.oracle_samples.map(v=>['0x'+(11+v.index-1).toString(16),BigInt(v.data)])]);
 demand(Object.keys(storage).length===expected.size,'LFJ_TARGET_STORAGE_SCOPE');for(const [key,value]of expected)demand(BigInt(storage[key])===value,'LFJ_TARGET_STORAGE_VALUE:'+key);
 banks++;reads+=seen.size;
}
for(const c of n.cases){check(c.before,c.getter_reads,c.storage_reads,c.original_before_block);if(c.expected.swapped)check(c.expected.after,c.expected.getter_reads,c.expected.storage_reads,c.expected.original_after_block);}
for(const seq of n.sequences)for(const c of seq.steps){check(c.before,c.getter_reads,c.storage_reads,c.original_before_block);check(c.expected.after,c.expected.getter_reads,c.expected.storage_reads,c.expected.original_after_block);}
console.log(JSON.stringify({passed:true,banks,reads}));
