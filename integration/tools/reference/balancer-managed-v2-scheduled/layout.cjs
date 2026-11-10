'use strict';
function deriveLayout(build,name){const defs=Object.values(build.output.sources).flatMap(s=>s.ast.nodes).filter(n=>n.nodeType==='ContractDefinition'),c=defs.find(n=>n.name===name);let slot=0,offset=0;const rows=[],immutables=[];
 for(const id of [...c.linearizedBaseContracts].reverse()){const d=defs.find(n=>n.id===id);for(const v of d.nodes.filter(n=>n.nodeType==='VariableDeclaration'&&n.stateVariable)){
  const type=v.typeDescriptions.typeString;if(v.constant)continue;if(v.mutability==='immutable'){immutables.push({id:v.id,name:v.name,declaring_contract:d.name,type});continue;}
  let size;if(type==='bool')size=1;else if(/^u?int\d*$/.test(type))size=Number(type.match(/\d+/)?.[0]||256)/8;else if(/^bytes\d+$/.test(type))size=Number(type.slice(5));else if(type==='address'||type.startsWith('contract '))size=20;else if(type.startsWith('mapping(')||type==='string'||type==='bytes')size=32;else throw Error('unhandled AST layout type '+type);
  if(offset+size>32||size===32&&offset){slot++;offset=0;}rows.push({id:v.id,declaring_contract:d.name,name:v.name,type,slot,byte_offset:offset,bytes:size,authority:'derived from original linearized AST; observed storage/getter equality separately'});offset+=size;if(offset===32){slot++;offset=0;}
 }}return {variables:rows,immutables};}
module.exports={deriveLayout};

// Fixed SOURCE_NAME declarations are read from original gzip build-info input,
// then independently evaluated from the original compiler AST (forward refs allowed).
function packedLayout(build){
 const crypto=require('node:crypto'),out={};
 for(const [kind,source]of [['pool','contracts/managed/ManagedPoolStorageLib.sol'],['token','contracts/managed/ManagedPoolTokenStorageLib.sol']]){
  const text=build.input.sources[source].content,ast=build.output.sources[source].ast;
  const declarations=ast.nodes.flatMap(n=>n.nodeType==='ContractDefinition'?n.nodes:[]).filter(n=>n.nodeType==='VariableDeclaration'&&n.constant&&n.typeDescriptions.typeString==='uint256');
  const byID=new Map(declarations.map(d=>[d.id,d])),byName=new Map(declarations.map(d=>[d.name,d])),memo=new Map(),active=new Set();
  function value(n){if(n.nodeType==='Literal'&&/^\d+$/.test(n.value))return Number(n.value);if(n.nodeType==='Identifier')return constant(byID.get(n.referencedDeclaration));if(n.nodeType==='BinaryOperation'&&n.operator==='+')return value(n.leftExpression)+value(n.rightExpression);throw Error('unsupported original packed constant AST');}
  function constant(d){if(!d)throw Error('unknown original constant');if(memo.has(d.name))return memo.get(d.name);if(active.has(d.name))throw Error('cyclic original constant');active.add(d.name);const v=value(d.value);active.delete(d.name);memo.set(d.name,v);return v;}
  const expressions=Object.fromEntries([...text.matchAll(/uint256\s+private\s+constant\s+(\w+)\s*=\s*([^;]+);/g)].map(m=>[m[1],m[2].trim()]));
  function fromSource(name,seen=new Set()) {if(seen.has(name)||!byName.has(name))throw Error('unknown/cyclic fixed source constant');seen.add(name);const expr=expressions[name];if(!expr)throw Error('missing input.sources declaration');return expr.split('+').reduce((sum,s)=>{const token=s.trim();if(/^\d+$/.test(token))return sum+Number(token);if(/^_\w+$/.test(token))return sum+fromSource(token,new Set(seen));throw Error('unsupported original source expression');},0);}
  const constants=Object.fromEntries(declarations.map(d=>{const v=constant(d);if(v!==fromSource(d.name))throw Error('original AST/input.source constant mismatch');return [d.name,v];}));
  out[kind]={source_name:source,sha256:crypto.createHash('sha256').update(text).digest('hex'),constants,declarations:declarations.map(d=>({name:d.name,ast_id:d.id,src:d.src,expression:expressions[d.name]}))};
 }
 out.token.unused={offset:out.token.constants._DECIMAL_DIFF_OFFSET+out.token.constants._DECIMAL_DIFF_WIDTH,bits:256-out.token.constants._DECIMAL_DIFF_OFFSET-out.token.constants._DECIMAL_DIFF_WIDTH,observed_value:'0'};
 return out;
}
module.exports.packedLayout=packedLayout;
