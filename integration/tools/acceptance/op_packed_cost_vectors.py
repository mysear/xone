#!/usr/bin/env python3
"""Execute unchanged pinned op-geth packed DA/operator arithmetic and FastLZ.

Actual public inputs must first be exported by the normal Go source verifier.
Other rows are labelled arithmetic byte probes, never executable transactions.
"""
import hashlib
import json
from pathlib import Path
import subprocess
from op_l1_cost_vectors import ROOT, SOURCE_SHA256, digest, function, public_cases, require


def method(source, prefix):
    start = source.index(prefix)
    brace = source.index('{', start)
    depth, end = 1, brace + 1
    while depth:
        require(end < len(source), 'Unclosed original method')
        depth += (source[end] == '{') - (source[end] == '}')
        end += 1
    return source[start:end]


def attributes(format_name, base, blob, scalar, blob_scalar, op_scalar=0, op_constant=0):
    selector, length = {'ecotone': ('440a5e20', 164), 'isthmus': ('098999be', 176), 'jovian': ('3db6be2b', 178)}[format_name]
    data = bytearray.fromhex(selector)
    for value, size in [(scalar, 4), (blob_scalar, 4), (0, 8), (1700000000, 8), (1, 8), (base, 32), (blob, 32)]:
        data.extend(value.to_bytes(size, 'big'))
    data.extend(bytes(64))
    if length >= 176:
        data.extend(op_scalar.to_bytes(4, 'big'))
        data.extend(op_constant.to_bytes(8, 'big'))
    if length == 178:
        data.extend((400).to_bytes(2, 'big'))
    require(len(data) == length, 'Invalid probe attributes')
    return data.hex()


