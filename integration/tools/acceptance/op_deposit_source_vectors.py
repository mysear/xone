#!/usr/bin/env python3
"""Run the pinned original Optimism L1-info deposit source-hash method."""
import hashlib
import json
from pathlib import Path
import subprocess
import urllib.request
from op_l1_cost_vectors import ROOT, public_cases, require, digest

SOURCE_SHA256 = '33321b6605185db3a03a0a4bb00797efc88f8bd379a88eada91b622369b3fd95'
URL = 'https://raw.githubusercontent.com/ethereum-optimism/optimism/v1.9.5/op-node/rollup/derive/deposit_source.go'


def write(output):
    output = Path(output).resolve()
    output.parent.mkdir(parents=True, exist_ok=False)
    source_path = ROOT/'integration/artifacts/local/op-l1-cost-reference/deposit_source.go'
    source_path.parent.mkdir(parents=True, exist_ok=True)
    if not source_path.exists():
        with urllib.request.urlopen(URL, timeout=30) as response:
            raw = response.read()
        require(hashlib.sha256(raw).hexdigest() == SOURCE_SHA256, 'Original source hash download differs')
        source_path.write_bytes(raw)
    require(digest(source_path) == SOURCE_SHA256, 'Pinned original deposit source changed')
    source = source_path.read_text()
    start = source.index('func (dep *L1InfoDepositSource) SourceHash()')
    end = source.index('\n}', start)+2
    method = source[start:end]
    start = source.index('type L1InfoDepositSource struct {')
    end = source.index('\n}', start)+2
    declaration = source[start:end]
    require('L1InfoDepositSourceDomain     = 1' in source, 'L1-info source domain changed')
    runner = '''package main
import("encoding/binary";"encoding/json";"fmt";"os";"strconv";
"github.com/ethereum/go-ethereum/common";"github.com/ethereum/go-ethereum/crypto")
const L1InfoDepositSourceDomain=1
''' + declaration+'\n'+method+'''
func main(){
 var rows []map[string]string
 if err:=json.NewDecoder(os.Stdin).Decode(&rows);err!=nil{panic(err)}
 for _,r:=range rows{
  sequence,err:=strconv.ParseUint(r["sequence"],10,64);if err!=nil{panic(err)}
  d:=L1InfoDepositSource{L1BlockHash:common.HexToHash(r["l1_hash"]),SeqNumber:sequence}
  r["source_hash"]=d.SourceHash().Hex()
 }
 raw,err:=json.Marshal(rows);if err!=nil{panic(err)};fmt.Println(string(raw))
}
'''
    runner_path = output.parent/'original-deposit-source.go'
    runner_path.write_text(runner)
    rows=[]
    for i in range(4):
        l1_hash='0x'+hashlib.sha256(('op-l1-source-'+str(i)).encode()).hexdigest()
        for sequence in [0,1,255,2**32,2**63,2**64-1]:
            rows.append(dict(l1_hash=l1_hash, sequence=str(sequence)))
    public,_=public_cases()
    data=bytes.fromhex(public[0]['attributes'])
    rows.append(dict(l1_hash='0x'+data[100:132].hex(), sequence=str(int.from_bytes(data[132:164],'big'))))
    result=subprocess.run(['go','-C',str(ROOT/'chain-parser-go'),'run',str(runner_path)],input=json.dumps(rows),text=True,capture_output=True,timeout=120)
    (output.parent/'original-deposit-source.stderr').write_text(result.stderr)
    require(result.returncode==0,'Original deposit source execution failed: '+result.stderr[-1500:])
    actual=json.loads(result.stdout)
    require(len(actual)==25 and all(all(r.get(k)==v for k,v in expected.items()) for r,expected in zip(actual,rows)), 'Original deposit reference changed inputs')
    doc=dict(cases=actual,source_url=URL,source_sha256=SOURCE_SHA256,original_method_sha256=hashlib.sha256(method.encode()).hexdigest())
    output.write_text(json.dumps(doc,indent=2)+'\n')
    return dict(cases=len(actual),references={str(p.relative_to(ROOT)):digest(p) for p in [source_path,runner_path,output]})
