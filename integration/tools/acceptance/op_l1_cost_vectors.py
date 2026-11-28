#!/usr/bin/env python3
"""Original pinned op-geth fee kernel plus retained signed OP public wires.

Extracted upstream functions are executed unchanged. The small Go wrapper only
provides their types/constants and I/O. No Rust calculation supplies expectations.
"""
import gzip
import hashlib
import json
from pathlib import Path
import subprocess
import urllib.request

ROOT = Path(__file__).resolve().parents[3]
COMMIT = '79e9d244748a1201fe686ca8a882b5fe7e173348'
SOURCE_SHA256 = '35a4e1976fca49cb5f328c6bf3f1513458eda450c9e7425c24a78a8750c51dec'
URL = f'https://raw.githubusercontent.com/ethereum-optimism/op-geth/{COMMIT}/core/types/rollup_cost.go'
MAXIMUM = 2**256 - 1


def require(condition, message):
    if not condition:
        raise ValueError(message)


def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def function(source, name):
    start = source.index('\nfunc ' + name + '(') + 1
    brace = source.index('{', start)
    depth = 1
    end = brace + 1
    while depth:
        require(end < len(source), 'Unclosed upstream function')
        depth += (source[end] == '{') - (source[end] == '}')
        end += 1
    return source[start:end]


def rlp_one(data):
    """Strict byte/list decoding for the retained attributes deposit wire."""
    require(bool(data), 'Empty RLP')
    prefix = data[0]
    if prefix < 0x80:
        return data[:1], data[1:]
    is_list = prefix >= 0xc0
    offset = 0xc0 if is_list else 0x80
    size = prefix - offset
    head = 1
    if size > 55:
        length_bytes = size - 55
        require(len(data) > length_bytes and data[1] != 0, 'Noncanonical RLP length')
        size = int.from_bytes(data[1:1+length_bytes], 'big')
        require(size >= 56, 'Noncanonical long RLP')
        head += length_bytes
    require(head+size <= len(data), 'Truncated RLP')
    body, tail = data[head:head+size], data[head+size:]
    if not is_list:
        require(not (size == 1 and body[0] < 0x80), 'Noncanonical short RLP')
        return body, tail
    items = []
    while body:
        item, body = rlp_one(body)
        items.append(item)
    return items, tail


def attributes(base, overhead, scalar):
    words = [1, 1686068903, base, 0, 0, 0, overhead, scalar]
    return bytes.fromhex('015d8eb9') + b''.join(n.to_bytes(32, 'big') for n in words)


def public_cases():
    base = ROOT / 'chain-parser-go/test/fixtures/historical-receipts'
    body_path, vector_path = base/'optimism-regolith/body.json.gz', base/'optimism-regolith.json.gz'
    body, vector = (json.loads(gzip.decompress(path.read_bytes())) for path in (body_path, vector_path))
    txs = body['raw_transactions']
    deposit = bytes.fromhex(txs[0]['response']['result'][2:])
    require(deposit[0] == 0x7e, 'First transaction is not a deposit')
    fields, tail = rlp_one(deposit[1:])
    require(not tail and len(fields) == 8 and all(isinstance(f, bytes) for f in fields), 'Deposit layout')
    require(fields[1].hex() == 'deaddeaddeaddeaddeaddeaddeaddeaddead0001', 'Wrong depositor')
    require(fields[2].hex() == '4200000000000000000000000000000000000015', 'Wrong attributes target')
    data = fields[7]
    require(len(data) == 260 and data[:4].hex() == '015d8eb9', 'Wrong attributes ABI')
    rows = []
    for i, tx in enumerate(txs[1:], 1):
        receipt = vector['receipts'][i]
        require(tx['request']['params'][0] == vector['block']['transactions'][i] == receipt['transactionHash'], 'Retained transaction identity differs')
        wire = bytes.fromhex(tx['response']['result'][2:])
        require(wire and (wire[0] in [1, 2] or wire[0] >= 0xc0), 'Not a supported user transaction')
        rows.append(dict(id='public-op-'+str(i), chain_key='evm:10',
                         timestamp=int(vector['block']['timestamp'], 16), attributes=data.hex(),
                         encoded_transaction=wire.hex(), receipt_fee=str(int(receipt['l1Fee'], 16)),
                         receipt_gas=str(int(receipt['l1GasUsed'], 16)),
                         transaction_hash=receipt['transactionHash']))
    require(len(rows) == 3, 'Unexpected public coverage')
    return rows, {str(p.relative_to(ROOT)): digest(p) for p in [body_path, vector_path]}