def write(output, source_input):
    output, source_input = Path(output).resolve(), Path(source_input).resolve()
    output.parent.mkdir(parents=True, exist_ok=False)
    source_path = ROOT/'integration/artifacts/local/op-l1-cost-reference/rollup_cost.go'
    require(digest(source_path) == SOURCE_SHA256, 'Pinned original op-geth source changed')
    source = source_path.read_text()
    functions = ['NewRollupCostData', 'newL1CostFuncEcotone', 'NewL1CostFuncFjord',
                 'bedrockCalldataGasUsed', 'FlzCompressLen', 'newOperatorCostFuncIsthmus',
                 'newOperatorCostFuncOperatorFeeFix']
    selected = '\n\n'.join(function(source, name) for name in functions)
    selected += '\n\n' + method(source, 'func (cd RollupCostData) estimatedDASizeScaled()')
    # Definitions are original source text too. No replacement fee formula.
    constant_start = source.index('\toneHundred     =')
    constant_end = source.index('\n\temptyScalars', constant_start)
    constants = source[constant_start:constant_end]
    runner = source[:source.index('package types')] + '''package main
import("encoding/binary";"encoding/hex";"encoding/json";"fmt";"math/big";"os";
"github.com/holiman/uint256")
type RollupCostData struct { Zeroes, Ones, FastLzSize uint64 }
type l1CostFunc func(RollupCostData)(*big.Int,*big.Int)
type operatorCostFunc func(uint64)*uint256.Int
var params=struct{TxDataZeroGas,TxDataNonZeroGasEIP2028 uint64}{4,16}
var (
''' + constants + '\n)\n' + selected + '''
func main(){
 var rows []map[string]interface{}
 if err:=json.NewDecoder(os.Stdin).Decode(&rows);err!=nil{panic(err)}
 for _,r:=range rows{
  wire,err:=hex.DecodeString(r["encoded_transaction"].(string));if err!=nil{panic(err)}
  a,err:=hex.DecodeString(r["attributes"].(string));if err!=nil||len(a)<164{panic("attributes")}
  d:=NewRollupCostData(wire)
  base,blob:=new(big.Int).SetBytes(a[36:68]),new(big.Int).SetBytes(a[68:100])
  scalar:=new(big.Int).SetUint64(uint64(binary.BigEndian.Uint32(a[4:8])))
  blobscalar:=new(big.Int).SetUint64(uint64(binary.BigEndian.Uint32(a[8:12])))
  var f l1CostFunc
  if r["data_rule"]=="Ecotone" {f=newL1CostFuncEcotone(base,blob,scalar,blobscalar)}else if r["data_rule"]=="Fjord" {f=NewL1CostFuncFjord(base,blob,scalar,blobscalar)}else{panic("data rule")}
  fee,gas:=f(d)
  r["fee"]=fee.String();r["l1_gas_used"]=gas.String();r["overflow"]=fee.BitLen()>256
  r["zeroes"]=d.Zeroes;r["ones"]=d.Ones;r["fastlz_bytes"]=d.FastLzSize
  r["estimated_da_bytes_scaled"]=d.estimatedDASizeScaled().String()
  if rule,ok:=r["operator_rule"].(string);ok{
   if len(a)<176{panic("operator attributes")}
   s:=new(big.Int).SetUint64(uint64(binary.BigEndian.Uint32(a[164:168])))
   c:=new(big.Int).SetUint64(binary.BigEndian.Uint64(a[168:176]))
   gas,ok:=new(big.Int).SetString(r["gas_used"].(string),10);if !ok||!gas.IsUint64(){panic("gas")}
   var op operatorCostFunc
   if rule=="Isthmus"{op=newOperatorCostFuncIsthmus(s,c)}else if rule=="Jovian"{op=newOperatorCostFuncOperatorFeeFix(s,c)}else{panic("operator rule")}
   r["operator_fee"]=op(gas.Uint64()).ToBig().String()
  }
 }
 b,err:=json.Marshal(rows);if err!=nil{panic(err)};fmt.Println(string(b))
}
'''
    runner_path = output.parent/'original-packed-reference.go'
    runner_path.write_text(runner)
    actual_source = json.loads(source_input.read_bytes())
    require(actual_source.get('passed') is True and actual_source.get('normal_archive_replayed') is True, 'Current normal source verification required')
    require(actual_source.get('authority') == 'RPC_HEADER_BODY_COMMITMENT_NOT_CONSENSUS' and actual_source.get('production_ready') is False, 'Source authority changed')
    require(len(actual_source['users']) == 49, 'Current source user coverage')
    rows = []
    for user in actual_source['users']:
        observed = user['observed_rpc_receipt']
        rows.append(dict(id='public-current-'+str(user['index']), encoded_transaction=user['encoded_transaction'],
                         transaction_hash=user['hash'], attributes=actual_source['attributes'], data_rule='Fjord',
                         operator_rule='Jovian', gas_used=str(user['gas_used']),
                         receipt_fee=str(int(observed['l1Fee'], 16)), receipt_gas=str(int(observed['l1GasUsed'], 16))))
    wires = [bytes([2])+bytes(n) for n in [0,1,11,12,13,30,31,32,33,63,64,65,127,128,129,261,262,263,511,512,8190,8191,8192]]
    for n in [13,32,170,171,255,262,263,1024,8191,8192,16384,65536]:
        random_bytes = b''.join(hashlib.sha256(f'op-packed-{i}'.encode()).digest() for i in range((n+31)//32))[:n]
        wires.append(bytes([2])+random_bytes)
        wires.append(bytes([2])+random_bytes+random_bytes)
    historic, historic_refs = public_cases()
    wires.extend(bytes.fromhex(row['encoded_transaction']) for row in historic)
    maximum = 2**256-1
    parameters = [(0,0,0,0),(1,1,1,1),(7,11,684000,1000000),
                  (maximum,maximum,1,1),(maximum,maximum,2**32-1,2**32-1)]
    for rule in ['Ecotone', 'Fjord']:
        for i, values in enumerate(parameters):
            for j, wire in enumerate(wires):
                rows.append(dict(id=f'bytes-{rule}-{i}-{j}', encoded_transaction=wire.hex(),
                                 attributes=attributes('ecotone', *values), data_rule=rule))
    for rule in ['Isthmus', 'Jovian']:
        for gas in [0,1,999999,1000000,1000001,2**32,2**64-1]:
            for scalar, constant in [(0,0),(0,2**64-1),(1,0),(2**32-1,2**64-1)]:
                rows.append(dict(id=f'operator-{rule}-{gas}-{scalar}-{constant}', encoded_transaction='02',
                                 attributes=attributes('isthmus' if rule=='Isthmus' else 'jovian',1,1,1,1,scalar,constant),
                                 data_rule='Fjord', operator_rule=rule, gas_used=str(gas)))
    result = subprocess.run(['go','-C',str(ROOT/'chain-parser-go'),'run',str(runner_path)],
                            input=json.dumps(rows), text=True, capture_output=True, timeout=120)
    (output.parent/'original-packed-reference.stderr').write_text(result.stderr)
    require(result.returncode == 0, 'Original packed reference failed: '+result.stderr[-2000:])
    actual = json.loads(result.stdout)
    require(len(actual)==len(rows) and all(all(r.get(k)==v for k,v in expected.items()) for r,expected in zip(actual,rows)), 'Original reference changed input')
    for row in actual:
        if 'receipt_fee' in row:
            require(row['fee']==row['receipt_fee'] and row['l1_gas_used']==row['receipt_gas'], 'Original current fee differs from captured RPC observation')
    references = {str(p.relative_to(ROOT)):digest(p) for p in [source_path,runner_path,source_input]}
    references.update(historic_refs)
    doc = dict(passed=True, cases=actual, references=references, source_sha256=SOURCE_SHA256,
               original_functions_sha256=hashlib.sha256(selected.encode()).hexdigest(),
               public_signed_user_transactions=49, arithmetic_byte_probes=len(rows)-49,
               production_ready=False, complete_chain_fees_claimed=False, quote_ranking_changed=False)
    output.write_text(json.dumps(doc,indent=2)+'\n')
    return dict(cases=len(actual), overflow_cases=sum(r['overflow'] for r in actual),
                references=references, sha256=digest(output), public_signed_user_transactions=49)