def write(output):
    output = Path(output).resolve()
    output.parent.mkdir(parents=True, exist_ok=False)
    source_path = ROOT / 'integration/artifacts/local/op-l1-cost-reference/rollup_cost.go'
    source_path.parent.mkdir(parents=True, exist_ok=True)
    if not source_path.exists():
        with urllib.request.urlopen(URL, timeout=30) as response:
            source = response.read()
        require(hashlib.sha256(source).hexdigest() == SOURCE_SHA256, 'Upstream download hash differs')
        source_path.write_bytes(source)
    require(digest(source_path) == SOURCE_SHA256, 'Pinned upstream source changed')
    source = source_path.read_text()
    require('oneMillion     = big.NewInt(1_000_000)' in source, 'Scalar denominator changed')
    original = '\n\n'.join(function(source, name) for name in ['newL1CostFuncBedrockHelper', 'l1CostHelper'])
    runner = source[:source.index('package types')] + '''package main
import ("encoding/hex"; "encoding/json"; "fmt"; "math/big"; "os")
type RollupCostData struct { Zeroes, Ones, FastLzSize uint64 }
type l1CostFunc func(RollupCostData) (*big.Int, *big.Int)
var params = struct{ TxDataZeroGas, TxDataNonZeroGasEIP2028 uint64 }{4,16}
var oneMillion = big.NewInt(1_000_000)
''' + original + '''
func main() {
 var rows []map[string]interface{}
 if err:=json.NewDecoder(os.Stdin).Decode(&rows);err!=nil {panic(err)}
 for _,r:=range rows {
  wire,err:=hex.DecodeString(r["encoded_transaction"].(string));if err!=nil{panic(err)}
  attr,err:=hex.DecodeString(r["attributes"].(string));if err!=nil||len(attr)!=260{panic("attributes")}
  var data RollupCostData
  for _,b:=range wire{if b==0{data.Zeroes++}else{data.Ones++}}
  base:=new(big.Int).SetBytes(attr[68:100])
  overhead:=new(big.Int).SetBytes(attr[196:228])
  scalar:=new(big.Int).SetBytes(attr[228:260])
  fee,gas:=newL1CostFuncBedrockHelper(base,overhead,scalar,true)(data)
  r["fee"]=fee.String();r["gas"]=gas.String();r["zeroes"]=data.Zeroes;r["ones"]=data.Ones
  r["overflow"]=fee.BitLen()>256
 }
 b,err:=json.Marshal(rows);if err!=nil{panic(err)};fmt.Println(string(b))
}
'''
    runner_path = output.parent/'original-reference.go'
    runner_path.write_text(runner)
    rows, references = public_cases()
    wires = [bytes([2])+b'\x00'*z+b'\x01'*o for z,o in [(0,0),(0,67),(0,68),(1,0),(31,32),(32,31),(255,256),(4096,4095)]]
    parameters = [(0,0,0),(1,0,1),(1,0,1000000),(7,3,684000),
                  (2**80+1,2**64+1,1234567),(MAXIMUM,0,1),
                  (MAXIMUM,0,1000000),(1,MAXIMUM,0),(1,MAXIMUM,1)]
    for index, (base, overhead, scalar) in enumerate(parameters):
        for j, wire in enumerate(wires):
            rows.append(dict(id=f'integer-{index}-{j}', chain_key='evm:8453' if j%2 else 'evm:10',
                             timestamp=1700000000, attributes=attributes(base,overhead,scalar).hex(),
                             encoded_transaction=wire.hex()))
    result = subprocess.run(['go', 'run', str(runner_path)], cwd=output.parent, input=json.dumps(rows), text=True, capture_output=True, timeout=120)
    (output.parent/'original-reference.stderr').write_text(result.stderr)
    require(result.returncode == 0, 'Original Go reference failed: '+result.stderr[-1500:])
    actual = json.loads(result.stdout)
    require(len(actual) == len(rows), 'Original reference lost rows')
    for row, expected in zip(actual, rows):
        require(all(row.get(k) == v for k,v in expected.items()), 'Original reference changed input')
        wire = bytes.fromhex(row['encoded_transaction'])
        require(row['zeroes'] == wire.count(0) and row['ones'] == len(wire)-wire.count(0), 'Original byte counts differ')
        if 'receipt_fee' in row:
            require(row['fee'] == row['receipt_fee'] and row['gas'] == row['receipt_gas'], 'Original fee differs from retained public RPC receipt')
    references[str(source_path.relative_to(ROOT))] = SOURCE_SHA256
    references[str(runner_path.relative_to(ROOT))] = digest(runner_path)
    doc = dict(schema_version=1, cases=actual, source_url=URL, source_sha256=SOURCE_SHA256,
               original_function_sha256=hashlib.sha256(original.encode()).hexdigest(),
               public_signed_user_transactions=3, synthetic_arithmetic_cases=len(rows)-3,
               references=references, source_authority='PINNED_SOURCE_AND_RETAINED_RPC_OBSERVATIONS',
               production_ready=False, quote_ranking_changed=False)
    output.write_text(json.dumps(doc,indent=2)+'\n')
    return dict(sha256=digest(output), cases=len(rows), references=references,
                overflow_cases=sum(r['overflow'] for r in actual))
